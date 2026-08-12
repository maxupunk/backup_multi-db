//! Respostas de `/api/users`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::models::_entities::users;

/// Um usuário como a API o devolve.
///
/// A senha **não** está aqui, e nunca pode estar: `GET /api/users` lista todo
/// mundo, então um vazamento exporia o hash de todos de uma vez, não só o de
/// quem chamou. O mesmo vale para `api_key` e para os tokens de recuperação —
/// são credenciais, não atributos de perfil.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct User {
    #[ts(type = "number")]
    pub id: i64,
    pub full_name: Option<String>,
    pub email: String,
    pub is_active: bool,
    pub is_admin: bool,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    #[ts(type = "string")]
    pub updated_at: chrono::DateTime<chrono::FixedOffset>,
}

impl From<users::Model> for User {
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

impl From<&users::Model> for User {
    fn from(user: &users::Model) -> Self {
        Self::from(user.clone())
    }
}

/// Corpo de `PATCH /api/users/:id/status` — só o que mudou.
///
/// Devolver o usuário inteiro seria mais uniforme, mas esta rota existe para
/// alternar um campo: o cliente que a chama quer confirmar o novo estado, e
/// tudo o mais que viesse junto seria dado que ele já tem.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct UserStatus {
    #[ts(type = "number")]
    pub id: i64,
    pub is_active: bool,
}

impl From<&users::Model> for UserStatus {
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
            password: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_string(),
            api_key: "bk_test".to_string(),
            pid: uuid::Uuid::nil(),
            reset_token: None,
            reset_sent_at: None,
            email_verification_token: None,
            email_verification_sent_at: None,
            email_verified_at: None,
            is_active: false,
            is_admin: false,
            created_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
            updated_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
        }
    }

    #[test]
    fn never_serialises_a_credential() {
        let rendered = serde_json::to_string(&User::from(user())).expect("serializa");

        for secret in [
            "password",
            "argon2",
            "api_key",
            "apiKey",
            "reset_token",
            "pid",
        ] {
            assert!(!rendered.contains(secret), "vazou `{secret}`: {rendered}");
        }
    }

    #[test]
    fn carries_exactly_the_seven_public_columns() {
        let json = serde_json::to_value(User::from(user())).expect("serializa");

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
    fn the_status_payload_reports_only_the_new_state() {
        let json = serde_json::to_value(UserStatus::from(&user())).expect("serializa");

        assert_eq!(json, serde_json::json!({ "id": 11, "isActive": false }));
    }
}
