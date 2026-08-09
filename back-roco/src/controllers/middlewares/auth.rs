//! Extractor de sessao para as rotas protegidas (tarefa 3.4 do roadmap).
//!
//! O Adonis liga `middleware.auth()` na rota e o handler le' `auth.user!`. Aqui
//! a mesma coisa e' um **extractor**: quem declara [`Authenticated`] na
//! assinatura so' roda se a sessao existir, e o compilador impede que alguem
//! esqueca a checagem — nao ha' o equivalente do `!` do TypeScript.
//!
//! ## Autenticacao nao e' autorizacao
//!
//! [`Authenticated`] prova **quem** e' o usuario, nunca **o que** ele pode. Os
//! controllers de `users` e de `diagnostics` decidem `is_admin` explicitamente,
//! porque o Adonis responde 403 com mensagens diferentes em cada um. Um
//! extractor `AdminOnly` esconderia essa decisao e uniformizaria as duas
//! mensagens sem ninguem perceber.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use loco_rs::prelude::*;

use crate::models::_entities::{auth_access_tokens, users};
use crate::views::errors::ApiError;

/// Prefixo do cabecalho `Authorization`, comparado sem diferenciar caixa.
const BEARER: &str = "bearer ";

/// Uma sessao valida: o usuario e o token que a sustenta.
#[derive(Debug, Clone)]
pub struct Authenticated {
    pub user: users::Model,
    pub token: auth_access_tokens::Model,
}

impl Authenticated {
    /// Exige perfil de administrador, com a mensagem que o recurso usa.
    ///
    /// A mensagem entra por parametro porque cada recurso tem a sua — "Apenas
    /// administradores podem gerenciar usuarios." em `users`, e outra em
    /// `diagnostics`. Fixar uma unica aqui mudaria o corpo de uma das duas.
    pub fn require_admin(&self, message: &str) -> std::result::Result<(), ApiError> {
        if self.user.is_admin {
            Ok(())
        } else {
            Err(ApiError::forbidden(message))
        }
    }
}

/// Le' o token do cabecalho `Authorization: Bearer …`.
fn bearer_token(parts: &Parts) -> Option<&str> {
    let value = parts
        .headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .trim();

    // `Bearer`, `bearer` e `BEARER` sao equivalentes pela RFC 7235; um cliente
    // que mande a variante "errada" nao pode levar 401.
    if value.len() < BEARER.len() || !value[..BEARER.len()].eq_ignore_ascii_case(BEARER) {
        return None;
    }

    let token = value[BEARER.len()..].trim();
    (!token.is_empty()).then_some(token)
}

impl<S> FromRequestParts<S> for Authenticated
where
    AppContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let ctx = AppContext::from_ref(state);

        let Some(presented) = bearer_token(parts) else {
            return Err(ApiError::UnauthorizedAccess);
        };

        let session = auth_access_tokens::Model::verify(&ctx.db, presented)
            .await
            .map_err(|err| {
                // Um erro de banco nao pode virar 401: o cliente concluiria que
                // a sessao caiu e mandaria o usuario logar de novo, escondendo
                // uma indisponibilidade real.
                tracing::error!(error = %err, "failed to verify the access token");
                ApiError::Controller {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "Erro ao validar a sessão".to_string(),
                    error: None,
                }
            })?;

        session
            .map(|session| Self {
                user: session.user,
                token: session.token,
            })
            .ok_or(ApiError::UnauthorizedAccess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn parts_with(header_value: Option<&str>) -> Parts {
        let mut builder = Request::builder().uri("/api/auth/me");
        if let Some(value) = header_value {
            builder = builder.header(header::AUTHORIZATION, value);
        }
        builder
            .body(())
            .expect("requisicao de teste")
            .into_parts()
            .0
    }

    #[test]
    fn reads_the_bearer_token() {
        let parts = parts_with(Some("Bearer oat_MQ.YWJj"));
        assert_eq!(bearer_token(&parts), Some("oat_MQ.YWJj"));
    }

    #[test]
    fn accepts_any_casing_of_the_scheme() {
        for value in [
            "bearer oat_MQ.YWJj",
            "BEARER oat_MQ.YWJj",
            "BeArEr oat_MQ.YWJj",
        ] {
            assert_eq!(bearer_token(&parts_with(Some(value))), Some("oat_MQ.YWJj"));
        }
    }

    #[test]
    fn ignores_anything_that_is_not_a_bearer() {
        for value in [
            "",
            "Basic dXNlcjpwYXNz",
            "Bearer",
            "Bearer    ",
            "oat_MQ.YWJj",
        ] {
            assert_eq!(
                bearer_token(&parts_with(Some(value))),
                None,
                "aceitou {value:?}"
            );
        }
    }

    #[test]
    fn reports_no_token_when_the_header_is_absent() {
        assert_eq!(bearer_token(&parts_with(None)), None);
    }

    #[test]
    fn admin_authorization_is_explicit() {
        let admin = Authenticated {
            user: user_model(true),
            token: token_model(),
        };
        let member = Authenticated {
            user: user_model(false),
            token: token_model(),
        };

        assert!(admin.require_admin("mensagem").is_ok());
        let error = member.require_admin("mensagem").unwrap_err();
        assert_eq!(error.status(), StatusCode::FORBIDDEN);
    }

    fn user_model(is_admin: bool) -> users::Model {
        users::Model {
            id: 1,
            full_name: None,
            email: "admin@contract.test".to_string(),
            password: String::new(),
            is_active: true,
            is_admin,
            created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
            updated_at: None,
        }
    }

    fn token_model() -> auth_access_tokens::Model {
        auth_access_tokens::Model {
            id: 1,
            tokenable_id: 1,
            r#type: "auth_token".to_string(),
            name: None,
            hash: String::new(),
            abilities: "[\"*\"]".to_string(),
            created_at: None,
            updated_at: None,
            last_used_at: None,
            expires_at: None,
        }
    }
}
