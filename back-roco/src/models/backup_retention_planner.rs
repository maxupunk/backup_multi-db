//! Planejamento deterministico da retencao GFS de backups (tarefa 11.5).
//!
//! A logica e' a mesma do `backup_retention_planner.ts` do Adonis: a partir da
//! idade do backup e da configuracao de janelas, decide quem e' mantido,
//! promovido ou apagado. Nao depende do horario em que o job roda.

use chrono::{Datelike, NaiveDateTime, Timelike};

use crate::models::backups::{BackupStatus, RetentionType};

/// Configuracao de retencao.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupRetentionConfig {
    pub daily: u32,
    pub weekly: u32,
    pub monthly: u32,
    pub yearly: u32,
}

/// Candidato ao planejamento — propositalmente enxuto.
#[derive(Debug, Clone)]
pub struct BackupRetentionCandidate {
    pub id: i64,
    pub connection_id: Option<i64>,
    pub connection_database_id: Option<i64>,
    pub database_name: String,
    pub created_at: NaiveDateTime,
    pub status: BackupStatus,
    pub retention_type: RetentionType,
}

/// Resultado do planejamento.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackupRetentionPlan {
    /// Backup ID -> tier alvo.
    pub retained: std::collections::HashMap<i64, RetentionType>,
    /// IDs a apagar.
    pub to_delete: Vec<i64>,
}

/// Planeja a retencao GFS.
#[derive(Debug, Clone, Copy)]
pub struct BackupRetentionPlanner {
    config: BackupRetentionConfig,
}

impl BackupRetentionPlanner {
    pub fn new(config: BackupRetentionConfig) -> Self {
        Self {
            config: BackupRetentionConfig {
                daily: config.daily,
                weekly: config.weekly,
                monthly: config.monthly,
                yearly: config.yearly,
            },
        }
    }

    pub fn plan(
        &self,
        backups: &[BackupRetentionCandidate],
        now: NaiveDateTime,
    ) -> BackupRetentionPlan {
        let mut plan = BackupRetentionPlan::default();
        let mut claimed_buckets = std::collections::HashSet::<String>::new();

        let mut ordered = backups.to_vec();
        ordered.sort_by_key(|b| std::cmp::Reverse(b.created_at));

        for backup in ordered {
            let tier = self.resolve_tier(backup.status, backup.created_at, now);

            if tier.is_none() && backup.status != BackupStatus::Completed {
                plan.to_delete.push(backup.id);
                continue;
            }

            let Some(tier) = tier else {
                plan.to_delete.push(backup.id);
                continue;
            };

            let bucket_key = self.bucket_key(&backup, tier);

            if claimed_buckets.contains(&bucket_key) {
                plan.to_delete.push(backup.id);
                continue;
            }

            claimed_buckets.insert(bucket_key);
            plan.retained.insert(backup.id, tier);
        }

        plan
    }

    fn resolve_tier(
        &self,
        status: BackupStatus,
        created_at: NaiveDateTime,
        now: NaiveDateTime,
    ) -> Option<RetentionType> {
        if Self::is_current_day(created_at, now) {
            return Some(RetentionType::Hourly);
        }

        if status != BackupStatus::Completed {
            return None;
        }

        if self.is_within_daily_window(created_at, now) {
            return Some(RetentionType::Daily);
        }

        if self.is_within_weekly_window(created_at, now) {
            return Some(RetentionType::Weekly);
        }

        if self.is_within_monthly_window(created_at, now) {
            return Some(RetentionType::Monthly);
        }

        if self.is_within_yearly_window(created_at, now) {
            return Some(RetentionType::Yearly);
        }

        None
    }

    fn bucket_key(&self, backup: &BackupRetentionCandidate, tier: RetentionType) -> String {
        let scope = self.scope_key(backup);

        if backup.status != BackupStatus::Completed {
            return format!("{scope}:terminal:{}", backup.id);
        }

        match tier {
            RetentionType::Hourly => format!(
                "{scope}:hourly:{:04}-{:02}-{:02}-{:02}",
                backup.created_at.year(),
                backup.created_at.month(),
                backup.created_at.day(),
                backup.created_at.hour()
            ),
            RetentionType::Daily => format!(
                "{scope}:daily:{:04}-{:02}-{:02}",
                backup.created_at.year(),
                backup.created_at.month(),
                backup.created_at.day()
            ),
            RetentionType::Weekly => {
                let iso_week = backup.created_at.iso_week();
                format!("{scope}:weekly:{}-{}", iso_week.year(), iso_week.week())
            }
            RetentionType::Monthly => format!(
                "{scope}:monthly:{:04}-{:02}",
                backup.created_at.year(),
                backup.created_at.month()
            ),
            RetentionType::Yearly => format!("{scope}:yearly:{:04}", backup.created_at.year()),
        }
    }

    fn scope_key(&self, backup: &BackupRetentionCandidate) -> String {
        if let Some(db_id) = backup.connection_database_id {
            return format!("connection-database:{db_id}");
        }

        if let Some(conn_id) = backup.connection_id {
            return format!("connection:{conn_id}:database:{}", backup.database_name);
        }

        format!("backup:{}", backup.id)
    }

    fn is_current_day(created_at: NaiveDateTime, now: NaiveDateTime) -> bool {
        created_at.date() == now.date()
    }

    fn is_within_daily_window(&self, created_at: NaiveDateTime, now: NaiveDateTime) -> bool {
        if self.config.daily == 0 {
            return false;
        }
        let start = Self::start_of_day(now - chrono::Duration::days(self.config.daily as i64));
        created_at >= start
    }

    fn is_within_weekly_window(&self, created_at: NaiveDateTime, now: NaiveDateTime) -> bool {
        if self.config.weekly == 0 {
            return false;
        }
        let start = Self::start_of_week(now - chrono::Duration::weeks(self.config.weekly as i64));
        created_at >= start
    }

    fn is_within_monthly_window(&self, created_at: NaiveDateTime, now: NaiveDateTime) -> bool {
        if self.config.monthly == 0 {
            return false;
        }
        let start = Self::start_of_month(now, self.config.monthly);
        created_at >= start
    }

    fn is_within_yearly_window(&self, created_at: NaiveDateTime, now: NaiveDateTime) -> bool {
        if self.config.yearly == 0 {
            return false;
        }
        let start = Self::start_of_year(now, self.config.yearly);
        created_at >= start
    }

    fn start_of_day(moment: NaiveDateTime) -> NaiveDateTime {
        moment.date().and_hms_opt(0, 0, 0).unwrap_or(moment)
    }

    fn start_of_week(moment: NaiveDateTime) -> NaiveDateTime {
        let weekday = moment.weekday().num_days_from_monday();
        let start_date = moment.date() - chrono::Duration::days(weekday as i64);
        start_date.and_hms_opt(0, 0, 0).unwrap_or(moment)
    }

    fn start_of_month(now: NaiveDateTime, months_back: u32) -> NaiveDateTime {
        let target = now - chrono::Duration::days(months_back as i64 * 30);
        // Aproximacao razoavel: primeiro dia do mes alvo.
        target
            .date()
            .with_day(1)
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap_or(now))
            .unwrap_or(now)
    }

    fn start_of_year(now: NaiveDateTime, years_back: u32) -> NaiveDateTime {
        let target_year = now.year() - years_back as i32;
        NaiveDateTime::new(
            chrono::NaiveDate::from_ymd_opt(target_year, 1, 1).unwrap_or_else(|| now.date()),
            chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap_or(now.time()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").expect("data de teste")
    }

    fn candidate(
        id: i64,
        created_at: NaiveDateTime,
        retention: RetentionType,
    ) -> BackupRetentionCandidate {
        BackupRetentionCandidate {
            id,
            connection_id: Some(1),
            connection_database_id: Some(1),
            database_name: "app".into(),
            created_at,
            status: BackupStatus::Completed,
            retention_type: retention,
        }
    }

    fn config() -> BackupRetentionConfig {
        BackupRetentionConfig {
            daily: 7,
            weekly: 4,
            monthly: 12,
            yearly: 5,
        }
    }

    #[test]
    fn current_day_backup_is_hourly() {
        let now = at("2026-08-10 14:00:00");
        let backup = candidate(1, at("2026-08-10 10:00:00"), RetentionType::Hourly);
        let plan = BackupRetentionPlanner::new(config()).plan(&[backup], now);

        assert_eq!(plan.retained.get(&1), Some(&RetentionType::Hourly));
        assert!(plan.to_delete.is_empty());
    }

    #[test]
    fn failed_backup_outside_today_is_deleted() {
        let now = at("2026-08-10 14:00:00");
        let mut backup = candidate(1, at("2026-08-09 10:00:00"), RetentionType::Hourly);
        backup.status = BackupStatus::Failed;

        let plan = BackupRetentionPlanner::new(config()).plan(&[backup], now);

        assert!(plan.retained.is_empty());
        assert_eq!(plan.to_delete, vec![1]);
    }

    #[test]
    fn only_one_backup_per_bucket_is_retained() {
        let now = at("2026-08-10 14:00:00");
        let a = candidate(1, at("2026-08-10 10:00:00"), RetentionType::Hourly);
        let b = candidate(2, at("2026-08-10 10:30:00"), RetentionType::Hourly);

        let plan = BackupRetentionPlanner::new(config()).plan(&[a, b], now);

        // Mesmo bucket "hourly" no dia corrente; o mais recente fica.
        assert_eq!(plan.retained.len(), 1);
        assert!(plan.retained.contains_key(&2));
        assert_eq!(plan.to_delete, vec![1]);
    }

    #[test]
    fn daily_window_keeps_one_per_day() {
        let now = at("2026-08-10 14:00:00");
        let a = candidate(1, at("2026-08-09 10:00:00"), RetentionType::Hourly);
        let b = candidate(2, at("2026-08-08 15:00:00"), RetentionType::Hourly);
        let c = candidate(3, at("2026-08-08 16:00:00"), RetentionType::Hourly);

        let plan = BackupRetentionPlanner::new(config()).plan(&[a, b, c], now);

        assert_eq!(plan.retained.get(&1), Some(&RetentionType::Daily));
        assert_eq!(plan.retained.get(&3), Some(&RetentionType::Daily));
        assert_eq!(plan.to_delete, vec![2]);
    }

    #[test]
    fn zero_window_deletes_everything_in_that_tier() {
        let now = at("2026-08-10 14:00:00");
        let backup = candidate(1, at("2026-08-09 10:00:00"), RetentionType::Hourly);

        let plan = BackupRetentionPlanner::new(BackupRetentionConfig {
            daily: 0,
            weekly: 0,
            monthly: 0,
            yearly: 0,
        })
        .plan(&[backup], now);

        assert!(plan.retained.is_empty());
        assert_eq!(plan.to_delete, vec![1]);
    }
}
