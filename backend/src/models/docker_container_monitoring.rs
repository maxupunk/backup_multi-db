//! Metricas de containers Docker para o painel (tarefa 11.1).
//!
//! Espelha `DockerContainerMonitoringService` da implementacao anterior, mas usa `bollard`
//! diretamente em vez do binario `docker`. Os valores do contrato sao os
//! mesmos: CPU, memoria, rede, block IO e PIDs por container.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bollard::container::{ListContainersOptions, StatsOptions};
use bollard::models::ContainerSummary;
use bollard::Docker;
use futures_util::StreamExt;
use loco_rs::prelude::*;
use serde::Serialize;
use tokio::sync::Mutex;

const CACHE_TTL: Duration = Duration::from_secs(15);
const DOCKER_TIMEOUT: Duration = Duration::from_secs(4);

/// Estado do cache de overview, compartilhado no `AppContext`.
#[derive(Clone, Default)]
pub struct State {
    inner: Arc<Mutex<Option<(Instant, ContainerMetricsOverview)>>>,
}

impl State {
    fn get(&self) -> Option<ContainerMetricsOverview> {
        let guard = self.inner.try_lock().ok()?;
        let (measured_at, overview) = guard.as_ref()?;
        (measured_at.elapsed() < CACHE_TTL).then(|| overview.clone())
    }

    async fn set(&self, overview: ContainerMetricsOverview) {
        let mut guard = self.inner.lock().await;
        *guard = Some((Instant::now(), overview));
    }
}

pub fn register(ctx: &AppContext) {
    if !ctx.shared_store.contains::<State>() {
        ctx.shared_store.insert(State::default());
    }
}

fn state(ctx: &AppContext) -> loco_rs::Result<State> {
    ctx.shared_store.get::<State>().ok_or_else(|| {
        Error::Message("docker container monitoring state was not initialized".into())
    })
}

fn client() -> Option<Docker> {
    Docker::connect_with_local_defaults().ok()
}

/// Uma amostra de metricas de um container.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerMetrics {
    pub container_id: String,
    pub container_name: String,
    pub project_name: Option<String>,
    pub image_name: String,
    pub status: String,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub network: NetworkMetrics,
    pub block_io: BlockIoMetrics,
    pub pids: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CpuMetrics {
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryMetrics {
    pub usage_bytes: u64,
    pub limit_bytes: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMetrics {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockIoMetrics {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

/// Resposta de `GET /api/system/containers/resources`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerMetricsOverview {
    pub docker_available: bool,
    pub unavailable_reason: Option<String>,
    pub collected_at: String,
    pub containers: Vec<ContainerMetrics>,
}

/// Coleta o overview, usando cache quando valido.
pub async fn overview(ctx: &AppContext) -> ContainerMetricsOverview {
    if let Some(cached) = state(ctx).ok().and_then(|s| s.get()) {
        return cached;
    }

    let overview = collect().await;
    if let Ok(state) = state(ctx) {
        state.set(overview.clone()).await;
    }
    overview
}

async fn collect() -> ContainerMetricsOverview {
    let Some(client) = client() else {
        return ContainerMetricsOverview {
            docker_available: false,
            unavailable_reason: Some("Docker indisponivel".into()),
            collected_at: collected_at(),
            containers: Vec::new(),
        };
    };

    let list = match tokio::time::timeout(
        DOCKER_TIMEOUT,
        client.list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        })),
    )
    .await
    {
        Ok(Ok(list)) => list,
        Ok(Err(err)) => {
            return ContainerMetricsOverview {
                docker_available: false,
                unavailable_reason: Some(format!("Falha ao listar containers: {err}")),
                collected_at: collected_at(),
                containers: Vec::new(),
            };
        }
        Err(_) => {
            return ContainerMetricsOverview {
                docker_available: false,
                unavailable_reason: Some("Tempo limite ao consultar Docker".into()),
                collected_at: collected_at(),
                containers: Vec::new(),
            };
        }
    };

    let mut containers = Vec::with_capacity(list.len());
    for summary in list {
        let Some(ref id) = summary.id else { continue };
        match container_metrics(&client, id, &summary).await {
            Some(metrics) => containers.push(metrics),
            None => continue,
        }
    }

    containers.sort_by(|a, b| a.container_name.cmp(&b.container_name));

    ContainerMetricsOverview {
        docker_available: true,
        unavailable_reason: None,
        collected_at: collected_at(),
        containers,
    }
}

async fn container_metrics(
    client: &Docker,
    id: &str,
    summary: &ContainerSummary,
) -> Option<ContainerMetrics> {
    let mut stream = client.stats(
        id,
        Some(StatsOptions {
            stream: false,
            one_shot: true,
        }),
    );

    let stats = tokio::time::timeout(DOCKER_TIMEOUT, stream.next())
        .await
        .ok()??
        .ok()?;

    let name = container_name(summary, id);
    let labels = summary.labels.as_ref();

    Some(ContainerMetrics {
        container_id: id.to_string(),
        container_name: name.clone(),
        project_name: project_name(labels, &name),
        image_name: summary
            .image
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("N/A")
            .to_string(),
        status: summary
            .state
            .as_deref()
            .or(summary.status.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string(),
        cpu: CpuMetrics {
            usage_percent: cpu_usage(&stats),
        },
        memory: MemoryMetrics {
            usage_bytes: stats.memory_stats.usage.unwrap_or(0),
            limit_bytes: stats.memory_stats.limit.unwrap_or(0),
            usage_percent: percentage(
                stats.memory_stats.usage.unwrap_or(0),
                stats.memory_stats.limit.unwrap_or(0),
            ),
        },
        network: network_usage(&stats),
        block_io: block_io_usage(&stats),
        pids: stats.pids_stats.current,
    })
}

fn container_name(summary: &ContainerSummary, id: &str) -> String {
    summary
        .names
        .as_ref()
        .and_then(|names| names.first())
        .map(|name| name.trim_start_matches('/').trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.chars().take(12).collect())
}

fn project_name(labels: Option<&HashMap<String, String>>, container_name: &str) -> Option<String> {
    let label = labels.and_then(|map| {
        map.get("com.docker.compose.project")
            .or_else(|| map.get("io.podman.compose.project"))
            .or_else(|| map.get("project.name"))
    });

    if let Some(value) = label {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    infer_project_name(container_name)
}

fn infer_project_name(container_name: &str) -> Option<String> {
    let normalized = container_name.trim();
    if normalized.is_empty() {
        return None;
    }

    // Heuristica da implementacao anterior: nome do projeto e' o prefixo antes do ultimo
    // ou penultimo segmento separado por '-' ou '_'.
    let re = regex::Regex::new(r"^(.*?)(?:[-_][^-_]+){1,2}$").ok()?;
    re.captures(normalized)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|candidate| !candidate.is_empty() && candidate != normalized)
}

fn cpu_usage(stats: &bollard::container::Stats) -> f64 {
    let current_total = stats.cpu_stats.cpu_usage.total_usage;
    let previous_total = stats.precpu_stats.cpu_usage.total_usage;
    let current_system = stats.cpu_stats.system_cpu_usage.unwrap_or(0);
    let previous_system = stats.precpu_stats.system_cpu_usage.unwrap_or(0);

    let cpu_delta = current_total.saturating_sub(previous_total);
    let system_delta = current_system.saturating_sub(previous_system);

    let online_cpus = stats
        .cpu_stats
        .online_cpus
        .unwrap_or_else(|| {
            stats
                .cpu_stats
                .cpu_usage
                .percpu_usage
                .as_ref()
                .map(|v| v.len() as u64)
                .unwrap_or(1)
        })
        .max(1);

    if cpu_delta == 0 || system_delta == 0 {
        return 0.0;
    }

    round_percent((cpu_delta as f64 / system_delta as f64) * online_cpus as f64 * 100.0)
}

fn network_usage(stats: &bollard::container::Stats) -> NetworkMetrics {
    let mut rx = 0u64;
    let mut tx = 0u64;

    if let Some(networks) = stats.networks.as_ref() {
        for net in networks.values() {
            rx += net.rx_bytes;
            tx += net.tx_bytes;
        }
    }

    NetworkMetrics {
        rx_bytes: rx,
        tx_bytes: tx,
    }
}

fn block_io_usage(stats: &bollard::container::Stats) -> BlockIoMetrics {
    let mut read = 0u64;
    let mut write = 0u64;

    if let Some(values) = stats.blkio_stats.io_service_bytes_recursive.as_ref() {
        for item in values {
            let value = item.value;
            match item.op.to_lowercase().as_str() {
                "read" => read += value,
                "write" => write += value,
                _ => {}
            }
        }
    }

    BlockIoMetrics {
        read_bytes: read,
        write_bytes: write,
    }
}

fn percentage(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    round_percent(part as f64 * 100.0 / whole as f64)
}

fn round_percent(value: f64) -> f64 {
    (value.clamp(0.0, 100.0) * 100.0).round() / 100.0
}

fn collected_at() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cpu_usage_stats(
        total: u64,
        system: u64,
        online: u64,
        prev_total: u64,
        prev_system: u64,
    ) -> bollard::container::Stats {
        use bollard::container::{
            BlkioStats, CPUStats, CPUUsage, MemoryStats, PidsStats, Stats, StorageStats,
            ThrottlingData,
        };

        let cpu = CPUStats {
            cpu_usage: CPUUsage {
                total_usage: total,
                percpu_usage: None,
                usage_in_usermode: 0,
                usage_in_kernelmode: 0,
            },
            system_cpu_usage: Some(system),
            online_cpus: Some(online),
            throttling_data: ThrottlingData {
                periods: 0,
                throttled_periods: 0,
                throttled_time: 0,
            },
        };
        let precpu = CPUStats {
            cpu_usage: CPUUsage {
                total_usage: prev_total,
                percpu_usage: None,
                usage_in_usermode: 0,
                usage_in_kernelmode: 0,
            },
            system_cpu_usage: Some(prev_system),
            online_cpus: None,
            throttling_data: ThrottlingData {
                periods: 0,
                throttled_periods: 0,
                throttled_time: 0,
            },
        };

        Stats {
            read: String::new(),
            preread: String::new(),
            num_procs: 1,
            pids_stats: PidsStats {
                current: None,
                limit: None,
            },
            network: None,
            networks: None,
            memory_stats: MemoryStats {
                stats: None,
                max_usage: None,
                usage: None,
                failcnt: None,
                limit: None,
                commit: None,
                commit_peak: None,
                commitbytes: None,
                commitpeakbytes: None,
                privateworkingset: None,
            },
            blkio_stats: BlkioStats {
                io_service_bytes_recursive: None,
                io_serviced_recursive: None,
                io_queue_recursive: None,
                io_service_time_recursive: None,
                io_wait_time_recursive: None,
                io_merged_recursive: None,
                io_time_recursive: None,
                sectors_recursive: None,
            },
            cpu_stats: cpu,
            precpu_stats: precpu,
            storage_stats: StorageStats {
                read_count_normalized: None,
                read_size_bytes: None,
                write_count_normalized: None,
                write_size_bytes: None,
            },
            name: String::new(),
            id: String::new(),
        }
    }

    #[test]
    fn cpu_usage_with_zero_delta_returns_zero() {
        let stats = cpu_usage_stats(100, 1_000, 1, 100, 1_000);
        assert_eq!(cpu_usage(&stats), 0.0);
    }

    #[test]
    fn cpu_usage_calculates_normalized_percent() {
        let stats = cpu_usage_stats(200, 2_000, 2, 100, 1_000);
        // (100 / 1000) * 2 * 100 = 20%
        assert_eq!(cpu_usage(&stats), 20.0);
    }

    #[test]
    fn percentage_handles_zero_total() {
        assert_eq!(percentage(10, 0), 0.0);
    }

    #[test]
    fn project_name_comes_from_compose_label() {
        let mut labels = HashMap::new();
        labels.insert("com.docker.compose.project".into(), "app".into());
        assert_eq!(project_name(Some(&labels), "app-db-1"), Some("app".into()));
    }

    #[test]
    fn project_name_is_inferred_from_container_name() {
        assert_eq!(project_name(None, "myapp-db-1"), Some("myapp".into()));
        assert_eq!(project_name(None, "short"), None);
    }

    fn container_summary(
        names: Option<Vec<&str>>,
        id: Option<&str>,
    ) -> bollard::models::ContainerSummary {
        bollard::models::ContainerSummary {
            id: id.map(ToString::to_string),
            names: names.map(|list| list.iter().map(ToString::to_string).collect()),
            image: None,
            image_id: None,
            command: None,
            created: None,
            ports: None,
            size_rw: None,
            size_root_fs: None,
            labels: None,
            state: None,
            status: None,
            host_config: None,
            network_settings: None,
            mounts: None,
        }
    }

    #[test]
    fn container_name_strips_leading_slash() {
        let summary = container_summary(Some(vec!["/web-1"]), Some("abc123"));
        assert_eq!(container_name(&summary, "abc123"), "web-1");
    }

    #[test]
    fn container_name_falls_back_to_truncated_id() {
        let summary = container_summary(None, Some("abc123def456"));
        assert_eq!(
            container_name(&summary, "abc123def456"),
            "abc123def456"[..12]
        );
    }
}
