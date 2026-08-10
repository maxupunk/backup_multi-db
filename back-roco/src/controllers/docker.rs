//! `/api/docker` — disponibilidade da Docker Engine (Fase 9).

use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use serde::Serialize;

use crate::controllers::middlewares::auth::Authenticated;
use crate::models::docker;
use crate::views::errors::ApiError;

type Reply = std::result::Result<Response, ApiError>;

#[derive(Debug, Serialize)]
struct StatusData {
    available: bool,
}

#[derive(Debug, Serialize)]
struct StatusEnvelope {
    success: bool,
    available: bool,
    data: StatusData,
}

/// `GET /api/docker/status`.
///
/// A ausência do Docker é um estado válido, não uma indisponibilidade da API:
/// retorna 200 para que o frontend possa desabilitar a área correspondente.
#[debug_handler]
pub async fn status(State(_ctx): State<AppContext>, _session: Authenticated) -> Reply {
    let status = docker::status().await;
    Ok(axum::Json(StatusEnvelope {
        success: true,
        available: status.available,
        data: StatusData {
            available: status.available,
        },
    })
    .into_response())
}

/// Rotas iniciais do Docker Manager.
pub fn routes() -> Routes {
    Routes::new().add("/api/docker/status", get(status))
}
