//! Equivalente ao `force_json_response` do Adonis (tarefa 3.7 do roadmap).
//!
//! O middleware do Adonis reescreve o `Accept` de toda requisicao de `/api`
//! para `application/json`, e o efeito util nao esta' no caminho feliz — esta'
//! no erro. Sem ele, o handler de excecao roda com `debug = !inProduction` e
//! devolve a pagina HTML do Youch para um cliente que esperava JSON.
//!
//! O teste `middlewares globais > erros tambem saem em JSON` da suite de
//! contrato e' exatamente esse caso.
//!
//! Em Axum nao ha' handler de excecao global reescrevendo o corpo: quem decide
//! o formato e' o `IntoResponse` de [`crate::views::errors::ApiError`], que ja'
//! sempre emite JSON. O que resta e' garantir que **nada** responda HTML sob
//! `/api` — inclusive o 404 do router e uma resposta de terceiro que venha a
//! ser montada por outra camada.

use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

const API_PREFIX: &str = "/api";
const JSON: HeaderValue = HeaderValue::from_static("application/json");
const JSON_UTF8: HeaderValue = HeaderValue::from_static("application/json; charset=utf-8");

/// Forca `Accept: application/json` na entrada e JSON na saida, sob `/api`.
pub async fn force_json(mut request: Request, next: Next) -> Response {
    let is_api = request.uri().path().starts_with(API_PREFIX);

    if is_api {
        request.headers_mut().insert(header::ACCEPT, JSON);
    }

    let response = next.run(request).await;

    if !is_api {
        return response;
    }

    ensure_json(response)
}

/// Converte uma resposta nao-JSON de `/api` num corpo JSON equivalente.
fn ensure_json(response: Response) -> Response {
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();

    if content_type.starts_with("application/json") {
        return response;
    }

    let status = response.status();

    // Corpo vazio com content-type ausente e' o 204/304 normal — nada a fazer.
    if content_type.is_empty() && !status.is_client_error() && !status.is_server_error() {
        return response;
    }

    // Um erro que chegou aqui sem JSON so' pode vir de fora dos nossos
    // handlers: 404 do router, 405 de metodo, ou um layer de terceiro. Todos
    // viram a familia de erro dos controllers, que e' o que o cliente da API
    // espera.
    let message = match status {
        StatusCode::NOT_FOUND => "Recurso não encontrado",
        StatusCode::METHOD_NOT_ALLOWED => "Método não permitido",
        _ => "Erro ao processar a requisição",
    };

    let body = serde_json::json!({ "success": false, "message": message });
    let mut rebuilt = (status, axum::Json(body)).into_response();
    rebuilt
        .headers_mut()
        .insert(header::CONTENT_TYPE, JSON_UTF8);
    rebuilt
}

/// Constroi uma resposta JSON vazia com o status dado. Util em testes.
#[cfg(test)]
fn text_response(status: StatusCode, content_type: &'static str, body: &'static str) -> Response {
    let mut response = Response::new(axum::body::Body::from(body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};

    async fn body_string(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn leaves_json_responses_untouched() {
        let original = text_response(
            StatusCode::OK,
            "application/json; charset=utf-8",
            r#"{"ok":true}"#,
        );

        let result = ensure_json(original);
        assert_eq!(body_string(result).await, r#"{"ok":true}"#);
    }

    #[tokio::test]
    async fn converts_an_html_error_into_json() {
        // Este e' o caso que o middleware existe para impedir: a pagina de
        // debug do framework chegando a um cliente que espera JSON.
        let original = text_response(
            StatusCode::NOT_FOUND,
            "text/html; charset=utf-8",
            "<html><body>Not Found</body></html>",
        );

        let result = ensure_json(original);
        assert_eq!(
            result.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json; charset=utf-8"
        );

        let body = body_string(result).await;
        assert!(
            body.contains(r#""success":false"#),
            "corpo inesperado: {body}"
        );
        assert!(!body.contains("<html"));
    }

    #[tokio::test]
    async fn keeps_the_original_status() {
        // Trocar o status pelo do JSON seria pior que o HTML: o cliente
        // perderia a unica informacao que tinha.
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::METHOD_NOT_ALLOWED,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let result = ensure_json(text_response(status, "text/plain", "erro"));
            assert_eq!(result.status(), status);
        }
    }

    #[tokio::test]
    async fn leaves_successful_empty_responses_alone() {
        let mut original = Response::new(Body::empty());
        *original.status_mut() = StatusCode::NO_CONTENT;

        let result = ensure_json(original);
        assert_eq!(result.status(), StatusCode::NO_CONTENT);
        assert!(body_string(result).await.is_empty());
    }

    #[tokio::test]
    async fn uses_a_distinct_message_per_status() {
        let not_found = body_string(ensure_json(text_response(
            StatusCode::NOT_FOUND,
            "text/plain",
            "x",
        )))
        .await;
        let method = body_string(ensure_json(text_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain",
            "x",
        )))
        .await;

        assert_ne!(not_found, method);
    }
}
