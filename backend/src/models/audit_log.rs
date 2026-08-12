//! Auditoria.
//!
//! O que esta' aqui e' a parte que **nao** depende do banco: os enums e as
//! tabelas de traducao. `actionDescription`, `actionIcon` e `statusColor` sao
//! derivados no controller da implementacao anterior a partir da `action`, e nunca gravados —
//! entao o backend precisa das mesmas tabelas ou a interface fica sem
//! rotulo e sem icone.
//!
//! A persistencia (`AuditService.log`) entra na Fase 4, junto com a entidade
//! Sea-ORM de `audit_logs`. Separar as duas coisas permite travar agora, com
//! teste, os textos exatos que a suite de contrato ja' fixou nos goldens.
//!
//! As strings estao em portugues com acento porque e' assim que estao no
//! implementacao anterior e e' assim que o frontend as exibe. Mudar acentuacao aqui quebraria
//! o `actionDescription` gravado em `audit-logs/show.json`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Acoes auditaveis, com o mesmo valor de string do `AuditAction` da implementacao anterior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuditAction {
    #[serde(rename = "connection.created")]
    ConnectionCreated,
    #[serde(rename = "connection.updated")]
    ConnectionUpdated,
    #[serde(rename = "connection.deleted")]
    ConnectionDeleted,
    #[serde(rename = "connection.tested")]
    ConnectionTested,
    #[serde(rename = "backup.started")]
    BackupStarted,
    #[serde(rename = "backup.completed")]
    BackupCompleted,
    #[serde(rename = "backup.failed")]
    BackupFailed,
    #[serde(rename = "backup.deleted")]
    BackupDeleted,
    #[serde(rename = "backup.downloaded")]
    BackupDownloaded,
    #[serde(rename = "backup.imported")]
    BackupImported,
    #[serde(rename = "settings.updated")]
    SettingsUpdated,
    #[serde(rename = "diagnostics.downloaded")]
    DiagnosticsDownloaded,
    #[serde(rename = "diagnostics.deleted")]
    DiagnosticsDeleted,
}

impl AuditAction {
    /// Todas as acoes, na ordem em que aparecem no model da implementacao anterior.
    pub const ALL: [Self; 13] = [
        Self::ConnectionCreated,
        Self::ConnectionUpdated,
        Self::ConnectionDeleted,
        Self::ConnectionTested,
        Self::BackupStarted,
        Self::BackupCompleted,
        Self::BackupFailed,
        Self::BackupDeleted,
        Self::BackupDownloaded,
        Self::BackupImported,
        Self::SettingsUpdated,
        Self::DiagnosticsDownloaded,
        Self::DiagnosticsDeleted,
    ];

    /// Valor gravado na coluna `action`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConnectionCreated => "connection.created",
            Self::ConnectionUpdated => "connection.updated",
            Self::ConnectionDeleted => "connection.deleted",
            Self::ConnectionTested => "connection.tested",
            Self::BackupStarted => "backup.started",
            Self::BackupCompleted => "backup.completed",
            Self::BackupFailed => "backup.failed",
            Self::BackupDeleted => "backup.deleted",
            Self::BackupDownloaded => "backup.downloaded",
            Self::BackupImported => "backup.imported",
            Self::SettingsUpdated => "settings.updated",
            Self::DiagnosticsDownloaded => "diagnostics.downloaded",
            Self::DiagnosticsDeleted => "diagnostics.deleted",
        }
    }

    /// `actionDescription` — derivado, nunca gravado.
    pub const fn description(self) -> &'static str {
        match self {
            Self::ConnectionCreated => "Conexão criada",
            Self::ConnectionUpdated => "Conexão atualizada",
            Self::ConnectionDeleted => "Conexão removida",
            Self::ConnectionTested => "Conexão testada",
            Self::BackupStarted => "Backup iniciado",
            Self::BackupCompleted => "Backup concluído",
            Self::BackupFailed => "Backup falhou",
            Self::BackupDeleted => "Backup removido",
            Self::BackupDownloaded => "Backup baixado",
            Self::BackupImported => "Backup importado",
            Self::SettingsUpdated => "Configurações atualizadas",
            Self::DiagnosticsDownloaded => "Artefato de diagnóstico baixado",
            Self::DiagnosticsDeleted => "Artefato de diagnóstico removido",
        }
    }

    /// `actionIcon` — nomes do Material Design Icons que o frontend usa.
    pub const fn icon(self) -> &'static str {
        match self {
            Self::ConnectionCreated => "mdi-database-plus",
            Self::ConnectionUpdated => "mdi-database-edit",
            Self::ConnectionDeleted => "mdi-database-remove",
            Self::ConnectionTested => "mdi-database-check",
            Self::BackupStarted => "mdi-play-circle",
            Self::BackupCompleted => "mdi-check-circle",
            Self::BackupFailed => "mdi-alert-circle",
            Self::BackupDeleted => "mdi-delete",
            Self::BackupDownloaded => "mdi-download",
            Self::BackupImported => "mdi-database-import",
            Self::SettingsUpdated => "mdi-cog",
            Self::DiagnosticsDownloaded => "mdi-stethoscope",
            Self::DiagnosticsDeleted => "mdi-delete-sweep",
        }
    }

    /// Entidade a que a acao se refere.
    pub const fn entity_type(self) -> AuditEntityType {
        match self {
            Self::ConnectionCreated
            | Self::ConnectionUpdated
            | Self::ConnectionDeleted
            | Self::ConnectionTested => AuditEntityType::Connection,
            Self::BackupStarted
            | Self::BackupCompleted
            | Self::BackupFailed
            | Self::BackupDeleted
            | Self::BackupDownloaded
            | Self::BackupImported => AuditEntityType::Backup,
            Self::SettingsUpdated => AuditEntityType::Settings,
            Self::DiagnosticsDownloaded | Self::DiagnosticsDeleted => AuditEntityType::Diagnostics,
        }
    }
}

impl fmt::Display for AuditAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AuditAction {
    type Err = UnknownAuditValue;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|action| action.as_str() == input)
            .ok_or_else(|| UnknownAuditValue(input.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditEntityType {
    Connection,
    Backup,
    Settings,
    Diagnostics,
}

impl AuditEntityType {
    pub const ALL: [Self; 4] = [
        Self::Connection,
        Self::Backup,
        Self::Settings,
        Self::Diagnostics,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connection => "connection",
            Self::Backup => "backup",
            Self::Settings => "settings",
            Self::Diagnostics => "diagnostics",
        }
    }
}

impl fmt::Display for AuditEntityType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AuditEntityType {
    type Err = UnknownAuditValue;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|entity| entity.as_str() == input)
            .ok_or_else(|| UnknownAuditValue(input.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditStatus {
    Success,
    Failure,
    Warning,
}

impl AuditStatus {
    pub const ALL: [Self; 3] = [Self::Success, Self::Failure, Self::Warning];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Warning => "warning",
        }
    }

    /// `statusColor`. Repare que `failure` vira **`error`**, e nao `failure` —
    /// e' o nome da cor no tema do frontend, nao o do status.
    pub const fn color(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "error",
            Self::Warning => "warning",
        }
    }
}

impl fmt::Display for AuditStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AuditStatus {
    type Err = UnknownAuditValue;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == input)
            .ok_or_else(|| UnknownAuditValue(input.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("valor de auditoria desconhecido: {0}")]
pub struct UnknownAuditValue(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_strings_match_the_database_values() {
        // Estes valores estao gravados no banco de producao. Renomear um deles
        // faria os registros antigos deixarem de casar com os filtros.
        assert_eq!(
            AuditAction::ConnectionCreated.as_str(),
            "connection.created"
        );
        assert_eq!(AuditAction::BackupCompleted.as_str(), "backup.completed");
        assert_eq!(
            AuditAction::DiagnosticsDeleted.as_str(),
            "diagnostics.deleted"
        );
    }

    #[test]
    fn descriptions_match_the_recorded_goldens() {
        // `audit-logs/show.json` gravou exatamente estes textos, com acento.
        assert_eq!(
            AuditAction::ConnectionCreated.description(),
            "Conexão criada"
        );
        assert_eq!(
            AuditAction::BackupCompleted.description(),
            "Backup concluído"
        );
        assert_eq!(
            AuditAction::SettingsUpdated.description(),
            "Configurações atualizadas"
        );
    }

    #[test]
    fn icons_match_the_recorded_goldens() {
        assert_eq!(AuditAction::ConnectionCreated.icon(), "mdi-database-plus");
        assert_eq!(AuditAction::BackupFailed.icon(), "mdi-alert-circle");
    }

    #[test]
    fn failure_maps_to_the_error_color() {
        // O ponto que mais convida ao engano: o status e' `failure`, a cor e'
        // `error`. Copiar o status como cor deixaria o frontend sem estilo.
        assert_eq!(AuditStatus::Failure.color(), "error");
        assert_eq!(AuditStatus::Success.color(), "success");
        assert_eq!(AuditStatus::Warning.color(), "warning");
    }

    #[test]
    fn every_action_has_description_and_icon() {
        // Uma acao nova sem rotulo apareceria na interface como um item mudo.
        for action in AuditAction::ALL {
            assert!(!action.description().is_empty(), "{action} sem descricao");
            assert!(action.icon().starts_with("mdi-"), "{action} sem icone");
        }
    }

    #[test]
    fn descriptions_and_icons_are_unique() {
        // Duas acoes com o mesmo icone ou o mesmo texto seriam
        // indistinguiveis na tela de auditoria — e o erro so' apareceria para
        // quem estivesse investigando um incidente.
        let descriptions: std::collections::HashSet<_> =
            AuditAction::ALL.iter().map(|a| a.description()).collect();
        let icons: std::collections::HashSet<_> =
            AuditAction::ALL.iter().map(|a| a.icon()).collect();

        assert_eq!(descriptions.len(), AuditAction::ALL.len());
        assert_eq!(icons.len(), AuditAction::ALL.len());
    }

    #[test]
    fn action_implies_entity_type() {
        assert_eq!(
            AuditAction::ConnectionTested.entity_type(),
            AuditEntityType::Connection
        );
        assert_eq!(
            AuditAction::BackupImported.entity_type(),
            AuditEntityType::Backup
        );
        assert_eq!(
            AuditAction::DiagnosticsDeleted.entity_type(),
            AuditEntityType::Diagnostics
        );
    }

    #[test]
    fn round_trips_through_strings() {
        for action in AuditAction::ALL {
            assert_eq!(action.as_str().parse::<AuditAction>().unwrap(), action);
        }
        for entity in AuditEntityType::ALL {
            assert_eq!(entity.as_str().parse::<AuditEntityType>().unwrap(), entity);
        }
        for status in AuditStatus::ALL {
            assert_eq!(status.as_str().parse::<AuditStatus>().unwrap(), status);
        }
    }

    #[test]
    fn rejects_unknown_values_instead_of_guessing() {
        assert!("connection.exploded".parse::<AuditAction>().is_err());
        assert!("nave".parse::<AuditEntityType>().is_err());
        assert!("ok".parse::<AuditStatus>().is_err());
    }

    #[test]
    fn serializes_with_the_database_representation() {
        assert_eq!(
            serde_json::to_string(&AuditAction::ConnectionCreated).unwrap(),
            r#""connection.created""#
        );
        assert_eq!(
            serde_json::to_string(&AuditStatus::Failure).unwrap(),
            r#""failure""#
        );
    }
}
