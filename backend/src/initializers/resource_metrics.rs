//! Inicializador do polling contínuo de métricas do sistema.
//!
//! O Loco usa [`BackgroundWorker`] para jobs enfileiráveis (restore, cópia,
//! archive). Um loop infinito de coleta de métricas **não** é um job: ele não
//! recebe argumentos, não deve ser reexecutado pela fila e precisa de um
//! shutdown graceful quando o processo desliga. Por isso vive aqui, como uma
//! tarefa `tokio::spawn` com [`CancellationToken`], e não como worker.
//!
//! A tarefa é iniciada em [`Initializer::before_run`] e cancelada em
//! [`Hooks::on_shutdown`](crate::app::App). Em testes (`ForegroundBlocking`) o
//! polling não é ligado, para não travar o boot.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loco_rs::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::initializers::settings::Settings;

const ACTIVE_INTERVAL: Duration = Duration::from_secs(10);
const IDLE_INTERVAL: Duration = Duration::from_secs(30);

/// Estado compartilhado que indica se o polling já foi iniciado neste processo.
#[derive(Clone, Default)]
pub struct ResourceMetricsPollingState {
    started: Arc<AtomicBool>,
    cancel: CancellationToken,
}

/// Inicia o polling de métricas em uma tarefa separada.
///
/// Retorna `Ok(())` imediatamente se o modo de workers for bloqueante (testes)
/// ou se o polling já estiver rodando.
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

    let ctx = ctx.clone();
    let cancel = state.cancel.clone();

    tokio::spawn(async move {
        loop {
            let emitted = match tick(&ctx).await {
                Ok(value) => value,
                Err(err) => {
                    tracing::error!(error = %err, "falha no ciclo de métricas");
                    false
                }
            };

            let interval = if emitted {
                ACTIVE_INTERVAL
            } else {
                IDLE_INTERVAL
            };

            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = cancel.cancelled() => {
                    tracing::info!("polling de métricas encerrado");
                    break;
                }
            }
        }
    });

    tracing::info!("polling de métricas iniciado");
    Ok(())
}

/// Solicita o cancelamento do polling de métricas.
pub fn stop(ctx: &AppContext) {
    if let Some(state) = ctx.shared_store.get::<ResourceMetricsPollingState>() {
        state.cancel.cancel();
    }
}

/// Um ciclo de coleta e emissão de métricas.
async fn tick(ctx: &AppContext) -> Result<bool> {
    let mut emitted = false;

    if crate::models::resource_metrics::emit_if_subscribed(ctx).await? {
        emitted = true;
    }

    let container_overview = crate::models::docker_container_monitoring::overview(ctx).await;
    if crate::models::resource_metrics::emit_containers_if_subscribed(ctx, &container_overview)
        .await?
    {
        emitted = true;
    }

    let system_overview = crate::models::system_monitor::SystemOverview::collect(ctx).await;
    crate::models::resource_metric_history::record_system(ctx, &system_overview).await?;
    crate::models::resource_metric_history::record_containers(ctx, &container_overview).await?;

    crate::models::memory_watermark::sample(ctx, "resource-metrics").await?;

    Ok(emitted)
}

/// Inicializador que liga o polling no boot.
pub struct ResourceMetricsInitializer;

#[async_trait]
impl Initializer for ResourceMetricsInitializer {
    fn name(&self) -> String {
        "resource-metrics".to_string()
    }

    async fn before_run(&self, ctx: &AppContext) -> Result<()> {
        // Valida as settings para manter o mesmo comportamento do antigo
        // `after_routes`, que falhava cedo se a configuração estivesse quebrada.
        let _settings = Settings::from_json(ctx.config.settings.as_ref())?;
        start(ctx).await
    }
}
