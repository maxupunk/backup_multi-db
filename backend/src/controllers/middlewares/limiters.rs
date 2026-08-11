//! Os quatro limitadores nomeados, prontos para virar camada (tarefa 3.5).
//!
//! [`super::rate_limit`] tem o **algoritmo** — janela fixa, contagem, chave.
//! Aqui esta' a ligacao com o HTTP: de onde sai o IP, como o e-mail entra na
//! chave, quais cabecalhos a resposta ganha e quem tem precedencia quando duas
//! camadas querem escrever o mesmo cabecalho.
//!
//! ## Por que uma so' loja para os quatro
//!
//! O Adonis usa um unico store em memoria e prefixa a chave com o nome do
//! limitador (`auth_1.2.3.4_maria@x.com`). Duas lojas separadas dariam ao mesmo
//! par IP+e-mail dois contadores independentes em `login` e `register` — e as
//! duas rotas usam o limitador `auth` justamente para dividirem o mesmo
//! orcamento de cinco tentativas por minuto.
//!
//! ## Precedencia dos cabecalhos `X-RateLimit-*`
//!
//! No Adonis o middleware escreve os cabecalhos **antes** de chamar o proximo,
//! entao o limitador de rota (que roda por ultimo) sobrescreve o global. Em
//! Axum a resposta so' existe na volta, e a ordem se inverte: o limitador de
//! rota escreve primeiro. Por isso [`enforce`] so' escreve o cabecalho que
//! ainda **nao** existe. Sem essa regra, `POST /api/auth/login` responderia
//! `x-ratelimit-limit: 600` em vez de `5`, contrariando o golden
//! `auth/login-rate-limited`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;

use crate::controllers::middlewares::rate_limit::{
    build_key, reset_header, KeyStrategy, RateLimiter,
};
use crate::initializers::settings::{RateLimit, Settings};
use crate::views::errors::ApiError;

/// Teto do corpo bufferizado para extrair o e-mail da chave.
///
/// E' o mesmo limite default do bodyparser do Adonis. Sem teto, uma requisicao
/// com corpo gigante seria lida inteira na memoria **antes** de o limitador
/// decidir bloquea-la — o limitador viraria o vetor de ataque.
const MAX_BUFFERED_BODY: usize = 1024 * 1024;

/// Nome do limitador, igual a' chave de `LIMITS` no Adonis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Global,
    Auth,
    Strict,
    Backup,
}

impl Named {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Auth => "auth",
            Self::Strict => "strict",
            Self::Backup => "backup",
        }
    }

    /// So' o limitador de autenticacao entra o e-mail na chave.
    ///
    /// E' o que impede que cinco tentativas contra `maria@x.com` bloqueiem
    /// tambem o `joao@x.com` que sai do mesmo IP corporativo — e, na direcao
    /// contraria, que trocar de e-mail zere o contador de um atacante.
    const fn strategy(self) -> KeyStrategy {
        match self {
            Self::Auth => KeyStrategy::IpEmail,
            _ => KeyStrategy::Ip,
        }
    }
}

/// Loja compartilhada mais os limites configurados.
#[derive(Debug, Clone)]
pub struct Limiters {
    store: Arc<RateLimiter>,
    limits: crate::initializers::settings::RateLimits,
}

impl Limiters {
    /// A instancia compartilhada da aplicacao, criada uma unica vez.
    ///
    /// Os limitadores nascem em dois ganchos diferentes do Loco — o global em
    /// `after_routes`, os de rota em `routes` — e sem um ponto comum cada
    /// gancho teria a sua propria loja. As chaves sao prefixadas pelo nome do
    /// limitador, entao duas lojas nao se confundiriam; mas contariam em
    /// separado o dia em que dois limitadores dividirem uma chave, e o
    /// `SharedStore` do Loco existe exatamente para este caso.
    pub fn shared(ctx: &AppContext) -> Result<Self> {
        if let Some(existing) = ctx.shared_store.get::<Self>() {
            return Ok(existing);
        }

        let limiters = Self::from_context(ctx)?;
        ctx.shared_store.insert(limiters.clone());

        Ok(limiters)
    }

    /// Le' os limites do bloco `settings:`.
    pub fn from_context(ctx: &AppContext) -> Result<Self> {
        let settings = Settings::from_json(ctx.config.settings.as_ref())?;

        Ok(Self {
            store: Arc::new(RateLimiter::new()),
            limits: settings.rate_limits,
        })
    }

    /// Limitadores com os numeros default — os mesmos do Adonis.
    ///
    /// Existe para o gancho `Hooks::routes`, que nao devolve `Result`: uma
    /// configuracao quebrada ja' derruba o boot no `SettingsInitializer`, e
    /// entrar em panico aqui trocaria aquela mensagem clara por um stack trace.
    pub fn with_defaults() -> Self {
        Self {
            store: Arc::new(RateLimiter::new()),
            limits: crate::initializers::settings::RateLimits::default(),
        }
    }

    /// Um limitador nomeado, pronto para virar camada.
    pub fn named(&self, name: Named) -> RouteLimiter {
        let config = match name {
            Named::Global => self.limits.global,
            Named::Auth => self.limits.auth,
            Named::Strict => self.limits.strict,
            Named::Backup => self.limits.backup,
        };

        RouteLimiter {
            store: Arc::clone(&self.store),
            name,
            config,
        }
    }

    /// Atalhos para os limitadores usados nas rotas.
    pub fn auth(&self) -> RouteLimiter {
        self.named(Named::Auth)
    }

    pub fn strict(&self) -> RouteLimiter {
        self.named(Named::Strict)
    }

    pub fn backup(&self) -> RouteLimiter {
        self.named(Named::Backup)
    }

    pub fn global(&self) -> RouteLimiter {
        self.named(Named::Global)
    }
}

/// Um limitador ja' resolvido, com a loja e a configuracao dele.
#[derive(Debug, Clone)]
pub struct RouteLimiter {
    store: Arc<RateLimiter>,
    name: Named,
    config: RateLimit,
}

/// Consome uma unidade do limitador e anexa os cabecalhos a' resposta.
///
/// Vira camada com `axum::middleware::from_fn_with_state(limiters.auth(),
/// enforce)`. Nao ha' um `RouteLimiter::layer()` embrulhando isso porque o tipo
/// de retorno de `from_fn_with_state` nao e' nomeavel sem `Box`, e o `Box` na
/// camada custaria uma alocacao por requisicao para esconder uma linha.
pub async fn enforce(
    State(limiter): State<RouteLimiter>,
    request: Request,
    next: Next,
) -> Response {
    let connect_info = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);

    let (parts, body) = request.into_parts();
    let ip = client_ip(&parts.headers, connect_info);

    // O corpo so' e' lido quando o e-mail faz parte da chave. Bufferizar em
    // toda rota custaria uma copia do upload de um backup para nada.
    let (email, body) = if limiter.name.strategy() == KeyStrategy::IpEmail {
        match buffer_email(body).await {
            Ok(pair) => pair,
            Err(error) => return error.into_response(),
        }
    } else {
        (None, body)
    };

    let key = build_key(
        limiter.name.as_str(),
        &ip,
        limiter.name.strategy(),
        email.as_deref(),
    );
    let decision = limiter.store.consume(&key, limiter.config);
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

    let mut response = next.run(Request::from_parts(parts, body)).await;

    apply_headers(
        response.headers_mut(),
        decision.limit,
        decision.remaining,
        &reset,
    );

    response
}

/// Escreve os `X-RateLimit-*` que ainda nao existem.
///
/// Nunca sobrescreve: o limitador mais interno e' o mais especifico, e e' o
/// numero dele que o Adonis mostra.
fn apply_headers(headers: &mut HeaderMap, limit: u32, remaining: u32, reset: &str) {
    for (name, value) in [
        ("x-ratelimit-limit", limit.to_string()),
        ("x-ratelimit-remaining", remaining.to_string()),
        ("x-ratelimit-reset", reset.to_string()),
    ] {
        let Ok(name) = name.parse::<axum::http::HeaderName>() else {
            continue;
        };
        if headers.contains_key(&name) {
            continue;
        }
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers.insert(name, value);
        }
    }
}

/// Le' o corpo, extrai `email` e devolve o corpo intacto para o handler.
///
/// Um corpo que nao seja JSON, ou sem `email`, nao e' erro: o Adonis cai na
/// chave so' de IP nesse caso, e a validacao do handler e' quem reclama do
/// formato.
async fn buffer_email(body: Body) -> std::result::Result<(Option<String>, Body), ApiError> {
    let bytes: Bytes = axum::body::to_bytes(body, MAX_BUFFERED_BODY)
        .await
        .map_err(|_| ApiError::Controller {
            status: axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            message: "Corpo da requisição é grande demais".to_string(),
            error: None,
        })?;

    let email = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| {
            value
                .get("email")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        });

    Ok((email, Body::from(bytes)))
}

/// IP do cliente, com a mesma precedencia que o Adonis usa.
///
/// `X-Forwarded-For` primeiro, porque em producao a aplicacao roda atras de um
/// proxy e o socket veria sempre o IP do proxy — o limitador viraria um limite
/// unico para o mundo inteiro. Cai no endereco do socket quando o cabecalho
/// nao existe.
pub fn client_ip(headers: &HeaderMap, connect_info: Option<SocketAddr>) -> String {
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
    use axum::http::Request as HttpRequest;

    fn headers_with(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut builder = HttpRequest::builder().uri("/api/health");
        for (name, value) in pairs {
            builder = builder.header(*name, *value);
        }
        builder.body(()).expect("requisicao").into_parts().0.headers
    }

    #[test]
    fn prefers_the_forwarded_header() {
        // Sem isto, atras de um proxy o limitador veria um unico IP e limitaria
        // o mundo inteiro junto.
        let headers = headers_with(&[("x-forwarded-for", "203.0.113.7")]);
        assert_eq!(client_ip(&headers, None), "203.0.113.7");
    }

    #[test]
    fn takes_the_first_address_of_the_chain() {
        let headers = headers_with(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1, 10.0.0.2")]);
        assert_eq!(client_ip(&headers, None), "203.0.113.7");
    }

    #[test]
    fn falls_back_to_the_socket_address() {
        let socket = "198.51.100.4:5000".parse::<SocketAddr>().expect("socket");
        assert_eq!(client_ip(&headers_with(&[]), Some(socket)), "198.51.100.4");
    }

    #[test]
    fn ignores_an_empty_forwarded_header() {
        let socket = "198.51.100.4:5000".parse::<SocketAddr>().expect("socket");
        let headers = headers_with(&[("x-forwarded-for", "   ")]);

        assert_eq!(client_ip(&headers, Some(socket)), "198.51.100.4");
    }

    #[test]
    fn reports_unknown_when_there_is_no_source() {
        // Melhor uma chave literal `unknown` — que agrupa e limita — do que
        // deixar passar sem limite nenhum.
        assert_eq!(client_ip(&headers_with(&[]), None), "unknown");
    }

    #[test]
    fn only_the_auth_limiter_keys_by_email() {
        assert_eq!(Named::Auth.strategy(), KeyStrategy::IpEmail);
        for name in [Named::Global, Named::Strict, Named::Backup] {
            assert_eq!(name.strategy(), KeyStrategy::Ip, "{name:?}");
        }
    }

    #[test]
    fn the_limiter_name_prefixes_the_key() {
        // Os quatro dividem a mesma loja; o prefixo e' o que os separa.
        assert_eq!(
            build_key(
                Named::Auth.as_str(),
                "1.2.3.4",
                KeyStrategy::IpEmail,
                Some("a@b.com")
            ),
            "auth_1.2.3.4_a@b.com"
        );
        assert_eq!(
            build_key(Named::Global.as_str(), "1.2.3.4", KeyStrategy::Ip, None),
            "global_1.2.3.4"
        );
    }

    #[test]
    fn never_overwrites_a_header_already_written() {
        // O limitador de rota escreve primeiro na volta; o global nao pode
        // trocar o `5` do login pelo `600` dele.
        let mut headers = HeaderMap::new();
        apply_headers(&mut headers, 5, 4, "2026-08-09T12:00:00.000Z");
        apply_headers(&mut headers, 600, 599, "2026-08-09T12:00:00.000Z");

        assert_eq!(headers.get("x-ratelimit-limit").unwrap(), "5");
        assert_eq!(headers.get("x-ratelimit-remaining").unwrap(), "4");
    }

    #[tokio::test]
    async fn extracts_the_email_and_gives_the_body_back() {
        let body = Body::from(r#"{"email":"Maria@X.com","password":"s"}"#);
        let (email, body) = buffer_email(body).await.expect("corpo pequeno");

        assert_eq!(email.as_deref(), Some("Maria@X.com"));
        // O handler ainda precisa ler o corpo inteiro depois do limitador.
        let bytes = axum::body::to_bytes(body, MAX_BUFFERED_BODY).await.unwrap();
        assert_eq!(
            String::from_utf8(bytes.to_vec()).unwrap(),
            r#"{"email":"Maria@X.com","password":"s"}"#
        );
    }

    #[tokio::test]
    async fn a_body_without_an_email_is_not_an_error() {
        for raw in ["{}", "nao e json", "", "[1,2,3]", r#"{"email":42}"#] {
            let (email, _) = buffer_email(Body::from(raw)).await.expect("corpo aceito");
            assert_eq!(email, None, "extraiu e-mail de {raw:?}");
        }
    }

    #[tokio::test]
    async fn refuses_a_body_over_the_ceiling() {
        let oversized = Body::from("x".repeat(MAX_BUFFERED_BODY + 1));
        let error = buffer_email(oversized).await.expect_err("devia recusar");

        assert_eq!(error.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    }
}
