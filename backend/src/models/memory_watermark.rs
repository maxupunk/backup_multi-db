//! Guarda-rail de memoria do processo (tarefa 11.8).
//!
//! O Adonis usa `process.memoryUsage()` do Node, que expoe RSS e heap do V8.
//! Em Rust nao ha' heap do V8, mas o RSS ainda e' util: ele mede a pressao de
//! memoria real do processo e ajuda a investigar OOMs. O pico observado desde o
//! start e' guardado para distinguir "sempre foi assim" de "acabou de subir".
//!
//! O estado fica no `AppContext`, nao em `static`, para nao vazar entre
//! instancias nem entre testes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use loco_rs::prelude::*;
use serde::Serialize;
use sysinfo::{get_current_pid, ProcessesToUpdate, System};
use tokio::sync::Mutex;

const PRESSURE_THRESHOLD_PERCENT: f64 = 70.0;
const PRESSURE_LOG_COOLDOWN: Duration = Duration::from_secs(60);

/// Leitura atual da memoria do processo.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReading {
    pub rss_bytes: u64,
    pub limit_bytes: u64,
    pub usage_percent: f64,
}

/// Picos observados desde o start (ou desde o ultimo reset).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWatermark {
    pub observed_since: String,
    pub current: MemoryReading,
    pub peak_rss_bytes: u64,
    pub peak_rss_observed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

struct StateInner {
    peak_rss_bytes: u64,
    peak_rss_observed_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    observed_since: String,
    last_pressure_log_at: Option<Instant>,
}

/// Estado em memoria da guarda de memoria.
#[derive(Clone, Default)]
pub struct State {
    inner: Arc<Mutex<StateInner>>,
}

impl Default for StateInner {
    fn default() -> Self {
        Self {
            peak_rss_bytes: 0,
            peak_rss_observed_at: None,
            observed_since: chrono::Utc::now().to_rfc3339(),
            last_pressure_log_at: None,
        }
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
        .ok_or_else(|| Error::Message("memory watermark state was not initialized".into()))
}

/// Coleta a memoria atual e atualiza os picos. Barato o suficiente para rodar
/// a cada ciclo de polling.
pub async fn sample(ctx: &AppContext, context: &str) -> Result<MemoryWatermark> {
    let reading = current_reading();
    let now = chrono::Utc::now().fixed_offset();

    let state = state(ctx)?;
    let mut guard = state.inner.lock().await;

    if reading.rss_bytes > guard.peak_rss_bytes {
        guard.peak_rss_bytes = reading.rss_bytes;
        guard.peak_rss_observed_at = Some(now);
    }

    let watermark = MemoryWatermark {
        observed_since: guard.observed_since.clone(),
        current: reading,
        peak_rss_bytes: guard.peak_rss_bytes,
        peak_rss_observed_at: guard.peak_rss_observed_at,
    };

    if reading.usage_percent >= PRESSURE_THRESHOLD_PERCENT {
        let should_log = guard
            .last_pressure_log_at
            .map(|instant| instant.elapsed() >= PRESSURE_LOG_COOLDOWN)
            .unwrap_or(true);

        if should_log {
            guard.last_pressure_log_at = Some(Instant::now());
            tracing::warn!(
                context = %context,
                rss_bytes = reading.rss_bytes,
                limit_bytes = reading.limit_bytes,
                usage_percent = reading.usage_percent,
                peak_rss_bytes = watermark.peak_rss_bytes,
                "pressao de memoria acima de {}% (rss {}%)",
                PRESSURE_THRESHOLD_PERCENT,
                reading.usage_percent
            );
        }
    }

    Ok(watermark)
}

/// Retorna a leitura atual sem atualizar os picos.
pub async fn current(ctx: &AppContext) -> Result<MemoryWatermark> {
    let reading = current_reading();
    let state = state(ctx)?;
    let guard = state.inner.lock().await;

    Ok(MemoryWatermark {
        observed_since: guard.observed_since.clone(),
        current: reading,
        peak_rss_bytes: guard.peak_rss_bytes,
        peak_rss_observed_at: guard.peak_rss_observed_at,
    })
}

/// Reseta os picos observados.
pub async fn reset(ctx: &AppContext) -> Result<()> {
    let state = state(ctx)?;
    let mut guard = state.inner.lock().await;
    guard.peak_rss_bytes = 0;
    guard.peak_rss_observed_at = None;
    guard.observed_since = chrono::Utc::now().to_rfc3339();
    guard.last_pressure_log_at = None;
    Ok(())
}

fn current_reading() -> MemoryReading {
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let rss = system
        .process(get_current_pid().unwrap())
        .map(|p| p.memory())
        .unwrap_or(0);

    let (limit, _) = match system.cgroup_limits() {
        Some(limits) => (limits.total_memory, true),
        None => (system.total_memory(), false),
    };

    MemoryReading {
        rss_bytes: rss,
        limit_bytes: limit,
        usage_percent: percentage(rss, limit),
    }
}

fn percentage(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    round_percent(part as f64 * 100.0 / whole as f64)
}

fn round_percent(value: f64) -> f64 {
    (value.clamp(0.0, 100.0) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn sample_updates_peak() {
        let boot = loco_rs::testing::prelude::boot_test::<crate::app::App>()
            .await
            .expect("test boot");
        let ctx = &boot.app_context;
        register(ctx);

        let first = sample(ctx, "test").await.unwrap();
        assert!(first.current.rss_bytes > 0);
        assert_eq!(first.peak_rss_bytes, first.current.rss_bytes);

        reset(ctx).await.unwrap();
        let after_reset = current(ctx).await.unwrap();
        assert_eq!(after_reset.peak_rss_bytes, 0);
    }
}
