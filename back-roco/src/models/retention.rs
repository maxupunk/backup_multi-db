//! Execucao da retencao GFS de backups (tarefa 11.6).
//!
//! Carrega backups candidatos, chama o planner, sincroniza os `retention_type`
//! promovidos e remove arquivos e registros dos backups descartados.

use loco_rs::prelude::*;
use sea_orm::ActiveValue::Set;
use serde::Serialize;

use crate::initializers::settings::Settings;
use crate::models::backup_retention_planner::{BackupRetentionCandidate, BackupRetentionPlanner};
use crate::models::backup_retention_policy;
use crate::models::backup_runner;
use crate::models::backup_storage;
use crate::models::backups::{BackupStatus, Model as Backup, RetentionType};
use crate::models::storage;

const BATCH_SIZE: usize = 500;

/// Backup candidato, com os campos que o planner e a remocao precisam.
#[derive(Debug, Clone)]
struct PrunableBackup {
    id: i64,
    connection_id: Option<i64>,
    connection_database_id: Option<i64>,
    database_name: String,
    storage_destination_id: Option<i64>,
    status: BackupStatus,
    retention_type: RetentionType,
    file_path: Option<String>,
    file_name: Option<String>,
    created_at: chrono::NaiveDateTime,
}

/// Backup que foi removido, para o relatorio.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedBackupSummary {
    pub id: i64,
    pub connection_id: Option<i64>,
    pub connection_database_id: Option<i64>,
    pub database_name: String,
    pub file_name: Option<String>,
    pub retention_type: String,
    pub created_at: Option<String>,
}

/// Resultado da execucao da retencao.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionResult {
    pub deleted: usize,
    pub promoted: usize,
    pub protected: u64,
    pub errors: Vec<String>,
    pub deleted_backups: Vec<DeletedBackupSummary>,
}

/// Executa o prune de backups antigos conforme a politica atual.
pub async fn prune_backups(ctx: &AppContext) -> Result<RetentionResult> {
    let policy = backup_retention_policy::get_policy(ctx).await?;
    let config = policy.into_config();
    let planner = BackupRetentionPlanner::new(config);

    let mut result = RetentionResult {
        deleted: 0,
        promoted: 0,
        protected: 0,
        errors: Vec::new(),
        deleted_backups: Vec::new(),
    };

    let backups = match load_prunable_backups(&ctx.db).await {
        Ok(backups) => backups,
        Err(err) => {
            result
                .errors
                .push(format!("Falha ao carregar backups: {err}"));
            return Ok(result);
        }
    };

    let plan = planner.plan(
        &backups
            .iter()
            .map(|b| BackupRetentionCandidate {
                id: b.id,
                connection_id: b.connection_id,
                connection_database_id: b.connection_database_id,
                database_name: b.database_name.clone(),
                created_at: b.created_at,
                status: b.status,
                retention_type: b.retention_type,
            })
            .collect::<Vec<_>>(),
        chrono::Utc::now().naive_utc(),
    );

    result.promoted = match sync_retention_types(&ctx.db, &backups, &plan.retained).await {
        Ok(count) => count,
        Err(err) => {
            result
                .errors
                .push(format!("Falha ao sincronizar retention types: {err}"));
            0
        }
    };

    let to_delete_ids: std::collections::HashSet<i64> = plan.to_delete.iter().copied().collect();
    let to_delete: Vec<&PrunableBackup> = backups
        .iter()
        .filter(|b| to_delete_ids.contains(&b.id))
        .collect();

    for backup in to_delete {
        match delete_backup(ctx, backup).await {
            Ok(summary) => {
                result.deleted += 1;
                result.deleted_backups.push(summary);
            }
            Err(err) => {
                result
                    .errors
                    .push(format!("Erro ao deletar backup {}: {err}", backup.id));
            }
        }
    }

    result.protected = match Backup::count_protected(&ctx.db).await {
        Ok(count) => count,
        Err(err) => {
            result
                .errors
                .push(format!("Falha ao contar backups protegidos: {err}"));
            0
        }
    };

    Ok(result)
}

async fn load_prunable_backups(db: &DatabaseConnection) -> Result<Vec<PrunableBackup>> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

    let rows = crate::models::_entities::backups::Entity::find()
        .select_only()
        .column(crate::models::_entities::backups::Column::Id)
        .column(crate::models::_entities::backups::Column::ConnectionId)
        .column(crate::models::_entities::backups::Column::ConnectionDatabaseId)
        .column(crate::models::_entities::backups::Column::DatabaseName)
        .column(crate::models::_entities::backups::Column::StorageDestinationId)
        .column(crate::models::_entities::backups::Column::Status)
        .column(crate::models::_entities::backups::Column::RetentionType)
        .column(crate::models::_entities::backups::Column::FilePath)
        .column(crate::models::_entities::backups::Column::FileName)
        .column(crate::models::_entities::backups::Column::CreatedAt)
        .filter(crate::models::_entities::backups::Column::Protected.eq(false))
        .filter(
            crate::models::_entities::backups::Column::Status.is_not_in([
                BackupStatus::Pending.as_str(),
                BackupStatus::Running.as_str(),
            ]),
        )
        .order_by_desc(crate::models::_entities::backups::Column::CreatedAt)
        .all(db)
        .await?;

    rows.into_iter()
        .map(|row| {
            let status = row.status_enum().map_err(|e| {
                Error::Message(format!("status de backup invalido para {}: {e}", row.id))
            })?;
            let retention_type = row.retention().map_err(|e| {
                Error::Message(format!(
                    "retention type invalido para backup {}: {e}",
                    row.id
                ))
            })?;
            Ok(PrunableBackup {
                id: row.id,
                connection_id: row.connection_id,
                connection_database_id: row.connection_database_id,
                database_name: row.database_name,
                storage_destination_id: row.storage_destination_id,
                status,
                retention_type,
                file_path: row.file_path,
                file_name: row.file_name,
                created_at: row.created_at,
            })
        })
        .collect::<Result<Vec<_>>>()
}

async fn sync_retention_types(
    db: &DatabaseConnection,
    backups: &[PrunableBackup],
    retained: &std::collections::HashMap<i64, RetentionType>,
) -> Result<usize> {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let mut ids_by_type: std::collections::HashMap<RetentionType, Vec<i64>> =
        std::collections::HashMap::new();
    let mut changed = 0;

    for backup in backups {
        let Some(target) = retained.get(&backup.id) else {
            continue;
        };
        if backup.retention_type == *target {
            continue;
        }
        ids_by_type.entry(*target).or_default().push(backup.id);
        changed += 1;
    }

    if changed == 0 {
        return Ok(0);
    }

    let now = chrono::Utc::now().naive_utc();

    for (retention_type, ids) in ids_by_type {
        for chunk in ids.chunks(BATCH_SIZE) {
            crate::models::_entities::backups::Entity::update_many()
                .filter(crate::models::_entities::backups::Column::Id.is_in(chunk.to_vec()))
                .set(crate::models::_entities::backups::ActiveModel {
                    retention_type: Set(retention_type.as_str().to_string()),
                    updated_at: Set(now),
                    ..Default::default()
                })
                .exec(db)
                .await?;
        }
    }

    Ok(changed)
}

async fn delete_backup(ctx: &AppContext, backup: &PrunableBackup) -> Result<DeletedBackupSummary> {
    let summary = DeletedBackupSummary {
        id: backup.id,
        connection_id: backup.connection_id,
        connection_database_id: backup.connection_database_id,
        database_name: backup.database_name.clone(),
        file_name: backup.file_name.clone(),
        retention_type: backup.retention_type.as_str().to_string(),
        created_at: Some(backup.created_at.to_string()),
    };

    if let Some(file_path) = backup.file_path.as_deref() {
        if let Err(err) = delete_backup_file(ctx, backup, file_path).await {
            return Err(Error::Message(format!(
                "falha ao remover arquivo do backup {}: {err}",
                backup.id
            )));
        }
    }

    Backup::delete_by_id(&ctx.db, backup.id).await?;

    Ok(summary)
}

async fn delete_backup_file(
    ctx: &AppContext,
    backup: &PrunableBackup,
    file_path: &str,
) -> Result<()> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let encryption = backup_runner::encryption_service(&settings)?;

    let destination =
        backup_storage::resolve_destination_for_backup(&ctx.db, backup.storage_destination_id)
            .await?;

    let base = backup_storage::local_base_path(
        destination.as_ref(),
        &encryption,
        &settings.backup_storage_path,
    );

    if let Some(path) = backup_storage::local_full_path(&base, file_path) {
        if tokio::fs::metadata(&path).await.is_ok() {
            backup_storage::delete_local_file(&path).await?;
        }
    }

    if let Some(destination) = destination.filter(|row| backup_storage::is_remote(Some(row))) {
        storage::explorer::remove_backup(
            &destination,
            &encryption,
            &settings.backup_storage_path,
            file_path,
        )
        .await
        .map_err(|err| Error::Message(err.message()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_summary_serializes_with_camel_case() {
        let summary = DeletedBackupSummary {
            id: 1,
            connection_id: Some(2),
            connection_database_id: Some(3),
            database_name: "app".into(),
            file_name: Some("app.sql.gz".into()),
            retention_type: "daily".into(),
            created_at: Some("2026-08-10 14:00:00".into()),
        };

        let json = serde_json::to_value(summary).unwrap();
        assert_eq!(json["connectionId"], 2);
        assert_eq!(json["connectionDatabaseId"], 3);
        assert_eq!(json["fileName"], "app.sql.gz");
    }
}
