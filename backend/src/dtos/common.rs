//! As formas que **toda** resposta da API tem em comum.
//!
//! Não são structs usadas para serializar: quem serializa é o `Pager` do Loco e
//! o `ErrorDetail` do `loco_rs::controller`. O que existe aqui é a **descrição**
//! desses formatos, num lugar onde o `ts-rs` consegue gerá-la em TypeScript.
//!
//! ## Por que declarar de novo o que o framework já tem
//!
//! Os tipos do Loco não derivam `TS`, e não dá para acrescentar um derive a um
//! tipo de outro crate. A alternativa seria o frontend redigitar os campos à
//! mão — que é o que ele fazia, e o que a Fase 8 existe para acabar. O teste
//! [`tests`] compara estas structs com o que o framework realmente emite, então
//! uma divergência quebra o build em vez de virar um `undefined` em produção.
//!
//! ## Como regerar
//!
//! ```sh
//! cargo test --lib dtos          # reescreve frontend/src/bindings/
//! git diff --exit-code ../frontend/src/bindings   # o CI reprova se mudou
//! ```

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Uma página de resultados — o corpo de toda listagem.
///
/// As chaves de `pagination` estão em snake_case porque é assim que o
/// `PagerMeta` do Loco serializa. Renomear aqui criaria dois vocabulários para
/// a mesma coisa, e o binding deixaria de descrever a resposta real.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct Paginated<T: TS> {
    pub results: Vec<T>,
    pub pagination: PageInfo,
}

/// O bloco `pagination` de uma página.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct PageInfo {
    #[ts(type = "number")]
    pub page: u64,
    #[ts(type = "number")]
    pub page_size: u64,
    #[ts(type = "number")]
    pub total_pages: u64,
    #[ts(type = "number")]
    pub total_items: u64,
}

/// O corpo de **qualquer** falha da API.
///
/// Os três campos são opcionais e mutuamente informativos, não alternativos:
/// um erro comum traz `error` + `description`; uma falha de validação traz
/// `errors`, com a lista de problemas de cada campo sob o nome dele.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct ApiErrorBody {
    /// Razão legível por máquina: `not_found`, `forbidden`, `unauthorized`…
    pub error: Option<String>,
    /// Texto destinado a uma pessoa.
    pub description: Option<String>,
    /// Presente só em falha de validação.
    pub errors: Option<std::collections::BTreeMap<String, Vec<FieldError>>>,
}

/// Um problema encontrado num campo.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct FieldError {
    /// Nome da regra que falhou: `required`, `length`, `email`, `enum`…
    pub code: String,
    pub message: Option<String>,
    /// O que a regra esperava — `min`, `max`, `choices`. Omitido quando vazio.
    ///
    /// **Nunca** o valor enviado: ver `controllers::validation_failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub params: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Resposta de uma rota que não tem recurso para devolver.
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct MessageResponse {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use loco_rs::controller::views::pagination::{Pager, PagerMeta};

    fn keys(value: &serde_json::Value) -> Vec<String> {
        value.as_object().expect("objeto").keys().cloned().collect()
    }

    #[test]
    fn the_page_binding_matches_what_the_framework_emits() {
        // Se o `Pager` do Loco mudar de forma, é aqui que se descobre — e não
        // num `undefined` na tela.
        let emitted = serde_json::to_value(Pager::new(
            vec![1, 2],
            PagerMeta {
                page: 1,
                page_size: 20,
                total_pages: 3,
                total_items: 45,
            },
        ))
        .expect("serializa");

        let described = serde_json::to_value(Paginated {
            results: vec![1, 2],
            pagination: PageInfo {
                page: 1,
                page_size: 20,
                total_pages: 3,
                total_items: 45,
            },
        })
        .expect("serializa");

        assert_eq!(emitted, described);
    }

    #[test]
    fn the_error_binding_matches_what_the_framework_emits() {
        let emitted = serde_json::to_value(loco_rs::controller::ErrorDetail::new(
            "not_found",
            "Conexão não encontrada",
        ))
        .expect("serializa");

        let mut fields = keys(&emitted);
        fields.sort();
        assert_eq!(fields, ["description", "error"]);
    }

    #[test]
    fn the_validation_binding_matches_what_the_framework_emits() {
        // O que vai no campo `errors` de um 400 é o mapa do
        // `ModelValidationErrors`; é a forma dele que o binding descreve.
        let errors: loco_rs::validation::ModelValidationErrors =
            crate::models::validation::single_error(
                "email",
                "unique",
                "Este e-mail já está cadastrado.",
            )
            .into();
        let emitted = serde_json::to_value(&errors.errors).expect("serializa");

        let described = serde_json::to_value(ApiErrorBody {
            error: None,
            description: None,
            errors: Some(std::collections::BTreeMap::from([(
                "email".to_string(),
                vec![FieldError {
                    code: "unique".to_string(),
                    message: Some("Este e-mail já está cadastrado.".to_string()),
                    params: None,
                }],
            )])),
        })
        .expect("serializa");

        assert_eq!(emitted, described["errors"]);
    }
}
