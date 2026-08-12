//! Lote 2.3 do contrato — `/api/connections`.
//!
//! O que da' para afirmar sem um servidor MySQL/PostgreSQL de pe' esta' aqui:
//! CRUD, validacao, reconciliacao dos databases, autorizacao e o **caminho de
//! falha** dos drivers, que e' testavel apontando para uma porta fechada.
//!
//! Os casos que exigem servidor real ficam `#[ignore]`d no fim do arquivo e
//! leem o endereco do ambiente — mesma convencao dos testes de dados de
//! producao. Sao eles que a tarefa 6.10 usa contra o compose da 0.7.

use backend::app::App;
use loco_rs::testing::prelude::*;
use sea_orm::EntityTrait;
use serde_json::Value;
use serial_test::serial;

use super::session;

/// Corpo minimo aceito por `POST /api/connections`.
fn payload(name: &str) -> Value {
    serde_json::json!({
        "name": name,
        "type": "mysql",
        "host": "127.0.0.1",
        "port": 13306,
        "username": "tester",
        "password": "test_pw",
        "databases": ["app_fixture"],
    })
}

async fn admin_token(request: &axum_test::TestServer) -> String {
    session::create_admin(request, "admin@contract.test")
        .await
        .token
        .expect("token do admin")
}

async fn create(request: &axum_test::TestServer, token: &str, name: &str) -> Value {
    let response = request
        .post("/api/connections")
        .authorization_bearer(token)
        .json(&payload(name))
        .await;

    assert_eq!(response.status_code(), 201, "{}", response.text());
    response.json()
}

#[tokio::test]
#[serial]
async fn creates_a_connection_with_its_databases() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let body = create(&request, &token, "Golden Da Criacao").await;

        let data = &body;
        assert_eq!(data["name"], "Golden Da Criacao");
        assert_eq!(data["status"], "active");
        // ACHADO 3: no corpo da criacao o registro ainda esta' na memoria, e
        // `scheduleEnabled` sai como booleano de verdade...
        assert_eq!(data["scheduleEnabled"], Value::Bool(false));
        // ...enquanto os databases, que foram recarregados do banco, saem `1`.
        assert_eq!(data["databases"][0]["enabled"], 1);
        assert_eq!(data["databases"][0]["databaseName"], "app_fixture");
        // Os tres campos de teste nunca foram atribuidos: `JSON.stringify`
        // omite `undefined`, e o contrato registra a ausencia.
        assert!(data.get("lastError").is_none());
        assert!(data.get("lastTestedAt").is_none());
        assert!(data.get("lastBackupAt").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn never_returns_the_encrypted_password() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        create(&request, &token, "Sem Vazamento").await;

        let text = request
            .get("/api/connections")
            .authorization_bearer(&token)
            .await
            .text();

        assert!(!text.contains("password"), "vazou a chave");
        assert!(!text.contains("test_pw"), "vazou a senha em claro");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn stores_the_password_encrypted() {
    // The password never reaches the column in the clear, and what does reach it
    // announces the format it was written in.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        create(&request, &token, "Cifrada").await;

        let row = backend::models::connections::Entity::find()
            .one(&ctx.db)
            .await
            .expect("consulta")
            .expect("conexao gravada");

        assert_ne!(row.password_encrypted, "test_pw");
        assert!(
            backend::models::encryption::EncryptionService::is_encrypted(&row.password_encrypted),
            "formato esperado v1.nonce.dados, veio {}",
            row.password_encrypted
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn lists_ordered_by_name_with_the_pagination_envelope() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        create(&request, &token, "Zulu").await;
        create(&request, &token, "Alfa").await;

        let body: Value = request
            .get("/api/connections")
            .authorization_bearer(&token)
            .await
            .json();

        assert_eq!(body["pagination"]["total_items"], 2);
        assert_eq!(body["pagination"]["page_size"], 20);
        assert_eq!(body["results"][0]["name"], "Alfa");
        assert_eq!(body["results"][1]["name"], "Zulu");
        // Na listagem o registro veio do banco: `0`, e nao `false`.
        assert_eq!(body["results"][0]["scheduleEnabled"], 0);
        assert_eq!(body["results"][0]["backups"], serde_json::json!([]));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn filters_by_type_and_search() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        create(&request, &token, "Producao MySQL").await;
        create(&request, &token, "Homologacao").await;

        let by_search: Value = request
            .get("/api/connections")
            .add_query_param("search", "producao")
            .authorization_bearer(&token)
            .await
            .json();
        // A busca do Adonis e' insensivel a caixa, e cobre nome **ou** host.
        assert_eq!(by_search["pagination"]["total_items"], 1);
        assert_eq!(by_search["results"][0]["name"], "Producao MySQL");

        let by_host: Value = request
            .get("/api/connections")
            .add_query_param("search", "127.0.0")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(by_host["pagination"]["total_items"], 2);

        let by_type: Value = request
            .get("/api/connections")
            .add_query_param("type", "postgresql")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(by_type["pagination"]["total_items"], 0);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_invalid_type_is_refused_with_the_accepted_choices() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let mut body = payload("Tipo Invalido");
        body["type"] = Value::String("oracle".to_string());

        let response = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&body)
            .await;

        assert_eq!(response.status_code(), 400);
        let error = &response.json::<Value>()["errors"]["type"][0];
        assert_eq!(error["code"], "enum");
        // A interface remonta o select com esta lista.
        assert_eq!(
            error["params"]["choices"],
            serde_json::json!(["mysql", "mariadb", "postgresql"])
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn rejects_an_out_of_range_port_and_an_empty_database_list() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let mut invalid_port = payload("Porta Invalida");
        invalid_port["port"] = serde_json::json!(70000);
        let response = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&invalid_port)
            .await;
        assert_eq!(response.status_code(), 400);
        assert!(response.json::<Value>()["errors"]["port"].is_array());

        let mut no_databases = payload("Sem Databases");
        no_databases["databases"] = serde_json::json!([]);
        let response = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&no_databases)
            .await;
        assert_eq!(response.status_code(), 400);
        assert!(response.json::<Value>()["errors"]["databases"].is_array());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn shows_one_connection_with_all_its_databases() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Detalhe").await["id"]
            .as_i64()
            .expect("id");

        let response = request
            .get(&format!("/api/connections/{id}"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200);
        let data = &response.json::<Value>();
        assert_eq!(data["id"], id);
        assert_eq!(data["databases"][0]["databaseName"], "app_fixture");
        assert!(data["backups"].is_array());
        assert!(data.get("lastError").is_some());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_missing_connection_is_a_404_in_the_controller_family() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        for response in [
            request
                .get("/api/connections/99999999")
                .authorization_bearer(&token)
                .await,
            request
                .delete("/api/connections/99999999")
                .authorization_bearer(&token)
                .await,
        ] {
            assert_eq!(response.status_code(), 404);
            let body: Value = response.json();
            assert_eq!(body["error"], "not_found");
            assert_eq!(body["description"], "Conexão não encontrada");
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn updates_only_the_fields_that_were_sent() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Antes Do Update").await["id"]
            .as_i64()
            .expect("id");

        let response = request
            .put(&format!("/api/connections/{id}"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "name": "Depois Do Update", "port": 13307 }))
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let body: Value = response.json();

        let data = &body;
        assert_eq!(data["name"], "Depois Do Update");
        assert_eq!(data["port"], 13307);
        // Nao enviados: intactos. Um `update` que zerasse o resto apagaria as
        // credenciais a cada renomeacao.
        assert_eq!(data["host"], "127.0.0.1");
        assert_eq!(data["username"], "tester");
        assert_eq!(data["databases"][0]["databaseName"], "app_fixture");
        // Registro vindo do banco: `0`, e nao `false`.
        assert_eq!(data["scheduleEnabled"], 0);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn removing_a_database_disables_it_instead_of_deleting_it() {
    // A FK de `backups.connection_database_id` e' o motivo: apagar a linha
    // levaria junto o historico do banco removido.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Sync").await["id"]
            .as_i64()
            .expect("id");

        let body: Value = request
            .put(&format!("/api/connections/{id}"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "databases": ["outro_banco"] }))
            .await
            .json();

        // A resposta mostra so' os habilitados.
        assert_eq!(body["databases"].as_array().map(Vec::len), Some(1));
        assert_eq!(body["databases"][0]["databaseName"], "outro_banco");

        // A linha antiga continua no banco, desabilitada.
        let all = backend::models::connection_databases::Model::all_for(&ctx.db, id)
            .await
            .expect("consulta");
        assert_eq!(all.len(), 2);
        let disabled = all
            .iter()
            .find(|row| row.database_name == "app_fixture")
            .expect("a linha antiga sumiu");
        assert_eq!(disabled.enabled, Some(false));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn re_adding_a_database_reactivates_the_existing_row() {
    // O indice unico `idx_conn_db_unique` impede a segunda linha; sem a
    // reativacao, o `PUT` falharia com erro de constraint.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Reativa").await["id"]
            .as_i64()
            .expect("id");

        for databases in [
            serde_json::json!(["outro"]),
            serde_json::json!(["app_fixture"]),
        ] {
            let response = request
                .put(&format!("/api/connections/{id}"))
                .authorization_bearer(&token)
                .json(&serde_json::json!({ "databases": databases }))
                .await;
            assert_eq!(response.status_code(), 200, "{}", response.text());
        }

        let all = backend::models::connection_databases::Model::all_for(&ctx.db, id)
            .await
            .expect("consulta");
        assert_eq!(all.len(), 2, "criou uma linha duplicada");
        let reactivated = all
            .iter()
            .find(|row| row.database_name == "app_fixture")
            .expect("linha original");
        assert_eq!(reactivated.enabled, Some(true));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn deletes_the_connection_and_its_databases() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Para Remover").await["id"]
            .as_i64()
            .expect("id");

        let response = request
            .delete(&format!("/api/connections/{id}"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200);
        assert_eq!(
            response.json::<Value>()["message"],
            "Conexão removida com sucesso"
        );

        // `CASCADE` na FK de `connection_databases`.
        let remaining = backend::models::connection_databases::Model::all_for(&ctx.db, id)
            .await
            .expect("consulta");
        assert!(remaining.is_empty());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_crud_writes_the_audit_trail() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Auditada").await["id"]
            .as_i64()
            .expect("id");

        request
            .put(&format!("/api/connections/{id}"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "name": "Auditada E Renomeada" }))
            .await;
        request
            .delete(&format!("/api/connections/{id}"))
            .authorization_bearer(&token)
            .await;

        let body: Value = request
            .get("/api/audit-logs")
            .authorization_bearer(&token)
            .await
            .json();

        let actions: Vec<&str> = body["results"]
            .as_array()
            .expect("lista")
            .iter()
            .filter_map(|entry| entry["action"].as_str())
            .collect();

        assert!(actions.contains(&"connection.created"));
        assert!(actions.contains(&"connection.updated"));
        assert!(actions.contains(&"connection.deleted"));

        // A senha **nunca** entra no diff em texto claro.
        let rendered = body.to_string();
        assert!(!rendered.contains("test_pw"));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn testing_an_unreachable_host_records_the_failure() {
    // Porta 1: nao ha' servico. E' o caminho de falha do driver, e o unico
    // testavel sem um servidor de verdade.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let mut body = payload("Inalcancavel");
        body["port"] = serde_json::json!(1);

        let created = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&body)
            .await;
        let id = created.json::<Value>()["id"].as_i64().expect("id");

        let response = request
            .post(&format!("/api/connections/{id}/test"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        let failure: Value = response.json();
        assert_eq!(failure["error"], "unprocessable_entity");
        assert!(
            failure["description"]
                .as_str()
                .is_some_and(|text| text.starts_with("Falha ao conectar ao banco de dados")),
            "descricao: {}",
            failure["description"]
        );
        // O motivo do SGBD chega ao usuario; sem ele o botao "Testar" nao
        // ajuda a diagnosticar nada.
        assert!(failure["error"].is_string());

        // E o estado fica gravado: e' o que a listagem exibe.
        let row = backend::models::connections::Model::find_one(&ctx.db, id)
            .await
            .expect("consulta")
            .expect("conexao");
        assert_eq!(row.status.as_deref(), Some("error"));
        assert!(row.last_error.is_some());
        assert!(row.last_tested_at.is_some());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn discovering_against_a_closed_port_is_a_422() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/connections/discover-databases")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "type": "mysql",
                "host": "127.0.0.1",
                "port": 1,
                "username": "tester",
                "password": "test_pw",
            }))
            .await;

        assert_eq!(response.status_code(), 422);
        assert!(
            response.json::<Value>()["description"]
                .as_str()
                .is_some_and(
                    |text| text.starts_with("Falha ao conectar ao servidor de banco de dados")
                ),
            "descricao inesperada"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn discovery_validates_before_dialing() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/connections/discover-databases")
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "host": "127.0.0.1" }))
            .await;

        assert_eq!(response.status_code(), 400);
        let body: Value = response.json();
        for field in ["type", "port", "username"] {
            assert!(body["errors"][field].is_array(), "faltou o erro de {field}");
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_hostile_database_name_never_reaches_the_engine() {
    // A validacao e' a primeira das duas barreiras contra injecao em DDL; a
    // segunda esta' em `database_driver::quote_identifier`. A recusa aqui prova
    // que a requisicao nem chega a abrir conexao.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let id = create(&request, &token, "Alvo").await["id"]
            .as_i64()
            .expect("id");

        for hostile in ["app`; DROP DATABASE x; --", "1app", "app fixture", ""] {
            let response = request
                .post(&format!("/api/connections/{id}/create-database"))
                .authorization_bearer(&token)
                .json(&serde_json::json!({ "databaseName": hostile }))
                .await;

            assert_eq!(response.status_code(), 400, "aceitou {hostile:?}");
            assert!(response.json::<Value>()["errors"]["databaseName"].is_array());
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn docker_hosts_answers_200_with_or_without_docker() {
    // A tela continua abrindo sem Docker, mas uma máquina com Engine disponível
    // passa a receber as sugestões reais da Fase 9.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/connections/docker-hosts")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200);
        let data = &response.json::<Value>();
        assert!(data["dockerAvailable"].is_boolean());
        assert!(data["hosts"].is_array());
        if data["dockerAvailable"] == true {
            assert!(data["unavailableReason"].is_null());
        } else {
            assert!(data["unavailableReason"].is_string());
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn every_connection_route_needs_a_session() {
    request::<App, _, _>(|request, _ctx| async move {
        for response in [
            request.get("/api/connections").await,
            request.post("/api/connections").json(&payload("x")).await,
            request.get("/api/connections/1").await,
            request.put("/api/connections/1").json(&payload("x")).await,
            request.delete("/api/connections/1").await,
            request.post("/api/connections/1/test").await,
            request.get("/api/connections/docker-hosts").await,
        ] {
            assert_eq!(response.status_code(), 401);
        }
    })
    .await;
}

// ============================================================================
// Contra servidores de verdade — `#[ignore]`d por padrao
// ============================================================================
//
// Rodam com o compose da tarefa 0.7 de pe':
//
// ```sh
// CONTRACT_MYSQL_PORT=13306 cargo test --test mod -- --ignored
// ```
//
// Ficam ignorados porque um `cargo test` numa maquina sem os servicos ficaria
// vermelho por motivo que nao e' defeito do codigo — e uma suite que falha por
// ambiente deixa de ser lida.

/// Porta do MySQL de teste, ou `None` quando o compose nao esta' de pe'.
fn mysql_port() -> Option<u16> {
    std::env::var("CONTRACT_MYSQL_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
}

#[tokio::test]
#[serial]
#[ignore = "exige o MySQL do compose (CONTRACT_MYSQL_PORT)"]
async fn tests_a_real_mysql_server() {
    let Some(port) = mysql_port() else {
        panic!("defina CONTRACT_MYSQL_PORT para rodar este teste");
    };

    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let mut body = payload("MySQL Real");
        body["port"] = serde_json::json!(port);

        let created = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&body)
            .await;
        let id = created.json::<Value>()["id"].as_i64().expect("id");

        let response = request
            .post(&format!("/api/connections/{id}/test"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let data = &response.json::<Value>();
        assert!(data["latencyMs"].as_i64().is_some_and(|value| value >= 0));
        assert!(
            data["version"].is_string(),
            "o servidor nao reportou versao"
        );

        let row = backend::models::connections::Model::find_one(&ctx.db, id)
            .await
            .expect("consulta")
            .expect("conexao");
        assert_eq!(row.status.as_deref(), Some("active"));
        assert_eq!(row.last_error, None);
    })
    .await;
}

#[tokio::test]
#[serial]
#[ignore = "exige o MySQL do compose (CONTRACT_MYSQL_PORT)"]
async fn discovers_databases_on_a_real_server() {
    let Some(port) = mysql_port() else {
        panic!("defina CONTRACT_MYSQL_PORT para rodar este teste");
    };

    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/connections/discover-databases")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "type": "mysql",
                "host": "127.0.0.1",
                "port": port,
                "username": "tester",
                "password": "test_pw",
            }))
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        let databases = response.json::<Value>()["databases"]
            .as_array()
            .expect("lista de databases")
            .clone();

        let names: Vec<&str> = databases.iter().filter_map(Value::as_str).collect();
        assert!(names.contains(&"app_fixture"), "faltou o banco de fixture");
        // Os bancos de sistema ficam de fora, como no Adonis.
        for system in ["information_schema", "mysql", "performance_schema", "sys"] {
            assert!(
                !names.contains(&system),
                "listou o banco de sistema {system}"
            );
        }
    })
    .await;
}
