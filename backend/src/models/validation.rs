//! Regras de validação que o `derive` do `validator` não alcança.
//!
//! A maior parte dos `Params` valida por atributo — `#[validate(required)]`,
//! `#[validate(length(...))]`, `#[validate(email)]` — e não passa por aqui.
//! Sobram dois casos que o `derive` não expressa, porque a regra não é do
//! campo:
//!
//! - **Conteúdo dependente de outro campo.** A `config` de um destino de
//!   storage é um `serde_json::Map`, e quais chaves são obrigatórias depende do
//!   `provider`. Não há atributo para "obrigatório quando o irmão vale `s3`".
//! - **Regra que precisa do banco.** Unicidade de e-mail só se responde
//!   consultando a tabela, então o erro é montado no controller, no mesmo
//!   formato dos demais — ver [`single_error`].
//!
//! O que sai daqui é `validator::ValidationErrors`, exatamente como o que o
//! `derive` produz. Quem traduz para HTTP é o `Error::Validation` do Loco: o
//! model não conhece status code.

use validator::{ValidationError, ValidationErrors};

/// Monta um erro com o código da regra e a mensagem exibida ao usuário.
pub fn rule(code: &'static str, message: impl Into<String>) -> ValidationError {
    let mut error = ValidationError::new(code);
    error.message = Some(message.into().into());
    error
}

/// Um único erro de campo, pronto para virar `Error::Validation`.
///
/// Usado pelas regras que precisam do banco: o controller descobre a colisão e
/// devolve o mesmo shape que o `derive` devolveria, em vez de um 409 avulso
/// que a tela teria de tratar por fora do fluxo de validação.
pub fn single_error(
    field: &'static str,
    code: &'static str,
    message: impl Into<String>,
) -> ValidationErrors {
    let mut errors = ValidationErrors::new();
    errors.add(field, rule(code, message));
    errors
}

/// Erro de valor fora de um conjunto fechado, com as opções aceitas.
///
/// A interface usa `params.choices` para remontar o select depois da recusa;
/// sem a lista o usuário só sabe que errou, não o que era aceito.
pub fn rule_enum(field: &str, choices: &[&str]) -> ValidationError {
    let mut error = rule("enum", format!("Valor inválido para `{field}`."));
    error.add_param(
        "choices".into(),
        &choices.iter().map(|c| (*c).to_string()).collect::<Vec<_>>(),
    );
    error
}

/// Campo de texto obrigatório com faixa de tamanho.
///
/// Devolve `false` quando já registrou um erro, para que o chamador não aplique
/// as regras seguintes sobre um valor que já se sabe inválido.
pub fn required_text(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<&String>,
    min: usize,
    max: usize,
) -> bool {
    required_str(errors, field, value.map(String::as_str), min, max)
}

/// Igual a [`required_text`], para quem já tem um `&str`.
///
/// Os campos de `config` chegam de um `serde_json::Map`, onde o valor é um
/// `&str` emprestado — clonar só para satisfazer a assinatura seria cópia por
/// conta da API, e não por necessidade.
pub fn required_str(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<&str>,
    min: usize,
    max: usize,
) -> bool {
    let Some(value) = value else {
        errors.add(field, rule("required", format!("Informe `{field}`.")));
        return false;
    };

    text_length(errors, field, value, min, max)
}

/// Faixa de tamanho de um texto já presente.
pub fn text_length(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: &str,
    min: usize,
    max: usize,
) -> bool {
    // Conta caracteres, não bytes: um `ç` não pode valer por dois.
    let length = value.chars().count();

    if length < min {
        errors.add(
            field,
            rule(
                "length",
                format!("`{field}` deve ter ao menos {min} caracteres."),
            ),
        );
        return false;
    }

    if length > max {
        errors.add(
            field,
            rule(
                "length",
                format!("`{field}` deve ter no máximo {max} caracteres."),
            ),
        );
        return false;
    }

    true
}

/// Valor obrigatório de um conjunto fechado.
pub fn required_enum(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<&String>,
    choices: &[&str],
) -> bool {
    let Some(value) = value else {
        errors.add(field, rule("required", format!("Informe `{field}`.")));
        return false;
    };

    optional_enum(errors, field, Some(value), choices)
}

/// Igual a [`required_enum`], mas aceita ausência.
pub fn optional_enum(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<&String>,
    choices: &[&str],
) -> bool {
    let Some(value) = value else {
        return true;
    };

    if choices.contains(&value.as_str()) {
        return true;
    }

    errors.add(field, rule_enum(field, choices));
    false
}

/// Inteiro obrigatório dentro de uma faixa.
pub fn required_number(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<i64>,
    max: i64,
) -> bool {
    let Some(value) = value else {
        errors.add(field, rule("required", format!("Informe `{field}`.")));
        return false;
    };

    number_range(errors, field, value, max)
}

/// Faixa de um inteiro já presente.
pub fn number_range(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: i64,
    max: i64,
) -> bool {
    if value <= 0 || value > max {
        errors.add(
            field,
            rule("range", format!("`{field}` deve estar entre 1 e {max}.")),
        );
        return false;
    }

    true
}

/// Fecha a validação: `Ok` quando nada foi registrado.
pub fn finish(errors: ValidationErrors) -> std::result::Result<(), ValidationErrors> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(errors: &ValidationErrors, field: &str) -> Vec<String> {
        errors
            .field_errors()
            .get(field)
            .map(|list| list.iter().map(|e| e.code.to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn reports_a_missing_field_as_required() {
        let mut errors = ValidationErrors::new();
        assert!(!required_text(&mut errors, "name", None, 1, 10));
        assert_eq!(codes(&errors, "name"), vec!["required"]);
    }

    #[test]
    fn both_length_failures_share_the_derive_code() {
        // Mesmo código que `#[validate(length(...))]` emitiria: quem consome a
        // resposta não deve precisar saber se a regra veio do derive ou daqui.
        let mut short = ValidationErrors::new();
        text_length(&mut short, "name", "", 1, 10);
        assert_eq!(codes(&short, "name"), vec!["length"]);

        let mut long = ValidationErrors::new();
        text_length(&mut long, "name", &"a".repeat(11), 1, 10);
        assert_eq!(codes(&long, "name"), vec!["length"]);
    }

    #[test]
    fn counts_characters_not_bytes() {
        // `çãoçãoçã` tem 8 caracteres e 13 bytes.
        let mut errors = ValidationErrors::new();
        assert!(text_length(&mut errors, "password", "çãoçãoçã", 8, 32));
    }

    #[test]
    fn an_enum_error_carries_the_choices() {
        let mut errors = ValidationErrors::new();
        optional_enum(
            &mut errors,
            "type",
            Some(&"oracle".to_string()),
            &["mysql", "mariadb", "postgresql"],
        );

        let field = errors.field_errors();
        let error = &field.get("type").expect("erro de tipo")[0];
        assert_eq!(error.code, "enum");
        assert!(error.params.contains_key("choices"));
    }

    #[test]
    fn an_absent_optional_enum_is_accepted() {
        let mut errors = ValidationErrors::new();
        assert!(optional_enum(&mut errors, "type", None, &["mysql"]));
        assert!(errors.is_empty());
    }

    #[test]
    fn a_number_outside_the_range_fails_on_either_side() {
        let mut zero = ValidationErrors::new();
        number_range(&mut zero, "port", 0, 65535);
        assert_eq!(codes(&zero, "port"), vec!["range"]);

        let mut huge = ValidationErrors::new();
        number_range(&mut huge, "port", 65536, 65535);
        assert_eq!(codes(&huge, "port"), vec!["range"]);

        let mut ok = ValidationErrors::new();
        assert!(number_range(&mut ok, "port", 3306, 65535));
    }

    #[test]
    fn a_single_error_names_its_field() {
        let errors = single_error("email", "unique", "Este e-mail já está cadastrado.");
        assert_eq!(codes(&errors, "email"), vec!["unique"]);
    }
}
