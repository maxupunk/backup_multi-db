//! Round-trip of the entities against the real schema.
//!
//! The unit tests in `src/models/*.rs` cover pure logic — formatting, masking,
//! retention promotion — without touching the database. What is left to prove
//! is that the generated entities **match the migration**: column type,
//! nullability, foreign key and index.
//!
//! It is a coupling that breaks silently. A migration changed without
//! regenerating the entities compiles fine and only fails on the first query in
//! production.

use backend::app::App;
use backend::models::_entities::{
    audit_logs, backups, connection_databases, connections, resource_metric_history,
    storage_destinations, system_settings, users,
};
use loco_rs::testing::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use serial_test::serial;

fn now() -> chrono::DateTime<chrono::FixedOffset> {
    chrono::DateTime::parse_from_rfc3339("2026-08-09T12:00:00Z").unwrap()
}

/// A minimal user, with the fields the schema demands.
async fn create_user(db: &sea_orm::DatabaseConnection, email: &str) -> users::Model {
    users::ActiveModel {
        pid: Set(uuid::Uuid::new_v4()),
        api_key: Set(format!("bk_{email}")),
        full_name: Set(Some("Teste".to_string())),
        email: Set(email.to_string()),
        password: Set("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_string()),
        is_active: Set(true),
        is_admin: Set(false),
        created_at: Set(now()),
        updated_at: Set(now()),
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
        config_encrypted: Set("v1.AAAAAAAAAAAAAAAA.Y2lwaGVy".to_string()),
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
        password_encrypted: Set("v1.AAAAAAAAAAAAAAAA.Y2lwaGVy".to_string()),
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
    assert!(user.is_active);
    assert!(!user.is_admin);
    // `pid` round-trips as a `Uuid`, not as the text SQLite actually stores.
    assert!(!user.pid.is_nil());

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

    // A real `bigint`: a 32-bit integer would overflow on both memory values
    // above and on the 1.5 GB `file_size`.
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
async fn timestamps_keep_their_offset_through_a_round_trip() {
    // The whole point of `timestamptz`: an instant written as UTC comes back as
    // the same instant, not as a naive value the reader has to guess an offset
    // for.
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let user = create_user(db, "tz@contract.test").await;
    let stored = users::Entity::find_by_id(user.id)
        .one(db)
        .await
        .unwrap()
        .expect("o usuario foi gravado");

    assert_eq!(stored.created_at, now());
    assert_eq!(stored.created_at.to_utc(), now().to_utc());
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

    // `idx_conn_db_unique`. Without it, the `PUT /api/connections/:id` path that
    // re-enables databases would append a duplicate on every call.
    assert!(
        insert().insert(db).await.is_err(),
        "o indice unico nao impediu a duplicata"
    );
}

#[tokio::test]
#[serial]
async fn the_email_is_unique() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    create_user(db, "duplicado@contract.test").await;

    let result = users::ActiveModel {
        pid: Set(uuid::Uuid::new_v4()),
        api_key: Set("bk_outra".to_string()),
        email: Set("duplicado@contract.test".to_string()),
        password: Set("$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_string()),
        is_active: Set(true),
        is_admin: Set(false),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await;

    assert!(result.is_err(), "o unique de email aceitou a duplicata");
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

    // SET NULL, not CASCADE: deleting a storage destination must not delete the
    // connections that used it.
    let survivor = connections::Entity::find_by_id(connection.id)
        .one(db)
        .await
        .unwrap()
        .expect("a conexao foi apagada junto com o storage");

    assert_eq!(survivor.storage_destination_id, None);
}

#[tokio::test]
#[serial]
async fn deleting_a_connection_cascades_to_its_backups() {
    let boot = boot_test::<App>().await.unwrap();
    let db = &boot.app_context.db;

    let connection = create_connection(db, None).await;

    let backup = backups::ActiveModel {
        connection_id: Set(Some(connection.id)),
        database_name: Set("app_fixture".to_string()),
        status: Set("completed".to_string()),
        retention_type: Set("daily".to_string()),
        r#trigger: Set("manual".to_string()),
        created_at: Set(now()),
        updated_at: Set(now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("insercao de backup");

    connections::Entity::delete_by_id(connection.id)
        .exec(db)
        .await
        .unwrap();

    assert!(
        backups::Entity::find_by_id(backup.id)
            .one(db)
            .await
            .unwrap()
            .is_none(),
        "o backup sobreviveu a' conexao apagada"
    );
}
