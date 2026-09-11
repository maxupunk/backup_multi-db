//! Fronteira com a Docker Engine (Fase 9).
//!
//! Todas as operacoes passam por esta camada. Os handlers nunca conhecem
//! `bollard`, o que deixa o contrato HTTP separado do transporte local da
//! Engine e evita depender do binario `docker` dentro do container da API.

use std::collections::HashMap;
use std::time::Duration;

use bollard::container::{ListContainersOptions, LogsOptions, RemoveContainerOptions};
use bollard::image::{ListImagesOptions, PruneImagesOptions, RemoveImageOptions};
use bollard::network::{
    ConnectNetworkOptions, CreateNetworkOptions, DisconnectNetworkOptions, ListNetworksOptions,
};
use bollard::volume::{ListVolumesOptions, RemoveVolumeOptions};
use bollard::Docker;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{json, Map, Value};

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
    #[error("Rede em uso")]
    NetworkInUse {
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

fn init_client() -> Result<Docker, DockerError> {
    Docker::connect_with_defaults()
        .or_else(|_| Docker::connect_with_local_defaults())
        .map_err(|err| {
            tracing::warn!("Failed to create Docker client: {err}");
            DockerError::Unavailable
        })
}

pub fn client() -> Result<Docker, DockerError> {
    static SHARED_CLIENT: std::sync::OnceLock<Docker> = std::sync::OnceLock::new();
    if let Some(c) = SHARED_CLIENT.get() {
        return Ok(c.clone());
    }
    let c = init_client()?;
    let _ = SHARED_CLIENT.set(c.clone());
    Ok(c)
}

async fn call<T>(
    future: impl std::future::Future<Output = Result<T, bollard::errors::Error>>,
) -> Result<T, DockerError> {
    future.await.map_err(|err| {
        tracing::error!("Docker Engine call failed: {err}");
        DockerError::Engine
    })
}

fn value<T: Serialize>(item: T) -> Result<Value, DockerError> {
    serde_json::to_value(item).map_err(|_| DockerError::Engine)
}

/// The Docker Engine uses PascalCase fields while the application API uses
/// camelCase. Keep that translation at this boundary so neither the HTTP
/// handlers nor the frontend depend on Bollard's serialization format.
fn field<'a>(value: &'a Value, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| value.get(*name))
}

fn string_field(value: &Value, names: &[&str]) -> String {
    field(value, names)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number_field(value: &Value, names: &[&str]) -> i64 {
    field(value, names)
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

fn bool_field(value: &Value, names: &[&str]) -> bool {
    field(value, names)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn string_map(value: Option<&Value>) -> Value {
    let entries = value
        .and_then(Value::as_object)
        .map(|items| {
            items
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|value| (key.clone(), Value::String(value.to_string())))
                })
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();
    Value::Object(entries)
}

fn array_or_empty(value: Option<&Value>) -> Value {
    Value::Array(value.and_then(Value::as_array).cloned().unwrap_or_default())
}

fn normalize_container_summary(item: &Value) -> Value {
    json!({
        "id": string_field(item, &["Id", "id"]),
        "names": array_or_empty(field(item, &["Names", "names"])),
        "image": string_field(item, &["Image", "image"]),
        "imageId": string_field(item, &["ImageID", "imageId"]),
        "state": string_field(item, &["State", "state"]),
        "status": string_field(item, &["Status", "status"]),
        "labels": string_map(field(item, &["Labels", "labels"])),
        // The port object deliberately preserves Docker's established
        // `PrivatePort`/`PublicPort` spelling, which is the UI contract.
        "ports": array_or_empty(field(item, &["Ports", "ports"])),
        "created": number_field(item, &["Created", "created"]),
    })
}

fn normalize_container_detail(item: &Value) -> Value {
    let state = field(item, &["State", "state"]).unwrap_or(&Value::Null);
    let config = field(item, &["Config", "config"]).unwrap_or(&Value::Null);
    let host_config = field(item, &["HostConfig", "hostConfig"]).unwrap_or(&Value::Null);
    let restart_policy =
        field(host_config, &["RestartPolicy", "restartPolicy"]).unwrap_or(&Value::Null);
    let networks = field(item, &["NetworkSettings", "networkSettings"])
        .and_then(|settings| field(settings, &["Networks", "networks"]))
        .and_then(Value::as_object);

    let networks = networks
        .map(|networks| {
            networks
                .iter()
                .map(|(name, endpoint)| {
                    json!({
                        "networkId": string_field(endpoint, &["NetworkID", "networkId"]),
                        "networkName": name,
                        "ipAddress": string_field(endpoint, &["IPAddress", "ipAddress"]),
                        "gateway": string_field(endpoint, &["Gateway", "gateway"]),
                        "aliases": field(endpoint, &["Aliases", "aliases"]).cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mounts = field(item, &["Mounts", "mounts"])
        .and_then(Value::as_array)
        .map(|mounts| {
            mounts
                .iter()
                .map(|mount| {
                    json!({
                        "type": string_field(mount, &["Type", "type"]),
                        "name": field(mount, &["Name", "name"]).cloned().unwrap_or(Value::Null),
                        "source": string_field(mount, &["Source", "source"]),
                        "destination": string_field(mount, &["Destination", "destination"]),
                        "mode": string_field(mount, &["Mode", "mode"]),
                        "rw": bool_field(mount, &["RW", "rw"]),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    json!({
        "id": string_field(item, &["Id", "id"]),
        "name": string_field(item, &["Name", "name"]),
        "image": string_field(config, &["Image", "image"]),
        "imageId": string_field(item, &["Image", "imageId"]),
        "created": string_field(item, &["Created", "created"]),
        "state": {
            "status": string_field(state, &["Status", "status"]),
            "running": bool_field(state, &["Running", "running"]),
            "paused": bool_field(state, &["Paused", "paused"]),
            "restarting": bool_field(state, &["Restarting", "restarting"]),
            "pid": number_field(state, &["Pid", "pid"]),
            "startedAt": string_field(state, &["StartedAt", "startedAt"]),
            "finishedAt": string_field(state, &["FinishedAt", "finishedAt"]),
            "exitCode": number_field(state, &["ExitCode", "exitCode"]),
        },
        "config": {
            "hostname": string_field(config, &["Hostname", "hostname"]),
            "env": array_or_empty(field(config, &["Env", "env"])),
            "cmd": field(config, &["Cmd", "cmd"]).cloned().unwrap_or(Value::Null),
            "entrypoint": field(config, &["Entrypoint", "entrypoint"]).cloned().unwrap_or(Value::Null),
            "labels": string_map(field(config, &["Labels", "labels"])),
            "workingDir": string_field(config, &["WorkingDir", "workingDir"]),
            "user": string_field(config, &["User", "user"]),
        },
        "hostConfig": {
            "restartPolicy": {
                "name": string_field(restart_policy, &["Name", "name"]),
                "maximumRetryCount": number_field(restart_policy, &["MaximumRetryCount", "maximumRetryCount"]),
            },
            "networkMode": string_field(host_config, &["NetworkMode", "networkMode"]),
        },
        "mounts": mounts,
        "networks": networks,
    })
}

fn normalize_volume(item: &Value, include_detail: bool) -> Value {
    let mut volume = Map::new();
    volume.insert(
        "name".into(),
        Value::String(string_field(item, &["Name", "name"])),
    );
    volume.insert(
        "driver".into(),
        Value::String(string_field(item, &["Driver", "driver"])),
    );
    volume.insert(
        "mountpoint".into(),
        Value::String(string_field(item, &["Mountpoint", "mountpoint"])),
    );
    volume.insert(
        "labels".into(),
        string_map(field(item, &["Labels", "labels"])),
    );
    volume.insert(
        "scope".into(),
        Value::String(string_field(item, &["Scope", "scope"])),
    );
    if let Some(created_at) = field(item, &["CreatedAt", "createdAt"]).and_then(Value::as_str) {
        volume.insert("createdAt".into(), Value::String(created_at.to_string()));
    }
    let usage_data = field(item, &["UsageData", "usageData"]);
    let size = field(item, &["Size", "size"])
        .and_then(Value::as_i64)
        .or_else(|| usage_data.and_then(|u| field(u, &["Size", "size"])).and_then(Value::as_i64));
    if let Some(size) = size {
        volume.insert("size".into(), json!(size));
    }
    if let Some(usage) = usage_data {
        let mut usage_obj = Map::new();
        if let Some(size) = field(usage, &["Size", "size"]).and_then(Value::as_i64) {
            usage_obj.insert("size".into(), json!(size));
        }
        if let Some(ref_count) = field(usage, &["RefCount", "refCount"]).and_then(Value::as_i64) {
            usage_obj.insert("refCount".into(), json!(ref_count));
        }
        volume.insert("usageData".into(), Value::Object(usage_obj));
    }
    if include_detail {
        volume.insert(
            "options".into(),
            string_map(field(item, &["Options", "options"])),
        );
        if let Some(status) = field(item, &["Status", "status"]).and_then(Value::as_object) {
            volume.insert("status".into(), Value::Object(status.clone()));
        }
    }
    Value::Object(volume)
}

fn normalize_network_containers(value: Option<&Value>) -> Value {
    let containers = value
        .and_then(Value::as_object)
        .map(|containers| {
            containers
                .iter()
                .map(|(id, container)| {
                    (
                        id.clone(),
                        json!({
                            "containerId": id,
                            "name": string_field(container, &["Name", "name"]),
                            "macAddress": string_field(container, &["MacAddress", "macAddress"]),
                            "ipv4Address": string_field(container, &["IPv4Address", "ipv4Address"]),
                            "ipv6Address": string_field(container, &["IPv6Address", "ipv6Address"]),
                        }),
                    )
                })
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();
    Value::Object(containers)
}

fn normalize_network(item: &Value, include_detail: bool) -> Value {
    let ipam = field(item, &["IPAM", "ipam"]).unwrap_or(&Value::Null);
    let config = field(ipam, &["Config", "config"])
        .and_then(Value::as_array)
        .map(|config| {
            config
                .iter()
                .map(|entry| {
                    json!({
                        "subnet": field(entry, &["Subnet", "subnet"]).cloned().unwrap_or(Value::Null),
                        "gateway": field(entry, &["Gateway", "gateway"]).cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let raw_containers = field(item, &["Containers", "containers"]);
    let connected_containers = raw_containers
        .and_then(Value::as_object)
        .map_or(0, |containers| containers.len());

    let mut network = Map::new();
    network.insert(
        "id".into(),
        Value::String(string_field(item, &["Id", "id"])),
    );
    network.insert(
        "name".into(),
        Value::String(string_field(item, &["Name", "name"])),
    );
    network.insert(
        "driver".into(),
        Value::String(string_field(item, &["Driver", "driver"])),
    );
    network.insert(
        "scope".into(),
        Value::String(string_field(item, &["Scope", "scope"])),
    );
    network.insert(
        "ipam".into(),
        json!({ "driver": string_field(ipam, &["Driver", "driver"]), "config": config }),
    );
    network.insert(
        "internal".into(),
        Value::Bool(bool_field(item, &["Internal", "internal"])),
    );
    network.insert("connectedContainers".into(), json!(connected_containers));
    network.insert(
        "labels".into(),
        string_map(field(item, &["Labels", "labels"])),
    );
    network.insert(
        "created".into(),
        Value::String(string_field(item, &["Created", "created"])),
    );
    if include_detail {
        network.insert(
            "containers".into(),
            normalize_network_containers(raw_containers),
        );
        network.insert(
            "options".into(),
            string_map(field(item, &["Options", "options"])),
        );
    }
    Value::Object(network)
}

fn container_display_name(container: &bollard::models::ContainerSummary) -> String {
    container
        .names
        .as_ref()
        .and_then(|names| names.first())
        .map(|name| name.trim_start_matches('/').to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| container.id.as_deref().map(short_id))
        .unwrap_or_else(|| "container desconhecido".into())
}

fn running_containers_for_network(
    network_id: &str,
    network_name: &str,
    containers: &[bollard::models::ContainerSummary],
) -> Vec<String> {
    let mut names = containers
        .iter()
        .filter(|container| container.state.as_deref() == Some("running"))
        .filter(|container| {
            container
                .network_settings
                .as_ref()
                .and_then(|settings| settings.networks.as_ref())
                .is_some_and(|networks| {
                    networks.iter().any(|(name, endpoint)| {
                        name == network_name || endpoint.network_id.as_deref() == Some(network_id)
                    })
                })
        })
        .map(container_display_name)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn add_network_runtime_usage(
    network: &mut Value,
    running_containers: &[bollard::models::ContainerSummary],
) {
    let network_id = network["id"].as_str().unwrap_or_default();
    let network_name = network["name"].as_str().unwrap_or_default();
    let names = running_containers_for_network(network_id, network_name, running_containers);

    if let Some(fields) = network.as_object_mut() {
        fields.insert("runningContainers".into(), json!(names.len()));
        fields.insert("runningContainerNames".into(), json!(names));
    }
}

fn normalize_image_summary(item: &Value) -> Value {
    json!({
        "id": string_field(item, &["Id", "id"]),
        "parentId": string_field(item, &["ParentId", "parentId"]),
        "repoTags": array_or_empty(field(item, &["RepoTags", "repoTags"])),
        "repoDigests": array_or_empty(field(item, &["RepoDigests", "repoDigests"])),
        "created": number_field(item, &["Created", "created"]),
        "size": number_field(item, &["Size", "size"]),
        "sharedSize": number_field(item, &["SharedSize", "sharedSize"]),
        "labels": string_map(field(item, &["Labels", "labels"])),
        "containers": number_field(item, &["Containers", "containers"]),
    })
}

fn normalize_image_detail(item: &Value) -> Value {
    let config = field(item, &["Config", "config"]).unwrap_or(&Value::Null);
    let root_fs = field(item, &["RootFS", "rootFs"]).unwrap_or(&Value::Null);
    json!({
        "id": string_field(item, &["Id", "id"]),
        "repoTags": array_or_empty(field(item, &["RepoTags", "repoTags"])),
        "created": string_field(item, &["Created", "created"]),
        "size": number_field(item, &["Size", "size"]),
        "config": {
            "env": field(config, &["Env", "env"]).cloned().unwrap_or(Value::Null),
            "cmd": field(config, &["Cmd", "cmd"]).cloned().unwrap_or(Value::Null),
            "entrypoint": field(config, &["Entrypoint", "entrypoint"]).cloned().unwrap_or(Value::Null),
            "labels": string_map(field(config, &["Labels", "labels"])),
            "workingDir": string_field(config, &["WorkingDir", "workingDir"]),
            "user": string_field(config, &["User", "user"]),
        },
        "rootFs": {
            "type": string_field(root_fs, &["Type", "type"]),
            "layers": array_or_empty(field(root_fs, &["Layers", "layers"])),
        },
    })
}

/// Sonda a Engine local sem transformar sua ausencia em erro HTTP.
pub async fn status() -> Status {
    let Ok(client) = client() else {
        tracing::warn!("Docker client could not be initialized from local defaults");
        return Status { available: false };
    };
    let available = match tokio::time::timeout(PING_TIMEOUT, client.ping()).await {
        Ok(Ok(_)) => {
            let _ = client.negotiate_version().await;
            true
        }
        Ok(Err(err)) => {
            tracing::warn!("Docker ping failed: {err}");
            false
        }
        Err(_) => {
            tracing::warn!("Docker ping timed out after {PING_TIMEOUT:?}");
            false
        }
    };
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
        groups
            .entry(project)
            .or_default()
            .push(normalize_container_summary(&item));
    }
    let mut result: Vec<Value> = groups.into_iter().map(|(project_name, containers)| json!({ "projectName": project_name, "containers": containers })).collect();
    result.sort_by(|a, b| a["projectName"].as_str().cmp(&b["projectName"].as_str()));
    Ok(Value::Array(result))
}

/// Descobre containers de banco em execucao para a tela de conexoes.
pub async fn discover_database_hosts(
) -> Result<Vec<crate::models::docker_connection_suggestion::HostSuggestion>, DockerError> {
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
        if let Some(descriptor) = suggestion::descriptor_from_bollard(&summary, &inspect) {
            descriptors.push(descriptor);
        }
    }

    Ok(suggestion::ConnectionSuggestionMapper::map(
        &descriptors,
        &context,
    ))
}

pub async fn inspect_container(id: &str) -> Result<Value, DockerError> {
    let container = value(call(client()?.inspect_container(id, None)).await?)?;
    Ok(normalize_container_detail(&container))
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
            bollard::container::LogOutput::Console { message } => ("stdout", message),
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
    let client = client()?;
    let list_future = client.list_volumes(Some(ListVolumesOptions::<String>::default()));
    let df_future = client.df();

    let (list_res, df_res) = tokio::join!(call(list_future), df_future);
    let response = value(list_res?)?;

    let mut usage_map: HashMap<String, (i64, i64)> = HashMap::new();
    if let Ok(df) = df_res {
        if let Some(vols) = df.volumes {
            for v in vols {
                if let Some(ud) = v.usage_data {
                    usage_map.insert(v.name, (ud.size, ud.ref_count));
                }
            }
        }
    }

    let volumes = response
        .get("Volumes")
        .or_else(|| response.get("volumes"))
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    Ok(Value::Array(
        volumes
            .as_array()
            .map(|volumes| {
                volumes
                    .iter()
                    .map(|volume| {
                        let mut norm = normalize_volume(volume, false);
                        if let Some(name) = norm["name"].as_str() {
                            if let Some(&(size, ref_count)) = usage_map.get(name) {
                                if let Some(obj) = norm.as_object_mut() {
                                    obj.insert("size".into(), json!(size));
                                    obj.insert(
                                        "usageData".into(),
                                        json!({
                                            "size": size,
                                            "refCount": ref_count,
                                        }),
                                    );
                                }
                            }
                        }
                        norm
                    })
                    .collect()
            })
            .unwrap_or_default(),
    ))
}
pub async fn inspect_volume(name: &str) -> Result<Value, DockerError> {
    let client = client()?;
    let inspect_future = client.inspect_volume(name);
    let df_future = client.df();

    let (inspect_res, df_res) = tokio::join!(call(inspect_future), df_future);
    let volume = value(inspect_res?)?;
    let mut norm = normalize_volume(&volume, true);

    if let Ok(df) = df_res {
        if let Some(vols) = df.volumes {
            if let Some(v) = vols.into_iter().find(|v| v.name == name) {
                if let Some(ud) = v.usage_data {
                    if let Some(obj) = norm.as_object_mut() {
                        obj.insert("size".into(), json!(ud.size));
                        obj.insert(
                            "usageData".into(),
                            json!({
                                "size": ud.size,
                                "refCount": ud.ref_count,
                            }),
                        );
                    }
                }
            }
        }
    }

    Ok(norm)
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
            tracing::error!("Docker remove_volume failed for {name}: {err}");
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
    let client = client()?;
    let networks =
        call(client.list_networks(Some(ListNetworksOptions::<String>::default()))).await?;
    let running_containers = call(
        client.list_containers(Some(ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        })),
    )
    .await?;

    Ok(Value::Array(
        networks
            .into_iter()
            .map(|network| {
                let mut network = normalize_network(&value(network)?, false);
                add_network_runtime_usage(&mut network, &running_containers);
                Ok(network)
            })
            .collect::<Result<Vec<_>, DockerError>>()?,
    ))
}
pub async fn inspect_network(id: &str) -> Result<Value, DockerError> {
    let client = client()?;
    let network = value(call(client.inspect_network::<String>(id, None)).await?)?;
    let running_containers = call(
        client.list_containers(Some(ListContainersOptions::<String> {
            all: false,
            ..Default::default()
        })),
    )
    .await?;
    let mut network = normalize_network(&network, true);
    add_network_runtime_usage(&mut network, &running_containers);
    Ok(network)
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
pub async fn remove_network(id: &str) -> Result<Value, DockerError> {
    let client = client()?;
    let network = call(client.inspect_network::<String>(id, None)).await?;
    let network_name = network.name.as_deref().unwrap_or(id);
    let containers = call(
        client.list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        })),
    )
    .await?;
    let running = running_containers_for_network(id, network_name, &containers);

    if !running.is_empty() {
        return Err(DockerError::NetworkInUse {
            message: format!(
                "A rede está em uso por contêineres em execução: {}. Pare-os antes de remover a rede.",
                running.join(", ")
            ),
            container_names: running,
        });
    }

    // A Engine também mantém endpoints de contêineres parados. A regra desta
    // operação permite a remoção nesse caso, então eles são desconectados antes.
    for container_id in network.containers.unwrap_or_default().keys() {
        call(client.disconnect_network(
            id,
            DisconnectNetworkOptions {
                container: container_id.clone(),
                force: true,
            },
        ))
        .await?;
    }

    call(client.remove_network(id)).await?;
    Ok(json!({ "success": true, "message": "Rede removida com sucesso." }))
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
    let images =
        value(call(client()?.list_images(Some(ListImagesOptions::<String>::default()))).await?)?;
    Ok(Value::Array(
        images
            .as_array()
            .map(|images| images.iter().map(normalize_image_summary).collect())
            .unwrap_or_default(),
    ))
}
pub async fn inspect_image(id: &str) -> Result<Value, DockerError> {
    let image = value(call(client()?.inspect_image(id)).await?)?;
    Ok(normalize_image_detail(&image))
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

pub async fn system_df() -> Result<Value, DockerError> {
    let client = client()?;
    let df = call(client.df()).await?;

    let layers_size = df.layers_size.unwrap_or(0);

    let (img_total, img_active, img_size, img_reclaimable) = match df.images {
        Some(images) => {
            let total = images.len();
            let mut active = 0;
            let mut size = 0i64;
            let mut reclaimable = 0i64;
            for img in images {
                let s = img.size;
                size += s;
                if img.containers > 0 {
                    active += 1;
                } else {
                    reclaimable += s;
                }
            }
            (total, active, size, reclaimable)
        }
        None => (0, 0, 0, 0),
    };

    let (cnt_total, cnt_active, cnt_size, cnt_reclaimable) = match df.containers {
        Some(containers) => {
            let total = containers.len();
            let mut active = 0;
            let mut size = 0i64;
            let mut reclaimable = 0i64;
            for c in containers {
                let s = c.size_rw.unwrap_or(0);
                size += s;
                if c.state.as_deref() == Some("running") {
                    active += 1;
                } else {
                    reclaimable += s;
                }
            }
            (total, active, size, reclaimable)
        }
        None => (0, 0, 0, 0),
    };

    let (vol_total, vol_active, vol_size, vol_reclaimable) = match df.volumes {
        Some(volumes) => {
            let total = volumes.len();
            let mut active = 0;
            let mut size = 0i64;
            let mut reclaimable = 0i64;
            for v in volumes {
                if let Some(usage) = v.usage_data {
                    let s = usage.size;
                    size += s;
                    if usage.ref_count > 0 {
                        active += 1;
                    } else {
                        reclaimable += s;
                    }
                }
            }
            (total, active, size, reclaimable)
        }
        None => (0, 0, 0, 0),
    };

    let (bc_total, bc_active, bc_size, bc_reclaimable) = match df.build_cache {
        Some(caches) => {
            let total = caches.len();
            let mut active = 0;
            let mut size = 0i64;
            let mut reclaimable = 0i64;
            for b in caches {
                let s = b.size.unwrap_or(0);
                size += s;
                if b.in_use.unwrap_or(false) {
                    active += 1;
                } else {
                    reclaimable += s;
                }
            }
            (total, active, size, reclaimable)
        }
        None => (0, 0, 0, 0),
    };

    let total_size = img_size + cnt_size + vol_size + bc_size;
    let total_reclaimable = img_reclaimable + cnt_reclaimable + vol_reclaimable + bc_reclaimable;

    Ok(json!({
        "layersSize": layers_size,
        "totalSize": total_size,
        "totalReclaimable": total_reclaimable,
        "images": {
            "totalCount": img_total,
            "activeCount": img_active,
            "totalSize": img_size,
            "reclaimableSize": img_reclaimable,
        },
        "containers": {
            "totalCount": cnt_total,
            "activeCount": cnt_active,
            "totalSize": cnt_size,
            "reclaimableSize": cnt_reclaimable,
        },
        "volumes": {
            "totalCount": vol_total,
            "activeCount": vol_active,
            "totalSize": vol_size,
            "reclaimableSize": vol_reclaimable,
        },
        "buildCache": {
            "totalCount": bc_total,
            "activeCount": bc_active,
            "totalSize": bc_size,
            "reclaimableSize": bc_reclaimable,
        }
    }))
}

async fn prune_build_cache_raw(all: bool) -> (Vec<String>, i64) {
    let docker_host = std::env::var("DOCKER_HOST").ok();
    let query = if all { "all=true" } else { "all=false" };
    let req = format!(
        "POST /build/prune?{} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        query
    );

    let res: Option<Vec<u8>> = async {
        #[cfg(windows)]
        {
            if let Some(host) = &docker_host {
                if host.starts_with("tcp://") {
                    let addr = host.trim_start_matches("tcp://");
                    let mut stream = tokio::net::TcpStream::connect(addr).await.ok()?;
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    stream.write_all(req.as_bytes()).await.ok()?;
                    let mut buf = Vec::new();
                    stream.read_to_end(&mut buf).await.ok()?;
                    return Some(buf);
                }
            }
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::windows::named_pipe::ClientOptions;
            let mut client = ClientOptions::new().open(r"\\.\pipe\docker_engine").ok()?;
            client.write_all(req.as_bytes()).await.ok()?;
            let mut buf = Vec::new();
            client.read_to_end(&mut buf).await.ok()?;
            Some(buf)
        }
        #[cfg(unix)]
        {
            if let Some(host) = &docker_host {
                if host.starts_with("tcp://") {
                    let addr = host.trim_start_matches("tcp://");
                    let mut stream = tokio::net::TcpStream::connect(addr).await.ok()?;
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    stream.write_all(req.as_bytes()).await.ok()?;
                    let mut buf = Vec::new();
                    stream.read_to_end(&mut buf).await.ok()?;
                    return Some(buf);
                }
                if host.starts_with("unix://") {
                    let path = host.trim_start_matches("unix://");
                    let mut stream = tokio::net::UnixStream::connect(path).await.ok()?;
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    stream.write_all(req.as_bytes()).await.ok()?;
                    let mut buf = Vec::new();
                    stream.read_to_end(&mut buf).await.ok()?;
                    return Some(buf);
                }
            }
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::UnixStream;
            let mut stream = UnixStream::connect("/var/run/docker.sock").await.ok()?;
            stream.write_all(req.as_bytes()).await.ok()?;
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.ok()?;
            Some(buf)
        }
    }
    .await;

    let Some(bytes) = res else {
        return (Vec::new(), 0);
    };

    let text = String::from_utf8_lossy(&bytes);
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let json_slice = &text[start..=end];
                if let Ok(parsed) = serde_json::from_str::<Value>(json_slice) {
                    let space = parsed.get("SpaceReclaimed").and_then(Value::as_i64).unwrap_or(0);
                    let caches = parsed
                        .get("CachesDeleted")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    return (caches, space);
                }
            }
        }
    }

    (Vec::new(), 0)
}

pub async fn prune_system(all: bool, prune_vols: bool) -> Result<Value, DockerError> {
    let client = client()?;

    let mut total_space_reclaimed: i64 = 0;

    let container_res = call(client.prune_containers::<String>(None)).await?;
    let containers_deleted = container_res.containers_deleted.unwrap_or_default();
    total_space_reclaimed += container_res.space_reclaimed.unwrap_or(0) as i64;

    let network_res = call(client.prune_networks::<String>(None)).await?;
    let networks_deleted = network_res.networks_deleted.unwrap_or_default();

    let mut image_filters = HashMap::new();
    image_filters.insert(
        "dangling".to_string(),
        vec![if all { "false".to_string() } else { "true".to_string() }],
    );
    let image_res = call(client.prune_images(Some(PruneImagesOptions {
        filters: image_filters,
    })))
    .await?;
    let images_deleted = image_res
        .images_deleted
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            json!({
                "untagged": item.untagged,
                "deleted": item.deleted,
            })
        })
        .collect::<Vec<_>>();
    total_space_reclaimed += image_res.space_reclaimed.unwrap_or(0) as i64;

    let mut volumes_deleted = Vec::new();
    if prune_vols {
        if let Ok(vol_res) = client.prune_volumes::<String>(None).await {
            total_space_reclaimed += vol_res.space_reclaimed.unwrap_or(0) as i64;
            volumes_deleted = vol_res.volumes_deleted.unwrap_or_default();
        }
    }

    // Build Cache (BuildKit / Moby builder)
    let (build_caches_deleted, build_cache_space) = prune_build_cache_raw(all).await;
    total_space_reclaimed += build_cache_space;

    Ok(json!({
        "spaceReclaimed": total_space_reclaimed,
        "containersDeleted": containers_deleted,
        "imagesDeleted": images_deleted,
        "networksDeleted": networks_deleted,
        "volumesDeleted": volumes_deleted,
        "buildCacheDeleted": build_caches_deleted,
    }))
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

    #[test]
    fn docker_engine_summaries_are_mapped_to_the_frontend_contract() {
        let container = normalize_container_summary(&json!({
            "Id": "container-id",
            "Names": ["/database"],
            "Image": "postgres:18",
            "ImageID": "sha256:image",
            "State": "running",
            "Status": "Up 1 minute",
            "Labels": { "com.docker.compose.project": "sample" },
            "Ports": [{ "PrivatePort": 5432, "Type": "tcp" }],
            "Created": 1,
        }));
        assert_eq!(container["id"], "container-id");
        assert_eq!(container["names"][0], "/database");
        assert_eq!(container["labels"]["com.docker.compose.project"], "sample");

        let volume = normalize_volume(
            &json!({
                "Name": "database-data",
                "Driver": "local",
                "Mountpoint": "/var/lib/docker/volumes/database-data/_data",
                "Labels": { "com.docker.compose.project": "sample" },
                "Scope": "local",
                "UsageData": {
                    "Size": 1048576,
                    "RefCount": 2
                }
            }),
            false,
        );
        assert_eq!(volume["name"], "database-data");
        assert_eq!(volume["labels"]["com.docker.compose.project"], "sample");
        assert_eq!(volume["size"], 1048576);
        assert_eq!(volume["usageData"]["size"], 1048576);
        assert_eq!(volume["usageData"]["refCount"], 2);

        let network = normalize_network(
            &json!({
                "Id": "network-id",
                "Name": "sample_default",
                "Driver": "bridge",
                "Scope": "local",
                "IPAM": { "Driver": "default", "Config": [{ "Subnet": "172.18.0.0/16" }] },
                "Internal": false,
                "Containers": { "container-id": { "Name": "database", "IPv4Address": "172.18.0.2/16" } },
            }),
            true,
        );
        assert_eq!(network["id"], "network-id");
        assert_eq!(network["ipam"]["config"][0]["subnet"], "172.18.0.0/16");
        assert_eq!(network["connectedContainers"], 1);
        assert_eq!(network["containers"]["container-id"]["name"], "database");
    }

    #[test]
    fn only_running_containers_block_network_removal() {
        let containers = serde_json::from_value::<Vec<bollard::models::ContainerSummary>>(json!([
            {
                "Id": "running-id",
                "Names": ["/database"],
                "State": "running",
                "NetworkSettings": {
                    "Networks": {
                        "sample_default": { "NetworkID": "network-id" }
                    }
                }
            },
            {
                "Id": "stopped-id",
                "Names": ["/worker"],
                "State": "exited",
                "NetworkSettings": {
                    "Networks": {
                        "sample_default": { "NetworkID": "network-id" }
                    }
                }
            }
        ]))
        .expect("resumo de containers válido");

        let running = running_containers_for_network("network-id", "sample_default", &containers);

        assert_eq!(running, vec!["database"]);
    }
}
