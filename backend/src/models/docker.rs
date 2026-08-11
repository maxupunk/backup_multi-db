//! Fronteira com a Docker Engine (Fase 9).
//!
//! Todas as operacoes passam por esta camada. Os handlers nunca conhecem
//! `bollard`, o que deixa o contrato HTTP separado do transporte local da
//! Engine e evita depender do binario `docker` dentro do container da API.

use std::collections::HashMap;
use std::time::Duration;

use bollard::container::{ListContainersOptions, LogsOptions, RemoveContainerOptions};
use bollard::image::{ListImagesOptions, RemoveImageOptions};
use bollard::network::{
    ConnectNetworkOptions, CreateNetworkOptions, DisconnectNetworkOptions, ListNetworksOptions,
};
use bollard::volume::{ListVolumesOptions, RemoveVolumeOptions};
use bollard::Docker;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Value};

const PING_TIMEOUT: Duration = Duration::from_secs(3);

/// Resultado minimo e estavel de `GET /api/docker/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub available: bool,
}

/// Erro seguro para operacoes da Engine. A resposta HTTP nunca revela socket,
/// caminhos do host ou configuracao do daemon.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("Docker indisponivel")]
    Unavailable,
    #[error("Falha ao comunicar com o Docker Engine")]
    Engine,
    #[error("{0}")]
    Validation(String),
    #[error("Volume em uso")]
    VolumeInUse {
        message: String,
        container_names: Vec<String>,
    },
}

/// Filtros aceitos pela rota de logs.
#[derive(Debug, Clone, Default)]
pub struct LogFilters {
    pub tail: String,
    pub since: i64,
    pub until: i64,
    pub timestamps: bool,
}

pub fn client() -> Result<Docker, DockerError> {
    Docker::connect_with_local_defaults().map_err(|_| DockerError::Unavailable)
}

async fn call<T>(
    future: impl std::future::Future<Output = Result<T, bollard::errors::Error>>,
) -> Result<T, DockerError> {
    future.await.map_err(|_| DockerError::Engine)
}

fn value<T: Serialize>(item: T) -> Result<Value, DockerError> {
    serde_json::to_value(item).map_err(|_| DockerError::Engine)
}

/// Sonda a Engine local sem transformar sua ausencia em erro HTTP.
pub async fn status() -> Status {
    let Ok(client) = client() else {
        return Status { available: false };
    };
    let available = matches!(
        tokio::time::timeout(PING_TIMEOUT, client.ping()).await,
        Ok(Ok(_))
    );
    Status { available }
}

fn resolve_backend_container_id() -> Option<String> {
    std::env::var("BACKEND_CONTAINER_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .or_else(|| {
            std::env::var("HOSTNAME").ok().filter(|id| {
                let id = id.trim();
                (12..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_hexdigit())
            })
        })
}

/// Identifica se o backend executa dentro de um container e se a Engine esta
/// acessivel. `HOSTNAME` so e' considerado id quando possui o formato Docker.
pub async fn environment() -> Value {
    let available = status().await.available;
    let backend_container_id = resolve_backend_container_id();
    let backend_networks = if available {
        match backend_container_id.as_deref() {
            Some(id) => inspect_container_networks(id).await.unwrap_or_default(),
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let backend_network_ids: Vec<String> = backend_networks
        .iter()
        .map(|network| network.network_id.clone())
        .collect();
    let docker_host_ip = std::env::var("DOCKER_HOST_IP")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            backend_networks
                .iter()
                .find(|network| network.gateway.is_some())
                .and_then(|network| network.gateway.clone())
                .unwrap_or_else(|| {
                    if backend_container_id.is_some() {
                        "host.docker.internal".into()
                    } else {
                        std::env::var("DOCKER_FALLBACK_HOST")
                            .ok()
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or_else(|| "127.0.0.1".into())
                    }
                })
        });
    json!({
        "dockerAvailable": available,
        "unavailableReason": if available { Value::Null } else { Value::String("Docker indisponivel".into()) },
        "backendContainerId": backend_container_id,
        "backendNetworkIds": backend_network_ids,
        "dockerHostIp": docker_host_ip,
    })
}

async fn inspect_container_networks(
    container_id: &str,
) -> Result<Vec<crate::models::docker_connection_suggestion::NetworkAttachment>, DockerError> {
    let client = client()?;
    let inspect = call(client.inspect_container(container_id, None)).await?;
    let inspect: bollard::models::ContainerInspectResponse =
        serde_json::from_value(serde_json::to_value(inspect).map_err(|_| DockerError::Engine)?)
            .map_err(|_| DockerError::Engine)?;
    Ok(inspect
        .network_settings
        .as_ref()
        .and_then(|settings| settings.networks.as_ref())
        .map(|networks| {
            networks
                .iter()
                .map(|(network_name, endpoint)| {
                    crate::models::docker_connection_suggestion::NetworkAttachment {
                        network_id: endpoint
                            .network_id
                            .clone()
                            .unwrap_or_else(|| network_name.clone()),
                        network_name: network_name.clone(),
                        aliases: endpoint.aliases.clone().unwrap_or_default(),
                        gateway: endpoint.gateway.clone(),
                        ip_address: endpoint.ip_address.clone(),
                    }
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Lista containers, inclusive os parados, agrupados por projeto compose.
pub async fn list_containers() -> Result<Value, DockerError> {
    let client = client()?;
    let items = call(
        client.list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        })),
    )
    .await?;
    let mut groups: HashMap<String, Vec<Value>> = HashMap::new();
    for item in items {
        let item = value(item)?;
        let project = item
            .pointer("/Labels/com.docker.compose.project")
            .and_then(Value::as_str)
            .unwrap_or("_standalone")
            .to_string();
        groups.entry(project).or_default().push(item);
    }
    let mut result: Vec<Value> = groups.into_iter().map(|(project_name, containers)| json!({ "projectName": project_name, "containers": containers })).collect();
    result.sort_by(|a, b| a["projectName"].as_str().cmp(&b["projectName"].as_str()));
    Ok(Value::Array(result))
}

/// Descobre containers de banco em execucao para a tela de conexoes.
pub async fn discover_database_hosts() -> Result<Vec<Value>, DockerError> {
    use crate::models::docker_connection_suggestion as suggestion;

    let client = client()?;
    let containers = call(
        client.list_containers(Some(ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        })),
    )
    .await?;

    let backend_container_id = resolve_backend_container_id();
    let backend_networks = match backend_container_id.as_deref() {
        Some(id) => inspect_container_networks(id).await.unwrap_or_default(),
        None => Vec::new(),
    };
    let context =
        suggestion::BackendContext::from_environment(backend_container_id, backend_networks);

    let mut descriptors = Vec::new();
    for summary in containers {
        let id = summary.id.as_deref().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        let inspect = call(client.inspect_container(id, None)).await?;
        let inspect: bollard::models::ContainerInspectResponse =
            serde_json::from_value(value(inspect)?).map_err(|_| DockerError::Engine)?;
        if let Some(descriptor) = suggestion::descriptor_from_bollard(&summary, &inspect) {
            descriptors.push(descriptor);
        }
    }

    let suggestions = suggestion::ConnectionSuggestionMapper::map(&descriptors, &context);
    Ok(suggestions
        .iter()
        .map(suggestion::suggestion_to_value)
        .collect())
}

pub async fn inspect_container(id: &str) -> Result<Value, DockerError> {
    value(call(client()?.inspect_container(id, None)).await?)
}

pub async fn container_action(id: &str, action: ContainerAction) -> Result<Value, DockerError> {
    let client = client()?;
    match action {
        ContainerAction::Start => call(client.start_container::<String>(id, None)).await?,
        ContainerAction::Stop => call(client.stop_container(id, None)).await?,
        ContainerAction::Restart => call(client.restart_container(id, None)).await?,
        ContainerAction::Remove { force } => {
            call(client.remove_container(
                id,
                Some(RemoveContainerOptions {
                    force,
                    ..Default::default()
                }),
            ))
            .await?
        }
    }
    Ok(json!({ "success": true, "message": action.message() }))
}

#[derive(Debug, Clone, Copy)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Remove { force: bool },
}
impl ContainerAction {
    fn message(self) -> &'static str {
        match self {
            Self::Start => "Container iniciado com sucesso.",
            Self::Stop => "Container parado com sucesso.",
            Self::Restart => "Container reiniciado com sucesso.",
            Self::Remove { .. } => "Container removido com sucesso.",
        }
    }
}

pub async fn container_logs(id: &str, filters: LogFilters) -> Result<Value, DockerError> {
    let client = client()?;
    let mut stream = client.logs(
        id,
        Some(LogsOptions {
            follow: false,
            stdout: true,
            stderr: true,
            since: filters.since,
            until: filters.until,
            timestamps: filters.timestamps,
            tail: filters.tail,
        }),
    );
    let mut entries = Vec::new();
    while let Some(output) = stream.next().await {
        let output = output.map_err(|_| DockerError::Engine)?;
        let (stream_name, bytes) = match output {
            bollard::container::LogOutput::StdOut { message } => ("stdout", message),
            bollard::container::LogOutput::StdErr { message } => ("stderr", message),
            _ => continue,
        };
        let mut message = String::from_utf8_lossy(&bytes).trim_end().to_string();
        let timestamp = if filters.timestamps {
            let split = message
                .split_once(' ')
                .map(|(timestamp, rest)| (timestamp.to_string(), rest.to_string()));
            if let Some((timestamp, rest)) = split {
                message = rest;
                timestamp
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        if !message.is_empty() {
            entries
                .push(json!({ "timestamp": timestamp, "stream": stream_name, "message": message }));
        }
    }
    Ok(Value::Array(entries))
}

/// Trunca o arquivo de log apontado pela própria Engine. O caminho não vem da
/// requisição, portanto não há superfície de path traversal; ainda assim a
/// montagem do diretório de logs é uma decisão explícita do deploy.
pub async fn clear_container_logs(id: &str) -> Result<Value, DockerError> {
    let inspect = call(client()?.inspect_container(id, None)).await?;
    let path = inspect.log_path.ok_or(DockerError::Validation(
        "O container não possui um arquivo de log local.".into(),
    ))?;
    tokio::fs::File::create(path)
        .await
        .map_err(|_| DockerError::Engine)?;
    Ok(json!({ "success": true, "message": "Logs do container limpos com sucesso." }))
}

pub async fn list_volumes() -> Result<Value, DockerError> {
    let response =
        value(call(client()?.list_volumes(Some(ListVolumesOptions::<String>::default()))).await?)?;
    // A Engine envolve a lista em `Volumes`; o contrato HTTP expõe apenas a
    // coleção, como o manager legado.
    Ok(response
        .get("Volumes")
        .or_else(|| response.get("volumes"))
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new())))
}
pub async fn inspect_volume(name: &str) -> Result<Value, DockerError> {
    value(call(client()?.inspect_volume(name)).await?)
}
pub async fn remove_volume(name: &str, force: bool) -> Result<Value, DockerError> {
    let client = client()?;
    match client
        .remove_volume(name, Some(RemoveVolumeOptions { force }))
        .await
    {
        Ok(()) => Ok(json!({ "success": true, "message": "Volume removido com sucesso." })),
        Err(err) => {
            if let Some(container_names) = volume_in_use_containers(&err).await {
                return Err(DockerError::VolumeInUse {
                    message: format!(
                        "O volume está em uso pelos containers: {}. Pare-os antes de remover o volume.",
                        container_names.join(", ")
                    ),
                    container_names,
                });
            }
            Err(DockerError::Engine)
        }
    }
}

async fn volume_in_use_containers(err: &bollard::errors::Error) -> Option<Vec<String>> {
    let text = err.to_string();
    let ids = extract_container_ids(&text)?;
    Some(resolve_container_names(&ids).await)
}

fn extract_container_ids(message: &str) -> Option<Vec<&str>> {
    let bracket = message.split_once('[')?.1;
    let ids = bracket.split_once(']')?.0;
    Some(
        ids.split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .collect(),
    )
}

async fn resolve_container_names(ids: &[&str]) -> Vec<String> {
    let Ok(client) = client() else {
        return ids.iter().copied().map(short_id).collect();
    };
    let list = tokio::time::timeout(
        Duration::from_secs(3),
        client.list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        })),
    )
    .await;
    let Ok(Ok(containers)) = list else {
        return ids.iter().copied().map(short_id).collect();
    };

    ids.iter()
        .map(|id| {
            let found = containers.iter().find(|c| {
                let cid = c.id.as_deref().unwrap_or_default();
                cid == *id || cid.starts_with(id) || id.starts_with(cid)
            });
            match found {
                Some(c) => {
                    let name = c
                        .names
                        .as_ref()
                        .and_then(|names| names.first())
                        .map(|name| name.trim_start_matches('/').to_string())
                        .unwrap_or_else(|| short_id(id));
                    let project = c
                        .labels
                        .as_ref()
                        .and_then(|labels| labels.get("com.docker.compose.project"))
                        .map(String::as_str)
                        .filter(|value| !value.is_empty());
                    match project {
                        Some(project) => format!("{name} ({project})"),
                        None => name,
                    }
                }
                None => short_id(id),
            }
        })
        .collect()
}

fn short_id(id: &str) -> String {
    id.chars().take(12).collect()
}

pub async fn list_networks() -> Result<Value, DockerError> {
    value(call(client()?.list_networks(Some(ListNetworksOptions::<String>::default()))).await?)
}
pub async fn inspect_network(id: &str) -> Result<Value, DockerError> {
    value(call(client()?.inspect_network::<String>(id, None)).await?)
}
pub async fn create_network(name: String, driver: String) -> Result<Value, DockerError> {
    call(client()?.create_network(CreateNetworkOptions {
        name: name.clone(),
        driver,
        check_duplicate: true,
        ..Default::default()
    }))
    .await?;
    Ok(json!({ "success": true, "message": format!("Rede \"{name}\" criada com sucesso.") }))
}
pub async fn connect_network(network: &str, container: String) -> Result<Value, DockerError> {
    call(client()?.connect_network(
        network,
        ConnectNetworkOptions {
            container,
            ..Default::default()
        },
    ))
    .await?;
    Ok(json!({ "success": true, "message": "Container conectado a rede com sucesso." }))
}
pub async fn disconnect_network(
    network: &str,
    container: String,
    force: bool,
) -> Result<Value, DockerError> {
    call(client()?.disconnect_network(network, DisconnectNetworkOptions { container, force }))
        .await?;
    Ok(json!({ "success": true, "message": "Container desconectado da rede com sucesso." }))
}

pub async fn list_images() -> Result<Value, DockerError> {
    value(call(client()?.list_images(Some(ListImagesOptions::<String>::default()))).await?)
}
pub async fn inspect_image(id: &str) -> Result<Value, DockerError> {
    value(call(client()?.inspect_image(id)).await?)
}
pub async fn remove_image(id: &str, force: bool) -> Result<Value, DockerError> {
    value(
        call(client()?.remove_image(
            id,
            Some(RemoveImageOptions {
                force,
                ..Default::default()
            }),
            None,
        ))
        .await?,
    )
}
pub async fn prune_images() -> Result<Value, DockerError> {
    value(call(client()?.prune_images::<String>(None)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn probing_never_panics_when_the_engine_is_absent() {
        assert!(matches!(status().await.available, true | false));
    }
    #[tokio::test]
    async fn environment_always_has_the_frontend_contract_keys() {
        let context = environment().await;
        assert!(context["dockerAvailable"].is_boolean());
        assert!(context["backendNetworkIds"].is_array());
    }
}
