//! Worker contínuo de métricas em tempo real.
//!
//! Ele é iniciado somente em `BackgroundAsync`: no modo de teste, que é
//! bloqueante, um loop contínuo impediria o boot. Em produção, o worker é
//! cancelado junto com o runtime do Loco.

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
            let emitted = crate::models::resource_metrics::emit_if_subscribed(&self.ctx).await?;
            tokio::time::sleep(if emitted {
                ACTIVE_INTERVAL
            } else {
                IDLE_INTERVAL
            })
            .await;
        }
    }
}
