//! Jobs assíncronos de diagnóstico de rede da Fase 9.
//!
//! O registro vive no `AppContext`: processos em andamento não podem vazar
//! entre instâncias nem entre os testes. A Fase 10 poderá substituir o spawn
//! pela fila persistente sem alterar o controller.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use loco_rs::prelude::*;
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::RwLock;
use uuid::Uuid;

const JOB_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_JOBS: usize = 100;
const MAX_OUTPUT_LINES: usize = 200;
const DEFAULT_TIMEOUT: u64 = 2_000;

#[derive(Clone, Default)]
pub struct Registry {
    jobs: Arc<RwLock<HashMap<String, Job>>>,
}

pub fn register(ctx: &AppContext) {
    if !ctx.shared_store.contains::<Registry>() {
        ctx.shared_store.insert(Registry::default());
    }
}
fn registry(ctx: &AppContext) -> loco_rs::Result<Registry> {
    ctx.shared_store
        .get::<Registry>()
        .ok_or_else(|| Error::Message("docker diagnostics registry was not initialized".into()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub tool: String,
    pub status: String,
    pub target: String,
    pub port: Option<u16>,
    pub count: Option<u8>,
    pub timeout_ms: Option<u64>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub output_lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_open: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u128>,
}

#[derive(Debug)]
pub struct StartParams {
    pub tool: Option<String>,
    pub target: Option<String>,
    pub port: Option<u16>,
    pub count: Option<u8>,
    pub timeout_ms: Option<u64>,
}

pub async fn start(ctx: &AppContext, params: StartParams) -> Result<Job, String> {
    let tool = params.tool.unwrap_or_default();
    if !matches!(tool.as_str(), "ping" | "curl" | "port_scan") {
        return Err("Ferramenta de diagnóstico não suportada".into());
    }
    let target = params.target.unwrap_or_default().trim().to_string();
    if target.is_empty() || target.len() > 253 || target.chars().any(char::is_whitespace) {
        return Err("Destino do diagnóstico é obrigatório".into());
    }
    if tool == "port_scan" && params.port.is_none() {
        return Err("Porta é obrigatória para scan de porta".into());
    }
    let timeout_ms = params
        .timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT)
        .clamp(100, 30_000);
    let registry = registry(ctx).map_err(|error| error.to_string())?;
    let job = Job {
        id: format!("diag-{}", Uuid::new_v4()),
        tool,
        status: "pending".into(),
        target,
        port: params.port,
        count: (params.count.unwrap_or(4).clamp(1, 20)).into(),
        timeout_ms: Some(timeout_ms),
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
        output_lines: Vec::new(),
        summary: None,
        error: None,
        port_open: None,
        latency_ms: None,
    };
    let id = job.id.clone();
    registry.jobs.write().await.insert(id.clone(), job.clone());
    tokio::spawn(async move {
        run(registry, id).await;
    });
    Ok(job)
}

pub async fn get(ctx: &AppContext, id: &str) -> loco_rs::Result<Option<Job>> {
    Ok(registry(ctx)?.jobs.read().await.get(id).cloned())
}

async fn run(registry: Registry, id: String) {
    update(&registry, &id, |job| job.status = "running".into()).await;
    let snapshot = registry.jobs.read().await.get(&id).cloned();
    let Some(job) = snapshot else {
        return;
    };
    let result = match job.tool.as_str() {
        "ping" => ping(&job).await,
        "curl" => curl(&job).await,
        "port_scan" => port_scan(&job).await,
        _ => Err("Ferramenta de diagnóstico não suportada".into()),
    };
    update(&registry, &id, |stored| {
        stored.completed_at = Some(chrono::Utc::now().to_rfc3339());
        match result {
            Ok((lines, summary, open, latency)) => {
                stored.status = "completed".into();
                stored.output_lines = truncate_lines(lines);
                stored.summary = Some(summary);
                stored.port_open = open;
                stored.latency_ms = latency;
            }
            Err(error) => {
                stored.status = "failed".into();
                stored.error = Some(error);
                stored.summary = Some("Diagnóstico falhou.".into());
            }
        }
    })
    .await;
    retain(registry, id).await;
}

async fn ping(job: &Job) -> Result<(Vec<String>, String, Option<bool>, Option<u128>), String> {
    let count = job.count.unwrap_or(4);
    #[cfg(target_os = "windows")]
    let args = vec!["-n".to_string(), count.to_string(), job.target.clone()];
    #[cfg(not(target_os = "windows"))]
    let args = vec![
        "-c".to_string(),
        count.to_string(),
        "-W".to_string(),
        "2".to_string(),
        job.target.clone(),
    ];
    command(
        "ping",
        args,
        job.timeout_ms.unwrap_or(DEFAULT_TIMEOUT) * u64::from(count) + 1_000,
    )
    .await
    .map(|lines| {
        (
            lines,
            format!("Ping concluído com {count} tentativa(s)."),
            None,
            None,
        )
    })
}
async fn curl(job: &Job) -> Result<(Vec<String>, String, Option<bool>, Option<u128>), String> {
    let target = if job.target.contains("://") {
        job.target.clone()
    } else {
        format!("http://{}", job.target)
    };
    let timeout = (job.timeout_ms.unwrap_or(DEFAULT_TIMEOUT) as f64 / 1_000.0)
        .max(1.0)
        .to_string();
    command(
        "curl",
        vec![
            "--silent".into(),
            "--show-error".into(),
            "--include".into(),
            "--location".into(),
            "--connect-timeout".into(),
            timeout.clone(),
            "--max-time".into(),
            timeout,
            target,
        ],
        job.timeout_ms.unwrap_or(DEFAULT_TIMEOUT) + 1_000,
    )
    .await
    .map(|lines| (lines, "Curl concluído.".into(), None, None))
}
async fn port_scan(job: &Job) -> Result<(Vec<String>, String, Option<bool>, Option<u128>), String> {
    let port = job
        .port
        .ok_or_else(|| "Porta é obrigatória para o scan".to_string())?;
    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        Duration::from_millis(job.timeout_ms.unwrap_or(DEFAULT_TIMEOUT)),
        tokio::net::TcpStream::connect((job.target.as_str(), port)),
    )
    .await;
    let latency = Some(started.elapsed().as_millis());
    match result {
        Ok(Ok(_)) => Ok((
            vec![format!("Porta {port} aberta em {}.", job.target)],
            format!("Porta {port} está em uso e aceitando conexão."),
            Some(true),
            latency,
        )),
        Ok(Err(_)) | Err(_) => Ok((
            vec![format!(
                "Conexão recusada ou expirou em {}:{port}.",
                job.target
            )],
            format!("Porta {port} está fechada ou sem listener ativo."),
            Some(false),
            latency,
        )),
    }
}
async fn command(program: &str, args: Vec<String>, timeout_ms: u64) -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        Command::new(program).args(args).output(),
    )
    .await
    .map_err(|_| "Tempo limite do diagnóstico excedido".to_string())?
    .map_err(|_| format!("Comando {program} não está disponível no runtime do backend."))?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();
    if output.status.success() {
        Ok(lines)
    } else {
        Err(lines
            .last()
            .cloned()
            .unwrap_or_else(|| format!("{program} finalizou com erro")))
    }
}
fn truncate_lines(mut lines: Vec<String>) -> Vec<String> {
    if lines.len() > MAX_OUTPUT_LINES {
        lines.drain(..lines.len() - MAX_OUTPUT_LINES);
    }
    lines
}
async fn update(registry: &Registry, id: &str, apply: impl FnOnce(&mut Job)) {
    if let Some(job) = registry.jobs.write().await.get_mut(id) {
        apply(job);
    }
}
async fn retain(registry: Registry, id: String) {
    let mut jobs = registry.jobs.write().await;
    if jobs.len() > MAX_JOBS {
        let terminal: Vec<String> = jobs
            .values()
            .filter(|job| matches!(job.status.as_str(), "completed" | "failed"))
            .map(|job| job.id.clone())
            .take(jobs.len() - MAX_JOBS)
            .collect();
        for item in terminal {
            jobs.remove(&item);
        }
    }
    drop(jobs);
    tokio::spawn(async move {
        tokio::time::sleep(JOB_TTL).await;
        registry.jobs.write().await.remove(&id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn invalid_tool_is_rejected() {
        let boot = loco_rs::testing::prelude::boot_test::<crate::app::App>()
            .await
            .expect("test boot");
        let ctx = &boot.app_context;
        register(ctx);
        assert!(start(
            ctx,
            StartParams {
                tool: Some("bad".into()),
                target: Some("x".into()),
                port: None,
                count: None,
                timeout_ms: None
            }
        )
        .await
        .is_err());
    }
}
