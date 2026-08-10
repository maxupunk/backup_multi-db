pub mod audit_logs;
pub mod auth;
pub mod backups;
pub mod connections;
pub mod middlewares;
pub mod storage_destinations;
pub mod storages;
pub mod system;
pub mod users;

use axum::body::Bytes;
use serde::de::DeserializeOwned;

use crate::views::errors::ApiError;

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
