//! Round-trip das entidades contra o schema de verdade (tarefa 4.10).
//!
//! Os testes unitarios em `src/models/*.rs` cobrem a logica pura — formatacao,
//! mascaramento, promocao de retencao — sem tocar no banco. O que falta provar
//! e' que as entidades geradas **casam com as migrations**: tipo de coluna,
//! nulabilidade, chave estrangeira e constraint.
//!
//! E' um acoplamento que quebra em silencio. Uma migration alterada sem
//! regenerar as entidades compila normalmente e so' falha na primeira consulta
//! em producao.

use backend::app::App;
use backend::models::_entities::{
    audit_logs, auth_access_tokens, backups, connection_databases, connections,
    resource_metric_history, storage_destinations, system_settings, users,
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, PaginatorTrait};
use serial_test::serial;

fn now() -> chrono::NaiveDateTime {
    chrono::DateTime::parse_from_rfc3339("2026-08-09T12:00:00Z")
        .unwrap()
        .naive_utc()
}

/// Usuario minimo, com os campos que o schema exige.
async fn create_user(db: &sea_orm::DatabaseConnection, email: &str) -> users::Model {
    users::ActiveModel {
        full_name: Set(Some("Teste".to_string())),
        email: Set(email.to_string()),
        password: Set("$scrypt$n=16384,r=8,p=1$c2FsdA$aGFzaA".to_string()),
        is_active: Set(true),
        is_admin: Set(false),
        created_at: Set(now()),
        updated_at: Set(Some(now())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de usuario")
}

async fn create_storage(db: &sea_orm::DatabaseConnection) -> storage_destinations::Model {
    storage_destinations::ActiveModel {
        name: Set("Local".to_string()),
        r#type: Set("local".to_string()),
        status: Set("active".to_string()),
        is_default: Set(true),
        config_encrypted: Set("aWl2:dGFn:Y2lwaGVy".to_string()),
        provider: Set(Some("local".to_string())),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de storage")
}

async fn create_connection(
    db: &sea_orm::DatabaseConnection,
    storage_id: Option<i64>,
) -> connections::Model {
    connections::ActiveModel {
        name: Set("Conexao".to_string()),
        r#type: Set("mysql".to_string()),
        host: Set("127.0.0.1".to_string()),
        port: Set(3306),
        username: Set("root".to_string()),
        password_encrypted: Set("aWl2:dGFn:Y2lwaGVy".to_string()),
        schedule_enabled: Set(Some(false)),
        status: Set(Some("active".to_string())),
        storage_destination_id: Set(storage_id),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de conexao")
}

#[tokio::test]
#[serial]
async fn every_entity_round_trips_through_the_schema() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_user(db, "roundtrip@contract.test").await;
    assert_eq!(user.email, "roundtrip@contract.test");
    // `is_active` e `is_admin` sao booleanos de verdade na entidade, e nao
    // `0`/`1` — e' a diferenca em relacao ao Adonis registrada como achado da
    // Fase 2.
    assert!(user.is_active);
    assert!(!user.is_admin);

    auth_access_tokens::ActiveModel {
        tokenable_id: Set(user.id),
        r#type: Set("auth_token".to_string()),
        name: Set(None),
        hash: Set("a".repeat(64)),
        abilities: Set("[\"*\"]".to_string()),
        created_at: Set(Some(now())),
        updated_at: Set(Some(now())),
        expires_at: Set(Some(now())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de token");

    let storage = create_storage(db).await;
    let connection = create_connection(db, Some(storage.id)).await;

    let database = connection_databases::ActiveModel {
        connection_id: Set(connection.id),
        database_name: Set("app_fixture".to_string()),
        enabled: Set(Some(true)),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de database");

    backups::ActiveModel {
        connection_id: Set(Some(connection.id)),
        connection_database_id: Set(Some(database.id)),
        database_name: Set("app_fixture".to_string()),
        status: Set("completed".to_string()),
        retention_type: Set("daily".to_string()),
        r#trigger: Set("manual".to_string()),
        file_size: Set(Some(1_610_612_736)),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de backup");

    audit_logs::ActiveModel {
        action: Set("connection.created".to_string()),
        entity_type: Set("connection".to_string()),
        entity_id: Set(Some(connection.id)),
        description: Set("Conexão criada".to_string()),
        status: Set("success".to_string()),
        created_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de audit log");

    system_settings::ActiveModel {
        name: Set("backup_retention".to_string()),
        value: Set("{\"daily\":7}".to_string()),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de setting");

    resource_metric_history::ActiveModel {
        scope: Set("system".to_string()),
        cpu_usage_percent: Set(51.35),
        memory_usage_percent: Set(88.18),
        memory_used_bytes: Set(29_890_318_336),
        memory_total_bytes: Set(33_895_165_952),
        collected_at: Set(now()),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de metrica");

    // `bigint` de verdade: um `integer` de 32 bits estouraria nos dois valores
    // de memoria acima e no `file_size` de 1,5 GB.
    let metric = resource_metric_history::Entity::find()
        .one(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(metric.memory_total_bytes, 33_895_165_952);

    let backup = backups::Entity::find().one(db).await.unwrap().unwrap();
    assert_eq!(backup.file_size, Some(1_610_612_736));
}

#[tokio::test]
#[serial]
async fn the_unique_index_blocks_a_duplicated_database_name() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let connection = create_connection(db, None).await;

    let insert = || connection_databases::ActiveModel {
        connection_id: Set(connection.id),
        database_name: Set("mesmo_nome".to_string()),
        enabled: Set(Some(true)),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    };

    insert().insert(db).await.expect("primeira insercao");

    // `idx_conn_db_unique`. Sem ele, o `PUT /api/connections/:id` que reativa
    // databases criaria duplicatas a cada chamada.
    assert!(
        insert().insert(db).await.is_err(),
        "o indice unico nao impediu a duplicata"
    );
}

#[tokio::test]
#[serial]
async fn deleting_a_user_cascades_to_the_tokens() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_user(db, "cascade@contract.test").await;

    auth_access_tokens::ActiveModel {
        tokenable_id: Set(user.id),
        r#type: Set("auth_token".to_string()),
        hash: Set("b".repeat(64)),
        abilities: Set("[\"*\"]".to_string()),
        created_at: Set(Some(now())),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de token");

    users::Entity::delete_by_id(user.id).exec(db).await.unwrap();

    // Um token orfao continuaria autenticando contra um usuario que nao existe
    // mais — daí o CASCADE na FK.
    assert_eq!(
        auth_access_tokens::Entity::find().count(db).await.unwrap(),
        0
    );
}

#[tokio::test]
#[serial]
async fn deleting_a_storage_only_detaches_the_connection() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let storage = create_storage(db).await;
    let connection = create_connection(db, Some(storage.id)).await;

    storage_destinations::Entity::delete_by_id(storage.id)
        .exec(db)
        .await
        .unwrap();

    // SET NULL, e nao CASCADE: apagar um destino de armazenamento nao pode
    // apagar as conexoes que o usavam.
    let survivor = connections::Entity::find_by_id(connection.id)
        .one(db)
        .await
        .unwrap()
        .expect("a conexao foi apagada junto com o storage");

    assert_eq!(survivor.storage_destination_id, None);
}

#[tokio::test]
#[serial]
async fn the_check_constraint_rejects_a_value_outside_the_enum() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let result = connections::ActiveModel {
        name: Set("Invalida".to_string()),
        r#type: Set("oracle".to_string()),
        host: Set("127.0.0.1".to_string()),
        port: Set(1521),
        username: Set("system".to_string()),
        password_encrypted: Set(String::new()),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await;

    // O `CHECK` do schema e' a ultima barreira: mesmo que a validacao da
    // aplicacao passe batido, o banco recusa.
    assert!(result.is_err(), "o CHECK aceitou um tipo fora do enum");
}
