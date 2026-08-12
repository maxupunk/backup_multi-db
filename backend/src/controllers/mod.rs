pub mod audit_logs;
pub mod auth;
pub mod backups;
pub mod connections;
pub mod docker;
pub mod events;
pub mod middlewares;
pub mod public;
pub mod storage_destinations;
pub mod storages;
pub mod system;
pub mod users;

use axum::http::StatusCode;
use loco_rs::controller::extractor::auth::JWTWithUser;
use loco_rs::controller::ErrorDetail;
use loco_rs::model::query::PaginationQuery;
use loco_rs::Error;

use crate::models::_entities::users as users_entity;

/// A request that carries a valid JWT, plus the user it names.
///
/// Alias over the framework extractor so handlers do not repeat the generic
/// argument. It proves **who** the caller is and never **what** they may do:
/// authorisation stays an explicit call to [`require_admin`] in the handler,
/// because `users` and `diagnostics` answer 403 with different messages and an
/// `AdminOnly` extractor would quietly unify them.
pub type Auth = JWTWithUser<users_entity::Model>;

/// Refuses a caller who is not an administrator, with the resource's own wording.
///
/// The message is a parameter because each resource has its own — "Apenas
/// administradores podem gerenciar usuários." in `users`, another in
/// `diagnostics`.
///
/// # Errors
/// Returns 403 when `user` is not an administrator.
pub fn require_admin(user: &users_entity::Model, message: &str) -> loco_rs::Result<()> {
    if user.is_admin {
        Ok(())
    } else {
        Err(forbidden(message))
    }
}

/// Application errors, in the framework's own shape.
///
/// Every failure this API can produce serialises as [`ErrorDetail`]:
///
/// ```jsonc
/// { "error": "forbidden", "description": "Apenas administradores…" }
/// { "errors": { "email": [ { "code": "email", "message": "…" } ] } }  // 400 de validação
/// ```
///
/// `error` is the machine-readable reason and `description` the text meant for
/// a person — the same split the framework already uses for the 401 of the JWT
/// extractor and for `Error::NotFound`. Inventing a second convention here
/// would mean a client parsing two shapes on the same route.
///
/// The constructors below exist because [`Error`] has no variant carrying both
/// a status and a message: `Error::CustomError` does, but repeating the pair at
/// every call site is how the reason slugs would drift apart.
mod reason {
    pub const BAD_REQUEST: &str = "bad_request";
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const FORBIDDEN: &str = "forbidden";
    pub const NOT_FOUND: &str = "not_found";
    pub const UNPROCESSABLE: &str = "unprocessable_entity";
    pub const CONFLICT: &str = "conflict";
}

/// 400 — the request itself is wrong.
pub fn bad_request(message: impl Into<String>) -> Error {
    detailed(StatusCode::BAD_REQUEST, reason::BAD_REQUEST, message)
}

/// 401 — the caller did not prove who they are, or the proof is stale.
pub fn unauthorized(message: impl Into<String>) -> Error {
    detailed(StatusCode::UNAUTHORIZED, reason::UNAUTHORIZED, message)
}

/// 403 — the caller is known and still may not do this.
pub fn forbidden(message: impl Into<String>) -> Error {
    detailed(StatusCode::FORBIDDEN, reason::FORBIDDEN, message)
}

/// 404 — with a message of its own.
///
/// Distinct from `Error::NotFound`, which answers with a fixed generic text.
/// Use this one when the resource can say *which* thing is missing.
pub fn not_found(message: impl Into<String>) -> Error {
    detailed(StatusCode::NOT_FOUND, reason::NOT_FOUND, message)
}

/// 422 — the request is well formed but the operation cannot proceed.
pub fn unprocessable(message: impl Into<String>) -> Error {
    detailed(
        StatusCode::UNPROCESSABLE_ENTITY,
        reason::UNPROCESSABLE,
        message,
    )
}

/// 409 — the operation collides with the current state of the resource.
pub fn conflict(message: impl Into<String>) -> Error {
    detailed(StatusCode::CONFLICT, reason::CONFLICT, message)
}

/// Appends the technical detail to an error built above.
///
/// The detail lands in `description`, after the message, because that is the
/// field a person reads. Only pass something the caller can act on — a `DbErr`
/// carries the SQL and its bound values and belongs in the log.
pub fn with_detail(error: Error, detail: impl std::fmt::Display) -> Error {
    match error {
        Error::CustomError(
            status,
            ErrorDetail {
                error, description, ..
            },
        ) => {
            let description =
                description.map_or_else(|| detail.to_string(), |text| format!("{text}: {detail}"));

            Error::CustomError(
                status,
                ErrorDetail {
                    error,
                    description: Some(description),
                    errors: None,
                },
            )
        }
        other => other,
    }
}

fn detailed(status: StatusCode, reason: &str, message: impl Into<String>) -> Error {
    Error::CustomError(status, ErrorDetail::new(reason, message.into()))
}

/// Reads `?page=` and `?page_size=` into the framework's pagination request.
///
/// [`PaginationQuery`] deserialises itself, but only when it is the whole query
/// struct: `#[serde(flatten)]` inside a listing that also carries `?search=` or
/// `?status=` goes through `serde_urlencoded`, which does not buffer a
/// flattened map. Every listing here has such filters, so the two fields are
/// read as text and parsed once, in one place.
///
/// A value that cannot be parsed falls back to the default instead of becoming
/// a 400: `?page=` empty is what a form sends when the field was never touched,
/// and answering an error there would break the screen over nothing.
///
/// `max_page_size` is not decoration. Without a ceiling, `?page_size=1000000`
/// is a cheap way to make the process load the whole table into memory.
pub fn page_request(
    page: Option<&str>,
    page_size: Option<&str>,
    default_page_size: u64,
    max_page_size: u64,
) -> PaginationQuery {
    let page = page.and_then(|value| value.trim().parse::<u64>().ok());
    let page_size = page_size.and_then(|value| value.trim().parse::<u64>().ok());

    PaginationQuery {
        page: page.unwrap_or(1).max(1),
        page_size: page_size
            .unwrap_or(default_page_size)
            .clamp(1, max_page_size),
    }
}

/// Ceiling shared by the listings that do not set one of their own.
pub const MAX_PAGE_SIZE: u64 = 100;

/// Turns a `validator` failure into the framework's validation error, without
/// the submitted value.
///
/// **This is why the auto-validating extractors are not used here.** The
/// `validator` derive records what was submitted in `params.value`, and
/// `JsonValidateWithMessage` serialises that map as-is — so a password of the
/// wrong length comes back inside the 400, and from there into access logs,
/// proxies and error trackers. The caller already knows what it sent; nobody
/// downstream needs to.
///
/// The rest of `params` stays: `min`/`max` say what the rule expects, and
/// `choices` is what the interface uses to rebuild a select after a refusal.
pub fn validation_failed(errors: validator::ValidationErrors) -> Error {
    let mut model_errors: loco_rs::validation::ModelValidationErrors = errors.into();

    for failures in model_errors.errors.values_mut() {
        for failure in failures.iter_mut() {
            failure.params.remove("value");
        }
    }

    Error::Validation(model_errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    async fn body_of(error: Error) -> serde_json::Value {
        let bytes = axum::body::to_bytes(error.into_response().into_body(), usize::MAX)
            .await
            .expect("lê o corpo");

        serde_json::from_slice(&bytes).expect("o corpo é JSON")
    }

    #[test]
    fn each_constructor_carries_its_status() {
        for (error, expected) in [
            (bad_request("x"), StatusCode::BAD_REQUEST),
            (unauthorized("x"), StatusCode::UNAUTHORIZED),
            (forbidden("x"), StatusCode::FORBIDDEN),
            (not_found("x"), StatusCode::NOT_FOUND),
            (unprocessable("x"), StatusCode::UNPROCESSABLE_ENTITY),
            (conflict("x"), StatusCode::CONFLICT),
        ] {
            assert_eq!(error.into_response().status(), expected);
        }
    }

    #[tokio::test]
    async fn the_body_is_always_reason_plus_description() {
        assert_eq!(
            body_of(forbidden("Apenas administradores.")).await,
            serde_json::json!({
                "error": "forbidden",
                "description": "Apenas administradores.",
            })
        );
    }

    #[tokio::test]
    async fn a_validation_error_never_echoes_what_was_submitted() {
        // Sem isto, uma senha do tamanho errado volta dentro do 400 e daí para
        // o log de acesso, o proxy reverso e o rastreador de erros.
        let mut errors = validator::ValidationErrors::new();
        let mut failure = validator::ValidationError::new("length");
        failure.add_param("value".into(), &"senha-secreta");
        failure.add_param("min".into(), &8);
        errors.add("password", failure);

        let body = body_of(validation_failed(errors)).await;
        let reported = &body["errors"]["password"][0];

        assert!(!body.to_string().contains("senha-secreta"));
        assert!(reported["params"].get("value").is_none());
        // O que a regra esperava continua no corpo — é o que a tela exibe.
        assert_eq!(reported["params"]["min"], 8);
    }

    #[test]
    fn a_bad_page_query_falls_back_instead_of_failing() {
        let parsed = page_request(Some("abc"), Some(""), 10, MAX_PAGE_SIZE);

        assert_eq!(parsed.page, 1);
        assert_eq!(parsed.page_size, 10);
    }

    #[test]
    fn page_zero_becomes_the_first_page() {
        // `?page=0` viraria offset negativo no `Paginator`.
        assert_eq!(page_request(Some("0"), None, 10, MAX_PAGE_SIZE).page, 1);
    }

    #[test]
    fn the_page_size_is_clamped_to_the_ceiling() {
        let parsed = page_request(Some("1"), Some("1000000"), 50, MAX_PAGE_SIZE);
        assert_eq!(parsed.page_size, MAX_PAGE_SIZE);

        // Zero dividiria por zero no cálculo de total de páginas.
        assert_eq!(
            page_request(None, Some("0"), 50, MAX_PAGE_SIZE).page_size,
            1
        );
    }

    #[tokio::test]
    async fn the_detail_follows_the_message() {
        // Quem lê é uma pessoa, na tela: a mensagem primeiro, o detalhe
        // técnico depois — não um campo separado que a interface ignoraria.
        assert_eq!(
            body_of(with_detail(
                unprocessable("Falha ao conectar"),
                "ECONNREFUSED"
            ))
            .await["description"],
            "Falha ao conectar: ECONNREFUSED"
        );
    }
}
