//! De onde veio a requisicao, para a trilha de auditoria (tarefa 6.9).
//!
//! O `AuditService` do Adonis le' `ctx.request.ip()` e o cabecalho
//! `User-Agent` do proprio contexto HTTP. Em Axum o handler nao tem acesso ao
//! contexto — so' ao que declara na assinatura —, entao os dois viram um
//! extractor.
//!
//! E' **infalivel** de proposito: uma requisicao sem `User-Agent` ou sem
//! endereco conhecido continua valendo. Falhar aqui transformaria um detalhe de
//! telemetria em recusa de uma operacao legitima.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::header;
use axum::http::request::Parts;
use std::convert::Infallible;

use crate::controllers::middlewares::limiters::client_ip;

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
        // Mesma precedencia do limitador: `X-Forwarded-For` antes do socket,
        // porque atras de um proxy o socket e' sempre o do proxy — e uma
        // auditoria que registra o IP do proxy nao registra nada.
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
