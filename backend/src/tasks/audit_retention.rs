//! Tarefa diária para manter a trilha de auditoria dentro da retenção definida.

use loco_rs::prelude::*;
use loco_rs::task::{Task, TaskInfo, Vars};

use crate::models::audit_logs::Model as AuditLog;

pub struct AuditRetentionTask;

#[async_trait]
impl Task for AuditRetentionTask {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "audit_retention".to_string(),
            detail: "Remove logs de auditoria que excederam a retenção configurada".to_string(),
        }
    }

    async fn run(&self, app_context: &AppContext, _vars: &Vars) -> Result<()> {
        let report =
            AuditLog::prune_expired(app_context, chrono::Utc::now().fixed_offset()).await?;
        tracing::info!(
            retention_days = report.retention_days,
            deleted = report.deleted,
            capped = report.capped,
            "audit retention finished"
        );
        Ok(())
    }
}
