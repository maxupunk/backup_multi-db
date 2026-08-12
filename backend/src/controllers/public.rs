//! Public routes, no authentication: the health probe.
//!
//! `GET /api/health` is the readiness marker for the Docker healthcheck, so it
//! has to stay cheap and must not touch the database — a probe that queries is
//! a probe that reports "down" whenever the database is merely slow.

use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use loco_rs::prelude::*;
use serde::Serialize;

use crate::app::App;

/// Body of `GET /api/health`.
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub timestamp: String,
    /// Crate version plus the build SHA, from [`App::app_version`]. A fixed
    /// string here would make every deployment claim the same build, which is
    /// exactly the question a health probe is asked during an incident.
    pub version: String,
}

impl HealthResponse {
    #[must_use]
    pub fn now() -> Self {
        Self {
            status: "ok",
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: <App as loco_rs::app::Hooks>::app_version(),
        }
    }
}

/// `GET /api/health` — used by the Docker healthcheck.
#[debug_handler]
pub async fn health() -> Response {
    let body = serde_json::to_string(&HealthResponse::now()).unwrap_or_else(|_| {
        // A serialisation failure on a trivial object is not reachable in
        // practice; a constant keeps the handler panic-free anyway.
        r#"{"status":"ok","timestamp":"","version":"unknown"}"#.to_string()
    });

    (
        [(
            header::CONTENT_TYPE,
            "application/json; charset=utf-8".to_string(),
        )],
        body,
    )
        .into_response()
}

/// `/api/*` sem rota — 404 em JSON.
///
/// Existe porque o fallback do router serve a SPA: sem este catch-all,
/// `GET /api/typo` responderia **200 `text/html`** com a página inteira do Vue,
/// e um cliente que pediu JSON receberia um documento. O `matchit` prefere a
/// rota mais específica, então isto só é alcançado quando nenhuma outra casou.
#[debug_handler]
pub async fn unknown_route() -> Result<Response> {
    Err(Error::NotFound)
}

/// Public routes.
///
/// O catch-all vem por último de propósito: registrar `/api/{*path}` antes de
/// `/api/health` não muda o casamento (o `matchit` decide por especificidade),
/// mas ler o arquivo de cima para baixo deixaria a dúvida.
pub fn routes() -> Routes {
    Routes::new()
        .add("/api/health", get(health))
        .add("/api/{*path}", any(unknown_route))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_reports_the_running_build() {
        let health = HealthResponse::now();

        assert_eq!(health.status, "ok");
        assert!(!health.timestamp.is_empty());
        // The crate version, not a literal — the probe is how an operator finds
        // out which build is answering.
        assert!(
            health.version.starts_with(env!("CARGO_PKG_VERSION")),
            "versao inesperada: {}",
            health.version
        );
    }
}
