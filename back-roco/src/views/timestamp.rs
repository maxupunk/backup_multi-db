//! Serializacao dos timestamps no formato que o Adonis entrega.
//!
//! ## Por que sem fuso
//!
//! O banco guarda **hora local ingenua** — `2026-08-06 16:49:25`, sem
//! deslocamento. O Lucid le' isso como hora local e o Luxon serializa com o
//! offset da maquina (`…T16:49:25.000-03:00`).
//!
//! Aqui a coluna chega como `NaiveDateTime`, e o deslocamento original nao esta'
//! gravado em lugar nenhum. Escrever `Z` seria afirmar que aquele horario e'
//! UTC, e o navegador renderizaria o backup das 16h49 como 13h49. Emitir a hora
//! sem sufixo mantem a leitura correta: `new Date("2026-08-06T16:49:25.000")` em
//! JavaScript interpreta como hora local, que e' exatamente o que o valor e'.
//!
//! A suite de contrato compara o **tipo** do campo (string) e redige o valor,
//! entao a diferenca de sufixo em relacao ao Adonis nao a afeta.

use chrono::NaiveDateTime;
use serde::Serializer;

/// `2026-08-06T16:49:25.000`
const FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f";

/// Formata um timestamp para a resposta.
pub fn format(moment: NaiveDateTime) -> String {
    moment.format(FORMAT).to_string()
}

/// Para `#[serde(serialize_with = "…")]` num campo obrigatorio.
pub fn serialize<S: Serializer>(
    moment: &NaiveDateTime,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.serialize_str(&format(*moment))
}

/// Para `#[serde(serialize_with = "…")]` num campo opcional.
///
/// Um `None` vira `null`, e nao chave ausente: o Adonis emite `null` nesses
/// campos, e omitir a chave seria diferenca de shape.
pub fn serialize_option<S: Serializer>(
    moment: &Option<NaiveDateTime>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match moment {
        Some(value) => serializer.serialize_str(&format(*value)),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").expect("data de teste")
    }

    #[test]
    fn emits_milliseconds_and_no_zone() {
        // O `Z` faria o navegador deslocar o horario pelo fuso da maquina.
        assert_eq!(format(at("2026-08-06 16:49:25")), "2026-08-06T16:49:25.000");
    }

    #[test]
    fn matches_the_iso_pattern_the_suite_redacts() {
        // `^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?$`
        let rendered = format(at("2026-08-06 16:49:25"));
        let pattern = regex::Regex::new(
            r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:?\d{2})?$",
        )
        .expect("regex constante");

        assert!(
            pattern.is_match(&rendered),
            "nao seria redigido: {rendered}"
        );
    }

    #[test]
    fn an_absent_timestamp_becomes_null() {
        #[derive(serde::Serialize)]
        struct Row {
            #[serde(serialize_with = "serialize_option")]
            updated_at: Option<NaiveDateTime>,
        }

        let json = serde_json::to_value(Row { updated_at: None }).expect("serializa");
        assert!(json["updated_at"].is_null());
    }
}
