//! Sugestao de conexoes a partir de containers Docker (tarefa 6.8).
//!
//! Porte dos resolvers do Adonis (`connection_suggestion_mapper`,
//! `network_reachability_resolver`, `container_port_resolver` e
//! `connection_port_selection_resolver`). A logica e' a mesma: dado o contexto
//! de rede do backend, sugerir o host e a porta corretos para conectar em um
//! banco rodando dentro de um container.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tipo de banco inferido a partir do nome/imagem/ports do container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseTypeHint {
    Postgresql,
    Mysql,
    Mariadb,
}

impl DatabaseTypeHint {
    fn as_str(&self) -> &'static str {
        match self {
            DatabaseTypeHint::Postgresql => "postgresql",
            DatabaseTypeHint::Mysql => "mysql",
            DatabaseTypeHint::Mariadb => "mariadb",
        }
    }

    fn expected_port(&self) -> u16 {
        match self {
            DatabaseTypeHint::Postgresql => 5432,
            DatabaseTypeHint::Mysql | DatabaseTypeHint::Mariadb => 3306,
        }
    }
}

/// Origem do host sugerido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub enum HostResolutionSource {
    DockerDns,
    HostIp,
    Fallback,
}

/// Rede a que um container esta' conectado.
#[derive(Debug, Clone)]
pub struct NetworkAttachment {
    pub network_id: String,
    pub network_name: String,
    pub aliases: Vec<String>,
    pub gateway: Option<String>,
    pub ip_address: Option<String>,
}

/// Binding de porta de um container.
#[derive(Debug, Clone)]
pub struct PortBinding {
    pub container_port: u16,
    pub host_port: Option<u16>,
    pub protocol: String,
}

/// Descritor de um container elegivel para conexao.
#[derive(Debug, Clone)]
pub struct ContainerDescriptor {
    pub container_id: String,
    pub container_name: String,
    pub image_name: String,
    pub labels: std::collections::HashMap<String, String>,
    pub database_type_hint: Option<DatabaseTypeHint>,
    pub networks: Vec<NetworkAttachment>,
    pub ports: Vec<PortBinding>,
}

/// Opcao de porta apresentada na UI.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct PortOption {
    #[ts(type = "number")]
    pub container_port: u16,
    #[ts(type = "number")]
    pub host_port: u16,
    pub protocol: String,
    pub display: String,
    pub is_external: bool,
}

/// Sugestao final para um container.
///
/// Serializa direto para o payload de `GET /api/connections/docker-hosts`: o
/// `rename_all` ja' produz o camelCase que o frontend espera, e o binding
/// `ts-rs` e' gerado a partir desta struct. Havia uma funcao que remontava o
/// mesmo objeto campo a campo com `json!` — duas descricoes do mesmo formato,
/// e nada obrigando as duas a concordarem.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct HostSuggestion {
    pub container_id: String,
    pub container_name: String,
    #[ts(type = "\"mysql\" | \"mariadb\" | \"postgresql\" | null")]
    pub database_type_hint: Option<String>,
    pub same_network: bool,
    pub suggested_host: String,
    pub host_resolution_source: HostResolutionSource,
    pub network_names: Vec<String>,
    pub port_options: Vec<PortOption>,
    #[ts(type = "number | null")]
    pub recommended_port: Option<u16>,
    pub has_external_port: bool,
    pub connectivity_warning: Option<String>,
}

/// Contexto de rede do proprio backend.
#[derive(Debug, Clone)]
pub struct BackendContext {
    pub backend_container_id: Option<String>,
    pub backend_network_ids: Vec<String>,
    pub docker_host_ip: String,
}

impl BackendContext {
    /// Constroi o contexto a partir das informacoes de ambiente.
    pub fn from_environment(
        backend_container_id: Option<String>,
        backend_networks: Vec<NetworkAttachment>,
    ) -> Self {
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

        Self {
            backend_network_ids: backend_networks
                .into_iter()
                .map(|network| network.network_id)
                .collect(),
            docker_host_ip,
            backend_container_id,
        }
    }
}

/// Resolve o host sugerido com base na rede compartilhada entre backend e
/// container do banco.
pub struct NetworkReachabilityResolver;

impl NetworkReachabilityResolver {
    pub fn resolve(
        container: &ContainerDescriptor,
        backend_network_ids: &[String],
        docker_host_ip: &str,
    ) -> ResolvedHost {
        let container_network_ids: Vec<&str> = container
            .networks
            .iter()
            .map(|network| network.network_id.as_str())
            .collect();
        let same_network = container_network_ids.iter().any(|id| {
            backend_network_ids
                .iter()
                .any(|backend_id| backend_id == *id)
        });

        if same_network {
            if let Some(alias) = Self::preferred_alias(container) {
                return ResolvedHost {
                    same_network: true,
                    suggested_host: alias,
                    host_resolution_source: HostResolutionSource::DockerDns,
                };
            }
        }

        if !docker_host_ip.is_empty() {
            return ResolvedHost {
                same_network: false,
                suggested_host: docker_host_ip.into(),
                host_resolution_source: HostResolutionSource::HostIp,
            };
        }

        ResolvedHost {
            same_network: false,
            suggested_host: container.container_name.clone(),
            host_resolution_source: HostResolutionSource::Fallback,
        }
    }

    fn preferred_alias(container: &ContainerDescriptor) -> Option<String> {
        for network in &container.networks {
            for alias in &network.aliases {
                let alias = alias.trim();
                if alias.is_empty() {
                    continue;
                }
                if alias == container.container_id
                    || alias == &container.container_id[..12.min(container.container_id.len())]
                {
                    continue;
                }
                return Some(alias.to_string());
            }
        }
        Some(container.container_name.clone())
    }
}

pub struct ResolvedHost {
    pub same_network: bool,
    pub suggested_host: String,
    pub host_resolution_source: HostResolutionSource,
}

/// Resolve as opcoes de porta (interna e, quando existir, publicada no host).
pub struct ContainerPortResolver;

impl ContainerPortResolver {
    pub fn resolve(container: &ContainerDescriptor) -> Vec<PortOption> {
        let expected_port = container
            .database_type_hint
            .map(|hint| hint.expected_port());

        let filtered_ports: Vec<&PortBinding> = container
            .ports
            .iter()
            .filter(|port| expected_port.is_none_or(|expected| port.container_port == expected))
            .collect();

        let mut options = Vec::with_capacity(filtered_ports.len() * 2);
        for port in &filtered_ports {
            options.push(PortOption {
                container_port: port.container_port,
                host_port: port.container_port,
                protocol: port.protocol.clone(),
                display: format!(
                    "{}/{protocol} (interna — mesma rede Docker)",
                    port.container_port,
                    protocol = port.protocol
                ),
                is_external: false,
            });

            if let Some(host_port) = port.host_port {
                options.push(PortOption {
                    container_port: port.container_port,
                    host_port,
                    protocol: port.protocol.clone(),
                    display: format!(
                        "{host_port} (externa) -> {}/{protocol} (container)",
                        port.container_port,
                        protocol = port.protocol
                    ),
                    is_external: true,
                });
            }
        }

        // Dedup e ordena: primeiro porta do container crescente, depois interna
        // antes de externa, depois host port crescente.
        let mut unique: std::collections::BTreeMap<(u16, bool, u16), PortOption> =
            std::collections::BTreeMap::new();
        for option in options {
            let key = (option.container_port, option.is_external, option.host_port);
            unique.entry(key).or_insert(option);
        }
        unique.into_values().collect()
    }
}

/// Seleciona a porta recomendada conforme a acessibilidade de rede.
pub struct ConnectionPortSelectionResolver;

impl ConnectionPortSelectionResolver {
    pub fn resolve(all_options: &[PortOption], same_network: bool) -> ResolvedPortSelection {
        let accessible_options: Vec<&PortOption> = if same_network {
            all_options.iter().collect()
        } else {
            all_options
                .iter()
                .filter(|option| option.is_external)
                .collect()
        };

        let mut ordered: Vec<&PortOption> = accessible_options;
        ordered.sort_by(|left, right| {
            let left_priority = Self::priority(left, same_network);
            let right_priority = Self::priority(right, same_network);
            if left_priority != right_priority {
                return left_priority.cmp(&right_priority);
            }
            if left.host_port != right.host_port {
                return left.host_port.cmp(&right.host_port);
            }
            left.container_port.cmp(&right.container_port)
        });

        ResolvedPortSelection {
            recommended_port: ordered.first().map(|option| option.host_port),
            port_options: ordered.into_iter().cloned().collect(),
        }
    }

    fn priority(option: &PortOption, same_network: bool) -> u8 {
        if same_network {
            if option.is_external {
                1
            } else {
                0
            }
        } else if option.is_external {
            0
        } else {
            1
        }
    }
}

pub struct ResolvedPortSelection {
    pub recommended_port: Option<u16>,
    pub port_options: Vec<PortOption>,
}

/// Mapeia containers em sugestoes de conexao.
pub struct ConnectionSuggestionMapper;

impl ConnectionSuggestionMapper {
    pub fn map(
        containers: &[ContainerDescriptor],
        context: &BackendContext,
    ) -> Vec<HostSuggestion> {
        let mut suggestions: Vec<HostSuggestion> = containers
            .iter()
            .map(|container| {
                let host = NetworkReachabilityResolver::resolve(
                    container,
                    &context.backend_network_ids,
                    &context.docker_host_ip,
                );
                let all_port_options = ContainerPortResolver::resolve(container);
                let has_external_port = all_port_options.iter().any(|option| option.is_external);
                let selection =
                    ConnectionPortSelectionResolver::resolve(&all_port_options, host.same_network);

                let connectivity_warning = if !host.same_network && !has_external_port {
                    Some("Container selecionado não publica porta externa e não está na mesma rede do sistema. Pode não haver acesso ao banco.".into())
                } else {
                    None
                };

                HostSuggestion {
                    container_id: container.container_id.clone(),
                    container_name: container.container_name.clone(),
                    database_type_hint: container.database_type_hint.map(|hint| hint.as_str().into()),
                    same_network: host.same_network,
                    suggested_host: host.suggested_host,
                    host_resolution_source: host.host_resolution_source,
                    network_names: container
                        .networks
                        .iter()
                        .map(|network| network.network_name.clone())
                        .collect(),
                    port_options: selection.port_options,
                    recommended_port: selection.recommended_port,
                    has_external_port,
                    connectivity_warning,
                }
            })
            .collect();

        suggestions.sort_by(|a, b| a.container_name.cmp(&b.container_name));
        suggestions
    }
}

/// Infere o tipo de banco a partir do nome/imagem/ports.
pub fn detect_database_type_hint(
    container_name: &str,
    image_name: &str,
    ports: &[PortBinding],
) -> Option<DatabaseTypeHint> {
    let text = format!("{container_name} {image_name}").to_lowercase();

    if text.contains("postgres") {
        return Some(DatabaseTypeHint::Postgresql);
    }
    if text.contains("mariadb") {
        return Some(DatabaseTypeHint::Mariadb);
    }
    if text.contains("mysql") {
        return Some(DatabaseTypeHint::Mysql);
    }
    if ports.iter().any(|port| port.container_port == 5432) {
        return Some(DatabaseTypeHint::Postgresql);
    }
    if ports.iter().any(|port| port.container_port == 3306) {
        return Some(DatabaseTypeHint::Mysql);
    }
    None
}

/// Converte um `ContainerSummary` do bollard em descritor interno.
pub fn descriptor_from_bollard(
    summary: &bollard::models::ContainerSummary,
    inspect: &bollard::models::ContainerInspectResponse,
) -> Option<ContainerDescriptor> {
    let id = summary.id.as_deref().unwrap_or_default();
    if id.is_empty() {
        return None;
    }

    let name = summary
        .names
        .as_ref()
        .and_then(|names| names.first())
        .map(|name| name.trim_start_matches('/').trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.chars().take(12).collect());

    let image = summary
        .image
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("")
        .to_string();

    let labels = summary.labels.clone().unwrap_or_default();

    let networks = inspect
        .network_settings
        .as_ref()
        .and_then(|settings| settings.networks.as_ref())
        .map(|networks| {
            networks
                .iter()
                .map(|(network_name, endpoint)| NetworkAttachment {
                    network_id: endpoint
                        .network_id
                        .clone()
                        .unwrap_or_else(|| network_name.clone()),
                    network_name: network_name.clone(),
                    aliases: endpoint.aliases.clone().unwrap_or_default(),
                    gateway: endpoint.gateway.clone(),
                    ip_address: endpoint.ip_address.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    let ports: Vec<PortBinding> = summary
        .ports
        .as_ref()
        .map(|ports| {
            ports
                .iter()
                .map(|port| PortBinding {
                    container_port: port.private_port,
                    host_port: port.public_port,
                    protocol: port
                        .typ
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "tcp".into()),
                })
                .collect()
        })
        .unwrap_or_default();

    let database_type_hint = detect_database_type_hint(&name, &image, &ports);
    if database_type_hint.is_none()
        && !ports
            .iter()
            .any(|port| port.container_port == 3306 || port.container_port == 5432)
    {
        return None;
    }

    Some(ContainerDescriptor {
        container_id: id.into(),
        container_name: name,
        image_name: image,
        labels,
        database_type_hint,
        networks,
        ports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mysql_container() -> ContainerDescriptor {
        ContainerDescriptor {
            container_id: "mysql-container-id".into(),
            container_name: "mysql-db".into(),
            image_name: "mysql:8.4".into(),
            labels: std::collections::HashMap::new(),
            database_type_hint: Some(DatabaseTypeHint::Mysql),
            networks: vec![NetworkAttachment {
                network_id: "shared-network".into(),
                network_name: "shared-network".into(),
                aliases: vec!["mysql-db".into()],
                gateway: Some("172.22.0.1".into()),
                ip_address: Some("172.22.0.3".into()),
            }],
            ports: vec![PortBinding {
                container_port: 3306,
                host_port: Some(11961),
                protocol: "tcp".into(),
            }],
        }
    }

    #[test]
    fn recommends_internal_port_when_on_same_network() {
        let suggestion = ConnectionSuggestionMapper::map(
            &[mysql_container()],
            &BackendContext {
                backend_container_id: Some("backend-id".into()),
                backend_network_ids: vec!["shared-network".into()],
                docker_host_ip: "host.docker.internal".into(),
            },
        )
        .into_iter()
        .next()
        .unwrap();

        assert!(suggestion.same_network);
        assert_eq!(suggestion.suggested_host, "mysql-db");
        assert_eq!(suggestion.recommended_port, Some(3306));
        assert_eq!(suggestion.port_options.len(), 2);
        assert_eq!(suggestion.port_options[0].host_port, 3306);
        assert!(!suggestion.port_options[0].is_external);
        assert_eq!(suggestion.port_options[1].host_port, 11961);
        assert!(suggestion.port_options[1].is_external);
        assert!(suggestion.connectivity_warning.is_none());
    }

    #[test]
    fn recommends_only_host_port_when_outside_network() {
        let suggestion = ConnectionSuggestionMapper::map(
            &[mysql_container()],
            &BackendContext {
                backend_container_id: Some("backend-id".into()),
                backend_network_ids: vec!["another-network".into()],
                docker_host_ip: "host.docker.internal".into(),
            },
        )
        .into_iter()
        .next()
        .unwrap();

        assert!(!suggestion.same_network);
        assert_eq!(suggestion.suggested_host, "host.docker.internal");
        assert_eq!(suggestion.recommended_port, Some(11961));
        assert_eq!(suggestion.port_options.len(), 1);
        assert!(suggestion.port_options[0].is_external);
    }

    #[test]
    fn warns_when_no_external_port_and_outside_network() {
        let mut container = mysql_container();
        container.ports[0].host_port = None;

        let suggestion = ConnectionSuggestionMapper::map(
            &[container],
            &BackendContext {
                backend_container_id: Some("backend-id".into()),
                backend_network_ids: vec!["another-network".into()],
                docker_host_ip: "host.docker.internal".into(),
            },
        )
        .into_iter()
        .next()
        .unwrap();

        assert!(!suggestion.same_network);
        assert!(suggestion.connectivity_warning.is_some());
        assert_eq!(suggestion.recommended_port, None);
    }

    #[test]
    fn container_port_resolver_keeps_internal_option() {
        let options = ContainerPortResolver::resolve(&mysql_container());
        assert_eq!(options.len(), 2);
        assert!(options
            .iter()
            .any(|option| !option.is_external && option.host_port == 3306));
        assert!(options
            .iter()
            .any(|option| option.is_external && option.host_port == 11961));
    }

    #[test]
    fn detect_type_hint_from_image() {
        assert_eq!(
            detect_database_type_hint("app-db", "postgres:15", &[]),
            Some(DatabaseTypeHint::Postgresql)
        );
        assert_eq!(
            detect_database_type_hint("app-db", "mariadb:10", &[]),
            Some(DatabaseTypeHint::Mariadb)
        );
    }

    #[test]
    fn detect_type_hint_from_port() {
        assert_eq!(
            detect_database_type_hint(
                "app-db",
                "custom",
                &[PortBinding {
                    container_port: 5432,
                    host_port: None,
                    protocol: "tcp".into(),
                }]
            ),
            Some(DatabaseTypeHint::Postgresql)
        );
    }
}
