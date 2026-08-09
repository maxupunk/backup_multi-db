//! Ligacao dos middlewares ao router do Axum.
//!
//! O Loco expoe `Hooks::after_routes`, que recebe o `AxumRouter` ja' montado —
//! e' onde as camadas globais entram, sem precisar de um `MiddlewareLayer`
//! configuravel por YAML para cada uma.
//!
//! ## O que esta' ligado hoje
//!
//! - `force_json` (3.7), que vale para toda requisicao de `/api`;
//! - o limitador **global** (600 req/min por IP), tambem de `/api`.
//!
//! ## O que ainda nao esta'
//!
//! Os limitadores `auth`, `strict` e `backup` sao **por rota** — no Adonis eles
//! aparecem como `.use(middleware.rateLimit({ limiter: 'auth' }))` em rotas
//! especificas. Ligar isso agora exigiria inventar as rotas antes de existirem;
//! entram junto com cada grupo, nas Fases 5 em diante. O limitador global e'
//! o unico que ja' faz sentido, porque ja' vale para tudo.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router as AxumRouter;
use loco_rs::prelude::*;

use crate::controllers::middlewares::force_json::force_json;
use crate::controllers::middlewares::rate_limit::{
    build_key, reset_header, KeyStrategy, RateLimiter,
};
use crate::initializers::settings::Settings;
use crate::views::errors::ApiError;

/// Estado compartilhado do limitador global.
#[derive(Clone)]
pub struct GlobalLimiter {
    limiter: Arc<RateLimiter>,
    config: crate::initializers::settings::RateLimit,
}

/// Aplica o limitador global e acrescenta os cabecalhos `X-RateLimit-*`.
///
/// Os tres cabecalhos saem em **toda** resposta, e nao apenas no 429 — e' o
/// que o Adonis faz, e a suite de contrato afirma isso em `GET /api/health`.
async fn global_rate_limit(
    State(state): State<GlobalLimiter>,
    request: Request,
    next: Next,
) -> Response {
    // `ConnectInfo` sai das extensoes em vez de virar um extractor: como
    // extractor ele exigiria que o servidor tivesse sido montado com
    // `into_make_service_with_connect_info`, e quem monta o servidor aqui e' o
    // Loco. Lendo das extensoes, o middleware funciona nos dois casos e
    // degrada para o IP `unknown` em vez de nao compilar.
    let connect_info = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .copied();
    let ip = client_ip(&request, connect_info.as_ref());
    let key = build_key("global", &ip, KeyStrategy::Ip, None);
    let decision = state.limiter.consume(&key, state.config);
    let reset = reset_header(chrono::Utc::now(), decision.retry_after);

    if !decision.allowed {
        return ApiError::RateLimited {
            retry_after: decision.retry_after,
            limit: decision.limit,
            remaining: 0,
            reset,
        }
        .into_response();
    }

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    for (name, value) in [
        ("x-ratelimit-limit", decision.limit.to_string()),
        ("x-ratelimit-remaining", decision.remaining.to_string()),
        ("x-ratelimit-reset", reset),
    ] {
        if let (Ok(name), Ok(value)) = (
            name.parse::<axum::http::HeaderName>(),
            HeaderValue::from_str(&value),
        ) {
            headers.insert(name, value);
        }
    }

    response
}

/// IP do cliente, com a mesma precedencia que o Adonis usa.
///
/// `X-Forwarded-For` primeiro, porque em producao a aplicacao roda atras de um
/// proxy e o socket veria sempre o IP do proxy — o limitador viraria um limite
/// unico para o mundo inteiro. Cai no endereco do socket quando o cabecalho
/// nao existe.
fn client_ip(request: &Request, connect_info: Option<&ConnectInfo<SocketAddr>>) -> String {
    if let Some(forwarded) = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    {
        // O cabecalho pode trazer uma cadeia; o cliente original e' o primeiro.
        if let Some(first) = forwarded.split(',').next() {
            let trimmed = first.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    connect_info.map_or_else(|| "unknown".to_string(), |info| info.0.ip().to_string())
}

/// Registra as camadas globais no router.
pub fn apply(router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
    let settings = Settings::from_json(ctx.config.settings.as_ref())?;

    let state = GlobalLimiter {
        limiter: Arc::new(RateLimiter::new()),
        config: settings.rate_limits.global,
    };

    // A ordem importa: `force_json` fica por **fora**, para que ele veja
    // tambem a resposta 429 que o limitador gera e garanta o content-type.
    Ok(router
        .layer(axum::middleware::from_fn_with_state(
            state,
            global_rate_limit,
        ))
        .layer(axum::middleware::from_fn(force_json)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn request_with(headers: &[(&str, &str)]) -> Request {
        let mut builder = Request::builder().uri("/api/health");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[test]
    fn prefers_the_forwarded_header() {
        // Sem isto, atras de um proxy o limitador veria um unico IP e limitaria
        // o mundo inteiro junto.
        let request = request_with(&[("x-forwarded-for", "203.0.113.7")]);
        assert_eq!(client_ip(&request, None), "203.0.113.7");
    }

    #[test]
    fn takes_the_first_address_of_the_chain() {
        let request = request_with(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1, 10.0.0.2")]);
        assert_eq!(client_ip(&request, None), "203.0.113.7");
    }

    #[test]
    fn falls_back_to_the_socket_address() {
        let request = request_with(&[]);
        let connect_info = ConnectInfo("198.51.100.4:5000".parse::<SocketAddr>().unwrap());

        assert_eq!(client_ip(&request, Some(&connect_info)), "198.51.100.4");
    }

    #[test]
    fn ignores_an_empty_forwarded_header() {
        let request = request_with(&[("x-forwarded-for", "   ")]);
        let connect_info = ConnectInfo("198.51.100.4:5000".parse::<SocketAddr>().unwrap());

        assert_eq!(client_ip(&request, Some(&connect_info)), "198.51.100.4");
    }

    #[test]
    fn reports_unknown_when_there_is_no_source() {
        // Melhor uma chave literal `unknown` — que agrupa e limita — do que
        // deixar passar sem limite nenhum.
        assert_eq!(client_ip(&request_with(&[]), None), "unknown");
    }
}
