//! Logica de dominio de `connections` (tarefa 4.7 do roadmap).
//!
//! A senha do banco de origem vive aqui, cifrada em AES-256-GCM (D3). Duas
//! regras que este arquivo existe para sustentar:
//!
//! 1. o valor **em claro** nunca entra numa struct que derive `Serialize` —
//!    o unico caminho para ele e' [`Model::decrypted_password`], que exige o
//!    servico de criptografia explicitamente;
//! 2. a coluna se chama `password_encrypted` e e' o que a entidade expoe. Um
//!    campo `password` cru sequer existe, para que nao haja como serializa-lo
//!    por engano.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub use super::_entities::connections::{ActiveModel, Column, Entity, Model};
use crate::models::encryption::{EncryptionError, EncryptionService};

impl ActiveModelBehavior for ActiveModel {}

/// Motores suportados, com os mesmos valores da coluna `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    Mysql,
    Mariadb,
    Postgresql,
}

impl DatabaseType {
    pub const ALL: [Self; 3] = [Self::Mysql, Self::Mariadb, Self::Postgresql];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Mariadb => "mariadb",
            Self::Postgresql => "postgresql",
        }
    }

    /// Porta padrao do motor.
    ///
    /// MariaDB usa a mesma porta do MySQL — nao e' engano de copia: o
    /// protocolo e' o mesmo e a porta registrada tambem.
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Mysql | Self::Mariadb => 3306,
            Self::Postgresql => 5432,
        }
    }

    /// Binario de dump correspondente.
    ///
    /// MariaDB tambem usa `mysqldump`. O `mariadb-dump` existe nas versoes
    /// novas, mas o Adonis chama `mysqldump` e trocar isso mudaria qual
    /// binario precisa estar na imagem Docker.
    pub const fn dump_command(self) -> &'static str {
        match self {
            Self::Mysql | Self::Mariadb => "mysqldump",
            Self::Postgresql => "pg_dump",
        }
    }
}

impl FromStr for DatabaseType {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Frequencias de agendamento aceitas pela coluna `schedule_frequency`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleFrequency {
    #[serde(rename = "1h")]
    Hourly,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "12h")]
    TwelveHours,
    #[serde(rename = "24h")]
    Daily,
}

impl ScheduleFrequency {
    pub const ALL: [Self; 4] = [Self::Hourly, Self::SixHours, Self::TwelveHours, Self::Daily];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "1h",
            Self::SixHours => "6h",
            Self::TwelveHours => "12h",
            Self::Daily => "24h",
        }
    }

    /// Intervalo em milissegundos, como o `getScheduleIntervalMs` do Adonis.
    pub const fn interval_ms(self) -> i64 {
        match self {
            Self::Hourly => 60 * 60 * 1000,
            Self::SixHours => 6 * 60 * 60 * 1000,
            Self::TwelveHours => 12 * 60 * 60 * 1000,
            Self::Daily => 24 * 60 * 60 * 1000,
        }
    }
}

impl FromStr for ScheduleFrequency {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Estado da conexao, atualizado por `POST /api/connections/:id/test`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Active,
    Inactive,
    Error,
}

impl ConnectionStatus {
    pub const ALL: [Self; 3] = [Self::Active, Self::Inactive, Self::Error];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Error => "error",
        }
    }
}

impl FromStr for ConnectionStatus {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("valor desconhecido: {0}")]
pub struct UnknownValue(pub String);

impl Model {
    /// Tipo do banco como enum. `Err` quando a coluna tem lixo.
    pub fn database_type(&self) -> std::result::Result<DatabaseType, UnknownValue> {
        self.r#type.parse()
    }

    pub fn schedule(&self) -> Option<ScheduleFrequency> {
        self.schedule_frequency
            .as_deref()
            .and_then(|value| value.parse().ok())
    }

    /// Intervalo do agendamento, ou `None` quando nao ha' frequencia definida.
    pub fn schedule_interval_ms(&self) -> Option<i64> {
        self.schedule().map(ScheduleFrequency::interval_ms)
    }

    /// Senha em claro.
    ///
    /// Exige o servico de criptografia como argumento de proposito: sem ele
    /// nao ha' como obter o valor, e uma chamada acidental fica visivel na
    /// revisao. Devolve string vazia quando a conexao nao tem senha — e' o que
    /// o `getDecryptedPassword` do Adonis faz, e algumas conexoes locais de
    /// fato nao tem.
    pub fn decrypted_password(
        &self,
        encryption: &EncryptionService,
    ) -> std::result::Result<String, EncryptionError> {
        if self.password_encrypted.is_empty() {
            return Ok(String::new());
        }
        encryption.decrypt(&self.password_encrypted)
    }

    /// Binario de dump deste motor.
    pub fn dump_command(&self) -> std::result::Result<&'static str, UnknownValue> {
        Ok(self.database_type()?.dump_command())
    }

    /// Argumentos de SSL para os clientes MySQL/MariaDB.
    ///
    /// SSL fica **desligado** salvo pedido explicito em `options.ssl`. Ligar
    /// por padrao quebraria toda conexao com servidor sem TLS configurado, que
    /// e' o caso comum de um banco interno.
    pub fn mysql_ssl_args(&self) -> Vec<&'static str> {
        let enabled = self
            .options
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| value.get("ssl").and_then(serde_json::Value::as_bool))
            .unwrap_or(false);

        if enabled {
            vec![]
        } else {
            vec!["--skip-ssl"]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mariadb_shares_the_mysql_port_and_dump_binary() {
        assert_eq!(DatabaseType::Mariadb.default_port(), 3306);
        assert_eq!(DatabaseType::Mysql.default_port(), 3306);
        assert_eq!(DatabaseType::Mariadb.dump_command(), "mysqldump");
    }

    #[test]
    fn postgres_has_its_own_port_and_binary() {
        assert_eq!(DatabaseType::Postgresql.default_port(), 5432);
        assert_eq!(DatabaseType::Postgresql.dump_command(), "pg_dump");
    }

    #[test]
    fn schedule_intervals_match_the_adonis_table() {
        assert_eq!(ScheduleFrequency::Hourly.interval_ms(), 3_600_000);
        assert_eq!(ScheduleFrequency::SixHours.interval_ms(), 21_600_000);
        assert_eq!(ScheduleFrequency::TwelveHours.interval_ms(), 43_200_000);
        assert_eq!(ScheduleFrequency::Daily.interval_ms(), 86_400_000);
    }

    #[test]
    fn enum_values_round_trip_through_the_column_representation() {
        for value in DatabaseType::ALL {
            assert_eq!(value.as_str().parse::<DatabaseType>().unwrap(), value);
        }
        for value in ScheduleFrequency::ALL {
            assert_eq!(value.as_str().parse::<ScheduleFrequency>().unwrap(), value);
        }
        for value in ConnectionStatus::ALL {
            assert_eq!(value.as_str().parse::<ConnectionStatus>().unwrap(), value);
        }
    }

    #[test]
    fn rejects_values_outside_the_check_constraint() {
        // O `CHECK` do banco recusaria; o parse tem que recusar tambem, senao
        // um valor invalido so' apareceria no `INSERT`.
        assert!("oracle".parse::<DatabaseType>().is_err());
        assert!("30m".parse::<ScheduleFrequency>().is_err());
        assert!("paused".parse::<ConnectionStatus>().is_err());
    }

    fn model_with(options: Option<&str>) -> Model {
        Model {
            id: 1,
            name: "teste".into(),
            r#type: "mysql".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            username: "root".into(),
            password_encrypted: String::new(),
            schedule_frequency: None,
            schedule_enabled: None,
            status: None,
            last_error: None,
            last_tested_at: None,
            last_backup_at: None,
            options: options.map(ToString::to_string),
            created_at: chrono::Utc::now().naive_utc(),
            updated_at: chrono::Utc::now().naive_utc(),
            storage_destination_id: None,
        }
    }

    #[test]
    fn ssl_is_off_unless_explicitly_enabled() {
        // Ligar SSL por padrao quebraria toda conexao com banco interno sem
        // TLS — que e' a maioria.
        assert_eq!(model_with(None).mysql_ssl_args(), vec!["--skip-ssl"]);
        assert_eq!(
            model_with(Some(r#"{"charset":"utf8"}"#)).mysql_ssl_args(),
            vec!["--skip-ssl"]
        );
        assert_eq!(
            model_with(Some(r#"{"ssl":false}"#)).mysql_ssl_args(),
            vec!["--skip-ssl"]
        );
    }

    #[test]
    fn ssl_is_on_when_requested() {
        assert!(model_with(Some(r#"{"ssl":true}"#))
            .mysql_ssl_args()
            .is_empty());
    }

    #[test]
    fn malformed_options_do_not_enable_ssl_by_accident() {
        // JSON quebrado tem que cair no default seguro, e nao propagar erro
        // nem ligar SSL.
        assert_eq!(
            model_with(Some("isso nao e json")).mysql_ssl_args(),
            vec!["--skip-ssl"]
        );
    }

    #[test]
    fn an_empty_password_decrypts_to_an_empty_string() {
        let service = EncryptionService::from_hex_key(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();

        assert_eq!(model_with(None).decrypted_password(&service).unwrap(), "");
    }
}
