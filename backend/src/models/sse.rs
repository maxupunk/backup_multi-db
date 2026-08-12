//! Distribuição de eventos em tempo real.
//!
//! O registro pertence ao `AppContext`: uma instância da aplicação não pode
//! entregar eventos a conexões de outra, nem dois testes compartilharem fila.
//!
//! ## Um canal de broadcast, muitos ouvintes
//!
//! Quem publica não sabe quem está ouvindo — chama [`broadcast`] e segue. Cada
//! conexão aberta filtra o que lhe interessa pelos canais que pediu. O buffer
//! de [`EVENT_BUFFER`] eventos existe porque um cliente lento não pode segurar
//! o emissor: passado o buffer, o `tokio::sync::broadcast` descarta os eventos
//! antigos **daquele** receptor e o avisa com `Lagged`, em vez de bloquear
//! todos os outros.
//!
//! ## A contagem de ouvintes não é estatística
//!
//! [`has_listeners`] decide se vale coletar métricas de CPU, memória e
//! containers a cada 10 segundos. Sem ela, um servidor sem ninguém olhando a
//! tela de sistema pagaria a coleta para sempre. Por isso a contagem é mantida
//! por um guarda com `Drop` ([`Listener`]) e não por um contador solto: uma
//! conexão que cai sem avisar tem de decrementar do mesmo jeito.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use loco_rs::prelude::*;
use serde_json::Value;
use tokio::sync::broadcast;

/// Quantos eventos ficam pendentes por receptor antes de o mais antigo cair.
const EVENT_BUFFER: usize = 256;

/// Limite de tamanho de um nome de canal vindo do cliente.
const MAX_CHANNEL_LENGTH: usize = 256;

/// Quantos canais uma conexão pode acompanhar.
const MAX_CHANNELS: usize = 32;

#[derive(Debug, Clone)]
pub struct BroadcastEvent {
    pub channel: String,
    pub payload: Value,
}

#[derive(Clone)]
pub struct Registry {
    sender: broadcast::Sender<BroadcastEvent>,
    /// Quantas conexões acompanham cada canal.
    ///
    /// `std::sync::RwLock`, e não o do Tokio, para que o `Drop` de [`Listener`]
    /// possa decrementar sem um runtime por perto.
    listeners: Arc<RwLock<HashMap<String, usize>>>,
}

impl Default for Registry {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(EVENT_BUFFER);

        Self {
            sender,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Registry {
    /// Publica sem precisar carregar o `AppContext` numa task destacada.
    ///
    /// Um envio sem receptores é um no-op barato, e não um erro: o normal é
    /// ninguém estar com a tela aberta.
    pub fn broadcast(&self, channel: impl Into<String>, payload: Value) {
        let _ = self.sender.send(BroadcastEvent {
            channel: channel.into(),
            payload,
        });
    }

    /// Abre uma conexão para `channels` e conta o ouvinte.
    ///
    /// Canais vazios, longos demais ou com quebra de linha são descartados —
    /// um `\n` num nome de canal quebraria o enquadramento do SSE.
    #[must_use]
    pub fn listen(&self, channels: &[String]) -> Listener {
        let channels: Vec<String> = channels
            .iter()
            .filter(|channel| valid(channel))
            .take(MAX_CHANNELS)
            .cloned()
            .collect();

        if let Ok(mut counts) = self.listeners.write() {
            for channel in &channels {
                *counts.entry(channel.clone()).or_insert(0) += 1;
            }
        }

        Listener {
            receiver: self.sender.subscribe(),
            registry: self.clone(),
            channels,
        }
    }

    fn release(&self, channels: &[String]) {
        let Ok(mut counts) = self.listeners.write() else {
            return;
        };

        for channel in channels {
            if let Some(count) = counts.get_mut(channel) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.remove(channel);
                }
            }
        }
    }

    fn has_listeners(&self, channel: &str) -> bool {
        self.listeners
            .read()
            .is_ok_and(|counts| counts.contains_key(channel))
    }
}

/// Uma conexão aberta: o receptor de eventos mais os canais que ela acompanha.
///
/// Solta a contagem no `Drop`, que é o que acontece quando o cliente fecha a
/// aba ou a rede cai — não há evento de "desinscrever" para depender.
pub struct Listener {
    receiver: broadcast::Receiver<BroadcastEvent>,
    registry: Registry,
    channels: Vec<String>,
}

impl Listener {
    /// Quebra o guarda nas duas partes que o handler precisa: o fluxo de
    /// eventos e a lista de canais para filtrar.
    #[must_use]
    pub fn channels(&self) -> &[String] {
        &self.channels
    }

    /// O receptor, para o handler montar o stream.
    pub fn receiver(&mut self) -> &mut broadcast::Receiver<BroadcastEvent> {
        &mut self.receiver
    }

    /// Este evento interessa a esta conexão?
    #[must_use]
    pub fn wants(&self, channel: &str) -> bool {
        self.channels.iter().any(|wanted| wanted == channel)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.registry.release(&self.channels);
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
///
/// # Errors
/// Falha quando o registro não foi criado em `Hooks::before_run`.
pub fn shared(ctx: &AppContext) -> loco_rs::Result<Registry> {
    registry(ctx)
}

/// Publica um payload para quem estiver acompanhando o canal.
///
/// # Errors
/// Falha quando o registro não foi criado em `Hooks::before_run`.
pub fn broadcast(
    ctx: &AppContext,
    channel: impl Into<String>,
    payload: Value,
) -> loco_rs::Result<()> {
    registry(ctx)?.broadcast(channel, payload);
    Ok(())
}

/// Há pelo menos uma conexão acompanhando o canal?
///
/// Usado por emissores caros para não coletar métricas que ninguém está vendo.
///
/// # Errors
/// Falha quando o registro não foi criado em `Hooks::before_run`.
pub fn has_listeners(ctx: &AppContext, channel: &str) -> loco_rs::Result<bool> {
    Ok(registry(ctx)?.has_listeners(channel))
}

/// Abre uma conexão para os canais pedidos.
///
/// # Errors
/// Falha quando o registro não foi criado em `Hooks::before_run`.
pub fn listen(ctx: &AppContext, channels: &[String]) -> loco_rs::Result<Listener> {
    Ok(registry(ctx)?.listen(channels))
}

fn valid(channel: &str) -> bool {
    !channel.trim().is_empty()
        && channel.len() <= MAX_CHANNEL_LENGTH
        && !channel.contains(['\r', '\n'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_listener_only_wants_the_channels_it_asked_for() {
        let registry = Registry::default();
        let listener = registry.listen(&channels(&["notifications/backup"]));

        assert!(listener.wants("notifications/backup"));
        assert!(!listener.wants("notifications/storage"));
    }

    #[test]
    fn a_malformed_channel_is_dropped_instead_of_accepted() {
        // Um `\n` num nome de canal quebraria o enquadramento do SSE.
        let registry = Registry::default();
        let listener = registry.listen(&channels(&["   ", "com\nquebra", "bom"]));

        assert_eq!(listener.channels(), ["bom"]);
    }

    #[test]
    fn the_number_of_channels_is_capped() {
        let registry = Registry::default();
        let wanted: Vec<String> = (0..100).map(|index| format!("canal/{index}")).collect();

        assert_eq!(registry.listen(&wanted).channels().len(), MAX_CHANNELS);
    }

    #[test]
    fn dropping_the_listener_stops_the_collection() {
        // É o que impede um servidor sem ninguém olhando de coletar métricas
        // de CPU a cada 10 segundos para sempre.
        let registry = Registry::default();

        let listener = registry.listen(&channels(&["notifications/system-resources"]));
        assert!(registry.has_listeners("notifications/system-resources"));

        drop(listener);
        assert!(!registry.has_listeners("notifications/system-resources"));
    }

    #[test]
    fn two_connections_on_the_same_channel_count_separately() {
        let registry = Registry::default();

        let first = registry.listen(&channels(&["a"]));
        let second = registry.listen(&channels(&["a"]));

        drop(first);
        assert!(registry.has_listeners("a"), "a segunda conexão ainda ouve");

        drop(second);
        assert!(!registry.has_listeners("a"));
    }

    #[tokio::test]
    async fn an_event_reaches_every_open_connection() {
        let registry = Registry::default();
        let mut first = registry.listen(&channels(&["a"]));
        let mut second = registry.listen(&channels(&["a", "b"]));

        registry.broadcast("a", serde_json::json!({ "x": 1 }));

        for listener in [&mut first, &mut second] {
            let event = listener.receiver().recv().await.expect("evento");
            assert_eq!(event.channel, "a");
            assert_eq!(event.payload["x"], 1);
        }
    }
}
