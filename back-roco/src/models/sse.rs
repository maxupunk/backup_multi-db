//! Compatibilidade com `@adonisjs/transmit` usando SSE nativo do Axum.
//!
//! O registro pertence ao `AppContext`: uma instância da aplicação não pode
//! entregar eventos a conexões de outra instância nem compartilhar testes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use loco_rs::prelude::*;
use serde_json::Value;
use tokio::sync::{broadcast, RwLock};

const EVENT_BUFFER: usize = 256;

#[derive(Debug, Clone)]
pub struct BroadcastEvent {
    pub channel: String,
    pub payload: Value,
}

#[derive(Clone)]
pub struct Registry {
    subscriptions: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    sender: broadcast::Sender<BroadcastEvent>,
}

impl Default for Registry {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER);
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            sender,
        }
    }
}

/// Registra a dependência de estado no ciclo de vida da aplicação.
pub fn register(ctx: &AppContext) {
    if !ctx.shared_store.contains::<Registry>() {
        ctx.shared_store.insert(Registry::default());
    }
}

fn registry(ctx: &AppContext) -> loco_rs::Result<Registry> {
    ctx.shared_store
        .get::<Registry>()
        .ok_or_else(|| Error::Message("SSE registry was not initialized".to_string()))
}

/// Obtém o registry para uma tarefa que recebeu o contexto no boot.
pub fn shared(ctx: &AppContext) -> loco_rs::Result<Registry> {
    registry(ctx)
}

/// Inscreve um uid em um canal. Os limites evitam reter identificadores ou
/// nomes de canal arbitrariamente grandes em memória.
pub async fn subscribe(ctx: &AppContext, uid: &str, channel: &str) -> loco_rs::Result<bool> {
    if !valid(uid, 128) || !valid(channel, 256) {
        return Ok(false);
    }
    registry(ctx)?
        .subscriptions
        .write()
        .await
        .entry(uid.to_string())
        .or_default()
        .insert(channel.to_string());
    Ok(true)
}

/// Remove uma inscrição; como o Transmit legado, remover um canal já ausente
/// continua sendo sucesso.
pub async fn unsubscribe(ctx: &AppContext, uid: &str, channel: &str) -> loco_rs::Result<bool> {
    if !valid(uid, 128) || !valid(channel, 256) {
        return Ok(false);
    }
    let registry = registry(ctx)?;
    let mut subscriptions = registry.subscriptions.write().await;
    if let Some(channels) = subscriptions.get_mut(uid) {
        channels.remove(channel);
        if channels.is_empty() {
            subscriptions.remove(uid);
        }
    }
    Ok(true)
}

/// Publica um payload já serializável para todos os inscritos no canal.
/// Sem inscritos o envio é intencionalmente um no-op de baixo custo.
pub fn broadcast(
    ctx: &AppContext,
    channel: impl Into<String>,
    payload: Value,
) -> loco_rs::Result<()> {
    registry(ctx)?.broadcast(channel, payload);
    Ok(())
}

impl Registry {
    /// Publica sem precisar carregar o `AppContext` em uma task destacada.
    pub fn broadcast(&self, channel: impl Into<String>, payload: Value) {
        let _ = self.sender.send(BroadcastEvent {
            channel: channel.into(),
            payload,
        });
    }
}

pub fn receiver(ctx: &AppContext) -> loco_rs::Result<broadcast::Receiver<BroadcastEvent>> {
    Ok(registry(ctx)?.sender.subscribe())
}

pub async fn receives(ctx: &AppContext, uid: &str, channel: &str) -> loco_rs::Result<bool> {
    Ok(registry(ctx)?
        .subscriptions
        .read()
        .await
        .get(uid)
        .is_some_and(|channels| channels.contains(channel)))
}

/// Há pelo menos um cliente inscrito no canal? Usado por emissores caros para
/// evitar coletar métricas que ninguém está vendo.
pub async fn has_subscribers(ctx: &AppContext, channel: &str) -> loco_rs::Result<bool> {
    Ok(registry(ctx)?
        .subscriptions
        .read()
        .await
        .values()
        .any(|channels| channels.contains(channel)))
}

fn valid(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum && !value.contains(['\r', '\n'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[tokio::test]
    async fn registry_scopes_events_to_the_subscribed_uid_and_channel() {
        let boot = boot_test::<App>().await.expect("test boot");
        let ctx = &boot.app_context;
        register(ctx);
        assert!(subscribe(ctx, "client-a", "notifications/global")
            .await
            .expect("subscribe"));
        assert!(receives(ctx, "client-a", "notifications/global")
            .await
            .expect("membership"));
        assert!(!receives(ctx, "client-a", "notifications/backup")
            .await
            .expect("membership"));
    }
}
