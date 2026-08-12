//! Os quatro limitadores nomeados da aplicação, sobre o `tower-governor`.
//!
//! ## Janela deslizante, não fixa
//!
//! O algoritmo é GCRA: um balde de `requests` fichas que repõe uma ficha a cada
//! `duration / requests`. A janela fixa que existia aqui antes zerava o contador
//! na virada do minuto, e por isso aceitava **duas vezes** o limite em torno
//! dela — 5 tentativas às 12:00:59 e mais 5 às 12:01:00. O GCRA não tem virada.
//!
//! ## A chave é o IP, e só o IP
//!
//! O limitador de autenticação já foi `IP + e-mail`. A troca custa e ganha
//! coisas diferentes, e vale registrar as duas:
//!
//! - **perde-se** independência entre contas: um escritório atrás de um NAT
//!   divide o mesmo orçamento, e por isso o limite de `auth` subiu de 5 para 20
//!   por minuto — cinco por minuto para uma empresa inteira tranca a porta na
//!   primeira pessoa que erra a senha duas vezes;
//! - **ganha-se** defesa contra password spraying, que era justamente o furo da
//!   chave antiga: trocar de e-mail zerava o contador, então 5 por (IP, e-mail)
//!   dava tentativas ilimitadas a quem varria uma lista de endereços.
//!
//! Tecnicamente a chave antiga também não é reproduzível aqui: o extrator de
//! chave do `tower-governor` só vê as partes da requisição, e o e-mail está no
//! corpo. Ler o corpo antes de decidir bloquear é o que tornava o limitador
//! antigo um vetor de ataque por si só — daí o teto de 1 MB que ele precisava
//! carregar.
//!
//! ## Só os limitadores de rota escrevem `X-RateLimit-*`
//!
//! As camadas do Axum montam a resposta de dentro para fora, então o limitador
//! **global**, que é o mais externo, escreveria por último e sobrescreveria os
//! cabeçalhos do limitador da rota — `POST /api/auth/login` anunciaria o teto de
//! 600 em vez do seu próprio. Por isso o global roda sem cabeçalho: o número que
//! interessa a um cliente é o da rota que ele está chamando, e o global é uma
//! defesa contra abuso, não um orçamento para o cliente se organizar.
//!
//! `Retry-After` é escrito à mão em [`refused`], e por isso sai nos dois casos.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use loco_rs::prelude::*;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::{GovernorError, GovernorLayer};

use crate::initializers::settings::{RateLimit, RateLimits, Settings};

/// O middleware que acompanha cada resposta com os `x-ratelimit-*`.
///
/// Vem do `governor`, e não do `tower-governor`, porque este último usa o tipo
/// mas não o reexporta — nomeá-lo aqui exige a dependência direta, que é a
/// mesma versão que o `tower-governor` já traz.
type Headers = governor::middleware::StateInformationMiddleware;

/// O middleware do limitador global: conta, mas não escreve cabeçalho.
type Silent = governor::middleware::NoOpMiddleware<governor::clock::QuantaInstant>;

type RouteConfig = GovernorConfig<SmartIpKeyExtractor, Headers>;
type GlobalConfig = GovernorConfig<SmartIpKeyExtractor, Silent>;

/// A camada que um `routes()` pendura numa rota.
pub type RouteLayer = GovernorLayer<SmartIpKeyExtractor, Headers, Body>;

/// A camada global, ligada em `after_routes`.
pub type GlobalLayer = GovernorLayer<SmartIpKeyExtractor, Silent, Body>;

/// Os quatro limitadores, já configurados.
///
/// Cada um tem o seu próprio balde: o `GovernorConfig` guarda o mapa de chaves,
/// e compartilhar um só entre `auth` e `backup` faria uma requisição de backup
/// consumir ficha de login.
#[derive(Clone)]
pub struct Limiters {
    global: Arc<GlobalConfig>,
    auth: Arc<RouteConfig>,
    strict: Arc<RouteConfig>,
    backup: Arc<RouteConfig>,
}

impl Limiters {
    /// A instância compartilhada da aplicação, criada uma única vez.
    ///
    /// Os limitadores nascem em dois ganchos diferentes do Loco — o global em
    /// `after_routes`, os de rota em `routes` — e sem um ponto comum cada
    /// gancho teria o seu próprio balde, dobrando na prática cada limite.
    ///
    /// # Errors
    /// Falha quando o bloco `settings:` não pode ser lido.
    pub fn shared(ctx: &AppContext) -> Result<Self> {
        if let Some(existing) = ctx.shared_store.get::<Self>() {
            return Ok(existing);
        }

        let settings = Settings::from_json(ctx.config.settings.as_ref())?;
        let limiters = Self::new(&settings.rate_limits);
        ctx.shared_store.insert(limiters.clone());

        Ok(limiters)
    }

    fn new(limits: &RateLimits) -> Self {
        Self {
            global: Arc::new(silent(limits.global)),
            auth: Arc::new(with_headers(limits.auth)),
            strict: Arc::new(with_headers(limits.strict)),
            backup: Arc::new(with_headers(limits.backup)),
        }
    }

    /// Limitadores com os números default.
    ///
    /// Existe para o gancho `Hooks::routes`, que não devolve `Result`: uma
    /// configuração quebrada já derruba o boot no `SettingsInitializer`, e
    /// entrar em pânico aqui trocaria a mensagem clara dele por um backtrace.
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(&RateLimits::default())
    }

    #[must_use]
    pub fn global(&self) -> GlobalLayer {
        GovernorLayer::new(self.global.clone()).error_handler(refused)
    }

    #[must_use]
    pub fn auth(&self) -> RouteLayer {
        GovernorLayer::new(self.auth.clone()).error_handler(refused)
    }

    #[must_use]
    pub fn strict(&self) -> RouteLayer {
        GovernorLayer::new(self.strict.clone()).error_handler(refused)
    }

    #[must_use]
    pub fn backup(&self) -> RouteLayer {
        GovernorLayer::new(self.backup.clone()).error_handler(refused)
    }
}

/// Traduz `requests` por `duration` no par (período de reposição, rajada) do
/// GCRA.
///
/// O período é o intervalo de reposição de **uma** ficha; a rajada é quantas
/// cabem no balde. `600/60s` vira "uma ficha a cada 100 ms, até 600 acumuladas".
///
/// Os `max(1)` existem porque um período ou uma rajada zero fazem o builder
/// devolver `None` — e um número absurdo no YAML viraria "sem limitador
/// nenhum", o modo de falhar mais silencioso possível para isto. O fallback
/// abaixo cobre o resto: 60/min, e não um `expect`, porque entrar em pânico
/// aqui derrubaria o boot por um número mal digitado.
const FALLBACK: RateLimit = RateLimit::new(60, 60);

fn quota(limit: RateLimit) -> (Duration, u32) {
    let burst = limit.requests.max(1);
    let replenish = Duration::from_millis((limit.duration * 1000 / u64::from(burst)).max(1));

    (replenish, burst)
}

/// Limitador de rota: conta e anuncia o próprio teto nos `x-ratelimit-*`.
fn with_headers(limit: RateLimit) -> RouteConfig {
    build_with_headers(limit).unwrap_or_else(|| {
        tracing::error!(?limit, "limite inválido; usando o fallback");
        build_with_headers(FALLBACK).expect("o fallback do limitador é válido")
    })
}

fn build_with_headers(limit: RateLimit) -> Option<RouteConfig> {
    let (replenish, burst) = quota(limit);

    GovernorConfigBuilder::default()
        .period(replenish)
        .burst_size(burst)
        .key_extractor(SmartIpKeyExtractor)
        .use_headers()
        .finish()
}

/// Limitador global: conta sem escrever cabeçalho — ver a nota no topo.
fn silent(limit: RateLimit) -> GlobalConfig {
    build_silent(limit).unwrap_or_else(|| {
        tracing::error!(?limit, "limite global inválido; usando o fallback");
        build_silent(FALLBACK).expect("o fallback do limitador é válido")
    })
}

fn build_silent(limit: RateLimit) -> Option<GlobalConfig> {
    let (replenish, burst) = quota(limit);

    GovernorConfigBuilder::default()
        .period(replenish)
        .burst_size(burst)
        .key_extractor(SmartIpKeyExtractor)
        .finish()
}

/// Resposta de recusa, no mesmo shape de erro do resto da API.
///
/// Sem isto o `tower-governor` responde `text/plain` com "Too Many Requests!
/// Wait for Ns" — um segundo formato de erro que todo cliente teria de tratar.
fn refused(error: GovernorError) -> Response<Body> {
    let (status, description, wait, headers) = match error {
        GovernorError::TooManyRequests { wait_time, headers } => (
            StatusCode::TOO_MANY_REQUESTS,
            format!("Muitas requisições. Tente novamente em {wait_time}s."),
            Some(wait_time),
            headers,
        ),
        // O extrator de IP falhou: sem chave não há como contar, e responder
        // 200 seria abrir a rota justamente quando o limitador cegou.
        GovernorError::UnableToExtractKey => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Não foi possível identificar a origem da requisição.".to_string(),
            None,
            None,
        ),
        GovernorError::Other { code, msg, headers } => (
            code,
            msg.unwrap_or_else(|| "Requisição recusada.".to_string()),
            None,
            headers,
        ),
    };

    let mut response = Error::CustomError(
        status,
        loco_rs::controller::ErrorDetail::new(reason_for(status), description),
    )
    .into_response();

    if let Some(headers) = headers {
        response.headers_mut().extend(headers);
    }

    // Escrito aqui, e não só pelo `use_headers()`: o limitador global roda sem
    // cabeçalho, e um 429 sem `Retry-After` obriga o cliente a adivinhar.
    if let Some(wait) = wait {
        if let Ok(value) = HeaderValue::from_str(&wait.to_string()) {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, value);
        }
    }

    response
}

const fn reason_for(status: StatusCode) -> &'static str {
    match status {
        StatusCode::TOO_MANY_REQUESTS => "too_many_requests",
        StatusCode::INTERNAL_SERVER_ERROR => "internal_server_error",
        _ => "request_refused",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(value: &str) -> std::net::IpAddr {
        value.parse().expect("endereço")
    }

    #[test]
    fn the_burst_is_exactly_the_configured_number() {
        let config = with_headers(RateLimit::new(3, 60));
        let address = ip("10.0.0.1");

        for attempt in 1..=3 {
            assert!(
                config.limiter().check_key(&address).is_ok(),
                "tentativa {attempt} devia passar"
            );
        }

        // A quarta não tem ficha: repor uma leva 20 s.
        assert!(config.limiter().check_key(&address).is_err());
    }

    #[test]
    fn two_addresses_have_independent_buckets() {
        let config = with_headers(RateLimit::new(1, 60));

        assert!(config.limiter().check_key(&ip("10.0.0.1")).is_ok());
        assert!(config.limiter().check_key(&ip("10.0.0.1")).is_err());
        assert!(config.limiter().check_key(&ip("10.0.0.2")).is_ok());
    }

    #[test]
    fn a_nonsense_limit_still_limits() {
        // Zero requisições por minuto seria "período infinito"; o limitador cai
        // num valor real em vez de sumir.
        let config = with_headers(RateLimit::new(0, 60));

        assert!(config.limiter().check_key(&ip("10.0.0.9")).is_ok());
        assert!(config.limiter().check_key(&ip("10.0.0.9")).is_err());
    }

    #[test]
    fn each_limiter_has_its_own_bucket() {
        // Um balde compartilhado faria uma requisição de backup consumir ficha
        // de login.
        let limiters = Limiters::new(&RateLimits {
            auth: RateLimit::new(1, 60),
            backup: RateLimit::new(1, 60),
            ..RateLimits::default()
        });
        let address = ip("10.0.0.3");

        assert!(limiters.auth.limiter().check_key(&address).is_ok());
        assert!(limiters.auth.limiter().check_key(&address).is_err());
        assert!(limiters.backup.limiter().check_key(&address).is_ok());
    }

    #[tokio::test]
    async fn the_refusal_carries_the_api_error_shape_and_retry_after() {
        let response = refused(GovernorError::TooManyRequests {
            wait_time: 12,
            headers: None,
        });

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            Some("12")
        );

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("corpo");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

        assert_eq!(body["error"], "too_many_requests");
        assert_eq!(
            body["description"],
            "Muitas requisições. Tente novamente em 12s."
        );
    }
}
