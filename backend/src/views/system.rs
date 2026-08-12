//! Respostas de `/api/stats` e `/api/system/status` (tarefa 5.5).
//!
//! O golden `system/stats` marca `data.system`, `data.storageSpaces` e
//! `data.recentBackups` como nao-comparados, e `system/status` ignora `data`
//! inteiro — sao numeros que dependem da maquina. **O formato continua sendo
//! contrato**: e' o que o painel do frontend le'.
//!
//! Dois campos ainda nao tem origem real e saem vazios de proposito, com o
//! motivo registrado aqui:
//!
//! - `storageSpaces` depende do `StorageSpaceService`, da Fase 8;
//! - `recentBackups` traz os backups de verdade, mas o formato do item so' e'
//!   fixado pelo lote 2.4, na Fase 7.

use serde::Serialize;

use crate::models::_entities::backups;
use crate::models::system_monitor;

/// Cabecalho de `GET /api/system/status`.
///
/// `runtimeVersion` identifica o runtime que esta' respondendo — a versao do
/// Rust com que o binario foi compilado.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemOverview {
    pub version: &'static str,
    pub hostname: String,
    pub platform: String,
    pub architecture: &'static str,
    pub runtime_version: String,
    pub uptime_seconds: u64,
    pub resources: Resources,
    pub jobs: Jobs,
}

#[derive(Debug, Clone, Serialize)]
pub struct Resources {
    pub cpu: Cpu,
    pub memory: Memory,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cpu {
    pub usage_percent: f64,
    pub cores: usize,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f64,
    pub source: system_monitor::MemorySource,
    pub container_limited: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Jobs {
    pub is_running: bool,
    pub active_jobs: u32,
    /// `"ok"` ou `"down"`, derivado de `is_running` — nunca gravado a parte, ou
    /// os dois campos poderiam discordar.
    pub status: &'static str,
}

impl From<system_monitor::SystemOverview> for SystemOverview {
    fn from(overview: system_monitor::SystemOverview) -> Self {
        Self {
            version: overview.version,
            hostname: overview.hostname,
            platform: overview.platform,
            architecture: overview.architecture,
            runtime_version: overview.runtime_version,
            uptime_seconds: overview.uptime_seconds,
            resources: Resources {
                cpu: Cpu {
                    usage_percent: overview.cpu.usage_percent,
                    cores: overview.cpu.cores,
                    model: overview.cpu.model,
                },
                memory: Memory {
                    total_bytes: overview.memory.total_bytes,
                    used_bytes: overview.memory.used_bytes,
                    free_bytes: overview.memory.free_bytes,
                    usage_percent: overview.memory.usage_percent,
                    source: overview.memory.source,
                    container_limited: overview.memory.container_limited,
                },
            },
            jobs: Jobs {
                is_running: overview.jobs.is_running,
                active_jobs: overview.jobs.active_jobs,
                status: if overview.jobs.is_running {
                    "ok"
                } else {
                    "down"
                },
            },
        }
    }
}

/// Contadores de um recurso.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionCounts {
    pub total: u64,
    pub active: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupCounts {
    pub total: u64,
    pub today: u64,
}

/// Item de `recentBackups`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentBackup {
    pub id: i64,
    /// `"N/A"` quando a conexao foi apagada — a FK e' `SET NULL`, e o Adonis
    /// usa exatamente esse literal.
    pub connection_name: String,
    pub status: String,
    pub file_size: Option<i64>,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
}

impl RecentBackup {
    pub fn new(backup: backups::Model, connection_name: Option<String>) -> Self {
        Self {
            id: backup.id,
            connection_name: connection_name.unwrap_or_else(|| "N/A".to_string()),
            status: backup.status,
            file_size: backup.file_size,
            created_at: backup.created_at,
        }
    }
}

/// Corpo de `GET /api/stats`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub connections: ConnectionCounts,
    pub backups: BackupCounts,
    pub recent_backups: Vec<RecentBackup>,
    /// Preenchido desde a tarefa 8.13 — ver [`crate::models::storage::space`].
    pub storage_spaces: Vec<crate::views::storages::SpaceItem>,
    pub system: SystemOverview,
}

/// Resposta de `GET|PUT /api/system/backup-retention`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRetentionPolicy {
    pub daily: u32,
    pub weekly: u32,
    pub monthly: u32,
    pub yearly: u32,
    pub prune_cron: String,
    pub default_prune_cron: &'static str,
}

impl From<crate::models::backup_retention_policy::BackupRetentionPolicy> for BackupRetentionPolicy {
    fn from(policy: crate::models::backup_retention_policy::BackupRetentionPolicy) -> Self {
        Self {
            daily: policy.daily,
            weekly: policy.weekly,
            monthly: policy.monthly,
            yearly: policy.yearly,
            prune_cron: policy.prune_cron,
            default_prune_cron: "0 2 * * *",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overview(is_running: bool) -> system_monitor::SystemOverview {
        system_monitor::SystemOverview {
            version: "1.0.0",
            hostname: "contract-host".to_string(),
            platform: "Windows 11".to_string(),
            architecture: "x86_64",
            runtime_version: "rustc 1.96".to_string(),
            uptime_seconds: 4242,
            cpu: system_monitor::CpuMetrics {
                usage_percent: 51.35,
                cores: 8,
                model: "Contract CPU".to_string(),
            },
            memory: system_monitor::MemoryMetrics {
                total_bytes: 33_895_165_952,
                used_bytes: 29_890_318_336,
                free_bytes: 4_004_847_616,
                usage_percent: 88.18,
                source: system_monitor::MemorySource::Os,
                container_limited: false,
            },
            jobs: system_monitor::JobsStatus {
                is_running,
                active_jobs: 0,
            },
        }
    }

    #[test]
    fn the_overview_carries_every_key_the_panel_reads() {
        let json = serde_json::to_value(SystemOverview::from(overview(false))).expect("serializa");

        for key in [
            "version",
            "hostname",
            "platform",
            "architecture",
            "runtimeVersion",
            "uptimeSeconds",
            "resources",
            "jobs",
        ] {
            assert!(json.get(key).is_some(), "faltou `{key}`");
        }
        assert_eq!(json["resources"]["cpu"]["usagePercent"], 51.35);
        assert_eq!(json["resources"]["memory"]["source"], "os");
        assert_eq!(json["resources"]["memory"]["containerLimited"], false);
    }

    #[test]
    fn the_job_status_string_follows_is_running() {
        // Guardar os dois separadamente deixaria o painel dizer `ok` com o
        // agendador parado.
        assert_eq!(
            serde_json::to_value(SystemOverview::from(overview(false))).expect("ok")["jobs"]
                ["status"],
            "down"
        );
        assert_eq!(
            serde_json::to_value(SystemOverview::from(overview(true))).expect("ok")["jobs"]
                ["status"],
            "ok"
        );
    }

    #[test]
    fn a_backup_without_a_connection_reports_na() {
        // A FK e' `SET NULL`: apagar a conexao nao apaga os backups dela.
        let backup = backups::Model {
            id: 1,
            connection_id: None,
            connection_database_id: None,
            database_name: "app".to_string(),
            file_path: None,
            file_name: None,
            file_size: Some(1_610_612_736),
            checksum: None,
            status: "completed".to_string(),
            error_message: None,
            compressed: None,
            started_at: None,
            finished_at: None,
            duration_seconds: None,
            retention_type: "daily".to_string(),
            protected: None,
            metadata: None,
            trigger: "manual".to_string(),
            storage_destination_id: None,
            exit_code: None,
            created_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            updated_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
        };

        let json = serde_json::to_value(RecentBackup::new(backup, None)).expect("serializa");
        assert_eq!(json["connectionName"], "N/A");
        assert_eq!(json["fileSize"], 1_610_612_736_i64);
    }
}
