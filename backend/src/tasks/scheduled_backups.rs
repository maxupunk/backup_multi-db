//! Tarefa chamada pelo scheduler nativo para avaliar backups agendados.

use loco_rs::prelude::*;
use loco_rs::task::{Task, TaskInfo, Vars};

use crate::models::backup_scheduler;

pub struct ScheduledBackupsTask;

#[async_trait]
impl Task for ScheduledBackupsTask {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "scheduled_backups".to_string(),
            detail: "Executa backups de conexões vencidas conforme a configuração dinâmica"
                .to_string(),
        }
    }

    async fn run(&self, app_context: &AppContext, _vars: &Vars) -> Result<()> {
        let report =
            backup_scheduler::dispatch(app_context, chrono::Utc::now().fixed_offset()).await?;
        tracing::info!(
            eligible_connections = report.eligible_connections,
            due_connections = report.due_connections,
            backups_started = report.backups_started,
            failed_to_start = report.failed_to_start,
            "scheduled backup dispatch finished"
        );
        Ok(())
    }
}
