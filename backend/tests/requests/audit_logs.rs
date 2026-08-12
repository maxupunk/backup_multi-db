//! Lote 2.6 (parte) — `/api/audit-logs`, mais a persistencia da tarefa 3.8.
//!
//! Nenhuma rota da Fase 5 grava auditoria — no Adonis, `auth` e `users` nao
//! chamam o `AuditService`. As entradas aqui sao criadas pelo proprio
//! `AuditLog::record`, que e' o metodo que os controllers da Fase 6 em diante
//! vao usar; assim o lado da escrita e o da leitura sao testados juntos.

use backend::app::App;
use backend::models::audit_log::{AuditAction, AuditStatus};
use backend::models::audit_logs::{AuditEntry, Model as AuditLog};
use loco_rs::testing::prelude::*;
use serde_json::Value;
use serial_test::serial;

use super::session;

async fn seed_entries(db: &sea_orm::DatabaseConnection) {
    AuditLog::record(
        db,
        AuditEntry::success(
            AuditAction::ConnectionCreated,
            "Conexão \"Contract Postgres\" foi criada",
        )
        .entity(2, "Contract Postgres")
        .from_request(Some("127.0.0.1".to_string()), Some("curl/8".to_string())),
    )
    .await
    .expect("grava a primeira entrada");

    AuditLog::record(
        db,
        AuditEntry::success(AuditAction::BackupFailed, "Backup falhou")
            .entity(9, "app_fixture")
            .failed("ECONNREFUSED"),
    )
    .await
    .expect("grava a entrada de falha");
}

#[tokio::test]
#[serial]
async fn lists_the_entries_with_the_derived_fields() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");
        seed_entries(&ctx.db).await;

        let response = request
            .get("/api/audit-logs")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200);
        let body: Value = response.json();
        assert_eq!(body["pagination"]["total_items"], 2);
        assert_eq!(body["pagination"]["page_size"], 50);

        // A mais recente primeiro.
        let first = &body["results"][0];
        assert_eq!(first["action"], "backup.failed");
        assert_eq!(first["actionDescription"], "Backup falhou");
        assert_eq!(first["statusColor"], "error");
        assert_eq!(first["errorMessage"], "ECONNREFUSED");
        // Na listagem o agente nao e' carregado; a chave existe e vale `null`.
        assert!(first["userAgent"].is_null());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn filters_by_action_and_status() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");
        seed_entries(&ctx.db).await;

        let by_action: Value = request
            .get("/api/audit-logs")
            .add_query_param("action", "connection.created")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(by_action["pagination"]["total_items"], 1);
        assert_eq!(by_action["results"][0]["entityName"], "Contract Postgres");

        let by_status: Value = request
            .get("/api/audit-logs")
            .add_query_param("status", "failure")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(by_status["pagination"]["total_items"], 1);

        // Filtro vazio nao filtra — e' o que a tela manda com o campo em branco.
        let empty: Value = request
            .get("/api/audit-logs")
            .add_query_param("action", "")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(empty["pagination"]["total_items"], 2);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn caps_the_page_size_at_a_hundred() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");
        seed_entries(&ctx.db).await;

        let body: Value = request
            .get("/api/audit-logs")
            .add_query_param("page_size", "5000")
            .authorization_bearer(&token)
            .await
            .json();

        // Sem o teto, `?page_size=1000000` seria um jeito barato de derrubar o
        // processo pela memoria.
        assert_eq!(body["pagination"]["page_size"], 100);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn shows_one_entry_with_the_user_agent() {
    request::<App, _, _>(|request, ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");
        seed_entries(&ctx.db).await;

        let list: Value = request
            .get("/api/audit-logs")
            .add_query_param("action", "connection.created")
            .authorization_bearer(&token)
            .await
            .json();
        let id = list["results"][0]["id"].as_i64().expect("id");

        let body: Value = request
            .get(&format!("/api/audit-logs/{id}"))
            .authorization_bearer(&token)
            .await
            .json();

        assert_eq!(body["userAgent"], "curl/8");
        assert_eq!(body["ipAddress"], "127.0.0.1");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_missing_entry_is_a_404_with_its_own_message() {
    request::<App, _, _>(|request, _ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");

        let response = request
            .get("/api/audit-logs/99999999")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);
        let body: Value = response.json();
        assert_eq!(body["error"], "not_found");
        assert_eq!(body["description"], "Log de auditoria não encontrado");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn stats_never_collide_with_the_id_route() {
    // `/stats` tem que casar antes de `/{id}`, senao o Axum tenta ler "stats"
    // como inteiro e a rota de estatisticas responde 400.
    request::<App, _, _>(|request, ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");
        seed_entries(&ctx.db).await;

        let response = request
            .get("/api/audit-logs/stats")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let data = response.json::<Value>();

        assert_eq!(data["total"], 2);
        assert_eq!(data["today"], 2);
        assert_eq!(data["lastWeek"], 2);
        assert_eq!(data["byStatus"]["success"], 1);
        assert_eq!(data["byStatus"]["failure"], 1);
        assert_eq!(data["byAction"].as_array().map(Vec::len), Some(2));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_unknown_action_leaves_the_derived_fields_null() {
    // O schema afrouxou os enums de proposito; um valor fora da lista precisa
    // sair com `actionDescription` nulo, e nao sem a chave.
    request::<App, _, _>(|request, ctx| async move {
        let admin = session::create_admin(&request, "admin@contract.test").await;
        let token = admin.token.expect("token");

        sea_orm::EntityTrait::insert(backend::models::_entities::audit_logs::ActiveModel {
            action: sea_orm::ActiveValue::Set("plugin.executed".to_string()),
            entity_type: sea_orm::ActiveValue::Set("settings".to_string()),
            description: sea_orm::ActiveValue::Set("Plugin executado".to_string()),
            status: sea_orm::ActiveValue::Set(AuditStatus::Success.as_str().to_string()),
            created_at: sea_orm::ActiveValue::Set(chrono::Utc::now().fixed_offset()),
            ..Default::default()
        })
        .exec(&ctx.db)
        .await
        .expect("insere a entrada desconhecida");

        let body: Value = request
            .get("/api/audit-logs")
            .authorization_bearer(&token)
            .await
            .json();

        let entry = &body["results"][0];
        assert_eq!(entry["action"], "plugin.executed");
        assert!(entry["actionDescription"].is_null());
        assert!(entry["actionIcon"].is_null());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn every_audit_route_needs_a_session() {
    request::<App, _, _>(|request, _ctx| async move {
        for path in [
            "/api/audit-logs",
            "/api/audit-logs/stats",
            "/api/audit-logs/1",
        ] {
            assert_eq!(request.get(path).await.status_code(), 401, "{path}");
        }
    })
    .await;
}
