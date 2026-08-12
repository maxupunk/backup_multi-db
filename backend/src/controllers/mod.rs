pub mod audit_logs;
pub mod auth;
pub mod backups;
pub mod connections;
pub mod docker;
pub mod middlewares;
pub mod public;
pub mod storage_destinations;
pub mod storages;
pub mod system;
pub mod transmit;
pub mod users;

use axum::body::Bytes;
use loco_rs::controller::extractor::auth::JWTWithUser;
use serde::de::DeserializeOwned;

use crate::models::_entities::users as users_entity;
use crate::views::errors::ApiError;

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
pub fn require_admin(user: &users_entity::Model, message: &str) -> Result<(), ApiError> {
    if user.is_admin {
        Ok(())
    } else {
        Err(ApiError::forbidden(message))
    }
}

/// Desserializa o corpo JSON de uma requisicao.
///
/// Nao usa o extractor `Json` do Axum de proposito. A rejeicao dele responde
/// `400 text/plain`, e as rotas desta API respondem `422` no shape do VineJS
/// para corpo invalido — um cliente que recebesse os dois formatos precisaria
/// tratar dois contratos de erro na mesma rota.
///
/// Corpo vazio equivale a `{}`: os `Params` tem todos os campos opcionais, e
/// quem reclama de campo faltando e' a validacao, com o nome do campo.
pub fn json_body<T: DeserializeOwned + Default>(bytes: &Bytes) -> Result<T, ApiError> {
    if bytes.is_empty() {
        return Ok(T::default());
    }

    serde_json::from_slice(bytes).map_err(|err| {
        // O detalhe do serde diz linha e coluna, o que ajuda quem esta'
        // integrando e nao revela nada do servidor.
        ApiError::bad_request("Corpo da requisição inválido").with_detail(err.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Eq, serde::Deserialize)]
    struct Params {
        #[serde(default)]
        email: Option<String>,
    }

    #[test]
    fn an_empty_body_becomes_the_default() {
        let parsed: Params = json_body(&Bytes::new()).expect("corpo vazio e' aceito");
        assert_eq!(parsed, Params::default());
    }

    #[test]
    fn reads_the_fields_it_knows() {
        let parsed: Params =
            json_body(&Bytes::from(r#"{"email":"a@b.com","extra":1}"#)).expect("parseia");

        assert_eq!(parsed.email.as_deref(), Some("a@b.com"));
    }

    #[test]
    fn malformed_json_is_a_400_with_the_reason() {
        let error = json_body::<Params>(&Bytes::from("{isso nao e json")).unwrap_err();

        assert_eq!(error.status(), axum::http::StatusCode::BAD_REQUEST);
    }
}
