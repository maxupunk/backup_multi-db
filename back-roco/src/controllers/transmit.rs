//! Handshake compatível com `@adonisjs/transmit`.

use std::convert::Infallible;
use std::time::Duration;

use axum::body::Bytes;
use axum::response::{sse::Event, sse::KeepAlive, IntoResponse, Response, Sse};
use futures_util::StreamExt;
use loco_rs::prelude::*;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

use crate::controllers::json_body;
use crate::models::sse;
use crate::views::errors::ApiError;

type Reply = std::result::Result<Response, ApiError>;
const PING_CHANNEL: &str = "$$transmit/ping";

#[derive(Debug, Deserialize, Default)]
pub struct SubscriptionParams {
    uid: Option<String>,
    channel: Option<String>,
}

/// `GET /__transmit/events?uid=…`.
///
/// O cliente oficial espera mensagens SSE padrão cujo `data` contém
/// `{channel,payload}`. O comentário inicial desperta Safari, igual ao
/// `:ok` emitido pelo pacote original.
#[debug_handler]
pub async fn events(
    State(ctx): State<AppContext>,
    Query(params): Query<SubscriptionParams>,
) -> Reply {
    let uid = params
        .uid
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("Missing required field \"uid\" in the request body")
        })?;
    let receiver = sse::receiver(&ctx)?;
    let events = BroadcastStream::new(receiver).filter_map({
        let ctx = ctx.clone();
        let uid = uid.clone();
        move |event| {
            let ctx = ctx.clone();
            let uid = uid.clone();
            async move {
                let event = event.ok()?;
                sse::receives(&ctx, &uid, &event.channel)
                    .await
                    .ok()?
                    .then(|| {
                        Event::default().data(
                            json!({ "channel": event.channel, "payload": event.payload })
                                .to_string(),
                        )
                    })
            }
        }
    });
    let pings = IntervalStream::new(tokio::time::interval(Duration::from_secs(30))).map(|_| {
        Ok::<_, Infallible>(
            Event::default().data(json!({ "channel": PING_CHANNEL, "payload": {} }).to_string()),
        )
    });
    let events = futures_util::stream::select(events.map(Ok::<_, Infallible>), pings);
    Ok(
        Sse::new(tokio_stream::once(Ok(Event::default().comment("ok"))).chain(events))
            .keep_alive(KeepAlive::default())
            .into_response(),
    )
}

/// `POST /__transmit/subscribe`.
#[debug_handler]
pub async fn subscribe(State(ctx): State<AppContext>, body: Bytes) -> Reply {
    let params: SubscriptionParams = json_body(&body)?;
    let subscribed = sse::subscribe(
        &ctx,
        params.uid.as_deref().unwrap_or_default(),
        params.channel.as_deref().unwrap_or_default(),
    )
    .await?;
    if !subscribed {
        return Err(ApiError::bad_request("Invalid subscription"));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// `POST /__transmit/unsubscribe`.
#[debug_handler]
pub async fn unsubscribe(State(ctx): State<AppContext>, body: Bytes) -> Reply {
    let params: SubscriptionParams = json_body(&body)?;
    let unsubscribed = sse::unsubscribe(
        &ctx,
        params.uid.as_deref().unwrap_or_default(),
        params.channel.as_deref().unwrap_or_default(),
    )
    .await?;
    if !unsubscribed {
        return Err(ApiError::bad_request("Invalid subscription"));
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

pub fn routes() -> Routes {
    Routes::new()
        .add("/__transmit/events", get(events))
        .add("/__transmit/subscribe", post(subscribe))
        .add("/__transmit/unsubscribe", post(unsubscribe))
}
