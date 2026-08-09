//! Logica de dominio de `backups` (tarefa 4.7 do roadmap).

use loco_rs::prelude::ConnectionTrait;
use sea_orm::entity::prelude::*;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};

use sea_orm::ActiveValue::Set;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub use super::_entities::backups::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl BackupStatus {
    pub const ALL: [Self; 5] = [
        Self::Pending,
        Self::Running,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl FromStr for BackupStatus {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

/// Niveis da politica GFS, **em ordem crescente de importancia**.
///
/// A ordem nao e' decorativa: e' ela que define o que
/// [`RetentionType::promote`] considera promocao.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RetentionType {
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl RetentionType {
    pub const ALL: [Self; 5] = [
        Self::Hourly,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    /// Promove para `target`, **so' para cima**.
    ///
    /// Rebaixar seria destrutivo de um jeito silencioso: um backup anual
    /// virando horario passa a ser podado na primeira execucao da retencao,
    /// e o operador so' descobre quando precisar dele.
    #[must_use]
    pub fn promote(self, target: Self) -> Self {
        if target > self {
            target
        } else {
            self
        }
    }
}

impl FromStr for RetentionType {
    type Err = UnknownValue;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|value| value.as_str() == input)
            .ok_or_else(|| UnknownValue(input.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupTrigger {
    Scheduled,
    Manual,
}

impl BackupTrigger {
    pub const ALL: [Self; 2] = [Self::Scheduled, Self::Manual];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Manual => "manual",
        }
    }
}

impl FromStr for BackupTrigger {
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

/// Tamanho legivel, no formato que a API devolve em `fileSize`.
///
/// Reproduz o `getFormattedSize` do Adonis, inclusive as duas casas decimais e
/// o `N/A` para tamanho ausente — sao strings que o frontend exibe direto.
pub fn format_size(bytes: Option<i64>) -> String {
    let Some(bytes) = bytes else {
        return "N/A".to_string();
    };

    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;

    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }

    format!("{size:.2} {}", UNITS[unit])
}

/// Duracao legivel, no formato do `getFormattedDuration` do Adonis.
///
/// As unidades zeradas a' esquerda somem: `90` vira `1m 30s`, e nao
/// `0h 1m 30s`.
pub fn format_duration(seconds: Option<i64>) -> String {
    let Some(seconds) = seconds else {
        return "N/A".to_string();
    };

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let remaining = seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {remaining}s")
    } else if minutes > 0 {
        format!("{minutes}m {remaining}s")
    } else {
        format!("{remaining}s")
    }
}

impl Model {
    pub fn status_enum(&self) -> std::result::Result<BackupStatus, UnknownValue> {
        self.status.parse()
    }

    pub fn retention(&self) -> std::result::Result<RetentionType, UnknownValue> {
        self.retention_type.parse()
    }

    pub fn formatted_size(&self) -> String {
        format_size(self.file_size)
    }

    pub fn formatted_duration(&self) -> String {
        format_duration(self.duration_seconds)
    }
}

impl ActiveModel {
    /// Marca o inicio da execucao.
    pub fn mark_as_started(&mut self, now: chrono::NaiveDateTime) {
        self.status = Set(BackupStatus::Running.as_str().to_string());
        self.started_at = Set(Some(now));
    }

    /// Marca a conclusao com sucesso e calcula a duracao.
    ///
    /// A duracao so' e' calculada quando ha' `started_at`: um backup concluido
    /// sem inicio registrado teria duracao negativa ou absurda, e um numero
    /// errado no relatorio e' pior que um campo vazio.
    pub fn mark_as_completed(
        &mut self,
        now: chrono::NaiveDateTime,
        started_at: Option<chrono::NaiveDateTime>,
        file_path: impl Into<String>,
        file_name: impl Into<String>,
        file_size: i64,
        checksum: Option<String>,
    ) {
        self.status = Set(BackupStatus::Completed.as_str().to_string());
        self.finished_at = Set(Some(now));
        self.file_path = Set(Some(file_path.into()));
        self.file_name = Set(Some(file_name.into()));
        self.file_size = Set(Some(file_size));
        self.checksum = Set(checksum);
        self.exit_code = Set(Some(0));
        self.duration_seconds = Set(started_at.map(|start| (now - start).num_seconds()));
    }

    /// Marca a falha, preservando o codigo de saida do processo de dump.
    pub fn mark_as_failed(
        &mut self,
        now: chrono::NaiveDateTime,
        started_at: Option<chrono::NaiveDateTime>,
        error_message: impl Into<String>,
        exit_code: Option<i64>,
    ) {
        self.status = Set(BackupStatus::Failed.as_str().to_string());
        self.finished_at = Set(Some(now));
        self.error_message = Set(Some(error_message.into()));
        self.exit_code = Set(exit_code);
        self.duration_seconds = Set(started_at.map(|start| (now - start).num_seconds()));
    }
}

/// Consultas agregadas que alimentam `GET /api/stats` (tarefa 5.5).
impl Model {
    pub async fn count_all(db: &impl ConnectionTrait) -> loco_rs::Result<u64> {
        Ok(Entity::find().count(db).await?)
    }

    /// Quantos backups desde `since` — o corte de "hoje" no painel.
    pub async fn count_since(
        db: &impl ConnectionTrait,
        since: chrono::NaiveDateTime,
    ) -> loco_rs::Result<u64> {
        Ok(Entity::find()
            .filter(Column::CreatedAt.gte(since))
            .count(db)
            .await?)
    }

    /// Os `limit` backups mais recentes, com o nome da conexao de cada um.
    ///
    /// Uma consulta so', com `find_also_related`: buscar o nome dentro de um
    /// laco seria uma ida ao banco por linha, e o painel e' consultado a cada
    /// atualizacao da tela.
    pub async fn recent_with_connection(
        db: &impl ConnectionTrait,
        limit: u64,
    ) -> loco_rs::Result<Vec<(Self, Option<String>)>> {
        let rows = Entity::find()
            .find_also_related(super::_entities::connections::Entity)
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .limit(limit)
            .all(db)
            .await?;

        Ok(rows
            .into_iter()
            .map(|(backup, connection)| (backup, connection.map(|row| row.name)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotes_only_upwards() {
        assert_eq!(
            RetentionType::Hourly.promote(RetentionType::Monthly),
            RetentionType::Monthly
        );
        // Rebaixar um backup anual para horario o entregaria a' proxima poda.
        assert_eq!(
            RetentionType::Yearly.promote(RetentionType::Hourly),
            RetentionType::Yearly
        );
        assert_eq!(
            RetentionType::Weekly.promote(RetentionType::Weekly),
            RetentionType::Weekly
        );
    }

    #[test]
    fn retention_levels_are_ordered_from_least_to_most_important() {
        assert!(RetentionType::Hourly < RetentionType::Daily);
        assert!(RetentionType::Daily < RetentionType::Weekly);
        assert!(RetentionType::Weekly < RetentionType::Monthly);
        assert!(RetentionType::Monthly < RetentionType::Yearly);
    }

    #[test]
    fn formats_size_like_the_adonis_helper() {
        assert_eq!(format_size(None), "N/A");
        assert_eq!(format_size(Some(0)), "0.00 B");
        assert_eq!(format_size(Some(1023)), "1023.00 B");
        assert_eq!(format_size(Some(1024)), "1.00 KB");
        assert_eq!(format_size(Some(1_048_576)), "1.00 MB");
        assert_eq!(format_size(Some(1_610_612_736)), "1.50 GB");
    }

    #[test]
    fn stops_at_terabytes_instead_of_inventing_a_unit() {
        // A tabela do Adonis termina em TB. Um valor maior tem que continuar
        // em TB, e nao virar "PB" — que o frontend nao conhece.
        assert!(format_size(Some(i64::MAX)).ends_with(" TB"));
    }

    #[test]
    fn formats_duration_dropping_leading_zero_units() {
        assert_eq!(format_duration(None), "N/A");
        assert_eq!(format_duration(Some(0)), "0s");
        assert_eq!(format_duration(Some(45)), "45s");
        assert_eq!(format_duration(Some(90)), "1m 30s");
        assert_eq!(format_duration(Some(3_600)), "1h 0m 0s");
        assert_eq!(format_duration(Some(3_661)), "1h 1m 1s");
    }

    #[test]
    fn enum_values_round_trip() {
        for value in BackupStatus::ALL {
            assert_eq!(value.as_str().parse::<BackupStatus>().unwrap(), value);
        }
        for value in RetentionType::ALL {
            assert_eq!(value.as_str().parse::<RetentionType>().unwrap(), value);
        }
        for value in BackupTrigger::ALL {
            assert_eq!(value.as_str().parse::<BackupTrigger>().unwrap(), value);
        }
    }

    #[test]
    fn rejects_values_outside_the_check_constraint() {
        assert!("archived".parse::<BackupStatus>().is_err());
        assert!("decadal".parse::<RetentionType>().is_err());
        assert!("cron".parse::<BackupTrigger>().is_err());
    }
}
