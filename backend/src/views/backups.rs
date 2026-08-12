//! Respostas de `/api/backups` (tarefas 7.1 e 7.7).
//!
//! ## O item de backup carrega todas as colunas
//!
//! O `BackupsController` do Adonis serializa o model inteiro — nao ha' `fields`
//! nem `pick`. Recortar aqui seria uma chave a menos na resposta, e o matcher da
//! suite de contrato compara o conjunto de chaves nos dois sentidos.
//!
//! A **conexao** aninhada e' a excecao: ela vem com cinco campos escolhidos a
//! mao (`id`, `name`, `type`, `host`, `database`), e `database` **nao** e' uma
//! coluna de `connections` — o model do Adonis nao a tem, e o Lucid omite o que
//! nao existe. Por isso ela nao aparece em [`ConnectionSummary`].
//!
//! ## Os booleanos vem do banco
//!
//! Toda rota deste recurso le' o registro do SQLite, entao `compressed` e
//! `protected` saem como `0`/`1` — nunca `true`/`false`. E' o mesmo ACHADO 3 que
//! [`crate::views::connections`] documenta; aqui nao ha' o caso "em memoria",
//! porque nenhuma resposta devolve um backup recem-construido sem recarregar.

use serde::Serialize;

use crate::models::_entities::{backups, connections};
use crate::models::backup_import::{ImportedFormat, IntegrityResult};
use crate::views::connections::WireBool;

/// Conexao aninhada num item de backup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    pub id: i64,
    pub name: String,
    pub r#type: String,
    pub host: String,
}

impl From<&connections::Model> for ConnectionSummary {
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
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub connection_id: Option<i64>,
    pub connection_database_id: Option<i64>,
    pub database_name: String,
    pub storage_destination_id: Option<i64>,
    pub status: String,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_size: Option<i64>,
    pub checksum: Option<String>,
    pub compressed: WireBool,
    pub retention_type: String,
    pub protected: WireBool,
    pub started_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub finished_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub duration_seconds: Option<i64>,
    pub error_message: Option<String>,
    pub exit_code: Option<i64>,
    /// JSON ja' decodificado. A coluna guarda texto, mas o `consume` do Lucid
    /// entrega um objeto — emitir a string crua faria o frontend precisar de um
    /// `JSON.parse` que ele nao faz.
    pub metadata: serde_json::Value,
    pub trigger: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
    /// Tres estados, e nao dois.
    ///
    /// A chave **falta** onde o controller nao faz `preload('connection')`
    /// (`GET /api/connections/:id/backups`); vale `null` onde ha' preload e o
    /// backup ficou orfao pelo `SET NULL` da FK; e traz o objeto no caso comum.
    /// Um `Option` simples colapsaria os dois primeiros, e o matcher da suite de
    /// contrato reprova tanto chave a mais quanto chave a menos.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<Option<ConnectionSummary>>,
}

impl Item {
    /// Item sem a conexao aninhada — o shape de
    /// `GET /api/connections/:id/backups`.
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
            compressed: WireBool::from_database(row.compressed.unwrap_or(false)),
            retention_type: row.retention_type.clone(),
            protected: WireBool::from_database(row.protected.unwrap_or(false)),
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

    /// O mesmo item com a relacao **carregada**.
    ///
    /// Chamar com `None` emite `"connection": null`, e nao a chave ausente: o
    /// Lucid inclui a relacao carregada mesmo quando ela nao existe, e o
    /// frontend testa `backup.connection?.name`.
    #[must_use]
    pub fn with_connection(mut self, connection: Option<&connections::Model>) -> Self {
        self.connection = Some(connection.map(ConnectionSummary::from));
        self
    }
}

/// `data` de `POST /api/backups/:id/restore`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreAccepted {
    pub restore_id: String,
    pub database_name: String,
}

/// `data` de `POST /api/backups/import`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub backup: Item,
    pub format: ImportedFormat,
    pub checksum: String,
    pub file_size: i64,
    /// `null` quando a verificacao nao foi pedida — o Adonis emite a chave.
    pub integrity: Option<IntegrityResult>,
}

/// Um backup dentro da resposta de `POST /api/connections/:id/backup`.
///
/// Duas formas, e nao uma: o caminho de sucesso traz o tamanho e a duracao
/// **formatados** para exibicao, e o de falha parcial troca os dois pelo motivo
/// da falha. Um shape unico com campos opcionais emitiria `null` onde o Adonis
/// omite a chave.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ConnectionBackupItem {
    Succeeded {
        #[serde(rename = "databaseName")]
        database_name: String,
        #[serde(rename = "backupId")]
        backup_id: i64,
        #[serde(rename = "fileName")]
        file_name: Option<String>,
        /// Texto legivel (`1.50 MB`), nao o numero — e' o que o Adonis devolve.
        #[serde(rename = "fileSize")]
        file_size: String,
        duration: String,
    },
    Failed {
        #[serde(rename = "databaseName")]
        database_name: String,
        #[serde(rename = "backupId")]
        backup_id: i64,
        success: bool,
        error: Option<String>,
    },
}

/// `data` de `POST /api/connections/:id/backup`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionBackupResult {
    pub total_databases: usize,
    pub successful: usize,
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
    fn the_booleans_come_from_the_database_as_numbers() {
        // Toda rota deste recurso le' o registro do SQLite; `true` aqui
        // quebraria todo cliente que compare com `===` (ACHADO 3).
        let json = serde_json::to_value(Item::new(&row())).expect("serializa");

        assert_eq!(json["compressed"], 1);
        assert_eq!(json["protected"], 0);
    }

    #[test]
    fn the_metadata_is_decoded_not_a_raw_string() {
        // A coluna guarda texto; o `consume` do Lucid entrega objeto, e o
        // frontend nao faz `JSON.parse`.
        let json = serde_json::to_value(Item::new(&row())).expect("serializa");

        assert_eq!(json["metadata"]["isImported"], true);
    }

    #[test]
    fn the_connection_is_absent_unless_it_was_preloaded() {
        let plain = serde_json::to_value(Item::new(&row())).expect("serializa");
        assert!(plain.get("connection").is_none());

        let with = serde_json::to_value(Item::new(&row()).with_connection(Some(&connection())))
            .expect("serializa");
        assert_eq!(with["connection"]["name"], "Producao");
        assert_eq!(with["connection"]["host"], "db.local");
    }

    #[test]
    fn an_orphaned_backup_reports_a_null_connection_not_a_missing_key() {
        // O `SET NULL` da FK deixa backups sem conexao. Onde houve preload, o
        // Lucid emite `null`; omitir a chave seria uma diferenca de shape, e
        // inventar um objeto com id 0 faria a interface exibir um link morto.
        let json =
            serde_json::to_value(Item::new(&row()).with_connection(None)).expect("serializa");

        assert!(json["connection"].is_null());
        assert!(json
            .as_object()
            .is_some_and(|map| map.contains_key("connection")));
    }

    #[test]
    fn the_nested_connection_has_only_the_selected_fields() {
        // `database` nao entra: nao e' coluna de `connections`, e o Lucid omite
        // o campo inexistente.
        let json = serde_json::to_value(ConnectionSummary::from(&connection())).expect("serializa");

        assert_eq!(
            json.as_object().map(serde_json::Map::len),
            Some(4),
            "campo a mais ou a menos na conexao aninhada: {json}"
        );
        assert!(json.get("password").is_none());
        assert!(json.get("passwordEncrypted").is_none());
    }

    #[test]
    fn a_successful_backup_item_reports_formatted_size_and_duration() {
        let json = serde_json::to_value(ConnectionBackupItem::Succeeded {
            database_name: "vendas".to_string(),
            backup_id: 7,
            file_name: Some("vendas.sql.gz".to_string()),
            file_size: "1.50 MB".to_string(),
            duration: "12s".to_string(),
        })
        .expect("serializa");

        // Texto, nao numero: e' o `getFormattedSize()` do Adonis.
        assert_eq!(json["fileSize"], "1.50 MB");
        assert!(json.get("success").is_none());
    }

    #[test]
    fn a_failed_backup_item_reports_the_reason_instead() {
        let json = serde_json::to_value(ConnectionBackupItem::Failed {
            database_name: "vendas".to_string(),
            backup_id: 7,
            success: false,
            error: Some("Access denied".to_string()),
        })
        .expect("serializa");

        assert_eq!(json["error"], "Access denied");
        // As duas formas nao compartilham chaves de exibicao.
        assert!(json.get("fileSize").is_none());
        assert!(json.get("duration").is_none());
    }
}
