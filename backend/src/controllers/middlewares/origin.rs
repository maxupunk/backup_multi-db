//! De onde veio a requisicao, para a trilha de auditoria.
//!
//! Em Axum o handler so' tem acesso ao que declara na assinatura, entao o IP e
//! o agente do cliente viram um extractor.
//!
//! E' **infalivel** de proposito: uma requisicao sem `User-Agent` ou sem
//! endereco conhecido continua valendo. Falhar aqui transformaria um detalhe de
//! telemetria em recusa de uma operacao legitima.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap};
use std::convert::Infallible;

/// IP e agente do cliente.
#[derive(Debug, Clone, Default)]
pub struct RequestOrigin {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for RequestOrigin {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        // Mesma precedencia do `SmartIpKeyExtractor` que o limitador usa:
        // `X-Forwarded-For` antes do socket, porque atras de um proxy o socket
        // e' sempre o do proxy — e uma auditoria que registra o IP do proxy nao
        // registra nada.
        let connect_info = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0);
        let ip = client_ip(&parts.headers, connect_info);

        Ok(Self {
            // `unknown` e' o que o limitador usa como chave, mas numa coluna de
            // auditoria e' ruido: melhor `NULL`, que diz "nao sabemos".
            ip: (ip != "unknown").then_some(ip),
            user_agent: parts
                .headers
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(ToString::to_string),
        })
    }
}

/// IP do cliente: `X-Forwarded-For` primeiro, socket depois.
///
/// Espelha o `SmartIpKeyExtractor` do `tower-governor`, que o limitador usa.
/// Sao dois codigos porque o extractor do governor le' uma `Request` inteira e
/// aqui so' ha' `Parts` — mas a precedencia tem de ser a mesma, senao o IP que
/// o limitador conta nao e' o IP que a auditoria registra.
fn client_ip(headers: &HeaderMap, connect_info: Option<SocketAddr>) -> String {
    if let Some(forwarded) = headers
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

    connect_info.map_or_else(|| "unknown".to_string(), |info| info.ip().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    async fn origin_of(headers: &[(&str, &str)]) -> RequestOrigin {
        let mut builder = Request::builder().uri("/api/connections");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let (mut parts, ()) = builder.body(()).expect("requisicao").into_parts();

        RequestOrigin::from_request_parts(&mut parts, &())
            .await
            .expect("extractor infalivel")
    }

    #[tokio::test]
    async fn reads_the_forwarded_address_and_the_agent() {
        let origin =
            origin_of(&[("x-forwarded-for", "203.0.113.7"), ("user-agent", "curl/8")]).await;

        assert_eq!(origin.ip.as_deref(), Some("203.0.113.7"));
        assert_eq!(origin.user_agent.as_deref(), Some("curl/8"));
    }

    #[tokio::test]
    async fn an_unknown_source_becomes_null_instead_of_the_literal() {
        // `unknown` numa coluna de auditoria e' ruido que parece dado.
        let origin = origin_of(&[]).await;

        assert_eq!(origin.ip, None);
        assert_eq!(origin.user_agent, None);
    }
}
