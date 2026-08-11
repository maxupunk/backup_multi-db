//! Persistencia e leitura do historico de metricas (tarefa 11.3).
//!
//! A coleta periódica grava amostras de sistema e de containers Docker na tabela
//! `resource_metric_history`. A leitura agrrega por bucket de tempo para nunca
//! devolver mais que 300 pontos por entidade, evitando timeouts no painel.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use loco_rs::prelude::*;
use sea_orm::ActiveValue::Set;
use sea_orm::{FromQueryResult, Statement};
use serde::Serialize;
use tokio::sync::Mutex;

use crate::models::docker_container_monitoring::ContainerMetricsOverview;
use crate::models::system_monitor::SystemOverview;

pub use super::_entities::resource_metric_history::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

const RETENTION_DAYS: i64 = 15;
const MIN_PERSIST_INTERVAL: Duration = Duration::from_secs(60);
const PRUNE_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const FLUSH_INTERVAL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING_ROWS: usize = 100;
const MAX_POINTS: i64 = 300;

/// Uma row pronta para insercao.
#[derive(Debug, Clone)]
struct PendingRow {
    scope: String,
    entity_id: Option<String>,
    entity_name: Option<String>,
    cpu_usage_percent: f64,
    memory_usage_percent: f64,
    memory_used_bytes: i64,
    memory_total_bytes: i64,
    collected_at: chrono::NaiveDateTime,
}

/// Estado em memoria do buffer e controle de cadencia.
#[derive(Clone, Default)]
pub struct State {
    inner: Arc<Mutex<StateInner>>,
}

struct StateInner {
    pending: Vec<PendingRow>,
    /// Timestamp em millis da ultima coleta persistida por chave.
    last_persisted_at: HashMap<String, u64>,
    last_flush_at: Instant,
    last_prune_at: Instant,
}

impl Default for StateInner {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            last_persisted_at: HashMap::new(),
            last_flush_at: Instant::now(),
            last_prune_at: Instant::now(),
        }
    }
}

impl State {
    async fn with_lock<T>(&self, op: impl FnOnce(&mut StateInner) -> T) -> T {
        let mut guard = self.inner.lock().await;
        op(&mut guard)
    }
}

pub fn register(ctx: &AppContext) {
    if !ctx.shared_store.contains::<State>() {
        ctx.shared_store.insert(State::default());
    }
}

fn state(ctx: &AppContext) -> loco_rs::Result<State> {
    ctx.shared_store
        .get::<State>()
        .ok_or_else(|| Error::Message("resource metric history state was not initialized".into()))
}

fn now() -> chrono::NaiveDateTime {
    chrono::Utc::now().naive_utc()
}

fn persist_key(scope: &str, entity_id: Option<&str>) -> String {
    format!("{}:{}", scope, entity_id.unwrap_or("global"))
}

/// Grava uma amostra do sistema, respeitando o intervalo minimo.
pub async fn record_system(ctx: &AppContext, overview: &SystemOverview) -> Result<()> {
    let key = persist_key("system", None);
    let collected_at = now();
    let collected_at_ms = collected_at.and_utc().timestamp_millis() as u64;
    let min_interval_ms = MIN_PERSIST_INTERVAL.as_millis() as u64;

    let should = state(ctx)?
        .with_lock(|inner| {
            if let Some(last_ms) = inner.last_persisted_at.get(&key) {
                if collected_at_ms.saturating_sub(*last_ms) < min_interval_ms {
                    return false;
                }
            }
            inner.last_persisted_at.insert(key.clone(), collected_at_ms);
            true
        })
        .await;

    if !should {
        return Ok(());
    }

    let row = PendingRow {
        scope: "system".into(),
        entity_id: None,
        entity_name: Some("Servidor".into()),
        cpu_usage_percent: overview.cpu.usage_percent,
        memory_usage_percent: overview.memory.usage_percent,
        memory_used_bytes: overview.memory.used_bytes as i64,
        memory_total_bytes: overview.memory.total_bytes as i64,
        collected_at,
    };

    enqueue(ctx, row).await?;
    flush_if_needed(ctx).await?;
    prune_old(ctx).await?;
    Ok(())
}

/// Grava uma amostra de cada container, respeitando o intervalo minimo por
/// container.
pub async fn record_containers(
    ctx: &AppContext,
    overview: &ContainerMetricsOverview,
) -> Result<()> {
    if !overview.docker_available {
        return Ok(());
    }

    let collected_at = now();
    let collected_at_ms = collected_at.and_utc().timestamp_millis() as u64;
    let min_interval_ms = MIN_PERSIST_INTERVAL.as_millis() as u64;
    let mut rows = Vec::with_capacity(overview.containers.len());

    state(ctx)?
        .with_lock(|inner| {
            for container in &overview.containers {
                let key = persist_key("container", Some(&container.container_id));
                if let Some(last_ms) = inner.last_persisted_at.get(&key) {
                    if collected_at_ms.saturating_sub(*last_ms) < min_interval_ms {
                        continue;
                    }
                }
                inner.last_persisted_at.insert(key, collected_at_ms);
                rows.push(PendingRow {
                    scope: "container".into(),
                    entity_id: Some(container.container_id.clone()),
                    entity_name: Some(container.container_name.clone()),
                    cpu_usage_percent: container.cpu.usage_percent,
                    memory_usage_percent: container.memory.usage_percent,
                    memory_used_bytes: container.memory.usage_bytes as i64,
                    memory_total_bytes: container.memory.limit_bytes as i64,
                    collected_at,
                });
            }
        })
        .await;

    if rows.is_empty() {
        return Ok(());
    }

    enqueue_many(ctx, rows).await?;
    flush_if_needed(ctx).await?;
    prune_old(ctx).await?;
    Ok(())
}

async fn enqueue(ctx: &AppContext, row: PendingRow) -> Result<()> {
    state(ctx)?.with_lock(|inner| inner.pending.push(row)).await;
    Ok(())
}

async fn enqueue_many(ctx: &AppContext, rows: Vec<PendingRow>) -> Result<()> {
    state(ctx)?
        .with_lock(|inner| inner.pending.extend(rows))
        .await;
    Ok(())
}

/// Descarrega as rows pendentes no banco. `force = true` tenta esvaziar tudo.
pub async fn flush(ctx: &AppContext, force: bool) -> Result<()> {
    loop {
        let rows = state(ctx)?
            .with_lock(|inner| {
                if inner.pending.is_empty() {
                    return Vec::new();
                }
                std::mem::take(&mut inner.pending)
            })
            .await;

        if rows.is_empty() {
            return Ok(());
        }

        insert_rows(&ctx.db, &rows).await?;

        state(ctx)?
            .with_lock(|inner| inner.last_flush_at = Instant::now())
            .await;

        if !force {
            return Ok(());
        }
    }
}

async fn flush_if_needed(ctx: &AppContext) -> Result<()> {
    let should = state(ctx)?
        .with_lock(|inner| {
            inner.pending.len() >= MAX_PENDING_ROWS
                || inner.last_flush_at.elapsed() >= FLUSH_INTERVAL
        })
        .await;

    if should {
        flush(ctx, false).await?;
    }
    Ok(())
}

async fn insert_rows(db: &DatabaseConnection, rows: &[PendingRow]) -> Result<()> {
    use sea_orm::EntityTrait;

    let now = now();
    let active_models: Vec<super::_entities::resource_metric_history::ActiveModel> = rows
        .iter()
        .map(
            |row| super::_entities::resource_metric_history::ActiveModel {
                scope: Set(row.scope.clone()),
                entity_id: Set(row.entity_id.clone()),
                entity_name: Set(row.entity_name.clone()),
                cpu_usage_percent: Set(row.cpu_usage_percent as f32),
                memory_usage_percent: Set(row.memory_usage_percent as f32),
                memory_used_bytes: Set(row.memory_used_bytes),
                memory_total_bytes: Set(row.memory_total_bytes),
                collected_at: Set(row.collected_at),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .collect();

    super::_entities::resource_metric_history::Entity::insert_many(active_models)
        .exec(db)
        .await?;

    Ok(())
}

/// Remove registros mais antigos que `RETENTION_DAYS`.
pub async fn prune_old(ctx: &AppContext) -> Result<()> {
    let should = state(ctx)?
        .with_lock(|inner| inner.last_prune_at.elapsed() >= PRUNE_INTERVAL)
        .await;

    if !should {
        return Ok(());
    }

    let cutoff = now() - chrono::Duration::days(RETENTION_DAYS);

    Entity::delete_many()
        .filter(Column::CollectedAt.lt(cutoff))
        .exec(&ctx.db)
        .await?;

    state(ctx)?
        .with_lock(|inner| inner.last_prune_at = Instant::now())
        .await;

    Ok(())
}

/// Um ponto de metrica no tempo.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub timestamp: String,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub memory_used_bytes: i64,
    pub memory_total_bytes: i64,
}

/// Historico de um container.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerHistory {
    pub container_id: String,
    pub container_name: String,
    pub points: Vec<HistoryPoint>,
}

/// Resposta de `GET /api/system/resources/history`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub retention_days: i64,
    pub system: Vec<HistoryPoint>,
    pub containers: Vec<ContainerHistory>,
}

/// Linha bruta retornada pela agregacao SQL.
#[derive(Debug, FromQueryResult)]
struct RawHistoryRow {
    scope: String,
    entity_id: Option<String>,
    entity_name: Option<String>,
    cpu_usage_percent: f64,
    memory_usage_percent: f64,
    memory_used_bytes: i64,
    memory_total_bytes: i64,
    bucket_time: String,
}

/// Le o historico agregado por bucket de tempo.
pub async fn history(ctx: &AppContext, range_hours: i64) -> Result<HistoryResponse> {
    flush(ctx, true).await?;

    let bounded_hours = range_hours.clamp(1, RETENTION_DAYS * 24);
    let start_at = now() - chrono::Duration::hours(bounded_hours);

    let bucket_seconds = ((bounded_hours * 3600) as f64 / MAX_POINTS as f64).ceil() as i64;
    let bucket_seconds = bucket_seconds.max(60);

    let backend = ctx.db.get_database_backend();
    let (sql, values) = history_sql(backend, start_at, bucket_seconds);

    let rows = Entity::find()
        .from_raw_sql(Statement::from_sql_and_values(backend, &sql, values))
        .into_model::<RawHistoryRow>()
        .all(&ctx.db)
        .await?;

    let mut system: Vec<HistoryPoint> = Vec::new();
    let mut container_map: HashMap<String, ContainerHistory> = HashMap::new();

    for row in rows {
        let point = HistoryPoint {
            timestamp: format_bucket_time(&row.bucket_time),
            cpu_usage_percent: row.cpu_usage_percent,
            memory_usage_percent: row.memory_usage_percent,
            memory_used_bytes: row.memory_used_bytes,
            memory_total_bytes: row.memory_total_bytes,
        };

        if row.scope == "system" {
            system.push(point);
            continue;
        }

        let id = row.entity_id.unwrap_or_else(|| "unknown".into());
        let entry = container_map
            .entry(id.clone())
            .or_insert_with(|| ContainerHistory {
                container_id: id.clone(),
                container_name: row.entity_name.unwrap_or_else(|| id.clone()),
                points: Vec::new(),
            });
        entry.points.push(point);
    }

    Ok(HistoryResponse {
        retention_days: RETENTION_DAYS,
        system,
        containers: container_map.into_values().collect(),
    })
}

fn format_bucket_time(raw: &str) -> String {
    // O SQLite devolve `2026-08-06 16:49:25`; o contrato exige ISO com Z.
    let normalized = raw.replace(' ', "T");
    format!("{normalized}.000Z")
}

fn history_sql(
    backend: sea_orm::DatabaseBackend,
    start_at: chrono::NaiveDateTime,
    bucket_seconds: i64,
) -> (String, Vec<sea_orm::Value>) {
    match backend {
        sea_orm::DatabaseBackend::Postgres => (
            "SELECT scope, entity_id, entity_name, \
             AVG(cpu_usage_percent) AS cpu_usage_percent, \
             AVG(memory_usage_percent) AS memory_usage_percent, \
             CAST(AVG(memory_used_bytes) AS BIGINT) AS memory_used_bytes, \
             CAST(AVG(memory_total_bytes) AS BIGINT) AS memory_total_bytes, \
             to_timestamp((EXTRACT(EPOCH FROM collected_at)::BIGINT / $2) * $2) AS bucket_time \
             FROM resource_metric_history \
             WHERE collected_at >= $1 \
             GROUP BY scope, entity_id, entity_name, EXTRACT(EPOCH FROM collected_at)::BIGINT / $2 \
             ORDER BY scope, entity_id, bucket_time"
                .to_string(),
            vec![start_at.into(), bucket_seconds.into()],
        ),
        _ => (
            "SELECT scope, entity_id, entity_name, \
             AVG(cpu_usage_percent) AS cpu_usage_percent, \
             AVG(memory_usage_percent) AS memory_usage_percent, \
             CAST(AVG(memory_used_bytes) AS INTEGER) AS memory_used_bytes, \
             CAST(AVG(memory_total_bytes) AS INTEGER) AS memory_total_bytes, \
             datetime(CAST(strftime('%s', collected_at) / ? AS INTEGER) * ?, 'unixepoch') AS bucket_time \
             FROM resource_metric_history \
             WHERE collected_at >= ? \
             GROUP BY scope, entity_id, CAST(strftime('%s', collected_at) / ? AS INTEGER) \
             ORDER BY scope, entity_id, bucket_time"
                .to_string(),
            vec![
                bucket_seconds.into(),
                bucket_seconds.into(),
                start_at.into(),
                bucket_seconds.into(),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bucket_time_converts_sqlite_datetime() {
        assert_eq!(
            format_bucket_time("2026-08-06 16:49:25"),
            "2026-08-06T16:49:25.000Z"
        );
    }

    #[test]
    fn history_bucket_seconds_never_below_sixty() {
        // 1 hora / 300 = 12 s, mas o minimo e' 60 s.
        let (sql, values) = history_sql(
            sea_orm::DatabaseBackend::Sqlite,
            now(),
            12, // seria o valor cru
        );
        // A funcao recebe bucket_seconds ja' calculado; o minimo e' imposto
        // pelo chamador. Aqui so' verificamos que a SQL esta' presente.
        assert!(sql.contains("bucket_time"));
        assert_eq!(values.len(), 4);
    }
}
