//! Respostas de `/api/connections` (tarefa 6.1).
//!
//! ## O contrato tem tres formas de conexao, nao uma
//!
//! O Lucid serializa o objeto que estiver na memoria, e o que esta' na memoria
//! depende de como ele chegou la'. Os golden files da Fase 2 registram a
//! diferenca:
//!
//! | Rota | `scheduleEnabled` | `lastError`/`lastTestedAt`/`lastBackupAt` | `backups` |
//! |---|---|---|---|
//! | `POST` (store) | `false` — booleano | **ausentes** | ausente |
//! | `PUT` (update) | `0` — numero | presentes | ausente |
//! | `GET` (index/show) | `0` — numero | presentes | presente |
//!
//! No `store` o registro nunca voltou do banco: os tres campos de teste jamais
//! foram atribuidos, e `JSON.stringify` **omite** `undefined`. Nas outras rotas
//! o registro veio do SQLite, onde booleano e' `0`/`1`, e o model de
//! `connections` — diferente do de `users` — nao tem `consume` convertendo de
//! volta.
//!
//! E' o ACHADO 3 da Fase 2. Emitir `true` onde o contrato diz `1` quebraria
//! todo cliente que compare com `===`, e o matcher de shape da suite reprova a
//! troca de tipo.

use serde::Serialize;

use crate::models::_entities::{backups, connection_databases, connections};
use crate::views::timestamp;

/// Um booleano com o tipo JSON que o contrato exige naquele ponto.
///
/// Existe para que a escolha seja **explicita** em cada view. Um `bool` puro
/// emitiria `true`/`false` em toda parte, e a divergencia so' apareceria na
/// suite de contrato — ou, pior, no cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum WireBool {
    /// `0`/`1` — o valor como o SQLite devolve.
    FromDatabase(u8),
    /// `true`/`false` — o valor que ainda esta' na memoria da aplicacao.
    InMemory(bool),
}

impl WireBool {
    pub const fn from_database(value: bool) -> Self {
        Self::FromDatabase(value as u8)
    }

    pub const fn in_memory(value: bool) -> Self {
        Self::InMemory(value)
    }
}

/// Item de `databases`, com os tres campos que o controller seleciona.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseItem {
    pub id: i64,
    pub database_name: String,
    /// Sempre `0`/`1`: o controller recarrega a relacao do banco em **todas**
    /// as rotas, inclusive na de criacao.
    pub enabled: WireBool,
}

impl From<connection_databases::Model> for DatabaseItem {
    fn from(row: connection_databases::Model) -> Self {
        Self {
            id: row.id,
            database_name: row.database_name,
            enabled: WireBool::from_database(row.enabled.unwrap_or(false)),
        }
    }
}

/// Backup resumido, como aparece em `GET /api/connections`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub id: i64,
    pub status: String,
    pub file_size: Option<i64>,
    pub database_name: String,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
    #[serde(serialize_with = "timestamp::serialize_option")]
    pub finished_at: Option<chrono::NaiveDateTime>,
}

impl From<backups::Model> for BackupSummary {
    fn from(row: backups::Model) -> Self {
        Self {
            id: row.id,
            status: row.status,
            file_size: row.file_size,
            database_name: row.database_name,
            created_at: row.created_at,
            finished_at: row.finished_at,
        }
    }
}

/// Backup detalhado, como aparece em `GET /api/connections/:id`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupDetail {
    pub id: i64,
    pub status: String,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
    pub database_name: String,
    pub retention_type: String,
    pub trigger: String,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
    #[serde(serialize_with = "timestamp::serialize_option")]
    pub finished_at: Option<chrono::NaiveDateTime>,
    pub duration_seconds: Option<i64>,
}

impl From<backups::Model> for BackupDetail {
    fn from(row: backups::Model) -> Self {
        Self {
            id: row.id,
            status: row.status,
            file_name: row.file_name,
            file_size: row.file_size,
            database_name: row.database_name,
            retention_type: row.retention_type,
            trigger: row.trigger,
            created_at: row.created_at,
            finished_at: row.finished_at,
            duration_seconds: row.duration_seconds,
        }
    }
}

/// Os campos comuns as tres formas.
///
/// `password_encrypted` **nao** esta' aqui, e nunca pode estar: a coluna leva
/// `serializeAs: null` no Lucid, e o ciphertext de uma senha de producao nao
/// tem por que sair da aplicacao.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Core {
    id: i64,
    name: String,
    r#type: String,
    host: String,
    port: i64,
    username: String,
    schedule_frequency: Option<String>,
    schedule_enabled: WireBool,
    status: Option<String>,
    storage_destination_id: Option<i64>,
    options: Option<serde_json::Value>,
    #[serde(serialize_with = "timestamp::serialize")]
    created_at: chrono::NaiveDateTime,
    #[serde(serialize_with = "timestamp::serialize")]
    updated_at: chrono::NaiveDateTime,
}

impl Core {
    fn new(row: &connections::Model, schedule_enabled: WireBool) -> Self {
        Self {
            id: row.id,
            name: row.name.clone(),
            r#type: row.r#type.clone(),
            host: row.host.clone(),
            port: row.port,
            username: row.username.clone(),
            schedule_frequency: row.schedule_frequency.clone(),
            schedule_enabled,
            status: row.status.clone(),
            storage_destination_id: row.storage_destination_id,
            options: parse_options(row.options.as_deref()),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Os tres campos que so' existem quando o registro veio do banco.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestState {
    last_error: Option<String>,
    #[serde(serialize_with = "timestamp::serialize_option")]
    last_tested_at: Option<chrono::NaiveDateTime>,
    #[serde(serialize_with = "timestamp::serialize_option")]
    last_backup_at: Option<chrono::NaiveDateTime>,
}

impl From<&connections::Model> for TestState {
    fn from(row: &connections::Model) -> Self {
        Self {
            last_error: row.last_error.clone(),
            last_tested_at: row.last_tested_at,
            last_backup_at: row.last_backup_at,
        }
    }
}

/// Corpo de `POST /api/connections` — o registro como ele esta' na memoria.
#[derive(Debug, Clone, Serialize)]
pub struct Created {
    #[serde(flatten)]
    core: Core,
    pub databases: Vec<DatabaseItem>,
}

impl Created {
    pub fn new(row: &connections::Model, databases: Vec<DatabaseItem>) -> Self {
        Self {
            // Booleano de verdade: o valor nunca passou pelo SQLite nesta rota.
            core: Core::new(
                row,
                WireBool::in_memory(row.schedule_enabled.unwrap_or(false)),
            ),
            databases,
        }
    }
}

/// Corpo de `PUT`/`PATCH /api/connections/:id`.
#[derive(Debug, Clone, Serialize)]
pub struct Updated {
    #[serde(flatten)]
    core: Core,
    #[serde(flatten)]
    test_state: TestState,
    pub databases: Vec<DatabaseItem>,
}

impl Updated {
    pub fn new(row: &connections::Model, databases: Vec<DatabaseItem>) -> Self {
        Self {
            core: Core::new(
                row,
                WireBool::from_database(row.schedule_enabled.unwrap_or(false)),
            ),
            test_state: TestState::from(row),
            databases,
        }
    }
}

/// Item de `GET /api/connections`.
#[derive(Debug, Clone, Serialize)]
pub struct ListItem {
    #[serde(flatten)]
    core: Core,
    #[serde(flatten)]
    test_state: TestState,
    pub databases: Vec<DatabaseItem>,
    /// So' o backup mais recente — e' o `groupLimit(1)` do Adonis.
    pub backups: Vec<BackupSummary>,
}

impl ListItem {
    pub fn new(
        row: &connections::Model,
        databases: Vec<DatabaseItem>,
        backups: Vec<BackupSummary>,
    ) -> Self {
        Self {
            core: Core::new(
                row,
                WireBool::from_database(row.schedule_enabled.unwrap_or(false)),
            ),
            test_state: TestState::from(row),
            databases,
            backups,
        }
    }
}

/// Corpo de `GET /api/connections/:id`.
#[derive(Debug, Clone, Serialize)]
pub struct Detail {
    #[serde(flatten)]
    core: Core,
    #[serde(flatten)]
    test_state: TestState,
    pub databases: Vec<DatabaseItem>,
    pub backups: Vec<BackupDetail>,
}

impl Detail {
    pub fn new(
        row: &connections::Model,
        databases: Vec<DatabaseItem>,
        backups: Vec<BackupDetail>,
    ) -> Self {
        Self {
            core: Core::new(
                row,
                WireBool::from_database(row.schedule_enabled.unwrap_or(false)),
            ),
            test_state: TestState::from(row),
            databases,
            backups,
        }
    }
}

/// `data` de `POST /api/connections/:id/test` bem-sucedido.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub latency_ms: i64,
    pub version: Option<String>,
}

/// `data` de `POST /api/connections/discover-databases`.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredDatabases {
    pub databases: Vec<String>,
}

/// `data` de `POST /api/connections/:id/create-database`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedDatabase {
    pub database_name: String,
}

/// `data` de `GET /api/connections/docker-hosts`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerHosts {
    pub docker_available: bool,
    pub unavailable_reason: Option<String>,
    pub backend_container_id: Option<String>,
    pub hosts: Vec<serde_json::Value>,
}

/// Le' a coluna `options`, que guarda JSON como texto.
fn parse_options(raw: Option<&str>) -> Option<serde_json::Value> {
    let raw = raw?;
    if raw.is_empty() {
        return None;
    }

    serde_json::from_str(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").expect("data de teste")
    }

    fn connection() -> connections::Model {
        connections::Model {
            id: 4,
            name: "Contract MySQL".to_string(),
            r#type: "mysql".to_string(),
            host: "127.0.0.1".to_string(),
            port: 13306,
            username: "tester".to_string(),
            password_encrypted: "aWl2:dGFn:Y2lwaGVy".to_string(),
            schedule_frequency: None,
            schedule_enabled: Some(false),
            status: Some("active".to_string()),
            last_error: None,
            last_tested_at: Some(at("2026-08-06 16:49:25")),
            last_backup_at: Some(at("2026-08-06 16:52:39")),
            storage_destination_id: Some(1),
            options: None,
            created_at: at("2026-08-05 08:09:51"),
            updated_at: at("2026-08-06 16:52:39"),
        }
    }

    fn databases() -> Vec<DatabaseItem> {
        vec![DatabaseItem::from(connection_databases::Model {
            id: 2,
            connection_id: 4,
            database_name: "app_fixture".to_string(),
            enabled: Some(true),
            created_at: at("2026-08-05 08:09:51"),
            updated_at: at("2026-08-05 08:09:51"),
        })]
    }

    #[test]
    fn the_created_body_omits_the_test_state_and_uses_a_real_boolean() {
        // No `store` o registro nunca voltou do banco: os tres campos jamais
        // foram atribuidos, e `JSON.stringify` omite `undefined`.
        let json =
            serde_json::to_value(Created::new(&connection(), databases())).expect("serializa");

        assert!(json.get("lastError").is_none());
        assert!(json.get("lastTestedAt").is_none());
        assert!(json.get("lastBackupAt").is_none());
        assert!(json.get("backups").is_none());
        assert_eq!(json["scheduleEnabled"], serde_json::Value::Bool(false));
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(14));
    }

    #[test]
    fn the_updated_body_carries_the_test_state_and_a_numeric_boolean() {
        let json =
            serde_json::to_value(Updated::new(&connection(), databases())).expect("serializa");

        assert!(json.get("lastError").is_some());
        assert!(json.get("backups").is_none());
        // ACHADO 3: `0`, e nao `false`.
        assert_eq!(json["scheduleEnabled"], serde_json::json!(0));
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(17));
    }

    #[test]
    fn the_list_item_adds_the_backups() {
        let json = serde_json::to_value(ListItem::new(&connection(), databases(), Vec::new()))
            .expect("serializa");

        assert_eq!(json["backups"], serde_json::json!([]));
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(18));
    }

    #[test]
    fn the_database_item_is_always_numeric() {
        // O controller recarrega a relacao do banco em todas as rotas.
        let json = serde_json::to_value(&databases()[0]).expect("serializa");

        assert_eq!(
            json,
            serde_json::json!({ "id": 2, "databaseName": "app_fixture", "enabled": 1 })
        );
    }

    #[test]
    fn a_disabled_database_serialises_as_zero() {
        let item = DatabaseItem::from(connection_databases::Model {
            id: 3,
            connection_id: 4,
            database_name: "desativado".to_string(),
            enabled: Some(false),
            created_at: at("2026-08-05 08:09:51"),
            updated_at: at("2026-08-05 08:09:51"),
        });

        assert_eq!(serde_json::to_value(item).expect("serializa")["enabled"], 0);
    }

    #[test]
    fn no_view_ever_leaks_the_encrypted_password() {
        for rendered in [
            serde_json::to_string(&Created::new(&connection(), databases())).expect("ok"),
            serde_json::to_string(&Updated::new(&connection(), databases())).expect("ok"),
            serde_json::to_string(&ListItem::new(&connection(), databases(), Vec::new()))
                .expect("ok"),
            serde_json::to_string(&Detail::new(&connection(), databases(), Vec::new()))
                .expect("ok"),
        ] {
            assert!(!rendered.contains("password"), "vazou a chave: {rendered}");
            assert!(!rendered.contains("aWl2"), "vazou o ciphertext: {rendered}");
        }
    }

    #[test]
    fn reads_the_options_column_as_json() {
        let mut row = connection();
        row.options = Some(r#"{"ssl":true,"charset":"utf8mb4"}"#.to_string());

        let json = serde_json::to_value(Updated::new(&row, databases())).expect("serializa");
        assert_eq!(json["options"]["ssl"], true);
    }

    #[test]
    fn an_absent_options_column_is_null_not_missing() {
        let json =
            serde_json::to_value(Updated::new(&connection(), databases())).expect("serializa");

        assert!(json["options"].is_null());
    }
}
