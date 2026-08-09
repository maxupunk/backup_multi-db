//! `/api/stats` e `/api/system/status` (tarefa 5.5 — parcial).
//!
//! Sao as duas rotas de `system` que ja' tem tudo de que precisam. As outras
//! oito (`diagnostics`, `containers/resources`, `resources/history`,
//! `backup-retention`) dependem do cliente Docker da Fase 9 e da politica de
//! retencao da Fase 11, e entram junto com elas.
//!
//! O bloco `storageSpaces` de `GET /api/stats` sai **vazio** ate' a Fase 8
//! ligar o servico de espaco. A alternativa seria omitir a chave, e ai' o
//! painel do frontend quebraria ao iterar sobre `undefined`.

use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;

use crate::controllers::middlewares::auth::Authenticated;
use crate::models::_entities::{backups, connections};
use crate::models::system_monitor;
use crate::views::envelope::Data;
use crate::views::errors::ApiError;
use crate::views::system as view;

type Reply = std::result::Result<Response, ApiError>;

/// Quantos backups recentes o painel mostra. Igual ao `limit(5)` do Adonis.
const RECENT_BACKUPS: u64 = 5;

/// `GET /api/stats` — o painel inicial.
#[debug_handler]
pub async fn stats(State(ctx): State<AppContext>, _session: Authenticated) -> Reply {
    // Meia-noite local: os timestamps gravados sao hora local ingenua.
    let today = chrono::Local::now()
        .naive_local()
        .date()
        .and_hms_opt(0, 0, 0)
        .unwrap_or_else(|| chrono::Local::now().naive_local());

    let connections_total = connections::Model::count_all(&ctx.db).await?;
    let connections_active = connections::Model::count_active(&ctx.db).await?;
    let backups_total = backups::Model::count_all(&ctx.db).await?;
    let backups_today = backups::Model::count_since(&ctx.db, today).await?;
    let recent = backups::Model::recent_with_connection(&ctx.db, RECENT_BACKUPS).await?;
    let overview = system_monitor::SystemOverview::collect().await;

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
        storage_spaces: Vec::new(),
        system: view::SystemOverview::from(overview),
    }))
    .into_response())
}

/// `GET /api/system/status` — CPU, memoria, uptime e estado do agendador.
#[debug_handler]
pub async fn status(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    let overview = system_monitor::SystemOverview::collect().await;

    Ok(axum::Json(Data::new(view::SystemOverview::from(overview))).into_response())
}

/// Rotas de system.
///
/// `/api/stats` fica **fora** do prefixo `/api/system` — e' assim no Adonis, e
/// mover para `/api/system/stats` quebraria o painel.
pub fn routes() -> Routes {
    Routes::new()
        .add("/api/stats", get(stats))
        .add("/api/system/status", get(status))
}
