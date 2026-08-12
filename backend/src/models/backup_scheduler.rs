//! Despacho dinâmico dos backups agendados.
//!
//! A tarefa do Loco acorda em cadência horária, mas a decisão de executar vem
//! do banco em cada passagem. Assim, editar uma conexão atualiza o agendamento
//! sem recriar cron jobs, sem estado global e sem exigir reinício.

use loco_rs::prelude::*;

use crate::models::backup_runner::{self, BackupRequest};
use crate::models::backups::BackupTrigger;
use crate::models::connections;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DispatchReport {
    pub eligible_connections: usize,
    pub due_connections: usize,
    pub backups_started: usize,
    pub failed_to_start: usize,
}

/// Executa os backups que estão vencidos no instante informado.
pub async fn dispatch(
    ctx: &AppContext,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<DispatchReport> {
    let connections = connections::Model::scheduled_active(&ctx.db).await?;
    let mut report = DispatchReport {
        eligible_connections: connections.len(),
        ..Default::default()
    };

    for connection in connections {
        if !connection.is_backup_due(now) {
            continue;
        }
        report.due_connections += 1;

        let databases = backup_runner::enabled_databases(ctx, connection.id).await?;
        if databases.is_empty() {
            tracing::warn!(
                connection_id = connection.id,
                "scheduled connection has no enabled databases"
            );
            continue;
        }

        for database in databases {
            match backup_runner::run_backup(
                ctx,
                BackupRequest {
                    connection: &connection,
                    connection_database_id: Some(database.id),
                    database_name: database.database_name,
                    trigger: BackupTrigger::Scheduled,
                    metadata: None,
                },
            )
            .await
            {
                Ok(_) => report.backups_started += 1,
                Err(error) => {
                    report.failed_to_start += 1;
                    tracing::error!(connection_id = connection.id, error = %error, "scheduled backup could not start");
                }
            }
        }

        // Mantém o mesmo significado da execução manual: uma marca por lote,
        // inclusive quando um banco do lote falhou e os demais foram tentados.
        if let Err(error) = backup_runner::touch_last_backup(ctx, connection).await {
            tracing::error!(error = %error, "could not update last_backup_at after scheduled backup");
        }
    }

    Ok(report)
}
