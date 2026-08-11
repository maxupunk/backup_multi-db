//! Geração assíncrona de archives `.tar.gz` (tarefa 8.12).
//!
//! Cada objeto é lido e comprimido antes de abrir o próximo. Assim o uso de
//! memória não cresce com o tamanho nem com a quantidade de arquivos do bucket.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_compression::tokio::write::GzipEncoder;
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::config::normalize_path;
use super::explorer;
use super::{ListOptions, StorageError, StorageExplorer};
use crate::initializers::settings::Settings;
use crate::models::backup_runner;
use crate::models::storage_destinations::Model as StorageDestination;

const ARCHIVE_TTL: Duration = Duration::from_secs(15 * 60);
const TERMINAL_JOB_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_RETAINED_JOBS: usize = 50;

#[derive(Clone, Default)]
pub struct ArchiveJobRegistry {
    jobs: Arc<RwLock<HashMap<String, ArchiveJob>>>,
    files: Arc<RwLock<HashMap<String, PathBuf>>>,
}

pub fn register(ctx: &AppContext) {
    if !ctx.shared_store.contains::<ArchiveJobRegistry>() {
        ctx.shared_store.insert(ArchiveJobRegistry::default());
    }
}

fn registry(ctx: &AppContext) -> loco_rs::Result<ArchiveJobRegistry> {
    ctx.shared_store
        .get::<ArchiveJobRegistry>()
        .ok_or_else(|| Error::Message("archive job registry was not initialized".to_string()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveJob {
    pub id: String,
    pub storage_id: i64,
    pub path: Option<String>,
    pub status: ArchiveStatus,
    pub total_files: Option<u64>,
    pub processed_files: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveStatus {
    Pending,
    Building,
    Ready,
    Expired,
    Failed,
}

/// Argumentos serializáveis para o worker de geração do archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveWorkerArgs {
    pub job_id: String,
    pub storage: StorageDestination,
    pub settings: Settings,
}

/// Estado aceito e o trabalho que deve ser entregue à fila pelo controller.
pub struct StartedArchiveJob {
    pub job: ArchiveJob,
    pub args: ArchiveWorkerArgs,
}

pub async fn start(
    ctx: &AppContext,
    storage: StorageDestination,
    settings: Settings,
    path: Option<String>,
) -> loco_rs::Result<StartedArchiveJob> {
    let registry = registry(ctx)?;
    let job = ArchiveJob {
        id: format!("archive-{}", Uuid::new_v4()),
        storage_id: storage.id,
        path: path
            .map(|value| normalize_path(&value))
            .filter(|value| !value.is_empty()),
        status: ArchiveStatus::Pending,
        total_files: None,
        processed_files: 0,
        error: None,
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
        expires_at: None,
    };
    let id = job.id.clone();
    registry.jobs.write().await.insert(id.clone(), job.clone());

    Ok(StartedArchiveJob {
        job,
        args: ArchiveWorkerArgs {
            job_id: id,
            storage,
            settings,
        },
    })
}

/// Executa um archive aceito. Uma falha de provider torna o job observável como
/// `failed`; devolvê-la à fila causaria uma repetição automática insegura.
pub async fn perform(ctx: &AppContext, args: ArchiveWorkerArgs) -> Result<()> {
    let registry = registry(ctx)?;
    if let Err(error) = execute(&registry, &args.job_id, &args.storage, &args.settings).await {
        fail(&registry, &args.job_id, error.message()).await;
    }
    Ok(())
}

pub async fn get(ctx: &AppContext, id: &str) -> loco_rs::Result<Option<ArchiveJob>> {
    Ok(registry(ctx)?.jobs.read().await.get(id).cloned())
}

pub async fn download_path(ctx: &AppContext, id: &str) -> loco_rs::Result<Option<PathBuf>> {
    Ok(registry(ctx)?.files.read().await.get(id).cloned())
}

async fn execute(
    registry: &ArchiveJobRegistry,
    id: &str,
    storage: &StorageDestination,
    settings: &Settings,
) -> Result<(), StorageError> {
    update(registry, id, |job| job.status = ArchiveStatus::Building).await;
    let path = get_job(registry, id)
        .await
        .map_or_else(String::new, |job| job.path.unwrap_or_default());
    let encryption =
        backup_runner::encryption_service(settings).map_err(|_| StorageError::InvalidConfig)?;
    let (_, adapter) = explorer::open(storage, &encryption, &settings.backup_storage_path)?;

    let directory = Path::new(&settings.backup_storage_path).join(".archive-jobs");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(StorageError::backend)?;
    let output_path = directory.join(format!("{id}.tar.gz"));
    let file = tokio::fs::File::create(&output_path)
        .await
        .map_err(StorageError::backend)?;
    registry
        .files
        .write()
        .await
        .insert(id.to_string(), output_path.clone());
    let mut writer = TarGzipWriter::new(file);

    archive_tree(registry, id, adapter.as_ref(), &path, &mut writer).await?;
    writer.finish().await?;
    update(registry, id, |job| {
        job.status = ArchiveStatus::Ready;
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
        job.expires_at = Some((chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339());
    })
    .await;
    schedule_expiration(registry.clone(), id.to_string());
    Ok(())
}

async fn archive_tree(
    registry: &ArchiveJobRegistry,
    id: &str,
    adapter: &dyn StorageExplorer,
    root: &str,
    writer: &mut TarGzipWriter,
) -> Result<(), StorageError> {
    let mut directories = vec![root.to_string()];
    let mut discovered = 0_u64;
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
                    continue;
                }
                discovered += 1;
                let metadata = adapter.object_metadata(&object.key).await?;
                let reader = adapter.read_object(&object.key).await?;
                writer
                    .append_file(&object.key, metadata.size, reader)
                    .await?;
                update(registry, id, |job| job.processed_files += 1).await;
            }
            if !page.is_truncated {
                break;
            }
            cursor = Some(page.next_cursor.ok_or_else(|| {
                StorageError::Backend("O provider interrompeu a paginação do archive".to_string())
            })?);
        }
    }
    update(registry, id, |job| job.total_files = Some(discovered)).await;
    Ok(())
}

struct TarGzipWriter {
    output: GzipEncoder<tokio::fs::File>,
}

impl TarGzipWriter {
    fn new(file: tokio::fs::File) -> Self {
        Self {
            output: GzipEncoder::new(file),
        }
    }

    async fn append_file(
        &mut self,
        name: &str,
        size: i64,
        mut reader: super::ObjectReader,
    ) -> Result<(), StorageError> {
        let size = u64::try_from(size)
            .map_err(|_| StorageError::Backend(format!("Tamanho inválido do objeto \"{name}\"")))?;
        let header = tar_header(name, size)?;
        self.output
            .write_all(&header)
            .await
            .map_err(StorageError::backend)?;
        let written = tokio::io::copy(&mut reader, &mut self.output)
            .await
            .map_err(StorageError::backend)?;
        if written != size {
            return Err(StorageError::Backend(format!(
                "O objeto \"{name}\" mudou durante a geração do archive"
            )));
        }
        let padding = (512 - (size % 512)) % 512;
        if padding > 0 {
            self.output
                .write_all(&vec![0; padding as usize])
                .await
                .map_err(StorageError::backend)?;
        }
        Ok(())
    }

    async fn finish(mut self) -> Result<(), StorageError> {
        self.output
            .write_all(&[0; 1024])
            .await
            .map_err(StorageError::backend)?;
        self.output.shutdown().await.map_err(StorageError::backend)
    }
}

fn tar_header(name: &str, size: u64) -> Result<[u8; 512], StorageError> {
    let name = archive_name(name)?;
    let (prefix, leaf) = split_ustar_name(&name)?;
    let mut header = [0_u8; 512];
    header[..leaf.len()].copy_from_slice(leaf.as_bytes());
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    octal(&mut header[124..136], size)?;
    octal(
        &mut header[136..148],
        chrono::Utc::now().timestamp().max(0) as u64,
    )?;
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    if let Some(prefix) = prefix {
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    }
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let text = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(text.as_bytes());
    Ok(header)
}

fn archive_name(key: &str) -> Result<String, StorageError> {
    let name = normalize_path(key);
    if name.is_empty()
        || name
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(StorageError::PathTraversal);
    }
    Ok(name)
}

fn split_ustar_name(name: &str) -> Result<(Option<&str>, &str), StorageError> {
    if name.len() <= 100 {
        return Ok((None, name));
    }
    let Some(index) = name.rfind('/') else {
        return Err(StorageError::Backend(
            "Nome de arquivo excede o limite do tar".to_string(),
        ));
    };
    let (prefix, leaf) = name.split_at(index);
    let leaf = &leaf[1..];
    if prefix.len() <= 155 && leaf.len() <= 100 {
        Ok((Some(prefix), leaf))
    } else {
        Err(StorageError::Backend(
            "Caminho de arquivo excede o limite do tar".to_string(),
        ))
    }
}

fn octal(field: &mut [u8], value: u64) -> Result<(), StorageError> {
    let text = format!("{value:o}");
    if text.len() >= field.len() {
        return Err(StorageError::Backend(
            "Valor excede o limite do cabeçalho tar".to_string(),
        ));
    }
    field.fill(b'0');
    let start = field.len() - 1 - text.len();
    field[start..start + text.len()].copy_from_slice(text.as_bytes());
    field[field.len() - 1] = 0;
    Ok(())
}

async fn get_job(registry: &ArchiveJobRegistry, id: &str) -> Option<ArchiveJob> {
    registry.jobs.read().await.get(id).cloned()
}

async fn update(registry: &ArchiveJobRegistry, id: &str, change: impl FnOnce(&mut ArchiveJob)) {
    if let Some(job) = registry.jobs.write().await.get_mut(id) {
        change(job);
    }
}

async fn fail(registry: &ArchiveJobRegistry, id: &str, error: String) {
    if let Some(path) = registry.files.write().await.remove(id) {
        let _ = tokio::fs::remove_file(path).await;
    }
    update(registry, id, |job| {
        job.status = ArchiveStatus::Failed;
        job.error = Some(error);
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
    })
    .await;
    retain_terminal(registry, id).await;
}

fn schedule_expiration(registry: ArchiveJobRegistry, id: String) {
    schedule_expiration_after(registry, id, ARCHIVE_TTL);
}

fn schedule_expiration_after(registry: ArchiveJobRegistry, id: String, delay: Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        let should_expire = matches!(
            registry.jobs.read().await.get(&id).map(|job| job.status),
            Some(ArchiveStatus::Ready)
        );
        if !should_expire {
            return;
        }
        if let Some(path) = registry.files.write().await.remove(&id) {
            if let Err(error) = tokio::fs::remove_file(path).await {
                tracing::warn!(job_id = id, error = %error, "could not remove expired archive file");
            }
        }
        update(&registry, &id, |job| {
            job.status = ArchiveStatus::Expired;
            job.expires_at = None;
        })
        .await;
        retain_terminal(&registry, &id).await;
    });
}

async fn retain_terminal(registry: &ArchiveJobRegistry, id: &str) {
    schedule_removal(registry.clone(), id.to_string(), TERMINAL_JOB_TTL);
    let mut jobs = registry.jobs.write().await;
    let overflow = jobs.len().saturating_sub(MAX_RETAINED_JOBS);
    if overflow == 0 {
        return;
    }
    let mut removable: Vec<(String, String)> = jobs
        .values()
        .filter(|job| matches!(job.status, ArchiveStatus::Failed | ArchiveStatus::Expired))
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

fn schedule_removal(registry: ArchiveJobRegistry, id: String, delay: Duration) {
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        if let Some(path) = registry.files.write().await.remove(&id) {
            let _ = tokio::fs::remove_file(path).await;
        }
        registry.jobs.write().await.remove(&id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tar_headers_reject_traversal_and_preserve_ustar_fields() {
        assert!(matches!(
            tar_header("../secret", 1),
            Err(StorageError::PathTraversal)
        ));
        let header = tar_header("exports/clientes.sql", 8).expect("header valido");
        assert_eq!(&header[..20], b"exports/clientes.sql");
        assert_eq!(&header[257..263], b"ustar\0");
    }

    #[tokio::test]
    async fn expiration_removes_the_archive_file_and_marks_the_job() {
        let registry = ArchiveJobRegistry::default();
        let id = "archive-test".to_string();
        let directory = tempfile::tempdir().expect("diretorio temporario");
        let file = directory.path().join("archive.tar.gz");
        tokio::fs::write(&file, b"gzip")
            .await
            .expect("cria arquivo");
        registry
            .files
            .write()
            .await
            .insert(id.clone(), file.clone());
        registry.jobs.write().await.insert(
            id.clone(),
            ArchiveJob {
                id: id.clone(),
                storage_id: 1,
                path: None,
                status: ArchiveStatus::Ready,
                total_files: Some(0),
                processed_files: 0,
                error: None,
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: Some(chrono::Utc::now().to_rfc3339()),
                expires_at: Some(chrono::Utc::now().to_rfc3339()),
            },
        );

        schedule_expiration_after(registry.clone(), id.clone(), Duration::from_millis(5));

        // Espera ativa: o runtime pode estar ocupado compilando outros testes,
        // entao um sleep fixo vira flaky. Damos ate' 1s para a tarefa spawned
        // executar e atualizar o registro.
        let mut attempts = 0;
        loop {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let status = registry.jobs.read().await.get(&id).map(|job| job.status);
            if status == Some(ArchiveStatus::Expired) && !file.exists() {
                break;
            }
            attempts += 1;
            if attempts > 100 {
                panic!("a expiracao do archive nao completou em 1s");
            }
        }

        assert!(!file.exists());
        assert_eq!(
            registry.jobs.read().await.get(&id).map(|job| job.status),
            Some(ArchiveStatus::Expired)
        );
        assert!(registry.files.read().await.get(&id).is_none());
    }
}
