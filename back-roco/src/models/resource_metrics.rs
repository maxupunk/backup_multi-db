//! Emissão de métricas atuais para o canal SSE do painel.

use loco_rs::prelude::*;

pub const SYSTEM_RESOURCES: &str = "notifications/system-resources";

/// Coleta e emite uma amostra apenas quando há alguém acompanhando o painel.
pub async fn emit_if_subscribed(ctx: &AppContext) -> Result<bool> {
    if !crate::models::sse::has_subscribers(ctx, SYSTEM_RESOURCES).await? {
        return Ok(false);
    }

    let overview = crate::models::system_monitor::SystemOverview::collect().await;
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
