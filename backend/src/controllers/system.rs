//! `/api/stats`, `/api/system/*` e diagnostico do host (tarefas 5.5 e 11).
//!
//! O grupo `/api/system` reune monitoramento, retencao de backups e artefatos de
//! diagnostico. `/api/stats` fica fora do prefixo para manter o contrato do
//! Adonis.

use axum::body::Body;
use axum::extract::{Json, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use serde::Deserialize;

use crate::controllers::middlewares::origin::RequestOrigin;
use crate::controllers::Auth;
use crate::initializers::settings::Settings;
use crate::models::_entities::{backups, connections};
use crate::models::audit_log::{AuditAction, AuditEntityType};
use crate::models::audit_logs;
use crate::models::backup_retention_policy::UpdateBackupRetentionPolicy;
use crate::models::backup_runner;
use crate::models::storage::space;
use crate::models::system_monitor;
use crate::views::envelope::{Data, Message, MessageWithData};
use crate::views::errors::ApiError;
use crate::views::system as view;

type Reply = std::result::Result<Response, ApiError>;

/// Quantos backups recentes o painel mostra. Igual ao `limit(5)` do Adonis.
const RECENT_BACKUPS: u64 = 5;

/// `GET /api/stats` — o painel inicial.
#[debug_handler]
pub async fn stats(State(ctx): State<AppContext>, _session: Auth) -> Reply {
    // Local midnight: "today" is the operator's day, not UTC's.
    let now = chrono::Local::now().fixed_offset();
    let today = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(*now.offset()).single())
        .unwrap_or(now);

    let connections_total = connections::Model::count_all(&ctx.db).await?;
    let connections_active = connections::Model::count_active(&ctx.db).await?;
    let backups_total = backups::Model::count_all(&ctx.db).await?;
    let backups_today = backups::Model::count_since(&ctx.db, today).await?;
    let recent = backups::Model::recent_with_connection(&ctx.db, RECENT_BACKUPS).await?;
    let overview = system_monitor::SystemOverview::collect(&ctx).await;

    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let encryption = backup_runner::encryption_service(&settings)?;
    let storage_spaces =
        space::all_destinations_space(&ctx.db, &encryption, &settings.backup_storage_path).await?;

    Ok(axum::Json(Data::new(view::Stats {
        connections: view::ConnectionCounts {
            total: connections_total,
            active: connections_active,
        },
        backups: view::BackupCounts {
            total: backups_total,
            today: backups_today,
        },
        recent_backups: recent
            .into_iter()
            .map(|(backup, name)| view::RecentBackup::new(backup, name))
            .collect(),
        storage_spaces: storage_spaces
            .into_iter()
            .map(crate::views::storages::SpaceItem::from)
            .collect(),
        system: view::SystemOverview::from(overview),
    }))
    .into_response())
}

/// `GET /api/system/status` — CPU, memoria, uptime e estado do agendador.
#[debug_handler]
pub async fn status(State(ctx): State<AppContext>, _session: Auth) -> Reply {
    let overview = system_monitor::SystemOverview::collect(&ctx).await;

    Ok(axum::Json(Data::new(view::SystemOverview::from(overview))).into_response())
}

/// `GET /api/system/containers/resources` — metricas de containers Docker.
#[debug_handler]
pub async fn container_resources(State(ctx): State<AppContext>, _session: Auth) -> Reply {
    let overview = crate::models::docker_container_monitoring::overview(&ctx).await;
    Ok(axum::Json(Data::new(overview)).into_response())
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_range_hours")]
    pub range_hours: i64,
}

fn default_range_hours() -> i64 {
    24
}

/// `GET /api/system/resources/history` — historico de metricas agregado.
#[debug_handler]
pub async fn resources_history(
    State(ctx): State<AppContext>,
    _session: Auth,
    Query(query): Query<HistoryQuery>,
) -> Reply {
    let history = crate::models::resource_metric_history::history(&ctx, query.range_hours).await?;
    Ok(axum::Json(Data::new(history)).into_response())
}

/// `GET /api/system/backup-retention` — politica GFS atual.
#[debug_handler]
pub async fn backup_retention_policy(State(ctx): State<AppContext>, _session: Auth) -> Reply {
    let policy = crate::models::backup_retention_policy::get_policy(&ctx).await?;
    Ok(axum::Json(Data::new(view::BackupRetentionPolicy::from(policy))).into_response())
}

/// `PUT /api/system/backup-retention` — atualiza a politica GFS.
#[debug_handler]
pub async fn update_backup_retention_policy(
    State(ctx): State<AppContext>,
    _session: Auth,
    origin: RequestOrigin,
    Json(payload): Json<UpdateBackupRetentionPolicy>,
) -> Reply {
    validator::Validate::validate(&payload)
        .map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let prune_cron = payload.prune_cron.as_deref().unwrap_or_default();
    if !crate::models::backup_retention_policy::is_valid_cron(prune_cron) {
        return Err(ApiError::unprocessable(
            "Expressao cron invalida para o prune automatico",
        ));
    }

    let (policy, changes) =
        crate::models::backup_retention_policy::update_policy(&ctx, payload.into_policy()).await?;

    if !changes.is_empty() {
        let details = serde_json::json!({ "changes": changes });
        audit_logs::Model::record_or_warn(
            &ctx.db,
            audit_logs::AuditEntry::success(
                AuditAction::SettingsUpdated,
                "Politica de retencao atualizada",
            )
            .entity_type(AuditEntityType::Settings)
            .details(details)
            .from_request(origin.ip, origin.user_agent),
        )
        .await;
    }

    Ok(axum::Json(MessageWithData::new(
        "Política de retenção atualizada com sucesso",
        view::BackupRetentionPolicy::from(policy),
    ))
    .into_response())
}

/// `POST /api/system/backup-retention/run` — executa o prune de retencao.
#[debug_handler]
pub async fn run_backup_retention(State(ctx): State<AppContext>, __session: Auth) -> Reply {
    let result = crate::models::retention::prune_backups(&ctx).await?;
    Ok(axum::Json(MessageWithData::new(
        "Prune de backups executado com sucesso",
        result,
    ))
    .into_response())
}

/// `GET /api/system/diagnostics` — lista artefatos de diagnostico.
#[debug_handler]
pub async fn diagnostics(State(ctx): State<AppContext>, session: Auth) -> Reply {
    crate::controllers::require_admin(
        &session.user,
        "Apenas administradores podem acessar artefatos de diagnostico.",
    )?;

    let overview = crate::models::diagnostics::list(&ctx).await?;
    Ok(axum::Json(Data::new(overview)).into_response())
}

/// `GET /api/system/diagnostics/:name/download` — baixa um artefato.
#[debug_handler]
pub async fn download_diagnostic(
    State(ctx): State<AppContext>,
    session: Auth,
    origin: RequestOrigin,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Reply {
    crate::controllers::require_admin(
        &session.user,
        "Apenas administradores podem acessar artefatos de diagnostico.",
    )?;

    let Some(path) = crate::models::diagnostics::resolve(&ctx, &name)? else {
        return Err(ApiError::not_found(
            "Artefato de diagnostico nao encontrado",
        ));
    };

    let metadata = tokio::fs::metadata(&path).await.map_err(|err| {
        loco_rs::Error::Message(format!("falha ao ler metadados do artefato: {err}"))
    })?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|err| loco_rs::Error::Message(format!("falha ao abrir artefato: {err}")))?;

    audit_logs::Model::record_or_warn(
        &ctx.db,
        audit_logs::AuditEntry::success(
            AuditAction::DiagnosticsDownloaded,
            "Download de artefato de diagnostico",
        )
        .entity_type(AuditEntityType::Settings)
        .details(serde_json::json!({ "fileName": name }))
        .from_request(origin.ip, origin.user_agent),
    )
    .await;

    let body = Body::from_stream(tokio_util::io::ReaderStream::new(file));

    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream".to_string())],
        [(
            header::CONTENT_DISPOSITION,
            format!(r#"attachment; filename="{}""#, name),
        )],
        [(header::CONTENT_LENGTH, metadata.len().to_string())],
        body,
    )
        .into_response())
}

/// `DELETE /api/system/diagnostics/:name` — remove um artefato.
#[debug_handler]
pub async fn destroy_diagnostic(
    State(ctx): State<AppContext>,
    session: Auth,
    origin: RequestOrigin,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Reply {
    crate::controllers::require_admin(
        &session.user,
        "Apenas administradores podem acessar artefatos de diagnostico.",
    )?;

    let Some(path) = crate::models::diagnostics::resolve(&ctx, &name)? else {
        return Err(ApiError::not_found(
            "Artefato de diagnostico nao encontrado",
        ));
    };

    crate::models::diagnostics::remove(&path).await?;

    audit_logs::Model::record_or_warn(
        &ctx.db,
        audit_logs::AuditEntry::success(
            AuditAction::DiagnosticsDeleted,
            "Remocao de artefato de diagnostico",
        )
        .entity_type(AuditEntityType::Settings)
        .details(serde_json::json!({ "fileName": name }))
        .from_request(origin.ip, origin.user_agent),
    )
    .await;

    Ok(axum::Json(Message::new("Artefato de diagnostico removido")).into_response())
}

/// Rotas de system.
///
/// `/api/stats` fica **fora** do prefixo `/api/system` — e' assim no Adonis, e
/// mover para `/api/system/stats` quebraria o painel.
pub fn routes() -> Routes {
    Routes::new()
        .add("/api/stats", get(stats))
        .add("/api/system/status", get(status))
        .add("/api/system/containers/resources", get(container_resources))
        .add("/api/system/resources/history", get(resources_history))
        .add("/api/system/backup-retention", get(backup_retention_policy))
        .add(
            "/api/system/backup-retention",
            put(update_backup_retention_policy),
        )
        .add(
            "/api/system/backup-retention/run",
            post(run_backup_retention),
        )
        .add("/api/system/diagnostics", get(diagnostics))
        .add(
            "/api/system/diagnostics/{name}/download",
            get(download_diagnostic),
        )
        .add("/api/system/diagnostics/{name}", delete(destroy_diagnostic))
}
