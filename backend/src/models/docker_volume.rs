//! Exportacao e backup de volumes Docker.
//!
//! A Docker Engine nao exporta um volume diretamente: e' preciso criar um
//! container temporario, montar o volume em modo somente-leitura e baixar o
//! conteudo de `/data` como um tar stream. Tudo e' feito pela API, sem depender
//! do binario `docker` no host.

use std::pin::Pin;

use bollard::container::{Config, CreateContainerOptions, DownloadFromContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, Mount, MountTypeEnum};
use futures_util::StreamExt;
use loco_rs::prelude::*;
use tokio::io::AsyncWriteExt;

use crate::initializers::settings::Settings;
use crate::models::backup_runner;
use crate::models::backup_storage;
use crate::models::docker::{self, ContainerAction, DockerError};
use crate::models::storage::explorer;
use crate::models::storage_destinations::Model as StorageDestination;

const ALPINE_IMAGE: &str = "alpine:latest";
const TEMP_CONTAINER_PREFIX: &str = "backend-vol-";
const MOUNT_POINT: &str = "/data";

/// Resultado de um backup de volume para um destino.
#[derive(Debug, Clone)]
pub struct BackupOutcome {
    pub file_name: String,
    pub relative_path: String,
}

/// Stream de exportacao de um volume. O container temporario e' removido assim
/// que o struct sai de escopo, mesmo que o download seja interrompido.
pub struct VolumeExport {
    pub file_name: String,
    reader: Option<Pin<Box<dyn tokio::io::AsyncRead + Send>>>,
    container_id: String,
}

impl VolumeExport {
    /// Leitor que produz o tar cru da Engine. Gzipar (ou nao) e' decisao do
    /// chamador. O container temporario ainda e' removido pelo `Drop` deste
    /// struct quando ele sair de escopo.
    pub fn reader(&mut self) -> Pin<Box<dyn tokio::io::AsyncRead + Send>> {
        self.reader.take().expect("reader ja' foi consumido")
    }
}

impl Drop for VolumeExport {
    fn drop(&mut self) {
        let id = self.container_id.clone();
        tokio::spawn(async move {
            let _ = docker::container_action(&id, ContainerAction::Remove { force: true }).await;
        });
    }
}

/// Prepara o stream de exportacao de um volume.
pub async fn export(name: &str) -> Result<VolumeExport, DockerError> {
    let client = docker::client()?;

    // Garante que o volume existe antes de criar o container temporario.
    docker::inspect_volume(name).await?;

    ensure_image(&client).await?;

    let container_name = format!("{TEMP_CONTAINER_PREFIX}{}", uuid::Uuid::new_v4());
    let created = client
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }),
            Config {
                image: Some(ALPINE_IMAGE.to_string()),
                cmd: Some(vec!["true".to_string()]),
                host_config: Some(HostConfig {
                    mounts: Some(vec![Mount {
                        target: Some(MOUNT_POINT.to_string()),
                        source: Some(name.to_string()),
                        typ: Some(MountTypeEnum::VOLUME),
                        read_only: Some(true),
                        ..Default::default()
                    }]),
                    auto_remove: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .map_err(|_| DockerError::Engine)?;

    let container_id = created.id;
    if container_id.is_empty() {
        return Err(DockerError::Engine);
    }

    client
        .start_container::<String>(&container_id, None)
        .await
        .map_err(|_| DockerError::Engine)?;

    let stream = client
        .download_from_container(
            &container_id,
            Some(DownloadFromContainerOptions {
                path: MOUNT_POINT.to_string(),
            }),
        )
        .map(|result| {
            result.map_err(|err| {
                tracing::debug!(error = %err, "erro no stream de download do volume");
                std::io::Error::other("falha no download do volume")
            })
        });

    let reader = tokio_util::io::StreamReader::new(stream);

    Ok(VolumeExport {
        file_name: file_name(name),
        reader: Some(Box::pin(reader)),
        container_id,
    })
}

/// Exporta um volume para um arquivo temporario local ja' gzipado.
///
/// Usado por `GET /api/docker/volumes/{name}/export`, onde o arquivo precisa
/// existir no disco para que o Axum possa transmiti-lo com headers corretos e
/// remocao automatica ao final.
pub async fn export_to_temp_file(name: &str) -> Result<(std::path::PathBuf, String), DockerError> {
    let mut export = export(name).await?;
    let file_name = export.file_name.clone();
    let temp_path = std::env::temp_dir().join(format!(
        "backend-vol-export-{}.tar.gz",
        uuid::Uuid::new_v4()
    ));

    let mut gzip = async_compression::tokio::write::GzipEncoder::new(
        tokio::fs::File::create(&temp_path)
            .await
            .map_err(|_| DockerError::Engine)?,
    );

    let mut reader = export.reader();
    tokio::io::copy(&mut reader, &mut gzip)
        .await
        .map_err(|_| DockerError::Engine)?;
    gzip.shutdown().await.map_err(|_| DockerError::Engine)?;

    Ok((temp_path, file_name))
}

/// Faz backup de um volume para um destino de armazenamento.
///
/// O fluxo segue o Adonis: gera um `.tar.gz` local e, se o destino for remoto,
/// envia a copia e remove o arquivo temporario. O caminho relativo comeca em
/// `docker-volumes/` para nao misturar com dumps de banco.
pub async fn backup_to_storage(
    ctx: &AppContext,
    volume_name: &str,
    destination: &StorageDestination,
) -> Result<BackupOutcome, DockerError> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())
        .map_err(|err| DockerError::Validation(err.to_string()))?;
    let encryption = backup_runner::encryption_service(&settings)
        .map_err(|err| DockerError::Validation(err.to_string()))?;

    let file_name = file_name(volume_name);
    let relative_path = format!("docker-volumes/{file_name}");

    let base = backup_storage::local_base_path(
        Some(destination),
        &encryption,
        &settings.backup_storage_path,
    );
    let Some(full_path) = backup_storage::local_full_path(&base, &relative_path) else {
        return Err(DockerError::Validation(
            "Caminho de backup de volume invalido".into(),
        ));
    };

    if let Some(parent) = full_path.parent() {
        backup_storage::ensure_directory(parent)
            .await
            .map_err(|_| DockerError::Engine)?;
    }

    let mut export = export(volume_name).await?;
    let mut gzip = async_compression::tokio::write::GzipEncoder::new(
        tokio::fs::File::create(&full_path)
            .await
            .map_err(|_| DockerError::Engine)?,
    );

    let mut reader = export.reader();
    tokio::io::copy(&mut reader, &mut gzip)
        .await
        .map_err(|_| DockerError::Engine)?;
    gzip.shutdown().await.map_err(|_| DockerError::Engine)?;

    if backup_storage::is_remote(Some(destination)) {
        explorer::upload_backup(
            destination,
            &encryption,
            &settings.backup_storage_path,
            &relative_path,
            &full_path,
        )
        .await
        .map_err(|err| DockerError::Validation(err.to_string()))?;

        backup_storage::delete_local_file(&full_path)
            .await
            .map_err(|_| DockerError::Engine)?;
    }

    Ok(BackupOutcome {
        file_name,
        relative_path,
    })
}

/// Garante que a imagem `alpine:latest` esta' disponivel localmente.
async fn ensure_image(client: &bollard::Docker) -> Result<(), DockerError> {
    if client.inspect_image(ALPINE_IMAGE).await.is_ok() {
        return Ok(());
    }

    let mut stream = client.create_image(
        Some(CreateImageOptions {
            from_image: ALPINE_IMAGE,
            ..Default::default()
        }),
        None,
        None,
    );

    while let Some(result) = stream.next().await {
        if let Err(err) = result {
            tracing::warn!(error = %err, "falha ao fazer pull da imagem alpine");
            return Err(DockerError::Engine);
        }
    }

    Ok(())
}

/// Nome do arquivo de exportacao/backup.
fn file_name(volume_name: &str) -> String {
    let safe = volume_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let date = chrono::Utc::now().format("%Y-%m-%d");
    format!("volume-{safe}-{date}.tar.gz")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_name_sanitizes_special_characters() {
        assert_eq!(
            file_name("meu/volume:teste"),
            format!(
                "volume-meu_volume_teste-{}.tar.gz",
                chrono::Utc::now().format("%Y-%m-%d")
            )
        );
    }

    #[test]
    fn file_name_keeps_safe_characters() {
        let today = chrono::Utc::now().format("%Y-%m-%d");
        assert_eq!(
            file_name("meu-volume_teste"),
            format!("volume-meu-volume_teste-{today}.tar.gz")
        );
    }
}
