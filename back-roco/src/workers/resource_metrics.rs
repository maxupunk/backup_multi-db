//! Worker contínuo de métricas em tempo real.
//!
//! Ele é iniciado somente em `BackgroundAsync`: no modo de teste, que é
//! bloqueante, um loop contínuo impediria o boot. Em produção, o worker é
//! cancelado junto com o runtime do Loco.
//!
//! A cada ciclo o worker coleta:
//!
//! 1. Métricas do host (CPU, memória) e as emite pelo SSE quando há assinantes.
//! 2. Métricas dos containers Docker e as emite pelo SSE quando há assinantes
//!    no canal de containers.
//! 3. Uma amostra de sistema e de containers no histórico (`resource_metric_history`).
//! 4. O pico de RSS do processo para o `memory_watermark_service`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use loco_rs::prelude::*;

const ACTIVE_INTERVAL: Duration = Duration::from_secs(10);
const IDLE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Default)]
pub struct ResourceMetricsPollingState {
    started: Arc<AtomicBool>,
}

pub struct ResourceMetricsWorker {
    ctx: AppContext,
}

/// Inicia a única instância do worker para este `AppContext`.
pub async fn start(ctx: &AppContext) -> Result<()> {
    if !matches!(
        ctx.config.workers.mode,
        loco_rs::config::WorkerMode::BackgroundAsync
    ) {
        return Ok(());
    }
    if !ctx.shared_store.contains::<ResourceMetricsPollingState>() {
        ctx.shared_store
            .insert(ResourceMetricsPollingState::default());
    }
    let Some(state) = ctx.shared_store.get::<ResourceMetricsPollingState>() else {
        return Err(Error::Message(
            "resource metrics state was not initialized".to_string(),
        ));
    };
    if state.started.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    ResourceMetricsWorker::perform_later(ctx, ()).await?;
    Ok(())
}

#[async_trait]
impl BackgroundWorker<()> for ResourceMetricsWorker {
    fn build(ctx: &AppContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    async fn perform(&self, _args: ()) -> Result<()> {
        loop {
            let emitted = tick(&self.ctx).await?;
            tokio::time::sleep(if emitted {
                ACTIVE_INTERVAL
            } else {
                IDLE_INTERVAL
            })
            .await;
        }
    }
}

/// Um ciclo de coleta e emissão.
async fn tick(ctx: &AppContext) -> Result<bool> {
    let mut emitted = false;

    if crate::models::resource_metrics::emit_if_subscribed(ctx).await? {
        emitted = true;
    }

    let container_overview = crate::models::docker_container_monitoring::overview(ctx).await;
    if container_overview.docker_available
        && crate::models::sse::has_subscribers(
            ctx,
            crate::models::resource_metrics::SYSTEM_RESOURCES,
        )
        .await?
    {
        // O painel consome `system-resources` para ambos; enviamos o overview
        // completo de containers pelo mesmo canal por simplicidade.
        let payload = serde_json::json!({
            "containers": container_overview.containers,
            "collectedAt": container_overview.collected_at,
        });
        crate::models::sse::broadcast(
            ctx,
            crate::models::resource_metrics::SYSTEM_RESOURCES,
            payload,
        )?;
        emitted = true;
    }

    let system_overview = crate::models::system_monitor::SystemOverview::collect().await;
    crate::models::resource_metric_history::record_system(ctx, &system_overview).await?;
    crate::models::resource_metric_history::record_containers(ctx, &container_overview).await?;

    crate::models::memory_watermark::sample(ctx, "resource-metrics").await?;

    Ok(emitted)
}
