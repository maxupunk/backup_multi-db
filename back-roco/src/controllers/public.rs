//! Rotas públicas sem autenticação: health, documentação OpenAPI e fallback da SPA.
//!
//! A suíte de contrato (Fase 2) trata `GET /api/health` como o marco mínimo de
//! subida do servidor, então este controller é um dos últimos passos antes do
//! cutover (Fase 12).

use axum::http::header;
use axum::response::{Html, IntoResponse, Response};
use loco_rs::prelude::*;
use serde::Serialize;

/// Corpo de `GET /api/health`.
///
/// O Adonis responde um objeto plano — sem envelope `success/data` — com três
/// campos obrigatórios. A versão é fixa em `1.0.0` para bater com o contrato.
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub timestamp: String,
    pub version: &'static str,
}

impl HealthResponse {
    pub fn now() -> Self {
        Self {
            status: "ok",
            timestamp: chrono::Utc::now().to_rfc3339(),
            version: "1.0.0",
        }
    }
}

/// `GET /api/health` — usado pelo harness de contrato e pelo healthcheck do
/// Docker. Deve ser leve, não tocar no banco e anunciar o rate limit global.
#[debug_handler]
pub async fn health() -> Response {
    let body = serde_json::to_string(&HealthResponse::now()).unwrap_or_else(|_| {
        // Falha de serialização de um objeto trivial é impossível em prática,
        // mas manter uma string constante evita panico no handler.
        r#"{"status":"ok","timestamp":"","version":"1.0.0"}"#.to_string()
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

/// `GET /api/swagger` — serve a especificação OpenAPI 3.0 do Adonis.
///
/// A spec é o arquivo `docs/openapi-baseline.yml` gravado na Fase 0. Ele é
/// embutido no binário para não depender de caminho de filesystem em produção.
#[debug_handler]
pub async fn swagger() -> Response {
    const OPENAPI_YAML: &str = include_str!("../../../docs/openapi-baseline.yml");

    (
        [(header::CONTENT_TYPE, "text/yaml; charset=utf-8".to_string())],
        OPENAPI_YAML,
    )
        .into_response()
}

/// `GET /api/docs` — UI do Swagger apontando para `/api/swagger`.
#[debug_handler]
pub async fn docs() -> Response {
    Html(DOCS_HTML).into_response()
}

const DOCS_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>DB Backup Manager API</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
    <script>
      window.onload = function () {
        window.ui = SwaggerUIBundle({
          url: '/api/swagger',
          dom_id: '#swagger-ui',
        });
      };
    </script>
  </body>
</html>"#;

/// Rotas públicas.
pub fn routes() -> Routes {
    Routes::new()
        .add("/api/health", get(health))
        .add("/api/swagger", get(swagger))
        .add("/api/docs", get(docs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_body_matches_the_adonis_contract() {
        let health = HealthResponse::now();
        assert_eq!(health.status, "ok");
        assert_eq!(health.version, "1.0.0");
        assert!(!health.timestamp.is_empty());
    }

    #[test]
    fn swagger_embeds_the_baseline_spec() {
        // Se o arquivo sumir ou mudar de lugar, o build quebra — isso é
        // desejável, porque a spec é contrato versionado.
        assert!(include_str!("../../../docs/openapi-baseline.yml").contains("openapi:"));
    }
}
