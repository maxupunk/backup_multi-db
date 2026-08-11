//! Emissão de métricas atuais para os canais SSE do painel.

use loco_rs::prelude::*;

pub const SYSTEM_RESOURCES: &str = "notifications/system-resources";
pub const DOCKER_CONTAINER_RESOURCES: &str = "notifications/docker-container-resources";

/// Coleta e emite métricas do host apenas quando há assinantes no canal de sistema.
pub async fn emit_if_subscribed(ctx: &AppContext) -> Result<bool> {
    if !crate::models::sse::has_subscribers(ctx, SYSTEM_RESOURCES).await? {
        return Ok(false);
    }

    let overview = crate::models::system_monitor::SystemOverview::collect(ctx).await;
    let payload = serde_json::json!({
        "cpu": {
            "usagePercent": overview.cpu.usage_percent,
            "cores": overview.cpu.cores,
            "model": overview.cpu.model,
        },
        "memory": {
            "totalBytes": overview.memory.total_bytes,
            "usedBytes": overview.memory.used_bytes,
            "freeBytes": overview.memory.free_bytes,
            "usagePercent": overview.memory.usage_percent,
            "source": overview.memory.source,
            "containerLimited": overview.memory.container_limited,
        },
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    crate::models::sse::broadcast(ctx, SYSTEM_RESOURCES, payload)?;
    Ok(true)
}

/// Coleta e emite métricas de containers Docker no canal próprio.
pub async fn emit_containers_if_subscribed(
    ctx: &AppContext,
    overview: &crate::models::docker_container_monitoring::ContainerMetricsOverview,
) -> Result<bool> {
    if !crate::models::sse::has_subscribers(ctx, DOCKER_CONTAINER_RESOURCES).await? {
        return Ok(false);
    }

    let payload = serde_json::json!({
        "dockerAvailable": overview.docker_available,
        "unavailableReason": overview.unavailable_reason,
        "collectedAt": overview.collected_at,
        "containers": overview.containers,
    });
    crate::models::sse::broadcast(ctx, DOCKER_CONTAINER_RESOURCES, payload)?;
    Ok(true)
}
