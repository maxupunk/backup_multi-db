//! Lote 2.4 do contrato — `/api/backups` (tarefa 7.12).
//!
//! O caminho feliz de um backup exige `mysqldump` e um servidor MySQL de pe'.
//! O que **nao** exige nenhum dos dois esta' aqui: listagem, filtros, 404, 401,
//! a regra de `protected`, as guardas do restore e a barreira de extensao do
//! import — que e' a parte do recurso onde um erro custa mais caro.
//!
//! Os registros de backup entram direto pela entidade, e nao por
//! `POST /api/connections/:id/backup`: criar um backup de verdade dependeria do
//! binario de dump no PATH, e a suite ficaria verde por ter sido pulada em vez
//! de por ter medido alguma coisa.

use backend::app::App;
use backend::models::_entities::backups;
use loco_rs::testing::prelude::*;
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, EntityTrait};
use serde_json::Value;
use serial_test::serial;

use super::session;

/// Cria a conexao que os backups do teste apontam.
async fn create_connection(request: &axum_test::TestServer, token: &str) -> i64 {
    let response = request
        .post("/api/connections")
        .authorization_bearer(token)
        .json(&serde_json::json!({
            "name": "Conexao Dos Backups",
            "type": "mysql",
            "host": "127.0.0.1",
            "port": 13306,
            "username": "tester",
            "password": "test_pw",
            "databases": ["app_fixture"],
        }))
        .await;

    assert_eq!(response.status_code(), 201, "{}", response.text());
    response.json::<Value>()["data"]["id"]
        .as_i64()
        .expect("id da conexao")
}

/// Insere um registro de backup pronto.
async fn insert_backup(
    ctx: &loco_rs::app::AppContext,
    connection_id: Option<i64>,
    status: &str,
    protected: bool,
) -> backups::Model {
    let now = chrono::Utc::now().naive_utc();

    backups::ActiveModel {
        connection_id: Set(connection_id),
        connection_database_id: Set(None),
        database_name: Set("app_fixture".to_string()),
        storage_destination_id: Set(None),
        status: Set(status.to_string()),
        file_path: Set(Some("1/app_fixture_20260809_120000.sql.gz".to_string())),
        file_name: Set(Some("app_fixture_20260809_120000.sql.gz".to_string())),
        file_size: Set(Some(2048)),
        checksum: Set(Some("deadbeef".to_string())),
        compressed: Set(Some(true)),
        retention_type: Set("hourly".to_string()),
        protected: Set(Some(protected)),
        trigger: Set("manual".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await
    .expect("insere o backup de teste")
}

async fn admin_token(request: &axum_test::TestServer) -> String {
    session::create_admin(request, "admin@backups.test")
        .await
        .token
        .expect("token do admin")
}

// ============================================================================
// GET /api/backups
// ============================================================================

#[tokio::test]
#[serial]
async fn lists_backups_with_the_pagination_envelope() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        insert_backup(&ctx, Some(connection_id), "completed", false).await;

        let response = request
            .get("/api/backups")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["success"], true);
        // O golden `backups/index` fixa `data.data` e `data.meta`.
        assert!(body["data"]["data"].is_array());
        assert_eq!(body["data"]["meta"]["currentPage"], 1);
        assert_eq!(body["data"]["meta"]["total"], 1);

        let item = &body["data"]["data"][0];
        assert_eq!(item["databaseName"], "app_fixture");
        // ACHADO 3: o registro veio do SQLite, entao os booleanos sao `0`/`1`.
        assert_eq!(item["compressed"], 1);
        assert_eq!(item["protected"], 0);
        // O `preload('connection')` da listagem aninha o objeto.
        assert_eq!(item["connection"]["name"], "Conexao Dos Backups");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn filters_by_status_connection_and_database() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        insert_backup(&ctx, Some(connection_id), "completed", false).await;
        insert_backup(&ctx, Some(connection_id), "failed", false).await;

        let count = |query: Vec<(&'static str, String)>| {
            let request = &request;
            let token = token.clone();
            async move {
                let mut call = request.get("/api/backups").authorization_bearer(&token);
                for (key, value) in query {
                    call = call.add_query_param(key, value);
                }
                let response = call.await;
                assert_eq!(response.status_code(), 200, "{}", response.text());
                response.json::<Value>()["data"]["meta"]["total"]
                    .as_i64()
                    .expect("total")
            }
        };

        assert_eq!(count(vec![("status", "completed".into())]).await, 1);
        assert_eq!(
            count(vec![("connectionId", connection_id.to_string())]).await,
            2
        );
        assert_eq!(count(vec![("databaseName", "app_fixture".into())]).await, 2);
        assert_eq!(count(vec![("databaseName", "outro".into())]).await, 0);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_non_numeric_connection_filter_returns_nothing_instead_of_everything() {
    // Ignorar o filtro devolveria a lista inteira — o oposto do que foi pedido.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        insert_backup(&ctx, Some(connection_id), "completed", false).await;

        let response = request
            .get("/api/backups")
            .add_query_param("connectionId", "abc")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        assert_eq!(response.json::<Value>()["data"]["meta"]["total"], 0);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn caps_the_page_size() {
    // `?limit=1000000` seria um jeito barato de derrubar o processo.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/backups")
            .add_query_param("limit", "1000000")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.json::<Value>()["data"]["meta"]["perPage"], 100);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_listing_requires_authentication() {
    request::<App, _, _>(|request, _ctx| async move {
        assert_eq!(request.get("/api/backups").await.status_code(), 401);
    })
    .await;
}

// ============================================================================
// GET /api/connections/:id/backups
// ============================================================================

#[tokio::test]
#[serial]
async fn lists_the_backups_of_one_connection_without_nesting_it() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        insert_backup(&ctx, Some(connection_id), "completed", false).await;

        let response = request
            .get(&format!("/api/connections/{connection_id}/backups"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["data"]["meta"]["total"], 1);
        // Sem `preload('connection')`: quem chama ja' esta' na tela da conexao.
        assert!(body["data"]["data"][0].get("connection").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn listing_the_backups_of_a_missing_connection_is_a_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/connections/99999999/backups")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);
        assert_eq!(
            response.json::<Value>()["message"],
            "Conexão não encontrada"
        );
    })
    .await;
}

// ============================================================================
// GET /api/backups/:id
// ============================================================================

#[tokio::test]
#[serial]
async fn shows_a_backup_with_its_connection() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        let backup = insert_backup(&ctx, Some(connection_id), "completed", false).await;

        let response = request
            .get(&format!("/api/backups/{}", backup.id))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["data"]["id"], backup.id);
        assert_eq!(body["data"]["status"], "completed");
        assert_eq!(body["data"]["connection"]["type"], "mysql");
        // A conexao aninhada nunca carrega a senha.
        assert!(!response.text().contains("passwordEncrypted"));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_orphaned_backup_reports_a_null_connection() {
    // O `SET NULL` da FK deixa backups sem conexao; a chave continua presente.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let backup = insert_backup(&ctx, None, "completed", false).await;

        let response = request
            .get(&format!("/api/backups/{}", backup.id))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        assert!(response.json::<Value>()["data"]["connection"].is_null());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn shows_a_404_for_a_missing_backup() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/backups/99999999")
            .authorization_bearer(&token)
            .await;

        // O golden `backups/show-not-found` fixa a familia dos controllers.
        assert_eq!(response.status_code(), 404);
        let body: Value = response.json();
        assert_eq!(body["success"], false);
        assert_eq!(body["message"], "Backup não encontrado");
        assert!(body.get("errors").is_none());
    })
    .await;
}

// ============================================================================
// GET /api/backups/:id/download
// ============================================================================

#[tokio::test]
#[serial]
async fn downloading_a_missing_backup_is_a_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/backups/99999999/download")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn downloading_a_backup_whose_file_vanished_is_a_404_not_a_500() {
    // O registro existe, o arquivo nao. Um 500 aqui faria o operador procurar
    // um defeito no servidor em vez do arquivo que sumiu do volume.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let backup = insert_backup(&ctx, None, "completed", false).await;

        let response = request
            .get(&format!("/api/backups/{}/download", backup.id))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404, "{}", response.text());
        assert_eq!(
            response.json::<Value>()["message"],
            "Arquivo de backup não encontrado no servidor"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn downloading_requires_authentication() {
    request::<App, _, _>(|request, _ctx| async move {
        assert_eq!(
            request.get("/api/backups/1/download").await.status_code(),
            401
        );
    })
    .await;
}

// ============================================================================
// DELETE /api/backups/:id
// ============================================================================

#[tokio::test]
#[serial]
async fn deletes_a_finished_backup() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let backup = insert_backup(&ctx, None, "completed", false).await;

        let response = request
            .delete(&format!("/api/backups/{}", backup.id))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());
        assert_eq!(
            response.json::<Value>()["message"],
            "Backup removido com sucesso"
        );

        // Some da API **e** do banco.
        assert_eq!(
            request
                .get(&format!("/api/backups/{}", backup.id))
                .authorization_bearer(&token)
                .await
                .status_code(),
            404
        );
        assert!(backups::Entity::find_by_id(backup.id)
            .one(&ctx.db)
            .await
            .expect("consulta")
            .is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_to_delete_a_protected_backup() {
    // Sem esta regra, a protecao contra a poda seria contornavel pelo botao de
    // apagar.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let backup = insert_backup(&ctx, None, "completed", true).await;

        let response = request
            .delete(&format!("/api/backups/{}", backup.id))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        assert_eq!(
            response.json::<Value>()["message"],
            "Este backup não pode ser deletado (protegido ou em execução)"
        );

        // E continua la'.
        assert!(backups::Entity::find_by_id(backup.id)
            .one(&ctx.db)
            .await
            .expect("consulta")
            .is_some());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_to_delete_a_running_backup() {
    // O processo de dump ainda tem o arquivo aberto.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;

        for status in ["running", "pending"] {
            let backup = insert_backup(&ctx, None, status, false).await;

            let response = request
                .delete(&format!("/api/backups/{}", backup.id))
                .authorization_bearer(&token)
                .await;

            assert_eq!(response.status_code(), 422, "status {status}");
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn deleting_a_missing_backup_is_a_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        assert_eq!(
            request
                .delete("/api/backups/99999999")
                .authorization_bearer(&token)
                .await
                .status_code(),
            404
        );
    })
    .await;
}

// ============================================================================
// POST /api/backups/:id/restore
// ============================================================================

#[tokio::test]
#[serial]
async fn restoring_a_missing_backup_is_a_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/backups/99999999/restore")
            .authorization_bearer(&token)
            .json(&serde_json::json!({}))
            .await;

        assert_eq!(response.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_to_restore_a_backup_that_did_not_finish() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        let backup = insert_backup(&ctx, Some(connection_id), "failed", false).await;

        let response = request
            .post(&format!("/api/backups/{}/restore", backup.id))
            .authorization_bearer(&token)
            .json(&serde_json::json!({}))
            .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        assert_eq!(
            response.json::<Value>()["message"],
            "Apenas backups concluídos podem ser restaurados"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_a_target_connection_that_does_not_exist() {
    // Restaurar "para lugar nenhum" tem que falhar alto — nunca 200.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;
        let backup = insert_backup(&ctx, Some(connection_id), "completed", false).await;

        let response = request
            .post(&format!("/api/backups/{}/restore", backup.id))
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "targetConnectionId": 99999999 }))
            .await;

        assert!(
            [404, 422].contains(&response.status_code().as_u16()),
            "status inesperado: {} — {}",
            response.status_code(),
            response.text()
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_imported_backup_without_a_connection_demands_a_target() {
    // O backup importado nao tem conexao de origem; restaurar sem escolher o
    // destino nao tem para onde ir.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;
        let backup = insert_backup(&ctx, None, "completed", false).await;

        let response = request
            .post(&format!("/api/backups/{}/restore", backup.id))
            .authorization_bearer(&token)
            .json(&serde_json::json!({}))
            .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        assert!(response.text().contains("Selecione uma conexão de destino"));
    })
    .await;
}

#[tokio::test]
#[serial]
async fn restoring_requires_authentication() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/api/backups/1/restore")
            .json(&serde_json::json!({}))
            .await;

        assert_eq!(response.status_code(), 401);
    })
    .await;
}

// ============================================================================
// POST /api/backups/import
// ============================================================================

/// Monta um corpo multipart a mao — e' o unico endpoint da API que recebe
/// arquivo, e nao vale um crate so' para isto.
fn multipart_body(boundary: &str, file_name: &str, content: &str) -> String {
    [
        format!("--{boundary}"),
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\""),
        "Content-Type: application/octet-stream".to_string(),
        String::new(),
        content.to_string(),
        format!("--{boundary}--"),
        String::new(),
    ]
    .join("\r\n")
}

const BOUNDARY: &str = "----contract-boundary-0000";

/// Envia um corpo multipart cru.
///
/// O `.content_type()` vem **depois** do `.text()`: o helper do `axum-test`
/// carimba `text/plain` junto com o corpo, e na ordem inversa o servidor
/// receberia um corpo multipart anunciado como texto — que e' exatamente o caso
/// "nenhum arquivo enviado", e o teste passaria pelo motivo errado.
async fn post_multipart(
    request: &axum_test::TestServer,
    token: &str,
    body: String,
) -> axum_test::TestResponse {
    request
        .post("/api/backups/import")
        .authorization_bearer(token)
        .text(body)
        .content_type(&format!("multipart/form-data; boundary={BOUNDARY}"))
        .await
}

#[tokio::test]
#[serial]
async fn importing_without_a_file_is_a_422() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/backups/import")
            .authorization_bearer(&token)
            .json(&serde_json::json!({}))
            .await;

        // O golden `backups/import-no-file` fixa o 422.
        assert_eq!(response.status_code(), 422, "{}", response.text());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_an_executable_extension() {
    // Aceitar `.exe` significaria gravar um executavel na area de backups.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = post_multipart(
            &request,
            &token,
            multipart_body(BOUNDARY, "malicioso.exe", "MZ conteudo que nao e um dump"),
        )
        .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        // Recusado pela **extensao**, e nao por o multipart nao ter sido lido —
        // um 422 pelo motivo errado esconderia a barreira que importa aqui.
        assert!(
            response.text().contains("Formato de arquivo não suportado"),
            "recusado pelo motivo errado: {}",
            response.text()
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn imports_a_sql_file_and_registers_it() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;

        let response = post_multipart(
            &request,
            &token,
            multipart_body(BOUNDARY, "vendas.sql", "CREATE TABLE clientes (id int);\n"),
        )
        .await;

        assert_eq!(response.status_code(), 201, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["success"], true);
        assert_eq!(body["data"]["format"], "sql");
        assert_eq!(body["data"]["backup"]["status"], "completed");
        // Um arquivo trazido de fora nao pode ser podado pela retencao horaria.
        assert_eq!(body["data"]["backup"]["retentionType"], "daily");
        assert_eq!(body["data"]["backup"]["metadata"]["isImported"], true);
        // Sem `verifyIntegrity` a chave existe e vale `null`.
        assert!(body["data"]["integrity"].is_null());

        // O checksum e' de conteudo real, nao um placeholder.
        let checksum = body["data"]["checksum"].as_str().expect("checksum");
        assert_eq!(checksum.len(), 64);

        assert_eq!(
            backups::Entity::find()
                .all(&ctx.db)
                .await
                .expect("consulta")
                .len(),
            1
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_failed_integrity_check_refuses_the_import() {
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;

        let body = [
            format!("--{BOUNDARY}"),
            "Content-Disposition: form-data; name=\"verifyIntegrity\"".to_string(),
            String::new(),
            "true".to_string(),
            format!("--{BOUNDARY}"),
            "Content-Disposition: form-data; name=\"file\"; filename=\"vendas.sql\"".to_string(),
            String::new(),
            "isto nao tem instrucao SQL nenhuma".to_string(),
            format!("--{BOUNDARY}--"),
            String::new(),
        ]
        .join("\r\n");

        let response = post_multipart(&request, &token, body).await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        assert!(
            response.text().contains("integridade"),
            "recusado pelo motivo errado: {}",
            response.text()
        );

        // Nada foi registrado: um import recusado nao pode deixar rastro.
        assert!(backups::Entity::find()
            .all(&ctx.db)
            .await
            .expect("consulta")
            .is_empty());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn importing_requires_authentication() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request
            .post("/api/backups/import")
            .json(&serde_json::json!({}))
            .await;

        assert_eq!(response.status_code(), 401);
    })
    .await;
}

// ============================================================================
// POST /api/connections/:id/backup — a pendencia da Fase 6
// ============================================================================

#[tokio::test]
#[serial]
async fn refuses_to_back_up_a_connection_in_error() {
    // O golden `connections/backup-connection-in-error` fixa este 422.
    //
    // O `status` nao e' um campo atualizavel — nem aqui, nem no Adonis. A unica
    // forma de uma conexao chegar a `error` e' falhando um teste, que e' o
    // caminho percorrido abaixo com uma porta comprovadamente fechada.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "Conexao Morta",
                "type": "mysql",
                "host": "127.0.0.1",
                // Porta 1: nenhum servico escuta ali, em nenhuma maquina de CI.
                "port": 1,
                "username": "tester",
                "password": "test_pw",
                "databases": ["app_fixture"],
            }))
            .await;
        assert_eq!(created.status_code(), 201, "{}", created.text());
        let connection_id = created.json::<Value>()["data"]["id"]
            .as_i64()
            .expect("id da conexao");

        let tested = request
            .post(&format!("/api/connections/{connection_id}/test"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(tested.status_code(), 422, "{}", tested.text());

        let response = request
            .post(&format!("/api/connections/{connection_id}/backup"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());
        assert_eq!(
            response.json::<Value>()["message"],
            "Não é possível fazer backup de uma conexão com erro. Teste a conexão primeiro."
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_to_back_up_a_connection_with_no_enabled_database() {
    // Sem esta guarda o Adonis criaria um backup de `N/A` marcado como falho,
    // poluindo a listagem sem informar nada.
    //
    // Os databases sao desabilitados direto na entidade porque a API nao aceita
    // lista vazia (`minLength: 1` no validator) — a conexao chega a este estado
    // por outro caminho, e a guarda precisa valer de qualquer forma.
    request::<App, _, _>(|request, ctx| async move {
        use backend::models::_entities::connection_databases;
        use sea_orm::{ColumnTrait, QueryFilter};

        let token = admin_token(&request).await;
        let connection_id = create_connection(&request, &token).await;

        connection_databases::Entity::delete_many()
            .filter(connection_databases::Column::ConnectionId.eq(connection_id))
            .exec(&ctx.db)
            .await
            .expect("remove os databases da conexao");

        let backup = request
            .post(&format!("/api/connections/{connection_id}/backup"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(backup.status_code(), 422, "{}", backup.text());
        assert_eq!(
            backup.json::<Value>()["message"],
            "Nenhum database habilitado para backup nesta conexão."
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn backing_up_a_missing_connection_is_a_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/connections/99999999/backup")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn backing_up_requires_authentication() {
    request::<App, _, _>(|request, _ctx| async move {
        assert_eq!(
            request
                .post("/api/connections/1/backup")
                .await
                .status_code(),
            401
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_backup_route_advertises_its_own_rate_limit() {
    // O golden `connections/backup-connection-in-error` gravou
    // `x-ratelimit-limit: 60` — o limitador `backup`, e nao o global de 600.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/connections/99999999/backup")
            .authorization_bearer(&token)
            .await;

        assert_eq!(
            response
                .headers()
                .get("x-ratelimit-limit")
                .and_then(|value| value.to_str().ok()),
            Some("60")
        );
    })
    .await;
}
