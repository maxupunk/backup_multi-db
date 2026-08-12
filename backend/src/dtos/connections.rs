//! Respostas de `/api/connections`.
//!
//! ## Uma conexão, três respostas
//!
//! [`Connection`] é o registro e os databases que ele acompanha — é o corpo de
//! `POST`, `PUT` e `PATCH`. As duas listagens acrescentam backups, e diferem
//! só no recorte de cada um: a lista traz o **último** backup em forma resumida
//! ([`ConnectionListItem`]), e o detalhe traz os dez mais recentes com os
//! campos que a tela de histórico exibe ([`ConnectionDetail`]).
//!
//! O `#[serde(flatten)]` faz as duas serem *a* conexão mais os backups, em vez
//! de uma cópia dos dezesseis campos. No TypeScript isso vira uma interseção,
//! então o frontend continua enxergando um objeto só.
//!
//! ## A senha nunca sai
//!
//! `password_encrypted` não está em nenhuma destas structs, e não pode estar.
//! Não é só o texto claro que importa: o ciphertext com o IV ao lado é material
//! para um ataque offline, e não há tela que precise dele.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::_entities::{backups, connection_databases, connections};

/// Um database que a conexão acompanha.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionDatabase {
    #[ts(type = "number")]
    pub id: i64,
    pub database_name: String,
    /// Um database desabilitado continua na tabela e sai do próximo backup.
    pub enabled: bool,
}

impl From<connection_databases::Model> for ConnectionDatabase {
    fn from(row: connection_databases::Model) -> Self {
        Self {
            id: row.id,
            database_name: row.database_name,
            enabled: row.enabled.unwrap_or(false),
        }
    }
}

/// Backup resumido, como aparece na listagem de conexões.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionBackupSummary {
    #[ts(type = "number")]
    pub id: i64,
    pub status: String,
    #[ts(type = "number | null")]
    pub file_size: Option<i64>,
    pub database_name: String,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    #[ts(type = "string | null")]
    pub finished_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

impl From<backups::Model> for ConnectionBackupSummary {
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

/// Backup como a tela de histórico de uma conexão o mostra.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionBackupDetail {
    #[ts(type = "number")]
    pub id: i64,
    pub status: String,
    pub file_name: Option<String>,
    #[ts(type = "number | null")]
    pub file_size: Option<i64>,
    pub database_name: String,
    pub retention_type: String,
    pub trigger: String,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    #[ts(type = "string | null")]
    pub finished_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[ts(type = "number | null")]
    pub duration_seconds: Option<i64>,
}

impl From<backups::Model> for ConnectionBackupDetail {
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

/// Uma conexão e os databases que ela acompanha.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct Connection {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    /// A migration-level `CHECK` limits this column to the three supported
    /// database engines, so the generated contract can truthfully expose a
    /// discriminating union instead of an arbitrary string.
    #[ts(type = "\"mysql\" | \"mariadb\" | \"postgresql\"")]
    pub r#type: String,
    pub host: String,
    #[ts(type = "number")]
    pub port: i64,
    pub username: String,
    pub schedule_frequency: Option<String>,
    pub schedule_enabled: bool,
    pub status: Option<String>,
    /// Motivo da última falha de teste. `null` quando o último teste passou.
    pub last_error: Option<String>,
    #[ts(type = "string | null")]
    pub last_tested_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[ts(type = "string | null")]
    pub last_backup_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[ts(type = "number | null")]
    pub storage_destination_id: Option<i64>,
    /// A coluna é JSON livre; estas são as duas chaves que a validação aceita.
    #[ts(type = "{ ssl?: boolean; charset?: string } | null")]
    pub options: Option<serde_json::Value>,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    #[ts(type = "string")]
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    pub databases: Vec<ConnectionDatabase>,
}

impl Connection {
    #[must_use]
    pub fn new(row: &connections::Model, databases: Vec<ConnectionDatabase>) -> Self {
        Self {
            id: row.id,
            name: row.name.clone(),
            r#type: row.r#type.clone(),
            host: row.host.clone(),
            port: row.port,
            username: row.username.clone(),
            schedule_frequency: row.schedule_frequency.clone(),
            schedule_enabled: row.schedule_enabled.unwrap_or(false),
            status: row.status.clone(),
            last_error: row.last_error.clone(),
            last_tested_at: row.last_tested_at,
            last_backup_at: row.last_backup_at,
            storage_destination_id: row.storage_destination_id,
            options: parse_options(row.options.as_deref()),
            created_at: row.created_at,
            updated_at: row.updated_at,
            databases,
        }
    }
}

/// Item de `GET /api/connections`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionListItem {
    #[serde(flatten)]
    #[ts(flatten)]
    pub connection: Connection,
    /// Só o backup mais recente — a listagem mostra "último backup", e trazer o
    /// histórico inteiro de cada linha multiplicaria a resposta pelo número de
    /// backups já feitos.
    pub backups: Vec<ConnectionBackupSummary>,
}

/// Corpo de `GET /api/connections/:id`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionDetail {
    #[serde(flatten)]
    #[ts(flatten)]
    pub connection: Connection,
    pub backups: Vec<ConnectionBackupDetail>,
}

/// Corpo de `POST /api/connections/:id/test` bem-sucedido.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionTestResult {
    #[ts(type = "number")]
    pub latency_ms: i64,
    pub version: Option<String>,
}

/// Corpo de `POST /api/connections/discover-databases`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct DiscoveredDatabases {
    pub databases: Vec<String>,
}

/// Corpo de `POST /api/connections/:id/create-database`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct CreatedDatabase {
    pub database_name: String,
}

/// Corpo de `GET /api/connections/docker-hosts`.
///
/// Responde 200 mesmo sem Docker: a tela de nova conexão trata a ausência
/// mostrando o formulário manual, e um erro a faria exibir uma falha onde não
/// há nenhuma.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct DockerHosts {
    pub docker_available: bool,
    pub unavailable_reason: Option<String>,
    pub backend_container_id: Option<String>,
    pub hosts: Vec<crate::models::docker_connection_suggestion::HostSuggestion>,
}

/// Lê a coluna `options`, que guarda JSON como texto.
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

    fn at(text: &str) -> chrono::DateTime<chrono::FixedOffset> {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
            .expect("data de teste")
            .and_utc()
            .fixed_offset()
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

    fn databases() -> Vec<ConnectionDatabase> {
        vec![ConnectionDatabase::from(connection_databases::Model {
            id: 2,
            connection_id: 4,
            database_name: "app_fixture".to_string(),
            enabled: Some(true),
            created_at: at("2026-08-05 08:09:51"),
            updated_at: at("2026-08-05 08:09:51"),
        })]
    }

    #[test]
    fn the_booleans_are_booleans() {
        let json =
            serde_json::to_value(Connection::new(&connection(), databases())).expect("serializa");

        assert_eq!(json["scheduleEnabled"], serde_json::Value::Bool(false));
        assert_eq!(
            json["databases"][0]["enabled"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn carries_the_seventeen_columns_the_screen_reads() {
        let json =
            serde_json::to_value(Connection::new(&connection(), databases())).expect("serializa");

        assert_eq!(json.as_object().map(serde_json::Map::len), Some(17));
        for key in [
            "id",
            "name",
            "type",
            "host",
            "port",
            "username",
            "scheduleFrequency",
            "scheduleEnabled",
            "status",
            "lastError",
            "lastTestedAt",
            "lastBackupAt",
            "storageDestinationId",
            "options",
            "createdAt",
            "updatedAt",
            "databases",
        ] {
            assert!(json.get(key).is_some(), "faltou `{key}`");
        }
    }

    #[test]
    fn the_list_item_is_the_connection_plus_the_backups() {
        // O `flatten` precisa produzir um objeto so'; um objeto aninhado
        // quebraria toda leitura de `connection.name` no frontend.
        let json = serde_json::to_value(ConnectionListItem {
            connection: Connection::new(&connection(), databases()),
            backups: Vec::new(),
        })
        .expect("serializa");

        assert_eq!(json["name"], "Contract MySQL");
        assert_eq!(json["backups"], serde_json::json!([]));
        assert!(json.get("connection").is_none());
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(18));
    }

    #[test]
    fn no_response_ever_leaks_the_encrypted_password() {
        // O ciphertext com o IV ao lado e' material para ataque offline; nao ha'
        // tela que precise dele.
        for rendered in [
            serde_json::to_string(&Connection::new(&connection(), databases())).expect("ok"),
            serde_json::to_string(&ConnectionListItem {
                connection: Connection::new(&connection(), databases()),
                backups: Vec::new(),
            })
            .expect("ok"),
            serde_json::to_string(&ConnectionDetail {
                connection: Connection::new(&connection(), databases()),
                backups: Vec::new(),
            })
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

        let json = serde_json::to_value(Connection::new(&row, databases())).expect("serializa");
        assert_eq!(json["options"]["ssl"], true);
    }

    #[test]
    fn an_absent_options_column_is_null_not_missing() {
        // O frontend le' `connection.options?.ssl`; a chave ausente e o `null`
        // se comportam igual la', mas o binding declara o campo, e omiti-lo
        // faria a resposta divergir do tipo gerado.
        let json =
            serde_json::to_value(Connection::new(&connection(), databases())).expect("serializa");

        assert!(json["options"].is_null());
        assert!(json
            .as_object()
            .is_some_and(|map| map.contains_key("options")));
    }
}
