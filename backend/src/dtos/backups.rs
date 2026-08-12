//! Respostas de `/api/backups`.
//!
//! ## A conexão aninhada é sempre uma chave, às vezes nula
//!
//! `connection` vale `null` quando o backup ficou órfão — a FK é `SET NULL`,
//! então apagar uma conexão preserva o histórico dela. A chave existe em todas
//! as rotas: o frontend lê `backup.connection?.name`, e uma chave que aparece
//! numa listagem e some noutra obrigaria cada tela a saber de qual endpoint
//! aquele objeto veio.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::_entities::{backups, connections};
use crate::models::backup_import::{ImportedFormat, IntegrityResult};

/// Conexão aninhada num item de backup.
///
/// Quatro campos, e não o registro inteiro: é o que a lista de backups mostra
/// em cada linha, e trazer as credenciais junto exporia dado sensível numa
/// resposta que nem a tela usa.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct BackupConnection {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub r#type: String,
    pub host: String,
}

impl From<&connections::Model> for BackupConnection {
    fn from(row: &connections::Model) -> Self {
        Self {
            id: row.id,
            name: row.name.clone(),
            r#type: row.r#type.clone(),
            host: row.host.clone(),
        }
    }
}

/// Um backup como a API o devolve.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct Backup {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number | null")]
    pub connection_id: Option<i64>,
    #[ts(type = "number | null")]
    pub connection_database_id: Option<i64>,
    pub database_name: String,
    #[ts(type = "number | null")]
    pub storage_destination_id: Option<i64>,
    pub status: String,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    #[ts(type = "number | null")]
    pub file_size: Option<i64>,
    pub checksum: Option<String>,
    pub compressed: bool,
    pub retention_type: String,
    /// Um backup protegido não é apagado pela retenção nem pela rota de delete.
    pub protected: bool,
    #[ts(type = "string | null")]
    pub started_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[ts(type = "string | null")]
    pub finished_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[ts(type = "number | null")]
    pub duration_seconds: Option<i64>,
    pub error_message: Option<String>,
    #[ts(type = "number | null")]
    pub exit_code: Option<i64>,
    /// JSON já decodificado — a coluna guarda texto, e devolver a string crua
    /// obrigaria o frontend a um `JSON.parse` que ele não faz.
    #[ts(type = "Record<string, unknown>")]
    pub metadata: serde_json::Value,
    pub trigger: String,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    #[ts(type = "string")]
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    /// `null` quando o backup ficou órfão pelo `SET NULL` da chave estrangeira.
    pub connection: Option<BackupConnection>,
}

impl Backup {
    /// Item sem a conexão carregada — sai como `"connection": null`.
    #[must_use]
    pub fn new(row: &backups::Model) -> Self {
        Self {
            id: row.id,
            connection_id: row.connection_id,
            connection_database_id: row.connection_database_id,
            database_name: row.database_name.clone(),
            storage_destination_id: row.storage_destination_id,
            status: row.status.clone(),
            file_path: row.file_path.clone(),
            file_name: row.file_name.clone(),
            file_size: row.file_size,
            checksum: row.checksum.clone(),
            compressed: row.compressed.unwrap_or(false),
            retention_type: row.retention_type.clone(),
            protected: row.protected.unwrap_or(false),
            started_at: row.started_at,
            finished_at: row.finished_at,
            duration_seconds: row.duration_seconds,
            error_message: row.error_message.clone(),
            exit_code: row.exit_code,
            metadata: row.metadata_json(),
            trigger: row.trigger.clone(),
            created_at: row.created_at,
            updated_at: row.updated_at,
            connection: None,
        }
    }

    /// O mesmo item com a conexão anexada.
    #[must_use]
    pub fn with_connection(mut self, connection: Option<&connections::Model>) -> Self {
        self.connection = connection.map(BackupConnection::from);
        self
    }
}

/// Corpo de `POST /api/backups/:id/restore`.
///
/// A restauração é assíncrona: a resposta confirma o aceite e devolve o
/// identificador pelo qual o progresso chega no fluxo de eventos.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct RestoreAccepted {
    pub restore_id: String,
    pub database_name: String,
}

/// Corpo de `POST /api/backups/import`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ImportedBackup {
    pub backup: Backup,
    pub format: ImportedFormat,
    pub checksum: String,
    #[ts(type = "number")]
    pub file_size: i64,
    /// `null` quando a verificação de integridade não foi pedida.
    pub integrity: Option<IntegrityResult>,
}

/// Resultado do backup de **um** database dentro de
/// `POST /api/connections/:id/backup`.
///
/// Uma forma só para sucesso e falha, com `success` dizendo qual é. Havia duas
/// formas que não compartilhavam chave nenhuma, e o cliente precisava descobrir
/// qual recebera testando a presença de campos — num contrato tipado isso vira
/// uma união que não estreita.
///
/// `fileSize` e `durationSeconds` saem em bytes e segundos. Formatar (`1.50 MB`)
/// é decisão de apresentação, e o mesmo nome de campo não pode ser número numa
/// resposta e texto noutra.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionBackupItem {
    pub database_name: String,
    #[ts(type = "number")]
    pub backup_id: i64,
    pub success: bool,
    pub file_name: Option<String>,
    #[ts(type = "number | null")]
    pub file_size: Option<i64>,
    #[ts(type = "number | null")]
    pub duration_seconds: Option<i64>,
    /// Preenchido só quando `success` é `false`.
    pub error: Option<String>,
}

/// Corpo de `POST /api/connections/:id/backup`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ConnectionBackupResult {
    #[ts(type = "number")]
    pub total_databases: usize,
    #[ts(type = "number")]
    pub successful: usize,
    #[ts(type = "number")]
    pub failed: usize,
    pub backups: Vec<ConnectionBackupItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> backups::Model {
        backups::Model {
            id: 7,
            connection_id: Some(3),
            connection_database_id: Some(9),
            database_name: "vendas".to_string(),
            status: "completed".to_string(),
            file_path: Some("3/vendas_20260809_120000.sql.gz".to_string()),
            file_name: Some("vendas_20260809_120000.sql.gz".to_string()),
            file_size: Some(2048),
            checksum: Some("abc".to_string()),
            compressed: Some(true),
            retention_type: "hourly".to_string(),
            protected: Some(false),
            started_at: None,
            finished_at: None,
            duration_seconds: Some(12),
            error_message: None,
            exit_code: Some(0),
            metadata: Some(r#"{"isImported":true}"#.to_string()),
            trigger: "manual".to_string(),
            created_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            updated_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            storage_destination_id: None,
        }
    }

    fn connection() -> connections::Model {
        connections::Model {
            id: 3,
            name: "Producao".to_string(),
            r#type: "mysql".to_string(),
            host: "db.local".to_string(),
            port: 3306,
            username: "root".to_string(),
            password_encrypted: String::new(),
            status: Some("active".to_string()),
            last_error: None,
            last_tested_at: None,
            last_backup_at: None,
            schedule_enabled: Some(false),
            schedule_frequency: None,
            options: None,
            created_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            updated_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            storage_destination_id: None,
        }
    }

    #[test]
    fn the_booleans_are_booleans() {
        let json = serde_json::to_value(Backup::new(&row())).expect("serializa");

        assert_eq!(json["compressed"], serde_json::Value::Bool(true));
        assert_eq!(json["protected"], serde_json::Value::Bool(false));
    }

    #[test]
    fn the_metadata_is_decoded_not_a_raw_string() {
        let json = serde_json::to_value(Backup::new(&row())).expect("serializa");

        assert_eq!(json["metadata"]["isImported"], true);
    }

    #[test]
    fn the_connection_key_exists_in_both_shapes() {
        // Uma chave que aparece numa listagem e some noutra obrigaria cada tela
        // a saber de qual endpoint aquele objeto veio.
        let plain = serde_json::to_value(Backup::new(&row())).expect("serializa");
        assert!(plain["connection"].is_null());
        assert!(plain
            .as_object()
            .is_some_and(|map| map.contains_key("connection")));

        let with = serde_json::to_value(Backup::new(&row()).with_connection(Some(&connection())))
            .expect("serializa");
        assert_eq!(with["connection"]["name"], "Producao");
        assert_eq!(with["connection"]["host"], "db.local");
    }

    #[test]
    fn the_nested_connection_never_carries_credentials() {
        let json = serde_json::to_value(BackupConnection::from(&connection())).expect("serializa");

        assert_eq!(json.as_object().map(serde_json::Map::len), Some(4));
        assert!(json.get("password").is_none());
        assert!(json.get("passwordEncrypted").is_none());
        assert!(json.get("username").is_none());
    }

    #[test]
    fn a_backup_item_reports_bytes_and_seconds_not_formatted_text() {
        let json = serde_json::to_value(ConnectionBackupItem {
            database_name: "vendas".to_string(),
            backup_id: 7,
            success: true,
            file_name: Some("vendas.sql.gz".to_string()),
            file_size: Some(1_572_864),
            duration_seconds: Some(12),
            error: None,
        })
        .expect("serializa");

        assert_eq!(json["fileSize"], 1_572_864);
        assert_eq!(json["durationSeconds"], 12);
        assert!(json["error"].is_null());
    }

    #[test]
    fn success_and_failure_share_one_shape() {
        // O cliente le' `success`, e nao a presenca de campos.
        let failed = serde_json::to_value(ConnectionBackupItem {
            database_name: "vendas".to_string(),
            backup_id: 7,
            success: false,
            file_name: None,
            file_size: None,
            duration_seconds: None,
            error: Some("Access denied".to_string()),
        })
        .expect("serializa");

        assert_eq!(failed["success"], serde_json::Value::Bool(false));
        assert_eq!(failed["error"], "Access denied");
        assert!(failed
            .as_object()
            .is_some_and(|map| map.contains_key("fileSize")));
    }
}
