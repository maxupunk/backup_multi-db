//! Ligação das camadas globais ao router do Axum.
//!
//! O Loco expõe `Hooks::after_routes`, que recebe o `AxumRouter` já montado —
//! é onde entra o que vale para **toda** requisição. Os middlewares que o
//! próprio Loco traz (`catch_panic`, `timeout_request`, `limit_payload`,
//! `secure_headers`, `request_id`, `compression`) são ligados pelo bloco
//! `server.middlewares` do YAML, e não aqui.
//!
//! Sobra o limitador global de requisições por IP, que o Loco não cobre.

use axum::Router as AxumRouter;
use loco_rs::prelude::*;

use crate::controllers::middlewares::limiters::Limiters;

/// Registra as camadas globais no router.
///
/// O limitador global fica por **fora** dos limitadores de rota. Como o
/// `tower-governor` escreve os `x-ratelimit-*` na volta, quem escreve por
/// último vence — e é o global, o mais frouxo, que sobra na resposta. É o
/// oposto do que interessa a quem lê o cabeçalho numa rota com limite próprio,
/// e por isso o limitador de rota também escreve `retry-after` no 429, que o
/// global não sobrescreve.
///
/// # Errors
/// Falha quando o bloco `settings:` não pode ser lido.
pub fn apply(router: AxumRouter, ctx: &AppContext) -> Result<AxumRouter> {
    let limiters = Limiters::shared(ctx)?;

    Ok(router.layer(limiters.global()))
}
