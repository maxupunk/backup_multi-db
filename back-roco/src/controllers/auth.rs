//! `/api/auth/*` — status, cadastro, login, sessao e logout (tarefas 5.1 e 5.2).
//!
//! ## A ordem das checagens do cadastro e' contrato
//!
//! No Adonis o `registerValidator` inclui a regra `unique` do e-mail, e
//! `request.validateUsing` roda **antes** de qualquer linha do handler. Logo um
//! e-mail duplicado responde 422 mesmo quando ainda nao ha' nenhum usuario e o
//! token de bootstrap esta' errado. Inverter para "bootstrap primeiro" trocaria
//! o 422 por 403 nesse caso.
//!
//! ## O primeiro usuario e' diferente de todos os outros
//!
//! Quem cadastra com a tabela vazia vira administrador **ativo**; qualquer um
//! depois nasce inativo e sem token, aguardando aprovacao. E' por isso que o
//! cadastro inicial exige o token de bootstrap em producao — sem ele, o
//! primeiro estranho a achar a URL vira administrador.

use axum::body::Bytes;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use loco_rs::environment::Environment;
use loco_rs::prelude::*;
use subtle::ConstantTimeEq;
use validator::Validate;

use crate::controllers::json_body;
use crate::controllers::middlewares::auth::Authenticated;
use crate::controllers::middlewares::limiters::{enforce, Limiters};
use crate::initializers::settings::Settings;
use crate::models::_entities::{auth_access_tokens, users};
use crate::models::users::{LoginParams, NewUser, RegisterParams};
use crate::views::auth as view;
use crate::views::envelope::{Data, Message};
use crate::views::errors::{ApiError, ErrorItem};

type Reply = std::result::Result<Response, ApiError>;

/// `GET /api/auth/status` — publica, alimenta a tela de primeiro acesso.
///
/// Nao revela nada alem de "ja' existe alguem cadastrado", que e' justamente o
/// que o cliente precisa para escolher entre a tela de login e a de bootstrap.
#[debug_handler]
pub async fn status(State(ctx): State<AppContext>) -> Reply {
    let total = users::Model::count_all(&ctx.db).await?;
    let in_production = ctx.environment == Environment::Production;

    Ok(axum::Json(Data::new(view::SystemStatus {
        has_users: total > 0,
        requires_bootstrap_token: total == 0 && in_production,
    }))
    .into_response())
}

/// `POST /api/auth/register` — cadastro.
///
/// 201 nos dois desfechos de sucesso, com corpos diferentes: token e usuario
/// para o primeiro administrador, so' uma mensagem para quem fica pendente.
#[debug_handler]
pub async fn register(State(ctx): State<AppContext>, body: Bytes) -> Reply {
    let params: RegisterParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let email = params.normalized_email();

    // A unicidade faz parte da validacao no Adonis, e por isso vem antes da
    // regra de bootstrap.
    if users::Model::find_by_email(&ctx.db, &email)
        .await?
        .is_some()
    {
        return Err(ApiError::Validation(vec![ErrorItem::validation(
            "email",
            "database.unique",
            "The email has already been taken",
        )]));
    }

    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let in_production = ctx.environment == Environment::Production;
    let is_first_user = users::Model::count_all(&ctx.db).await? == 0;

    if is_first_user
        && !bootstrap_token_is_valid(
            &settings.initial_admin_bootstrap_token,
            params.bootstrap_token.as_deref(),
            in_production,
        )
    {
        return Err(ApiError::forbidden(if in_production {
            "Token de bootstrap invalido ou ausente para criar o administrador inicial."
        } else {
            "Nao foi possivel validar o bootstrap inicial."
        }));
    }

    let user = users::Model::create(
        &ctx.db,
        NewUser {
            full_name: params.full_name.clone(),
            email,
            password: params.password.clone().unwrap_or_default(),
            // O primeiro usuario ja' nasce ativo e administrador; os demais
            // esperam aprovacao.
            is_active: is_first_user,
            is_admin: is_first_user,
        },
    )
    .await?;

    if !user.is_active {
        return Ok((
            StatusCode::CREATED,
            axum::Json(Message::new(
                "Cadastro realizado. Aguarde aprovação de um administrador.",
            )),
        )
            .into_response());
    }

    let issued =
        auth_access_tokens::Model::issue(&ctx.db, user.id, settings.token_ttl_seconds()).await?;

    Ok((
        StatusCode::CREATED,
        axum::Json(Data::new(view::Session::new(issued.value, &user))),
    )
        .into_response())
}

/// `POST /api/auth/login`.
///
/// Tres desfechos, com tres status diferentes: 422 de payload invalido, **400**
/// de credencial errada (e nao 401 — foi um dos achados da Fase 2) e 401 de
/// conta ainda nao aprovada.
#[debug_handler]
pub async fn login(State(ctx): State<AppContext>, body: Bytes) -> Reply {
    let params: LoginParams = json_body(&body)?;
    Validate::validate(&params).map_err(|errors| ApiError::from_validation_errors(&errors))?;

    let user = users::Model::authenticate(
        &ctx.db,
        &params.normalized_email(),
        params.password.as_deref().unwrap_or_default(),
    )
    .await?
    .ok_or(ApiError::InvalidCredentials)?;

    // Depois da senha, nunca antes: responder "conta pendente" para quem errou
    // a senha confirmaria que o e-mail existe.
    if !user.is_active {
        return Err(ApiError::unauthorized("Sua conta aguarda aprovação."));
    }

    let settings = Settings::from_json(ctx.config.settings.as_ref())?;
    let issued =
        auth_access_tokens::Model::issue(&ctx.db, user.id, settings.token_ttl_seconds()).await?;

    Ok(axum::Json(Data::new(view::Session::new(issued.value, &user))).into_response())
}

/// `GET /api/auth/me` — o usuario da sessao atual.
///
/// Responde 200 mesmo para conta desativada. Nao e' descuido: desativar um
/// usuario no Adonis **nao** revoga o token dele, que segue valido ate' expirar
/// (7 dias). Esta' registrado como achado da Fase 2 e reproduzido de proposito.
#[debug_handler]
pub async fn me(State(_ctx): State<AppContext>, session: Authenticated) -> Reply {
    Ok(axum::Json(Data::new(view::CurrentUser::from(&session.user))).into_response())
}

/// `POST /api/auth/logout` — revoga **este** token, nao todos.
///
/// Um logout que derrubasse todas as sessoes desconectaria o celular de quem
/// so' fechou o navegador. O teste de contrato afirma exatamente isso.
#[debug_handler]
pub async fn logout(State(ctx): State<AppContext>, session: Authenticated) -> Reply {
    auth_access_tokens::Model::revoke(&ctx.db, session.token.id, session.user.id).await?;

    Ok(axum::Json(Message::new("Logged out successfully")).into_response())
}

/// Confere o token de bootstrap do primeiro administrador.
///
/// Sem token configurado, so' passa fora de producao — igual ao
/// `hasValidBootstrapToken` do Adonis. A comparacao e' em tempo constante: um
/// `==` comum sai no primeiro byte diferente, e a diferenca de tempo permite
/// descobrir o token caractere a caractere.
fn bootstrap_token_is_valid(
    configured: &str,
    candidate: Option<&str>,
    in_production: bool,
) -> bool {
    if configured.is_empty() {
        return !in_production;
    }

    let Some(candidate) = candidate else {
        return false;
    };

    // Tamanhos diferentes ja' bastam para recusar, e comparar em tempo
    // constante strings de tamanhos diferentes nao e' possivel mesmo.
    if configured.len() != candidate.len() {
        return false;
    }

    configured.as_bytes().ct_eq(candidate.as_bytes()).into()
}

/// Rotas de `/api/auth`.
///
/// `register` e `login` levam o limitador `auth` — 5 por minuto por IP+e-mail.
/// `status` fica so' com o global: e' a rota que a tela de abertura consulta, e
/// limita-la a cinco por minuto travaria o primeiro acesso de um escritorio
/// inteiro atras do mesmo IP.
pub fn routes(limiters: &Limiters) -> Routes {
    let auth_limit = axum::middleware::from_fn_with_state(limiters.auth(), enforce);

    Routes::new()
        .prefix("/api/auth")
        .add("/status", get(status))
        .add("/register", post(register).layer(auth_limit.clone()))
        .add("/login", post(login).layer(auth_limit))
        .add("/me", get(me))
        .add("/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_configured_token_only_development_passes() {
        // Em producao, ausencia de token configurado tem que **bloquear**: a
        // alternativa e' o primeiro estranho a achar a URL virar administrador.
        assert!(bootstrap_token_is_valid("", None, false));
        assert!(!bootstrap_token_is_valid("", None, true));
        assert!(!bootstrap_token_is_valid("", Some("qualquer"), true));
    }

    #[test]
    fn a_configured_token_must_match_exactly() {
        assert!(bootstrap_token_is_valid("s3gr3d0", Some("s3gr3d0"), true));
        assert!(!bootstrap_token_is_valid("s3gr3d0", Some("s3gr3d1"), true));
        assert!(!bootstrap_token_is_valid("s3gr3d0", Some("s3gr3d"), true));
        assert!(!bootstrap_token_is_valid("s3gr3d0", Some(""), true));
        assert!(!bootstrap_token_is_valid("s3gr3d0", None, true));
    }

    #[test]
    fn a_configured_token_is_required_outside_production_too() {
        // Configurar o token e' um ato deliberado; ignora-lo em
        // desenvolvimento faria o ambiente de homologacao mentir sobre o de
        // producao.
        assert!(!bootstrap_token_is_valid("s3gr3d0", Some("errado"), false));
        assert!(bootstrap_token_is_valid("s3gr3d0", Some("s3gr3d0"), false));
    }
}
