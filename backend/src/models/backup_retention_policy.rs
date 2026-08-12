//! Politica de retencao de backups (tarefa 11.4).
//!
//! A politica GFS e' armazenada em `system_settings` como JSON; este modulo
//! le, normaliza, valida o cron e salva, mantendo os defaults do Adonis.

use std::str::FromStr;

use loco_rs::prelude::*;
use sea_orm::ActiveValue::Set;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::models::system_settings;

pub const SETTING_NAME: &str = "backup_retention_policy";

const DEFAULT_DAILY: u32 = 7;
const DEFAULT_WEEKLY: u32 = 4;
const DEFAULT_MONTHLY: u32 = 12;
const DEFAULT_YEARLY: u32 = 5;
const DEFAULT_PRUNE_CRON: &str = "0 2 * * *";

/// Politica de retencao, como exposta pela API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRetentionPolicy {
    pub daily: u32,
    pub weekly: u32,
    pub monthly: u32,
    pub yearly: u32,
    pub prune_cron: String,
}

impl BackupRetentionPolicy {
    pub fn default_policy() -> Self {
        Self {
            daily: DEFAULT_DAILY,
            weekly: DEFAULT_WEEKLY,
            monthly: DEFAULT_MONTHLY,
            yearly: DEFAULT_YEARLY,
            prune_cron: DEFAULT_PRUNE_CRON.into(),
        }
    }

    pub fn into_config(self) -> crate::models::backup_retention_planner::BackupRetentionConfig {
        crate::models::backup_retention_planner::BackupRetentionConfig {
            daily: self.daily,
            weekly: self.weekly,
            monthly: self.monthly,
            yearly: self.yearly,
        }
    }
}

/// Alteracoes identificadas entre politicas, para auditoria.
pub type PolicyChanges = std::collections::HashMap<String, PolicyChange>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyChange {
    pub from: serde_json::Value,
    pub to: serde_json::Value,
}

/// Payload de entrada para `PUT /api/system/backup-retention`.
///
/// Usa `Option<i64>` para poder distinguir campo ausente de zero, que o VineJS
/// trata como `required`.
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateBackupRetentionPolicy {
    pub daily: Option<i64>,
    pub weekly: Option<i64>,
    pub monthly: Option<i64>,
    pub yearly: Option<i64>,
    #[serde(rename = "pruneCron")]
    pub prune_cron: Option<String>,
}

impl Validate for UpdateBackupRetentionPolicy {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        use crate::models::validation;

        let mut errors = validator::ValidationErrors::new();
        validation::required_number(&mut errors, "daily", self.daily, i64::MAX);
        validation::required_number(&mut errors, "weekly", self.weekly, i64::MAX);
        validation::required_number(&mut errors, "monthly", self.monthly, i64::MAX);
        validation::required_number(&mut errors, "yearly", self.yearly, i64::MAX);
        validation::required_str(&mut errors, "pruneCron", self.prune_cron.as_deref(), 1, 256);
        validation::finish(errors)
    }
}

impl UpdateBackupRetentionPolicy {
    /// Converte para a politica normalizada, preenchendo defaults quando o
    /// payload omitir um campo. A validacao ja' garantiu que nenhum campo
    /// obrigatorio esta' ausente.
    pub fn into_policy(self) -> BackupRetentionPolicy {
        let default = BackupRetentionPolicy::default_policy();
        BackupRetentionPolicy {
            daily: self.daily.map(|n| n as u32).unwrap_or(default.daily),
            weekly: self.weekly.map(|n| n as u32).unwrap_or(default.weekly),
            monthly: self.monthly.map(|n| n as u32).unwrap_or(default.monthly),
            yearly: self.yearly.map(|n| n as u32).unwrap_or(default.yearly),
            prune_cron: self.prune_cron.unwrap_or(default.prune_cron),
        }
    }
}

/// Le a politica atual, criando-a com defaults se necessario.
pub async fn get_policy(ctx: &AppContext) -> Result<BackupRetentionPolicy> {
    let setting = system_settings::Entity::find()
        .filter(system_settings::Column::Name.eq(SETTING_NAME))
        .one(&ctx.db)
        .await?;

    match setting {
        Some(row) => Ok(normalize_policy(&row.value)),
        None => {
            let policy = BackupRetentionPolicy::default_policy();
            upsert(ctx, &policy).await?;
            Ok(policy)
        }
    }
}

/// Atualiza a politica e retorna a politica resultante com as mudancas.
pub async fn update_policy(
    ctx: &AppContext,
    payload: BackupRetentionPolicy,
) -> Result<(BackupRetentionPolicy, PolicyChanges)> {
    let current = get_policy(ctx).await?;
    let next = normalize_policy(&serde_json::to_string(&payload).unwrap_or_default());

    let changes = build_changes(&current, &next);
    upsert(ctx, &next).await?;

    Ok((next, changes))
}

/// Valida uma expressao cron.
///
/// O Adonis expoe cron de 5 campos (`min hora dom mes dow`); o crate `cron`
/// espera 6 (`seg min hora dom mes dow`). Normalizamos adicionando `0` no
/// inicio quando a entrada tem exatamente 5 campos.
pub fn is_valid_cron(expression: &str) -> bool {
    let normalized = match normalize_cron_fields(expression.trim()) {
        Some(value) => value,
        None => return false,
    };
    cron::Schedule::from_str(&normalized).is_ok()
}

fn normalize_cron_fields(expression: &str) -> Option<String> {
    let fields: Vec<&str> = expression.split_whitespace().collect();

    match fields.len() {
        5 => Some(format!("0 {}", expression)),
        6 | 7 => Some(expression.to_string()),
        _ => None,
    }
}

fn normalize_policy(raw: &str) -> BackupRetentionPolicy {
    let default = BackupRetentionPolicy::default_policy();
    let parsed: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);

    BackupRetentionPolicy {
        daily: normalize_count(parsed.get("daily"), default.daily),
        weekly: normalize_count(parsed.get("weekly"), default.weekly),
        monthly: normalize_count(parsed.get("monthly"), default.monthly),
        yearly: normalize_count(parsed.get("yearly"), default.yearly),
        prune_cron: normalize_cron(parsed.get("pruneCron"), &default.prune_cron),
    }
}

fn normalize_count(value: Option<&serde_json::Value>, fallback: u32) -> u32 {
    value
        .and_then(|v| v.as_u64())
        .map(|n| n as u32)
        .unwrap_or(fallback)
}

fn normalize_cron(value: Option<&serde_json::Value>, fallback: &str) -> String {
    let expression = value
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);

    if is_valid_cron(expression) {
        expression.to_string()
    } else {
        fallback.to_string()
    }
}

async fn upsert(ctx: &AppContext, policy: &BackupRetentionPolicy) -> Result<()> {
    let value = serde_json::to_string(policy)
        .map_err(|err| Error::Message(format!("falha ao serializar politica: {err}")))?;
    let now = chrono::Utc::now().fixed_offset();

    let existing = system_settings::Entity::find()
        .filter(system_settings::Column::Name.eq(SETTING_NAME))
        .one(&ctx.db)
        .await?;

    match existing {
        Some(row) => {
            let mut active: system_settings::ActiveModel = row.into();
            active.value = Set(value);
            active.updated_at = Set(now);
            active.update(&ctx.db).await?;
        }
        None => {
            let active = system_settings::ActiveModel {
                name: Set(SETTING_NAME.into()),
                value: Set(value),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            };
            active.insert(&ctx.db).await?;
        }
    }

    Ok(())
}

fn build_changes(
    previous: &BackupRetentionPolicy,
    current: &BackupRetentionPolicy,
) -> PolicyChanges {
    let mut changes = PolicyChanges::new();

    if previous.daily != current.daily {
        changes.insert(
            "daily".into(),
            PolicyChange {
                from: previous.daily.into(),
                to: current.daily.into(),
            },
        );
    }
    if previous.weekly != current.weekly {
        changes.insert(
            "weekly".into(),
            PolicyChange {
                from: previous.weekly.into(),
                to: current.weekly.into(),
            },
        );
    }
    if previous.monthly != current.monthly {
        changes.insert(
            "monthly".into(),
            PolicyChange {
                from: previous.monthly.into(),
                to: current.monthly.into(),
            },
        );
    }
    if previous.yearly != current.yearly {
        changes.insert(
            "yearly".into(),
            PolicyChange {
                from: previous.yearly.into(),
                to: current.yearly.into(),
            },
        );
    }
    if previous.prune_cron != current.prune_cron {
        changes.insert(
            "pruneCron".into(),
            PolicyChange {
                from: previous.prune_cron.clone().into(),
                to: current.prune_cron.clone().into(),
            },
        );
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_valid_cron() {
        assert!(is_valid_cron("0 2 * * *"));
        assert!(is_valid_cron("*/5 * * * *"));
    }

    #[test]
    fn rejects_invalid_cron() {
        assert!(!is_valid_cron("not a cron"));
        assert!(!is_valid_cron(""));
    }

    #[test]
    fn normalize_fills_missing_fields_with_defaults() {
        let raw = r#"{"daily": 3}"#;
        let policy = normalize_policy(raw);

        assert_eq!(policy.daily, 3);
        assert_eq!(policy.weekly, DEFAULT_WEEKLY);
        assert_eq!(policy.monthly, DEFAULT_MONTHLY);
        assert_eq!(policy.yearly, DEFAULT_YEARLY);
        assert_eq!(policy.prune_cron, DEFAULT_PRUNE_CRON);
    }

    #[test]
    fn normalize_falls_back_for_invalid_cron() {
        let raw = r#"{"pruneCron": "invalid"}"#;
        let policy = normalize_policy(raw);

        assert_eq!(policy.prune_cron, DEFAULT_PRUNE_CRON);
    }

    #[test]
    fn changes_identify_differences() {
        let previous = BackupRetentionPolicy::default_policy();
        let mut current = previous.clone();
        current.daily = 5;
        current.prune_cron = "0 3 * * *".into();

        let changes = build_changes(&previous, &current);
        assert_eq!(changes.len(), 2);
        assert!(changes.contains_key("daily"));
        assert!(changes.contains_key("pruneCron"));
    }
}
