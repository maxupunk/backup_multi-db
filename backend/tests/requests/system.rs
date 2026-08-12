//! Lote 2.6 (parte) — `/api/stats` e `/api/system/status`.
//!
//! Os numeros de CPU e memoria dependem da maquina e a suite de contrato os
//! ignora. O que estes testes fixam e' o **formato** — que e' o que o painel do
//! frontend consome — e a autorizacao.

use backend::app::App;
use loco_rs::testing::prelude::*;
use serde_json::Value;
use serial_test::serial;

use super::session;

#[tokio::test]
#[serial]
async fn stats_aggregates_the_empty_installation() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");

        let response = request.get("/api/stats").authorization_bearer(&token).await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let data = &response.json::<Value>()["data"];

        assert_eq!(data["connections"]["total"], 0);
        assert_eq!(data["connections"]["active"], 0);
        assert_eq!(data["backups"]["total"], 0);
        assert_eq!(data["backups"]["today"], 0);
        // Presentes e vazios, nunca ausentes: o painel itera sobre os dois, e
        // uma chave faltando quebraria a tela.
        assert!(data["recentBackups"].is_array());
        assert!(data["storageSpaces"].is_array());
        assert!(data["system"].is_object());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_overview_keeps_the_adonis_shape() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");

        let response = request
            .get("/api/system/status")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let body: Value = response.json();
        assert_eq!(body["success"], true);

        let data = &body["data"];
        for key in [
            "version",
            "hostname",
            "platform",
            "architecture",
            "runtimeVersion",
            "uptimeSeconds",
            "resources",
            "jobs",
        ] {
            assert!(data.get(key).is_some(), "faltou `{key}`");
        }

        // Medicoes plausiveis, nao placeholders.
        assert!(data["resources"]["cpu"]["cores"].as_u64().unwrap_or(0) >= 1);
        assert!(
            data["resources"]["memory"]["totalBytes"]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
        // O agendador so' existe a partir da Fase 10.
        assert_eq!(data["jobs"]["status"], "down");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn both_routes_deny_without_a_session() {
    request::<App, _, _>(|request, _ctx| async move {
        for path in ["/api/stats", "/api/system/status"] {
            let response = request.get(path).await;
            assert_eq!(response.status_code(), 401, "{path}");
            assert!(
                response
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .starts_with("application/json"),
                "{path} nao respondeu JSON"
            );
        }
    })
    .await;
}
