//! Panorama do sistema — CPU, memoria, uptime e estado dos jobs (tarefa 5.5).
//!
//! Espelha o `SystemMonitoringService` da implementacao anterior. Fica em `models/` e nao no
//! controller porque e' logica de dominio reusada: alem de `GET
//! /api/system/status` e do bloco `system` de `GET /api/stats`, e' daqui que o
//! coletor de `resource_metric_history` da Fase 11 vai tirar as amostras.
//!
//! ## Dois desvios do padrao, ambos deliberados
//!
//! **1. Um crate novo (`sysinfo`).** O Loco nao expoe metricas de maquina, e
//! `std` nao da' acesso a CPU, memoria nem uptime. A alternativa seria devolver
//! numeros inventados num painel de monitoramento — pior que a dependencia.
//!
//! **2. Um cache em `static`.** Medir CPU exige **duas** amostras separadas por
//! um intervalo; sem cache, cada atualizacao do painel custaria esse intervalo
//! parado. a implementacao anterior resolve igual, com TTL de 2 segundos. O Loco nao tem um
//! slot de estado de aplicacao no `AppContext` (o mesmo motivo ja' registrado em
//! `initializers/settings.rs`), e o dado aqui e' do processo, nao do usuario.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use loco_rs::app::AppContext;
use sysinfo::System;

/// Versao que a implementacao anterior reporta. Fixa la', fixa aqui — trocar por
/// `CARGO_PKG_VERSION` mudaria o valor que o painel exibe hoje.
const REPORTED_VERSION: &str = "1.0.0";

/// Igual ao `CACHE_TTL_MS` da implementacao anterior.
const CACHE_TTL: Duration = Duration::from_secs(2);

/// Intervalo entre as duas amostras de CPU.
///
/// Abaixo do minimo do `sysinfo` a segunda leitura sai zerada, e o painel
/// mostraria 0% de uso permanentemente.
const CPU_SAMPLE_INTERVAL: Duration = sysinfo::MINIMUM_CPU_UPDATE_INTERVAL;

static CACHE: Mutex<Option<(Instant, SystemOverview)>> = Mutex::new(None);

/// De onde veio o numero de memoria.
///
/// Dentro de um container o total de memoria do sistema e' o do **host**, nao o
/// limite do container — um painel lendo isso mostraria 3% de uso enquanto o
/// processo e' morto por OOM. Por isso a leitura prefere o cgroup, e a origem
/// sai na resposta para que a diferenca seja visivel na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub enum MemorySource {
    Cgroup,
    Os,
}

#[derive(Debug, Clone)]
pub struct CpuMetrics {
    pub usage_percent: f64,
    pub cores: usize,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct MemoryMetrics {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub usage_percent: f64,
    pub source: MemorySource,
    pub container_limited: bool,
}

#[derive(Debug, Clone)]
pub struct JobsStatus {
    pub is_running: bool,
    pub active_jobs: u32,
}

#[derive(Debug, Clone)]
pub struct SystemOverview {
    pub version: &'static str,
    pub hostname: String,
    pub platform: String,
    pub architecture: &'static str,
    pub runtime_version: String,
    pub uptime_seconds: u64,
    pub cpu: CpuMetrics,
    pub memory: MemoryMetrics,
    pub jobs: JobsStatus,
}

impl SystemOverview {
    /// Coleta o panorama, reaproveitando a ultima medicao dentro do TTL.
    pub async fn collect(ctx: &AppContext) -> Self {
        if let Some(cached) = cached() {
            return cached;
        }

        let overview = measure(ctx).await;
        store(overview.clone());
        overview
    }
}

fn cached() -> Option<SystemOverview> {
    let guard = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (measured_at, overview) = guard.as_ref()?;

    (measured_at.elapsed() < CACHE_TTL).then(|| overview.clone())
}

fn store(overview: SystemOverview) {
    let mut guard = CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = Some((Instant::now(), overview));
}

async fn measure(ctx: &AppContext) -> SystemOverview {
    let mut system = System::new();

    // Duas amostras: a primeira estabelece a linha de base, a segunda mede o
    // que aconteceu no intervalo. Uma leitura so' devolveria sempre 0%.
    system.refresh_cpu_usage();
    tokio::time::sleep(CPU_SAMPLE_INTERVAL).await;
    system.refresh_cpu_usage();
    system.refresh_memory();

    SystemOverview {
        version: REPORTED_VERSION,
        hostname: System::host_name().unwrap_or_else(|| "unknown".to_string()),
        platform: format!(
            "{} {}",
            System::name().unwrap_or_else(|| std::env::consts::OS.to_string()),
            System::os_version().unwrap_or_default()
        )
        .trim_end()
        .to_string(),
        architecture: std::env::consts::ARCH,
        runtime_version: format!("rustc {}", env!("CARGO_PKG_RUST_VERSION")),
        uptime_seconds: System::uptime(),
        cpu: cpu_metrics(&system),
        memory: memory_metrics(&system),
        jobs: jobs_status(ctx),
    }
}

fn cpu_metrics(system: &System) -> CpuMetrics {
    CpuMetrics {
        usage_percent: round_percent(f64::from(system.global_cpu_usage())),
        cores: system.cpus().len(),
        model: system
            .cpus()
            .first()
            .map_or_else(|| "N/A".to_string(), |cpu| cpu.brand().trim().to_string()),
    }
}

fn memory_metrics(system: &System) -> MemoryMetrics {
    // O limite do cgroup vence o total do host quando existe — e' o numero que
    // decide se o processo morre por OOM.
    let (total, used, source, container_limited) = match system.cgroup_limits() {
        Some(limits) => (
            limits.total_memory,
            limits.total_memory.saturating_sub(limits.free_memory),
            MemorySource::Cgroup,
            true,
        ),
        None => (
            system.total_memory(),
            system.used_memory(),
            MemorySource::Os,
            false,
        ),
    };

    MemoryMetrics {
        total_bytes: total,
        used_bytes: used,
        free_bytes: total.saturating_sub(used),
        usage_percent: percentage(used, total),
        source,
        container_limited,
    }
}

/// Estado do agendador.
///
/// O Loco gerencia o scheduler nativamente quando `config.scheduler` existe e o
/// processo inicia em modo que o inclui (por exemplo `--scheduler` ou `--all`).
/// Como o `AppContext` não expõe o `StartMode` atual, usamos a presença da
/// configuração como proxy: se ela existe, o sistema está preparado para
/// executar jobs agendados; se não existe, o painel mostra `down` — a verdade,
/// e não um `ok` otimista que esconderia a ausência do agendador.
fn jobs_status(ctx: &AppContext) -> JobsStatus {
    let is_running = ctx.config.scheduler.is_some();
    let active_jobs = ctx
        .config
        .scheduler
        .as_ref()
        .map(|cfg| cfg.jobs.len() as u32)
        .unwrap_or(0);

    JobsStatus {
        is_running,
        active_jobs,
    }
}

/// Percentual com duas casas, como o `roundPercent` da implementacao anterior.
fn percentage(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        // Divisao por zero viraria `NaN`, que nao existe em JSON e faria a
        // serializacao inteira falhar.
        return 0.0;
    }

    round_percent(part as f64 * 100.0 / whole as f64)
}

fn round_percent(value: f64) -> f64 {
    (value.clamp(0.0, 100.0) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_total_never_produces_nan() {
        // `NaN` nao existe em JSON: a serializacao da resposta inteira falharia.
        assert_eq!(percentage(0, 0), 0.0);
        assert!(percentage(10, 0).is_finite());
    }

    #[test]
    fn rounds_to_two_decimals() {
        assert_eq!(percentage(1, 3), 33.33);
        assert_eq!(percentage(2, 3), 66.67);
        assert_eq!(percentage(1, 1), 100.0);
    }

    #[test]
    fn clamps_out_of_range_readings() {
        // O `sysinfo` pode devolver acima de 100 em maquina com muitos nucleos
        // logo apos o boot; um painel com 3200% nao ajuda ninguem.
        assert_eq!(round_percent(3200.0), 100.0);
        assert_eq!(round_percent(-1.0), 0.0);
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn collects_a_plausible_overview() {
        let ctx = loco_rs::testing::prelude::boot_test::<crate::app::App>()
            .await
            .unwrap()
            .app_context;
        let overview = SystemOverview::collect(&ctx).await;

        assert!(overview.cpu.cores >= 1);
        assert!(overview.memory.total_bytes > 0);
        assert!((0.0..=100.0).contains(&overview.cpu.usage_percent));
        assert!((0.0..=100.0).contains(&overview.memory.usage_percent));
        assert_eq!(overview.version, "1.0.0");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn the_second_call_comes_from_the_cache() {
        // Sem cache, cada atualizacao do painel custaria o intervalo de amostragem.
        let ctx = loco_rs::testing::prelude::boot_test::<crate::app::App>()
            .await
            .unwrap()
            .app_context;
        let _ = SystemOverview::collect(&ctx).await;

        let started = Instant::now();
        let _ = SystemOverview::collect(&ctx).await;

        assert!(
            started.elapsed() < CPU_SAMPLE_INTERVAL,
            "a segunda coleta mediu de novo em vez de usar o cache"
        );
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn the_scheduler_status_reflects_configuration() {
        let ctx = loco_rs::testing::prelude::boot_test::<crate::app::App>()
            .await
            .unwrap()
            .app_context;

        // A configuração de teste do projeto não define scheduler, então o
        // status deve refletir isso honestamente (`down`) em vez de inventar um
        // `ok` otimista.
        let jobs = jobs_status(&ctx);
        assert!(!jobs.is_running);
        assert_eq!(jobs.active_jobs, 0);
    }
}
