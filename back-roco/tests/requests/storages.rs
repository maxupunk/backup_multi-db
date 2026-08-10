//! Lote 2.5 do contrato — `/api/storages` e `/api/storage-destinations`.
//!
//! Os dois recursos leem a **mesma tabela** e respondem shapes diferentes; boa
//! parte destes testes existe para fixar essa diferença, que é fácil de perder
//! numa refatoração.
//!
//! O que exige um bucket ou um servidor SFTP de pé fica de fora: `test` e
//! `browse` são exercitados contra um destino **local**, que é real e não
//! precisa de rede. Os casos com MinIO/SFTP entram na tarefa 8.16, contra o
//! compose da 0.7.

use back_roco::app::App;
use loco_rs::testing::prelude::*;
use sea_orm::EntityTrait;
use serde_json::Value;
use serial_test::serial;

use super::session;

/// Config completa de um MinIO — o payload que o golden `storages/store` usa.
fn minio_config() -> Value {
    serde_json::json!({
        "bucket": "backups-primary",
        "accessKeyId": "contract-key",
        "secretAccessKey": "contract-secret",
        "endpoint": "http://127.0.0.1:19000",
        "forcePathStyle": true,
    })
}

async fn admin_token(request: &axum_test::TestServer) -> String {
    session::create_admin(request, "admin@contract.test")
        .await
        .token
        .expect("token do admin")
}

async fn create_storage(request: &axum_test::TestServer, token: &str, body: &Value) -> Value {
    let response = request
        .post("/api/storages")
        .authorization_bearer(token)
        .json(body)
        .await;

    assert_eq!(response.status_code(), 201, "{}", response.text());
    response.json()
}

/// Um destino local apontando para um diretório real, que é o que permite
/// exercitar `test` e `browse` sem rede.
async fn create_local(
    request: &axum_test::TestServer,
    token: &str,
    name: &str,
    base: &std::path::Path,
) -> Value {
    create_storage(
        request,
        token,
        &serde_json::json!({
            "name": name,
            "provider": "local",
            "config": { "basePath": base.to_string_lossy() },
        }),
    )
    .await
}

// ============================== /api/storages ==============================

#[tokio::test]
#[serial]
async fn creates_a_storage_and_masks_the_secret() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let body = create_storage(
            &request,
            &token,
            &serde_json::json!({
                "name": "Storage Criado",
                "provider": "minio",
                "config": minio_config(),
            }),
        )
        .await;

        assert_eq!(body["message"], "Armazenamento criado com sucesso");

        let data = &body["data"];
        assert_eq!(data["name"], "Storage Criado");
        // Tres providers colapsam em `s3`; o `provider` e' que distingue.
        assert_eq!(data["type"], "s3");
        assert_eq!(data["provider"], "minio");
        assert_eq!(data["providerLabel"], "MinIO");
        assert_eq!(data["status"], "active");
        assert_eq!(data["isDefault"], false);

        let config = &data["config"];
        assert_eq!(config["secretAccessKey"], "***");
        assert_eq!(config["accessKeyId"], "contract-key");
        assert_eq!(config["region"], "us-east-1", "regiao nao foi resolvida");
        assert_eq!(config["type"], "s3");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_raw_secret_never_appears_in_any_response() {
    // A rede de seguranca do recurso: o segredo entra por `POST`, e nao pode
    // sair por nenhuma das tres rotas que devolvem `config`.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = create_storage(
            &request,
            &token,
            &serde_json::json!({
                "name": "Segredo",
                "provider": "minio",
                "config": minio_config(),
            }),
        )
        .await;
        let id = created["data"]["id"].as_i64().expect("id");

        let show = request
            .get(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .await;
        let list = request
            .get("/api/storages")
            .authorization_bearer(&token)
            .await;

        for text in [created.to_string(), show.text(), list.text()] {
            assert!(
                !text.contains("contract-secret"),
                "o segredo vazou em {text}"
            );
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_unknown_provider_is_a_union_group_error() {
    // Golden `storages/store-invalid-provider`.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/storages")
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "name": "X", "provider": "dropbox" }))
            .await;

        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        assert_eq!(body["errors"][0]["field"], "");
        assert_eq!(body["errors"][0]["rule"], "unionGroup");
        assert_eq!(
            body["errors"][0]["message"],
            "Invalid value provided for data field"
        );
        // Familia do framework: sem `success` e sem `message` no topo.
        assert!(body.get("success").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn minio_without_an_endpoint_is_rejected() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let mut config = minio_config();
        config["endpoint"] = Value::String(String::new());

        let response = request
            .post("/api/storages")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "MinIO sem endpoint",
                "provider": "minio",
                "config": config,
            }))
            .await;

        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        let fields: Vec<&str> = body["errors"]
            .as_array()
            .expect("lista de erros")
            .iter()
            .filter_map(|error| error["field"].as_str())
            .collect();

        // Sem endpoint o SDK apontaria para a AWS em vez do MinIO local.
        assert!(fields.contains(&"config.endpoint"), "{fields:?}");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn lists_storages_in_a_paginated_envelope() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        for name in ["Zulu", "Alfa"] {
            create_storage(
                &request,
                &token,
                &serde_json::json!({ "name": name, "provider": "local" }),
            )
            .await;
        }

        let response = request
            .get("/api/storages")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200);

        let body: Value = response.json();
        let items = body["data"]["data"].as_array().expect("lista");

        // Ordenado por nome, como o `orderBy('name', 'asc')`.
        assert_eq!(items[0]["name"], "Alfa");
        assert_eq!(items[1]["name"], "Zulu");
        assert_eq!(body["data"]["meta"]["total"], 2);
        assert_eq!(body["data"]["meta"]["perPage"], 20);
        // A listagem nao carrega config: ela nem decifra a credencial.
        assert!(items[0].get("config").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn filters_by_provider_and_by_name() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Local Um", "provider": "local" }),
        )
        .await;
        create_storage(
            &request,
            &token,
            &serde_json::json!({
                "name": "Bucket Um", "provider": "minio", "config": minio_config(),
            }),
        )
        .await;

        let by_provider: Value = request
            .get("/api/storages?provider=minio")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(by_provider["data"]["meta"]["total"], 1);
        assert_eq!(by_provider["data"]["data"][0]["name"], "Bucket Um");

        let by_search: Value = request
            .get("/api/storages?search=Local")
            .authorization_bearer(&token)
            .await
            .json();
        assert_eq!(by_search["data"]["meta"]["total"], 1);
        assert_eq!(by_search["data"]["data"][0]["name"], "Local Um");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn an_unknown_storage_is_a_controller_shaped_404() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/storages/9999")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);

        let body: Value = response.json();
        assert_eq!(body["success"], false);
        assert_eq!(body["message"], "Armazenamento não encontrado");
        assert!(body.get("errors").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn updating_without_the_secret_keeps_the_stored_one() {
    // O caso que a fusao de segredos existe para cobrir: a interface limpa o
    // campo mascarado, e um renome nao pode apagar a credencial.
    request::<App, _, _>(|request, ctx| async move {
        let token = admin_token(&request).await;

        let created = create_storage(
            &request,
            &token,
            &serde_json::json!({
                "name": "Antes", "provider": "minio", "config": minio_config(),
            }),
        )
        .await;
        let id = created["data"]["id"].as_i64().expect("id");

        let mut config = minio_config();
        config["secretAccessKey"] = Value::String(String::new());

        let response = request
            .put(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "Storage Renomeado",
                "provider": "minio",
                "config": config,
            }))
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["message"], "Armazenamento atualizado com sucesso");
        assert_eq!(body["data"]["name"], "Storage Renomeado");
        // A resposta continua mascarando — e o valor gravado continua sendo o
        // original, que so' o banco pode confirmar.
        assert_eq!(body["data"]["config"]["secretAccessKey"], "***");

        let stored = back_roco::models::_entities::storage_destinations::Entity::find_by_id(id)
            .one(&ctx.db)
            .await
            .expect("consulta")
            .expect("destino");

        let settings =
            back_roco::initializers::settings::Settings::from_json(ctx.config.settings.as_ref())
                .expect("settings");
        let encryption =
            back_roco::models::backup_runner::encryption_service(&settings).expect("cripto");

        let config = stored.decrypted_config(&encryption).expect("config");
        assert_eq!(config["secretAccessKey"], "contract-secret");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_config_without_a_provider_is_rejected_on_update() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Sem provider", "provider": "local" }),
        )
        .await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .put(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "config": { "basePath": "/tmp" } }))
            .await;

        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        assert_eq!(body["errors"][0]["rule"], "storage.provider_required");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn only_one_storage_stays_the_default() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let first = create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Primeiro", "provider": "local", "isDefault": true }),
        )
        .await;
        create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Segundo", "provider": "local", "isDefault": true }),
        )
        .await;

        let id = first["data"]["id"].as_i64().expect("id");
        let reloaded: Value = request
            .get(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .await
            .json();

        // O segundo default derruba o primeiro; dois defaults fariam o backup
        // sem destino explicito escolher um deles ao acaso.
        assert_eq!(reloaded["data"]["isDefault"], false);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn removes_a_storage_that_nothing_points_to() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Descartavel", "provider": "local" }),
        )
        .await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .delete(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200);

        let body: Value = response.json();
        assert_eq!(body["message"], "Armazenamento removido com sucesso");

        let gone = request
            .get(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(gone.status_code(), 404);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_to_remove_a_storage_with_a_connection_attached() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Em uso", "provider": "local" }),
        )
        .await;
        let id = created["data"]["id"].as_i64().expect("id");

        let connection = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "Vinculada",
                "type": "mysql",
                "host": "127.0.0.1",
                "port": 13306,
                "username": "tester",
                "password": "test_pw",
                "databases": ["app_fixture"],
                "storageDestinationId": id,
            }))
            .await;
        assert_eq!(connection.status_code(), 201, "{}", connection.text());

        let response = request
            .delete(&format!("/api/storages/{id}"))
            .authorization_bearer(&token)
            .await;

        // 422, e nao 409: e' o `unprocessableEntity` do controller do Adonis.
        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        assert_eq!(body["success"], false);
        assert!(
            body["message"]
                .as_str()
                .expect("mensagem")
                .contains("1 conexão(ões)"),
            "{}",
            body["message"]
        );
    })
    .await;
}

// ------------------------- teste, browse e remoção -------------------------

#[tokio::test]
#[serial]
async fn tests_a_local_destination_that_exists() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        let created = create_local(&request, &token, "Local Real", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .post(&format!("/api/storages/{id}/test"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["message"], "Conexão testada com sucesso");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_failing_test_is_a_422_with_the_provider_message() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let missing = std::env::temp_dir().join("back-roco-destino-que-nao-existe");
        let created = create_local(&request, &token, "Local Ausente", &missing).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .post(&format!("/api/storages/{id}/test"))
            .authorization_bearer(&token)
            .await;

        // Diretorio inexistente e' erro de configuracao do usuario, e nao 500.
        assert_eq!(response.status_code(), 422, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["success"], false);
        assert!(
            body["message"]
                .as_str()
                .expect("mensagem")
                .starts_with("Falha no teste de conexão: "),
            "{}",
            body["message"]
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn browses_one_level_of_a_local_destination() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        std::fs::create_dir_all(base.path().join("12")).expect("subpasta");
        std::fs::write(base.path().join("12/vendas.sql.gz"), b"dump").expect("arquivo");
        std::fs::write(base.path().join("raiz.txt"), b"x").expect("arquivo na raiz");

        let created = create_local(&request, &token, "Local Navegavel", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let root: Value = request
            .get(&format!("/api/storages/{id}/browse"))
            .authorization_bearer(&token)
            .await
            .json();

        let objects = root["data"]["objects"].as_array().expect("objetos");
        assert_eq!(objects.len(), 2, "{objects:?}");

        let directory = objects
            .iter()
            .find(|object| object["isDirectory"] == true)
            .expect("a subpasta");
        assert_eq!(directory["name"], "12");
        // Pasta nao tem tamanho proprio; `0` faria a interface exibir "0 B".
        assert!(directory["size"].is_null());

        let inside: Value = request
            .get(&format!("/api/storages/{id}/browse?path=12"))
            .authorization_bearer(&token)
            .await
            .json();

        let objects = inside["data"]["objects"].as_array().expect("objetos");
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["key"], "12/vendas.sql.gz");
        assert_eq!(objects[0]["size"], 4);
        // Sem backup gravado apontando para o arquivo, nao ha' replica — e o
        // campo e' omitido, e nao emitido vazio.
        assert!(objects[0].get("replicas").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn browsing_outside_the_base_is_refused() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        let created = create_local(&request, &token, "Local Fechado", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .get(&format!("/api/storages/{id}/browse?path=../../etc"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 422, "{}", response.text());

        let body: Value = response.json();
        assert!(
            body["message"]
                .as_str()
                .expect("mensagem")
                .contains("path traversal"),
            "{}",
            body["message"]
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn a_page_size_above_the_ceiling_is_rejected() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        let created = create_local(&request, &token, "Local Limitado", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .get(&format!("/api/storages/{id}/browse?limit=100000"))
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        assert_eq!(body["errors"][0]["field"], "limit");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn deletes_an_object_from_a_local_destination() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        std::fs::create_dir_all(base.path().join("12")).expect("subpasta");
        let file = base.path().join("12/vendas.sql.gz");
        std::fs::write(&file, b"dump").expect("arquivo");

        let created = create_local(&request, &token, "Local Apagavel", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .delete(&format!("/api/storages/{id}/object"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "key": "12/vendas.sql.gz", "isDirectory": false,
            }))
            .await;

        assert_eq!(response.status_code(), 200, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(body["message"], "Arquivo excluído com sucesso");
        assert!(!file.exists(), "o arquivo continua no disco");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn refuses_to_delete_the_root_of_a_destination() {
    // A interface envia a chave que o usuario selecionou; um clique na linha
    // errada nao pode apagar o destino inteiro.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        let created = create_local(&request, &token, "Local Protegido", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        for key in ["/", "."] {
            let response = request
                .delete(&format!("/api/storages/{id}/object"))
                .authorization_bearer(&token)
                .json(&serde_json::json!({ "key": key, "isDirectory": true }))
                .await;

            assert_eq!(response.status_code(), 422, "aceitou {key:?}");
            assert!(base.path().exists(), "a base foi removida com {key:?}");
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn deleting_an_object_requires_both_fields() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;
        let base = tempfile::tempdir().expect("diretorio temporario");

        let created = create_local(&request, &token, "Local Exigente", base.path()).await;
        let id = created["data"]["id"].as_i64().expect("id");

        let response = request
            .delete(&format!("/api/storages/{id}/object"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({ "key": "12/a.gz" }))
            .await;

        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        assert_eq!(body["errors"][0]["field"], "isDirectory");
    })
    .await;
}

// ========================= /api/storage-destinations =========================

#[tokio::test]
#[serial]
async fn the_legacy_route_creates_without_a_provider() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .post("/api/storage-destinations")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "Destino Legado S3",
                "type": "s3",
                "config": {
                    "bucket": "backups-secondary",
                    "accessKeyId": "contract-key",
                    "secretAccessKey": "contract-secret",
                    "endpoint": "http://127.0.0.1:19000",
                    "forcePathStyle": true,
                },
            }))
            .await;

        assert_eq!(response.status_code(), 201, "{}", response.text());

        let body: Value = response.json();
        assert_eq!(
            body["message"],
            "Destino de armazenamento criado com sucesso"
        );

        let data = &body["data"];
        assert_eq!(data["type"], "s3");
        assert_eq!(data["config"]["secretAccessKey"], "***");
        // A rota legada nao conhece `provider`: inventar um faria a listagem
        // nova exibir "Amazon S3" para um MinIO.
        assert!(data.get("provider").is_none());
        assert!(data.get("providerLabel").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_legacy_list_omits_the_provider_columns() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        // Criado pela rota **nova**, com provider gravado.
        create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Local Novo", "provider": "local" }),
        )
        .await;

        let body: Value = request
            .get("/api/storage-destinations")
            .authorization_bearer(&token)
            .await
            .json();

        let item = &body["data"]["data"][0];
        assert_eq!(item["name"], "Local Novo");
        assert_eq!(item["type"], "local");
        // A mesma linha, vista pela rota antiga, nao expoe o provider.
        assert!(item.get("provider").is_none());
        assert!(item.get("providerLabel").is_none());
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_legacy_route_still_demands_the_secret_on_update() {
    // Ela nao funde nada: aceitar vazio aqui apagaria a credencial gravada.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = request
            .post("/api/storage-destinations")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "Destino Legado",
                "type": "s3",
                "config": {
                    "bucket": "b", "accessKeyId": "k", "secretAccessKey": "s",
                },
            }))
            .await;
        assert_eq!(created.status_code(), 201, "{}", created.text());

        let body: Value = created.json();
        let id = body["data"]["id"].as_i64().expect("id");

        let response = request
            .put(&format!("/api/storage-destinations/{id}"))
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "type": "s3",
                "config": { "bucket": "b", "accessKeyId": "k", "secretAccessKey": "" },
            }))
            .await;

        assert_eq!(response.status_code(), 422);

        let body: Value = response.json();
        let fields: Vec<&str> = body["errors"]
            .as_array()
            .expect("erros")
            .iter()
            .filter_map(|error| error["field"].as_str())
            .collect();
        assert!(fields.contains(&"config.secretAccessKey"), "{fields:?}");
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_legacy_route_deletes_without_checking_references() {
    // Divergencia deliberada em relacao a `/api/storages`: o controller legado
    // apaga direto, e os `ON DELETE SET NULL` do schema cuidam do resto.
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let created = create_storage(
            &request,
            &token,
            &serde_json::json!({ "name": "Legado Em Uso", "provider": "local" }),
        )
        .await;
        let id = created["data"]["id"].as_i64().expect("id");

        let connection = request
            .post("/api/connections")
            .authorization_bearer(&token)
            .json(&serde_json::json!({
                "name": "Vinculada Legado",
                "type": "mysql",
                "host": "127.0.0.1",
                "port": 13306,
                "username": "tester",
                "password": "test_pw",
                "databases": ["app_fixture"],
                "storageDestinationId": id,
            }))
            .await;
        assert_eq!(connection.status_code(), 201, "{}", connection.text());

        let legacy = request
            .delete(&format!("/api/storage-destinations/{id}"))
            .authorization_bearer(&token)
            .await;
        assert_eq!(legacy.status_code(), 200, "{}", legacy.text());

        let body: Value = legacy.json();
        assert_eq!(
            body["message"],
            "Destino de armazenamento removido com sucesso"
        );
    })
    .await;
}

#[tokio::test]
#[serial]
async fn the_legacy_404_has_its_own_message() {
    request::<App, _, _>(|request, _ctx| async move {
        let token = admin_token(&request).await;

        let response = request
            .get("/api/storage-destinations/9999")
            .authorization_bearer(&token)
            .await;

        assert_eq!(response.status_code(), 404);
        assert_eq!(
            response.json::<Value>()["message"],
            "Destino de armazenamento não encontrado"
        );
    })
    .await;
}

// ============================== autorização ==============================

#[tokio::test]
#[serial]
async fn every_route_of_the_resource_demands_a_session() {
    request::<App, _, _>(|request, _ctx| async move {
        let unauthenticated = [
            request.get("/api/storages").await,
            request.post("/api/storages").await,
            request.get("/api/storages/1").await,
            request.put("/api/storages/1").await,
            request.delete("/api/storages/1").await,
            request.post("/api/storages/1/test").await,
            request.get("/api/storages/1/browse").await,
            request.delete("/api/storages/1/object").await,
            request.get("/api/storage-destinations").await,
            request.post("/api/storage-destinations").await,
            request.get("/api/storage-destinations/1").await,
            request.put("/api/storage-destinations/1").await,
            request.delete("/api/storage-destinations/1").await,
        ];

        for response in unauthenticated {
            assert_eq!(response.status_code(), 401, "{}", response.text());
            // Familia do framework: quem responde e' o middleware.
            assert_eq!(
                response.json::<Value>()["errors"][0]["message"],
                "Unauthorized access"
            );
        }
    })
    .await;
}
