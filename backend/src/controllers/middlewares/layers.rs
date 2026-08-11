//! Ligacao dos middlewares globais ao router do Axum.
//!
//! O Loco expoe `Hooks::after_routes`, que recebe o `AxumRouter` ja' montado —
//! e' onde as camadas globais entram, sem precisar de um `MiddlewareLayer`
//! configuravel por YAML para cada uma.
//!
//! O que esta' ligado aqui e' o que vale para **toda** requisicao: o
//! `force_json` (3.7) e o limitador global de 600 req/min por IP. Os
//! limitadores `auth`, `strict` e `backup` sao por rota e entram em
//! `controllers::<recurso>::routes`, com o mesmo [`enforce`] — ver
//! [`super::limiters`].

use axum::Router as AxumRouter;
use loco_rs::prelude::*;

use crate::controllers::middlewares::force_json::force_json;
use crate::controllers::middlewares::limiters::{enforce, Limiters};

/// Registra as camadas globais no router.
///
/// A ordem importa duas vezes:
///
/// - `force_json` fica por **fora**, para ver tambem a resposta 429 que o
///   limitador gera e garantir o content-type nela;
/// - o limitador global fica por fora dos limitadores de rota, e por isso so'
///   escreve os cabecalhos `X-RateLimit-*` que a rota nao escreveu.
pub fn apply(router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
    let limiters = Limiters::shared(ctx)?;

    Ok(router
        .layer(axum::middleware::from_fn_with_state(
            limiters.global(),
            enforce,
        ))
        .layer(axum::middleware::from_fn(force_json)))
}
