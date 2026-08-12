//! `GET /api/events` — o fluxo de eventos em tempo real.
//!
//! ## O protocolo é o do navegador, sem intermediário
//!
//! Uma requisição, uma conexão, e os canais na query:
//!
//! ```text
//! GET /api/events?channels=notifications/backup,notifications/storage
//!
//! event: notifications/backup
//! data: {"type":"success","title":"Backup concluído",…}
//! ```
//!
//! O nome do canal vai no campo `event:` do SSE, que é exatamente o que o
//! `EventSource` do navegador usa para despachar por
//! `addEventListener(canal, …)`. Não há handshake, `uid`, nem rotas de
//! `subscribe`/`unsubscribe`: quem quer trocar de canal reabre a conexão, e o
//! servidor deixa de ter estado por cliente para envelhecer.
//!
//! ## Esta rota não exige sessão
//!
//! `EventSource` não permite cabeçalho `Authorization`, e as alternativas —
//! token na query, cookie — têm cada uma o seu custo. **Fica como estava**: o
//! fluxo era aberto antes e continua aberto. Está registrado como pendência no
//! roadmap; fechá-la é decisão de produto, não efeito colateral de um porte.

use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use serde::Deserialize;
use tokio_stream::StreamExt;

use crate::models::sse;

/// De quanto em quanto tempo o servidor manda um comentário de keep-alive.
///
/// Sem ele, um proxy que fecha conexões ociosas derruba o fluxo em silêncio, e
/// o cliente só descobre quando o evento que importava não chega.
const KEEP_ALIVE: Duration = Duration::from_secs(30);

/// `?channels=a,b,c`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Subscription {
    pub channels: Option<String>,
}

impl Subscription {
    fn requested(&self) -> Vec<String> {
        self.channels
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|channel| !channel.is_empty())
            .map(ToString::to_string)
            .collect()
    }
}

/// `GET /api/events`.
///
/// Uma conexão sem canal nenhum é aceita e não recebe evento algum — recusar
/// obrigaria o cliente a tratar um erro por uma situação transitória (a tela
/// que ainda não decidiu o que acompanhar).
#[debug_handler]
pub async fn stream(
    State(ctx): State<AppContext>,
    Query(subscription): Query<Subscription>,
) -> Result<Response> {
    let mut listener = sse::listen(&ctx, &subscription.requested())?;

    let events = async_stream::stream! {
        loop {
            match listener.receiver().recv().await {
                Ok(event) if listener.wants(&event.channel) => {
                    yield Ok::<_, Infallible>(
                        Event::default()
                            .event(event.channel)
                            .data(event.payload.to_string()),
                    );
                }
                Ok(_) => {}
                // O cliente ficou para trás e perdeu eventos. Continuar é
                // melhor que encerrar: o próximo evento ainda lhe serve, e
                // encerrar faria o navegador reconectar em laço.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "conexão SSE ficou para trás");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    // O comentário inicial acorda o Safari, que só dispara `onopen` depois do
    // primeiro byte.
    let opening = tokio_stream::once(Ok(Event::default().comment("ok")));

    Ok(Sse::new(opening.chain(events))
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
        .into_response())
}

/// Rotas de eventos.
pub fn routes() -> Routes {
    Routes::new().add("/api/events", get(stream))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(query: &str) -> Vec<String> {
        Subscription {
            channels: Some(query.to_string()),
        }
        .requested()
    }

    #[test]
    fn reads_a_comma_separated_list() {
        assert_eq!(
            parsed("notifications/backup, notifications/storage"),
            ["notifications/backup", "notifications/storage"]
        );
    }

    #[test]
    fn empty_entries_do_not_become_channels() {
        // `?channels=` é o que a tela manda antes de decidir o que acompanhar.
        assert_eq!(parsed(",,").len(), 0);
        assert_eq!(Subscription::default().requested().len(), 0);
    }
}
