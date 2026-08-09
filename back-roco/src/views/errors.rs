//! Formato de erro da API (tarefa 3.6 do roadmap, decisao D9 **corrigida**).
//!
//! A decisao D9 original assumia um shape unico de erro. Os golden files da
//! Fase 2 mostraram que isso esta' errado: a API tem **duas** familias, e o
//! back-roco precisa reproduzir as duas ou quebra clientes.
//!
//! ## Familia 1 — do framework
//!
//! Vem do VineJS, do `E_INVALID_CREDENTIALS` e do limiter. Nao tem `success`
//! nem `message` no topo:
//!
//! ```jsonc
//! // 422 — validacao
//! { "errors": [ { "field": "email", "message": "…", "rule": "database.unique" } ] }
//! // 400 — credencial invalida
//! { "errors": [ { "message": "Invalid user credentials" } ] }
//! // 429 — rate limit
//! { "errors": [ { "message": "Too many requests", "retryAfter": 60 } ] }
//! ```
//!
//! Repare que o **item nao tem forma fixa**. Modelar `errors` como uma lista
//! de um tipo unico com todos os campos opcionais produziria `"field": null`
//! onde o Adonis simplesmente omite a chave — e o matcher de shape da suite de
//! contrato acusaria a diferenca. Por isso cada variante serializa so' o que
//! lhe cabe.
//!
//! ## Familia 2 — escrita a mao nos controllers
//!
//! ```jsonc
//! { "success": false, "message": "Apenas administradores podem gerenciar usuarios." }
//! ```
//!
//! Usada em 401 de conta pendente, 403, 404 de recurso e 400 de regra de
//! negocio. Alguns pontos acrescentam `error` com o detalhe tecnico.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

/// Um item de `errors`. Cada variante serializa exatamente as chaves que o
/// Adonis emite naquele caso — nunca um campo nulo a mais.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ErrorItem {
    /// Erro de validacao do VineJS.
    Validation {
        message: String,
        field: String,
        rule: String,
    },
    /// Rate limit estourado.
    RateLimited {
        message: String,
        #[serde(rename = "retryAfter")]
        retry_after: u64,
    },
    /// Erro sem campo associado, como credencial invalida.
    Simple { message: String },
}

impl ErrorItem {
    pub fn validation(
        field: impl Into<String>,
        rule: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::Validation {
            message: message.into(),
            field: field.into(),
            rule: rule.into(),
        }
    }

    pub fn simple(message: impl Into<String>) -> Self {
        Self::Simple {
            message: message.into(),
        }
    }

    pub fn rate_limited(retry_after: u64) -> Self {
        Self::RateLimited {
            message: "Too many requests".to_string(),
            retry_after,
        }
    }
}

/// Corpo da familia do framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkError {
    pub errors: Vec<ErrorItem>,
}

/// Corpo da familia escrita a mao nos controllers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerError {
    pub success: bool,
    pub message: String,
    /// Detalhe tecnico. Omitido quando ausente — o Adonis nao emite `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Erro pronto para virar resposta.
#[derive(Debug, Clone)]
pub enum ApiError {
    /// 422 com a lista de campos invalidos.
    Validation(Vec<ErrorItem>),
    /// 400 do `E_INVALID_CREDENTIALS`.
    InvalidCredentials,
    /// 429, com o `Retry-After` e os cabecalhos de limite.
    RateLimited {
        retry_after: u64,
        limit: u32,
        remaining: u32,
        reset: String,
    },
    /// Familia dos controllers, com o status que o handler escolher.
    Controller {
        status: StatusCode,
        message: String,
        error: Option<String>,
    },
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::Controller {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            error: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Controller {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            error: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Controller {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            error: None,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::Controller {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            error: None,
        }
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::Controller {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: message.into(),
            error: None,
        }
    }

    /// Acrescenta o detalhe tecnico do campo `error`.
    pub fn with_detail(self, detail: impl Into<String>) -> Self {
        match self {
            Self::Controller {
                status, message, ..
            } => Self::Controller {
                status,
                message,
                error: Some(detail.into()),
            },
            other => other,
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::InvalidCredentials => StatusCode::BAD_REQUEST,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Controller { status, .. } => *status,
        }
    }

    /// Traduz os erros do crate `validator` para o shape do VineJS.
    ///
    /// O `rule` do VineJS e' o nome da regra que falhou (`required`,
    /// `minLength`, `email`, `database.unique`). O `validator` chama isso de
    /// `code`, com nomes proprios — o mapa abaixo faz a ponte para os que a
    /// suite de contrato fixou.
    pub fn from_validation_errors(errors: &validator::ValidationErrors) -> Self {
        let mut items: Vec<ErrorItem> = Vec::new();

        for (field, kind) in errors.errors() {
            if let validator::ValidationErrorsKind::Field(field_errors) = kind {
                for error in field_errors {
                    let rule = map_rule(&error.code);
                    let message = error
                        .message
                        .as_ref()
                        .map_or_else(|| default_message(field, &rule), ToString::to_string);

                    items.push(ErrorItem::validation(field.to_string(), rule, message));
                }
            }
        }

        // Ordena por campo: a ordem de iteracao de um mapa nao e' estavel, e a
        // suite de contrato compara a lista de campos. Sem isto o mesmo payload
        // invalido produziria respostas em ordens diferentes.
        items.sort_by(|a, b| field_of(a).cmp(field_of(b)));

        Self::Validation(items)
    }
}

fn field_of(item: &ErrorItem) -> &str {
    match item {
        ErrorItem::Validation { field, .. } => field,
        _ => "",
    }
}

/// Ponte entre os codigos do `validator` e os nomes de regra do VineJS.
fn map_rule(code: &str) -> String {
    match code {
        "required" | "required_nested" => "required",
        "length" => "minLength",
        "range" => "range",
        "email" => "email",
        "url" => "url",
        _ => code,
    }
    .to_string()
}

fn default_message(field: &str, rule: &str) -> String {
    // Mesmo texto que o VineJS gera quando nenhuma mensagem customizada foi
    // definida.
    match rule {
        "required" => format!("The {field} field must be defined"),
        "email" => format!("The {field} field must be a valid email address"),
        "minLength" => format!("The {field} field has an invalid length"),
        _ => format!("The {field} field is invalid"),
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        let (body, extra_headers) = match self {
            Self::Validation(items) => (
                serde_json::to_value(FrameworkError { errors: items }),
                Vec::new(),
            ),
            Self::InvalidCredentials => (
                serde_json::to_value(FrameworkError {
                    errors: vec![ErrorItem::simple("Invalid user credentials")],
                }),
                Vec::new(),
            ),
            Self::RateLimited {
                retry_after,
                limit,
                remaining,
                reset,
            } => (
                serde_json::to_value(FrameworkError {
                    errors: vec![ErrorItem::rate_limited(retry_after)],
                }),
                vec![
                    ("retry-after".to_string(), retry_after.to_string()),
                    ("x-ratelimit-limit".to_string(), limit.to_string()),
                    ("x-ratelimit-remaining".to_string(), remaining.to_string()),
                    ("x-ratelimit-reset".to_string(), reset),
                ],
            ),
            Self::Controller { message, error, .. } => (
                serde_json::to_value(ControllerError {
                    success: false,
                    message,
                    error,
                }),
                Vec::new(),
            ),
        };

        let payload = body
            .unwrap_or_else(|_| serde_json::json!({ "success": false, "message": "Erro interno" }));

        let mut response = (status, axum::Json(payload)).into_response();

        // `application/json; charset=utf-8`, igual ao Adonis. O `charset` nao
        // e' comparado pela suite (so' o mime), mas emitir o mesmo evita
        // diferenca gratuita para quem inspecionar a resposta a mao.
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );

        for (name, value) in extra_headers {
            if let (Ok(name), Ok(value)) = (
                name.parse::<axum::http::HeaderName>(),
                HeaderValue::from_str(&value),
            ) {
                response.headers_mut().insert(name, value);
            }
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_of(error: ApiError) -> serde_json::Value {
        match error {
            ApiError::Validation(items) => serde_json::to_value(FrameworkError { errors: items }),
            ApiError::InvalidCredentials => serde_json::to_value(FrameworkError {
                errors: vec![ErrorItem::simple("Invalid user credentials")],
            }),
            ApiError::RateLimited { retry_after, .. } => serde_json::to_value(FrameworkError {
                errors: vec![ErrorItem::rate_limited(retry_after)],
            }),
            ApiError::Controller { message, error, .. } => serde_json::to_value(ControllerError {
                success: false,
                message,
                error,
            }),
        }
        .unwrap()
    }

    #[test]
    fn validation_item_carries_field_and_rule() {
        let body = body_of(ApiError::Validation(vec![ErrorItem::validation(
            "email",
            "database.unique",
            "The email has already been taken",
        )]));

        assert_eq!(
            body,
            serde_json::json!({
                "errors": [{
                    "message": "The email has already been taken",
                    "field": "email",
                    "rule": "database.unique"
                }]
            })
        );
    }

    #[test]
    fn simple_item_omits_field_and_rule() {
        // O golden `auth/login-invalid-credentials` tem exatamente
        // `{"errors":[{"message":"Invalid user credentials"}]}`. Emitir
        // `"field": null` aqui seria uma chave a mais, e o matcher da suite de
        // contrato reprova chave a mais.
        assert_eq!(
            body_of(ApiError::InvalidCredentials),
            serde_json::json!({ "errors": [{ "message": "Invalid user credentials" }] })
        );
    }

    #[test]
    fn rate_limited_item_uses_camel_case_retry_after() {
        assert_eq!(
            body_of(ApiError::RateLimited {
                retry_after: 60,
                limit: 5,
                remaining: 0,
                reset: "2026-08-09T12:00:00.000Z".to_string(),
            }),
            serde_json::json!({
                "errors": [{ "message": "Too many requests", "retryAfter": 60 }]
            })
        );
    }

    #[test]
    fn controller_error_omits_the_detail_when_absent() {
        // O Adonis nao emite `"error": null`; emitir seria chave a mais.
        assert_eq!(
            body_of(ApiError::not_found("Conexão não encontrada")),
            serde_json::json!({ "success": false, "message": "Conexão não encontrada" })
        );
    }

    #[test]
    fn controller_error_includes_the_detail_when_present() {
        assert_eq!(
            body_of(ApiError::unprocessable("Falha ao conectar").with_detail("ECONNREFUSED")),
            serde_json::json!({
                "success": false,
                "message": "Falha ao conectar",
                "error": "ECONNREFUSED"
            })
        );
    }

    #[test]
    fn statuses_match_the_recorded_contract() {
        // 400 para credencial invalida, e nao 401 — foi um dos achados da
        // Fase 2, e e' facil "corrigir" isso por engano no porte.
        assert_eq!(
            ApiError::InvalidCredentials.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::Validation(vec![]).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(ApiError::forbidden("x").status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn the_two_families_never_share_keys() {
        // A diferenca entre as familias e' o que a suite de contrato compara.
        // Se um dia alguem "unificar" os dois shapes, este teste cai.
        let framework = body_of(ApiError::InvalidCredentials);
        let controller = body_of(ApiError::forbidden("x"));

        assert!(framework.get("success").is_none());
        assert!(framework.get("message").is_none());
        assert!(controller.get("errors").is_none());
    }

    #[test]
    fn translates_validator_errors_sorted_by_field() {
        let mut errors = validator::ValidationErrors::new();
        errors.add("password", validator::ValidationError::new("required"));
        errors.add("email", validator::ValidationError::new("email"));

        let ApiError::Validation(items) = ApiError::from_validation_errors(&errors) else {
            panic!("esperava a variante de validacao")
        };

        // Ordenado: a iteracao de um mapa nao e' estavel, e a suite compara a
        // lista de campos.
        assert_eq!(
            items.iter().map(field_of).collect::<Vec<_>>(),
            vec!["email", "password"]
        );
    }

    #[test]
    fn maps_validator_codes_to_vinejs_rules() {
        assert_eq!(map_rule("required"), "required");
        assert_eq!(map_rule("length"), "minLength");
        assert_eq!(map_rule("email"), "email");
        // Codigo desconhecido passa direto em vez de virar um nome inventado.
        assert_eq!(map_rule("custom_rule"), "custom_rule");
    }
}
