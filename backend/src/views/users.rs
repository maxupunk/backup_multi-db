//! Respostas de `/api/users` (tarefa 5.3).
//!
//! `GET /api/users` e' a unica rota da API que devolve a pagina **crua**, sem o
//! envelope `{success, data}` — o Adonis simplesmente serializa o paginador do
//! Lucid. Reproduzir a falta do envelope e' contrato, ainda que destoe do
//! resto.

use serde::Serialize;

use crate::models::_entities::users;
use crate::views::timestamp;

/// Item da listagem administrativa.
///
/// E' a serializacao completa do model do Lucid **menos** `password`, que leva
/// `serializeAs: null`. A rota lista todos os usuarios: um vazamento aqui expoe
/// o hash de todo mundo de uma vez, nao so' o de quem chamou.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserListItem {
    pub id: i64,
    pub full_name: Option<String>,
    pub email: String,
    pub is_active: bool,
    pub is_admin: bool,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
    #[serde(serialize_with = "timestamp::serialize_option")]
    pub updated_at: Option<chrono::NaiveDateTime>,
}

impl From<users::Model> for UserListItem {
    fn from(user: users::Model) -> Self {
        Self {
            id: user.id,
            full_name: user.full_name,
            email: user.email,
            is_active: user.is_active,
            is_admin: user.is_admin,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

/// `data` de `PATCH /api/users/:id/status` — so' o que mudou.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggledStatus {
    pub id: i64,
    pub is_active: bool,
}

impl From<&users::Model> for ToggledStatus {
    fn from(user: &users::Model) -> Self {
        Self {
            id: user.id,
            is_active: user.is_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> users::Model {
        users::Model {
            id: 11,
            full_name: Some("Contract Member".to_string()),
            email: "member@contract.test".to_string(),
            password: "$scrypt$n=16384,r=8,p=1$c2FsdA$aGFzaA".to_string(),
            is_active: false,
            is_admin: false,
            created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
            updated_at: None,
        }
    }

    #[test]
    fn never_serializes_the_password() {
        let rendered = serde_json::to_string(&UserListItem::from(user())).expect("serializa");

        // O teste de contrato faz exatamente estas duas asercoes sobre o corpo
        // cru de `GET /api/users`.
        assert!(!rendered.contains("password"));
        assert!(!rendered.contains("$scrypt$"));
    }

    #[test]
    fn carries_exactly_the_seven_serializable_columns() {
        let json = serde_json::to_value(UserListItem::from(user())).expect("serializa");

        for key in [
            "id",
            "fullName",
            "email",
            "isActive",
            "isAdmin",
            "createdAt",
            "updatedAt",
        ] {
            assert!(json.get(key).is_some(), "faltou `{key}`");
        }
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(7));
    }

    #[test]
    fn the_booleans_are_real_booleans() {
        // Um dos achados da Fase 2: no Adonis alguns campos vem como `0`/`1`
        // quando lidos do SQLite. Em `users` nao — o model converte, e o golden
        // grava `true`/`false`.
        let json = serde_json::to_value(UserListItem::from(user())).expect("serializa");

        assert_eq!(json["isActive"], serde_json::Value::Bool(false));
        assert_eq!(json["isAdmin"], serde_json::Value::Bool(false));
    }

    #[test]
    fn the_toggle_payload_reports_only_the_new_state() {
        let json = serde_json::to_value(ToggledStatus::from(&user())).expect("serializa");

        assert_eq!(json, serde_json::json!({ "id": 11, "isActive": false }));
    }
}
