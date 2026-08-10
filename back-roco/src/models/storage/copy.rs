//! Copia assíncrona entre destinos de armazenamento (tarefa 8.11).
//!
//! O registro e' em memoria porque a API legada tambem perde os jobs num
//! restart. A persistencia e os workers recuperaveis entram na Fase 10. A
//! transferencia, porem, ja' e' real e usa os adapters tipados da Fase 8.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use loco_rs::prelude::*;
use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::config::{join_key, normalize_path, strip_prefix};
use super::explorer;
use super::{ListOptions, StorageError, StorageExplorer};
use crate::initializers::settings::Settings;
use crate::models::backup_runner;
use crate::models::storage_destinations::Model as StorageDestination;

const JOB_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_RETAINED_JOBS: usize = 50;

/// Registro de jobs que pertence ao ciclo de vida do `AppContext`.
///
/// O `SharedStore` do Loco e' a injecao de dependencia para estado efemero da
/// aplicacao. Um `static` aqui misturaria instancias de app em testes e tornaria
/// o registro impossivel de substituir na Fase 10, quando a fila persistente
/// assumir a execucao.
#[derive(Clone, Default)]
pub struct CopyJobRegistry {
    jobs: Arc<RwLock<HashMap<String, CopyJob>>>,
}

/// Registra o estado efemero uma vez durante o boot da aplicacao.
pub fn register(ctx: &AppContext) {
    if !ctx.shared_store.contains::<CopyJobRegistry>() {
        ctx.shared_store.insert(CopyJobRegistry::default());
    }
}

fn registry(ctx: &AppContext) -> loco_rs::Result<CopyJobRegistry> {
    ctx.shared_store
        .get::<CopyJobRegistry>()
        .ok_or_else(|| Error::Message("copy job registry was not initialized".to_string()))
}

/// Estado exposto por `GET /api/storages/copy-jobs/:jobId`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyJob {
    pub id: String,
    pub source_storage_id: i64,
    pub destination_storage_id: i64,
    pub status: CopyStatus,
    pub files_transferred: u64,
    pub total_files: Option<u64>,
    pub bytes_transferred: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Estados compativeis com o `BucketCopyService` legado.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CopyStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Opcoes de `POST /api/storages/:id/copy`.
#[derive(Debug, Clone, Default)]
pub struct CopyOptions {
    pub source_path: Option<String>,
    pub destination_path: Option<String>,
    pub dry_run: bool,
    pub delete_extraneous: bool,
}

/// Cria o job e o destaca da requisicao. A configuracao e' construida antes do
/// spawn para que uma falha de chave/JSON seja reportada pelo job, nunca por um
/// panic em background.
pub async fn start(
    ctx: &AppContext,
    source: StorageDestination,
    destination: StorageDestination,
    settings: Settings,
    options: CopyOptions,
) -> loco_rs::Result<CopyJob> {
    let registry = registry(ctx)?;
    let job = CopyJob {
        id: format!("copy-{}", Uuid::new_v4()),
        source_storage_id: source.id,
        destination_storage_id: destination.id,
        status: CopyStatus::Pending,
        files_transferred: 0,
        total_files: None,
        bytes_transferred: 0,
        error: None,
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
    };
    let id = job.id.clone();
    registry.jobs.write().await.insert(id.clone(), job.clone());

    tokio::spawn(async move {
        if let Err(error) =
            execute(&registry, &id, &source, &destination, &settings, &options).await
        {
            fail(&registry, &id, error.message()).await;
        }
    });

    Ok(job)
}

/// Snapshot de um job. Retornar uma copia evita expor estado mutavel entre a
/// serializacao da resposta e a atualizacao do worker.
pub async fn get(ctx: &AppContext, id: &str) -> loco_rs::Result<Option<CopyJob>> {
    Ok(registry(ctx)?.jobs.read().await.get(id).cloned())
}

async fn execute(
    registry: &CopyJobRegistry,
    id: &str,
    source: &StorageDestination,
    destination: &StorageDestination,
    settings: &Settings,
    options: &CopyOptions,
) -> Result<(), StorageError> {
    update(registry, id, |job| job.status = CopyStatus::Running).await;

    let encryption =
        backup_runner::encryption_service(settings).map_err(|_| StorageError::InvalidConfig)?;
    let (_, source_adapter) = explorer::open(source, &encryption, &settings.backup_storage_path)?;
    let (_, destination_adapter) =
        explorer::open(destination, &encryption, &settings.backup_storage_path)?;
    let source_path = normalize_path(options.source_path.as_deref().unwrap_or_default());
    let destination_path = normalize_path(options.destination_path.as_deref().unwrap_or_default());
    let files = list_files(source_adapter.as_ref(), &source_path).await?;
    update(registry, id, |job| {
        job.total_files = Some(files.len() as u64)
    })
    .await;

    let temporary_directory = Path::new(&settings.backup_storage_path)
        .join(".copy-jobs")
        .join(id);
    tokio::fs::create_dir_all(&temporary_directory)
        .await
        .map_err(StorageError::backend)?;

    let result = copy_files(
        registry,
        id,
        source_adapter.as_ref(),
        destination_adapter.as_ref(),
        &files,
        &source_path,
        &destination_path,
        &temporary_directory,
        options.dry_run,
    )
    .await;

    let cleanup = tokio::fs::remove_dir_all(&temporary_directory).await;
    if let Err(error) = cleanup {
        tracing::warn!(job_id = id, error = %error, "could not remove copy job temporary directory");
    }
    result?;

    if options.delete_extraneous && !options.dry_run {
        remove_extraneous(
            destination_adapter.as_ref(),
            &files,
            &source_path,
            &destination_path,
        )
        .await?;
    }

    update(registry, id, |job| {
        job.status = CopyStatus::Completed;
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
    })
    .await;
    retain_terminal(registry, id).await;
    Ok(())
}

async fn list_files(
    adapter: &dyn StorageExplorer,
    root: &str,
) -> Result<Vec<String>, StorageError> {
    let mut directories = vec![root.to_string()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let mut cursor = None;
        loop {
            let page = adapter
                .list_objects(
                    &directory,
                    &ListOptions {
                        cursor: cursor.clone(),
                        limit: Some(1000),
                        prefix: None,
                    },
                )
                .await?;
            for object in page.objects {
                if object.is_directory {
                    directories.push(object.key);
                } else {
                    files.push(object.key);
                }
            }
            if !page.is_truncated {
                break;
            }
            cursor = Some(page.next_cursor.ok_or_else(|| {
                StorageError::Backend("O provider interrompeu a paginação da cópia".to_string())
            })?);
        }
    }
    files.sort();
    Ok(files)
}

#[allow(clippy::too_many_arguments)]
async fn copy_files(
    registry: &CopyJobRegistry,
    id: &str,
    source: &dyn StorageExplorer,
    destination: &dyn StorageExplorer,
    files: &[String],
    source_path: &str,
    destination_path: &str,
    temporary_directory: &Path,
    dry_run: bool,
) -> Result<(), StorageError> {
    for (index, source_key) in files.iter().enumerate() {
        let destination_key = destination_key(source_key, source_path, destination_path);
        if !dry_run {
            let temporary_file = temporary_directory.join(format!("{index}.part"));
            let mut reader = source.read_object(source_key).await?;
            let mut file = tokio::fs::File::create(&temporary_file)
                .await
                .map_err(StorageError::backend)?;
            let bytes = tokio::io::copy(&mut reader, &mut file)
                .await
                .map_err(StorageError::backend)?;
            file.sync_all().await.map_err(StorageError::backend)?;
            destination
                .put_file(&destination_key, &temporary_file)
                .await?;
            tokio::fs::remove_file(&temporary_file)
                .await
                .map_err(StorageError::backend)?;
            update(registry, id, |job| {
                job.files_transferred += 1;
                job.bytes_transferred += bytes;
            })
            .await;
        } else {
            update(registry, id, |job| job.files_transferred += 1).await;
        }
    }
    Ok(())
}

async fn remove_extraneous(
    destination: &dyn StorageExplorer,
    source_files: &[String],
    source_path: &str,
    destination_path: &str,
) -> Result<(), StorageError> {
    let expected: HashSet<String> = source_files
        .iter()
        .map(|key| destination_key(key, source_path, destination_path))
        .collect();
    let destination_files = list_files(destination, destination_path).await?;
    for key in destination_files {
        if !expected.contains(&key) {
            destination.delete_object(&key, false).await?;
        }
    }
    Ok(())
}

fn destination_key(source_key: &str, source_path: &str, destination_path: &str) -> String {
    join_key(destination_path, &strip_prefix(source_path, source_key))
}

async fn update(registry: &CopyJobRegistry, id: &str, change: impl FnOnce(&mut CopyJob)) {
    if let Some(job) = registry.jobs.write().await.get_mut(id) {
        change(job);
    }
}

async fn fail(registry: &CopyJobRegistry, id: &str, error: String) {
    update(registry, id, |job| {
        job.status = CopyStatus::Failed;
        job.error = Some(error);
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
    })
    .await;
    retain_terminal(registry, id).await;
}

/// Mantém o comportamento do serviço legado: jobs terminalizados ficam seis
/// horas para o frontend consultar, mas nunca acumulam indefinidamente.
async fn retain_terminal(registry: &CopyJobRegistry, id: &str) {
    schedule_removal(registry.clone(), id.to_string(), JOB_TTL);
    let mut jobs = registry.jobs.write().await;
    let overflow = jobs.len().saturating_sub(MAX_RETAINED_JOBS);
    if overflow == 0 {
        return;
    }
    let mut removable: Vec<(String, String)> = jobs
        .values()
        .filter(|job| matches!(job.status, CopyStatus::Completed | CopyStatus::Failed))
        .map(|job| {
            (
                job.id.clone(),
                job.completed_at
                    .clone()
                    .unwrap_or_else(|| job.started_at.clone()),
            )
        })
        .collect();
    removable.sort_by(|left, right| left.1.cmp(&right.1));
    for (id, _) in removable.into_iter().take(overflow) {
        jobs.remove(&id);
    }
}

fn schedule_removal(registry: CopyJobRegistry, id: String, delay: Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        registry.jobs.write().await.remove(&id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_content_of_the_source_path_to_the_destination_path() {
        assert_eq!(destination_key("a/b.sql", "a", "copies"), "copies/b.sql");
        assert_eq!(destination_key("a/b.sql", "", "copies"), "copies/a/b.sql");
    }
}
