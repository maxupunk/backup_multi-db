//! `/api/auth/*` — status, registration, login, session, recovery and logout.
//!
//! ## The order of the registration checks is contract
//!
//! E-mail uniqueness is part of validation, so a duplicate address answers 422
//! even when the table is empty and the bootstrap token is wrong. Reordering to
//! "bootstrap first" would turn that 422 into a 403.
//!
//! ## The first user is unlike every other
//!
//! Whoever registers against an empty table becomes an **active** administrator;
//! anyone after that is created inactive and without a token, awaiting
//! approval. That is why the initial registration demands the bootstrap token in
//! production — without it, the first stranger to find the URL becomes the
//! administrator.
//!
//! ## Sessions are stateless
//!
//! The token is a JWT signed with `auth.jwt.secret` and naming the user by
//! `pid`. Nothing is stored server-side, which is why [`logout`] cannot revoke
//! anything: it acknowledges, and the client drops the token. Shortening
//! `auth.jwt.expiration` is the lever that bounds a stolen token's life.

use loco_rs::controller::extractor::auth::get_jwt_from_config;
use loco_rs::environment::Environment;
use loco_rs::prelude::*;
use subtle::ConstantTimeEq;

use crate::controllers::{bad_request, forbidden, unauthorized, validation_failed, Auth};
use crate::dtos::auth as dto;
use crate::dtos::users::User;
use crate::initializers::settings::Settings;
use crate::mailers::auth::AuthMailer;
use crate::models::_entities::users;
use crate::models::users::{ForgotParams, LoginParams, NewUser, RegisterParams, ResetParams};
use crate::models::validation::single_error;

/// `GET /api/auth/status` — public, feeds the first-run screen.
///
/// Reveals nothing beyond "somebody is already registered", which is exactly
/// what the client needs to choose between the login and the bootstrap screen.
#[debug_handler]
pub async fn status(State(ctx): State<AppContext>) -> Result<Response> {
    let total = users::Model::count_all(&ctx.db).await?;
    let in_production = ctx.environment == Environment::Production;

    format::json(dto::SystemStatus {
        has_users: total > 0,
        requires_bootstrap_token: total == 0 && in_production,
    })
}

/// `POST /api/auth/register`.
///
/// 201 on both successful outcomes, with different bodies: a token and the user
/// for the first administrator, only a message for whoever stays pending.
#[debug_handler]
pub async fn register(
    State(ctx): State<AppContext>,
    Json(params): Json<RegisterParams>,
) -> Result<Response> {
    Validate::validate(&params).map_err(validation_failed)?;

    let email = params.normalized_email();

    if users::Model::find_by_email(&ctx.db, &email)
        .await?
        .is_some()
    {
        // Uniqueness is not a field rule the derive can express — it needs the
        // table — so it is raised by hand, in the same shape as the rest.
        return Err(Error::Validation(
            single_error("email", "unique", "Este e-mail já está cadastrado.").into(),
        ));
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
        return Err(forbidden(if in_production {
            "Token de bootstrap inválido ou ausente para criar o administrador inicial."
        } else {
            "Não foi possível validar o bootstrap inicial."
        }));
    }

    let user = users::Model::create(
        &ctx.db,
        NewUser {
            full_name: params.full_name.clone(),
            email,
            password: params.password.clone().unwrap_or_default(),
            // The first user is born active and administrator; the rest wait
            // for approval.
            is_active: is_first_user,
            is_admin: is_first_user,
        },
    )
    .await?;

    if !user.is_active {
        return format::render().status(201).json(data!({
            "message": "Cadastro realizado. Aguarde aprovação de um administrador.",
        }));
    }

    let token = issue_token(&ctx, &user)?;

    format::render()
        .status(201)
        .json(dto::Session::new(token, &user))
}

/// `POST /api/auth/login`.
///
/// Three outcomes with three statuses: 400 for an invalid payload, **401** for
/// wrong credentials, and 401 for an account still awaiting approval.
#[debug_handler]
pub async fn login(
    State(ctx): State<AppContext>,
    Json(params): Json<LoginParams>,
) -> Result<Response> {
    Validate::validate(&params).map_err(validation_failed)?;

    let user = users::Model::authenticate(
        &ctx.db,
        &params.normalized_email(),
        params.password.as_deref().unwrap_or_default(),
    )
    .await?
    .ok_or_else(|| unauthorized("E-mail ou senha inválidos."))?;

    // After the password, never before: answering "account pending" to someone
    // who typed the wrong password would confirm the e-mail exists.
    if !user.is_active {
        return Err(unauthorized("Sua conta aguarda aprovação."));
    }

    let token = issue_token(&ctx, &user)?;

    format::json(dto::Session::new(token, &user))
}

/// `GET /api/auth/me` — the user behind the current token.
///
/// Answers 200 even for a deactivated account: a JWT cannot be revoked, so
/// deactivating a user takes effect on the next token instead. `is_active` rides
/// along in the body so the client can act on it.
#[debug_handler]
pub async fn me(State(_ctx): State<AppContext>, auth: Auth) -> Result<Response> {
    format::json(User::from(&auth.user))
}

/// `POST /api/auth/logout`.
///
/// Acknowledges; there is nothing on the server to delete. The client drops the
/// token, which is what ends the session.
#[debug_handler]
pub async fn logout(State(_ctx): State<AppContext>, _auth: Auth) -> Result<Response> {
    format::json(data!({ "message": "Sessão encerrada." }))
}

/// `POST /api/auth/forgot` — starts the recovery flow.
///
/// **Always answers success**, including for an address that is not registered.
/// Reporting "no such user" here would turn the endpoint into a directory of
/// who has an account.
///
/// A failure to send is logged and swallowed for the same reason: an SMTP error
/// surfacing only for real addresses would leak the same fact.
#[debug_handler]
pub async fn forgot(
    State(ctx): State<AppContext>,
    Json(params): Json<ForgotParams>,
) -> Result<Response> {
    Validate::validate(&params).map_err(validation_failed)?;

    if let Some(user) = users::Model::find_by_email(&ctx.db, &params.normalized_email()).await? {
        let user = user.start_password_reset(&ctx.db).await?;

        if let Err(err) = AuthMailer::forgot_password(&ctx, &user).await {
            tracing::error!(error = %err, "failed to enqueue the password reset e-mail");
        }
    }

    format::json(data!({
        "message": "Se o e-mail estiver cadastrado, você receberá as instruções de redefinição.",
    }))
}

/// `POST /api/auth/reset` — finishes the recovery flow.
///
/// An unknown or expired token is a flat 400 with no detail: saying which of the
/// two it was would let an attacker mine valid tokens.
#[debug_handler]
pub async fn reset(
    State(ctx): State<AppContext>,
    Json(params): Json<ResetParams>,
) -> Result<Response> {
    Validate::validate(&params).map_err(validation_failed)?;

    let token = params.token.clone().unwrap_or_default();
    let password = params.password.clone().unwrap_or_default();

    let user = users::Model::find_by_reset_token(&ctx.db, &token)
        .await?
        .ok_or_else(|| bad_request("Token de redefinição inválido ou expirado."))?;

    user.finish_password_reset(&ctx.db, &password).await?;

    format::json(data!({ "message": "Senha redefinida com sucesso." }))
}

/// Signs a JWT for `user` using the configured secret and lifetime.
fn issue_token(ctx: &AppContext, user: &users::Model) -> Result<String> {
    let jwt = get_jwt_from_config(ctx)?;

    user.generate_jwt(&jwt.secret, jwt.expiration)
}

/// Checks the bootstrap token of the first administrator.
///
/// With no token configured it only passes outside production. The comparison
/// is constant-time: a plain `==` returns at the first differing byte, and that
/// timing difference lets the token be recovered character by character.
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

    // Different lengths are already grounds for refusal, and comparing strings
    // of different lengths in constant time is not possible anyway.
    if configured.len() != candidate.len() {
        return false;
    }

    configured.as_bytes().ct_eq(candidate.as_bytes()).into()
}

/// Routes of `/api/auth`.
///
/// `register`, `login`, `forgot` and `reset` carry the `auth` limiter — 5 per
/// minute per IP. `status` keeps only the global one: it is the endpoint the
/// opening screen polls, and five per minute would lock out a whole office
/// behind one address.
pub fn routes(limiters: &crate::controllers::middlewares::limiters::Limiters) -> Routes {
    let auth_limit = limiters.auth();

    Routes::new()
        .prefix("/api/auth")
        .add("/status", get(status))
        .add("/register", post(register).layer(auth_limit.clone()))
        .add("/login", post(login).layer(auth_limit.clone()))
        .add("/forgot", post(forgot).layer(auth_limit.clone()))
        .add("/reset", post(reset).layer(auth_limit))
        .add("/me", get(me))
        .add("/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_configured_token_only_development_passes() {
        // In production, no configured token has to **block**: the alternative
        // is the first stranger to find the URL becoming the administrator.
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
        // Configuring the token is a deliberate act; ignoring it in development
        // would make staging lie about production.
        assert!(!bootstrap_token_is_valid("s3gr3d0", Some("errado"), false));
        assert!(bootstrap_token_is_valid("s3gr3d0", Some("s3gr3d0"), false));
    }
}
