//! Regras de validacao no vocabulario do VineJS.
//!
//! Os `Params` de cada model implementam `validator::Validate` a mao, em vez de
//! usar o `derive`. O motivo e' o contrato: o corpo de um 422 traz o **nome da
//! regra** (`required`, `minLength`, `enum`, `database.unique`) e a **mensagem
//! exata** que o VineJS geraria, e o `derive` do crate `validator` nao permite
//! escolher os dois por campo.
//!
//! Este modulo concentra as regras para que `users`, `connections` e os models
//! das fases seguintes produzam texto identico. Espalhar as mensagens pelos
//! arquivos garantiria que uma delas divergisse.
//!
//! A traducao de `ValidationErrors` para o JSON de resposta fica em
//! [`crate::views::errors`] — o model nao conhece HTTP.

use validator::{ValidationError, ValidationErrors};

/// Monta um erro com o codigo (que vira `rule`) e a mensagem do VineJS.
pub fn rule(code: &'static str, message: String) -> ValidationError {
    let mut error = ValidationError::new(code);
    error.message = Some(message.into());
    error
}

/// Erro de `vine.enum()`, com as opcoes aceitas em `meta.choices`.
///
/// A interface usa a lista para remontar o select depois de um 422; sem ela o
/// usuario so' sabe que errou, nao o que era aceito.
pub fn rule_enum(field: &str, choices: &[&str]) -> ValidationError {
    let mut error = rule("enum", format!("The selected {field} is invalid"));
    error.add_param(
        "choices".into(),
        &choices.iter().map(|c| (*c).to_string()).collect::<Vec<_>>(),
    );
    error
}

/// Campo de texto obrigatorio com faixa de tamanho.
///
/// Devolve `false` quando ja' registrou um erro, para que o chamador nao
/// aplique as regras seguintes sobre um valor que ja' se sabe invalido — e'
/// como o VineJS encadeia.
pub fn required_text(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<&String>,
    min: usize,
    max: usize,
) -> bool {
    let Some(value) = value else {
        errors.add(
            field,
            rule("required", format!("The {field} field must be defined")),
        );
        return false;
    };

    text_length(errors, field, value, min, max)
}

/// Faixa de tamanho de um texto ja' presente.
pub fn text_length(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: &str,
    min: usize,
    max: usize,
) -> bool {
    // O VineJS conta caracteres, nao bytes: um `ç` nao pode valer por dois.
    let length = value.chars().count();

    if length < min {
        errors.add(
            field,
            rule(
                "minLength",
                format!("The {field} field must have at least {min} characters"),
            ),
        );
        return false;
    }

    if length > max {
        errors.add(
            field,
            rule(
                "maxLength",
                format!("The {field} field must not be greater than {max} characters"),
            ),
        );
        return false;
    }

    true
}

/// Valor obrigatorio de um conjunto fechado.
pub fn required_enum(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<&String>,
    choices: &[&str],
) -> bool {
    let Some(value) = value else {
        errors.add(
            field,
            rule("required", format!("The {field} field must be defined")),
        );
        return false;
    };

    optional_enum(errors, field, Some(value), choices)
}

/// Igual a [`required_enum`], mas aceita ausencia.
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

/// Inteiro obrigatorio dentro de uma faixa.
///
/// O VineJS separa `positive` de `max`; a mensagem muda conforme o lado que
/// estourou, e o cliente exibe a mensagem crua.
pub fn required_number(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: Option<i64>,
    max: i64,
) -> bool {
    let Some(value) = value else {
        errors.add(
            field,
            rule("required", format!("The {field} field must be defined")),
        );
        return false;
    };

    number_range(errors, field, value, max)
}

/// Faixa de um inteiro ja' presente.
pub fn number_range(
    errors: &mut ValidationErrors,
    field: &'static str,
    value: i64,
    max: i64,
) -> bool {
    if value <= 0 {
        errors.add(
            field,
            rule("positive", format!("The {field} field must be positive")),
        );
        return false;
    }

    if value > max {
        errors.add(
            field,
            rule(
                "max",
                format!("The {field} field must not be greater than {max}"),
            ),
        );
        return false;
    }

    true
}

/// Aceita o que o `vine.string().email()` aceita.
///
/// E' a checagem do `validator.js`: um `@`, algo de cada lado, e um ponto no
/// dominio. Deliberadamente frouxa — apertar aqui recusaria endereco que o
/// Adonis aceita hoje, e o cadastro passaria a falhar para gente que ja' tem
/// conta.
pub fn looks_like_an_email(value: &str) -> bool {
    let Some((local, domain)) = value.rsplit_once('@') else {
        return false;
    };

    !local.is_empty()
        && !domain.is_empty()
        && !value.contains(char::is_whitespace)
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

/// Campo de e-mail obrigatorio.
pub fn required_email(errors: &mut ValidationErrors, value: Option<&String>) {
    if !required_text(errors, "email", value, 1, 254) {
        return;
    }

    if let Some(value) = value {
        if !looks_like_an_email(value.trim()) {
            errors.add(
                "email",
                rule(
                    "email",
                    "The email field must be a valid email address".to_string(),
                ),
            );
        }
    }
}

/// Fecha a validacao: `Ok` quando nada foi registrado.
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
    fn distinguishes_the_two_length_failures() {
        let mut short = ValidationErrors::new();
        text_length(&mut short, "name", "", 1, 10);
        assert_eq!(codes(&short, "name"), vec!["minLength"]);

        let mut long = ValidationErrors::new();
        text_length(&mut long, "name", &"a".repeat(11), 1, 10);
        assert_eq!(codes(&long, "name"), vec!["maxLength"]);
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
        assert_eq!(
            error.message.as_deref(),
            Some("The selected type is invalid")
        );
        assert!(error.params.contains_key("choices"));
    }

    #[test]
    fn an_absent_optional_enum_is_accepted() {
        let mut errors = ValidationErrors::new();
        assert!(optional_enum(&mut errors, "type", None, &["mysql"]));
        assert!(errors.is_empty());
    }

    #[test]
    fn separates_positive_from_max() {
        let mut zero = ValidationErrors::new();
        number_range(&mut zero, "port", 0, 65535);
        assert_eq!(codes(&zero, "port"), vec!["positive"]);

        let mut huge = ValidationErrors::new();
        number_range(&mut huge, "port", 65536, 65535);
        assert_eq!(codes(&huge, "port"), vec!["max"]);

        let mut ok = ValidationErrors::new();
        assert!(number_range(&mut ok, "port", 3306, 65535));
    }

    #[test]
    fn rejects_a_malformed_email() {
        for bad in [
            "sem-arroba",
            "@dominio.com",
            "usuario@",
            "usuario@dominio",
            "a b@c.com",
        ] {
            assert!(!looks_like_an_email(bad), "aceitou {bad:?}");
        }
        assert!(looks_like_an_email("admin@contract.test"));
    }
}
