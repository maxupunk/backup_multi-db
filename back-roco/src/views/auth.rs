//! Respostas de `/api/auth/*` (tarefa 5.1).
//!
//! Sao tres formas de usuario no contrato, e a diferenca entre elas e'
//! deliberada, nao descuido do Adonis:
//!
//! - [`SessionUser`] — o que sai junto do token em `register` e `login`. **Sem
//!   `createdAt`.**
//! - [`CurrentUser`] — o de `GET /api/auth/me`, **com `createdAt`**.
//! - `UserListItem` (em [`crate::views::users`]) — o da listagem administrativa,
//!   com todas as colunas serializaveis.
//!
//! Unificar as tres numa so' acrescentaria chave em resposta que hoje nao tem —
//! e o matcher de shape da suite reprova chave a mais tanto quanto chave a
//! menos.

use serde::Serialize;

use crate::models::_entities::users;
use crate::views::timestamp;

/// Estado do sistema para a tela de primeiro acesso.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub has_users: bool,
    /// `true` so' quando **nao ha' nenhum usuario** e o ambiente e' producao.
    ///
    /// E' o que faz a tela de cadastro pedir o token de bootstrap. Fora de
    /// producao o primeiro admin e' criado sem token, para nao travar o
    /// desenvolvimento.
    pub requires_bootstrap_token: bool,
}

/// Usuario que acompanha um token recem-emitido.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUser {
    pub id: i64,
    pub email: String,
    pub full_name: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
}

impl From<&users::Model> for SessionUser {
    fn from(user: &users::Model) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            full_name: user.full_name.clone(),
            is_active: user.is_active,
            is_admin: user.is_admin,
        }
    }
}

/// Corpo de `register` e `login` bem-sucedidos.
#[derive(Debug, Clone, Serialize)]
pub struct Session {
    /// Sempre `"bearer"`. Literal no contrato, e nao um enum, porque o cliente
    /// so' o repassa no cabecalho `Authorization`.
    #[serde(rename = "type")]
    pub token_type: &'static str,
    pub token: String,
    pub user: SessionUser,
}

impl Session {
    pub fn new(token: String, user: &users::Model) -> Self {
        Self {
            token_type: "bearer",
            token,
            user: SessionUser::from(user),
        }
    }
}

/// Corpo de `GET /api/auth/me`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUser {
    pub id: i64,
    pub email: String,
    pub full_name: Option<String>,
    pub is_active: bool,
    pub is_admin: bool,
    #[serde(serialize_with = "timestamp::serialize")]
    pub created_at: chrono::NaiveDateTime,
}

impl From<&users::Model> for CurrentUser {
    fn from(user: &users::Model) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            full_name: user.full_name.clone(),
            is_active: user.is_active,
            is_admin: user.is_admin,
            created_at: user.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> users::Model {
        users::Model {
            id: 7,
            full_name: Some("Contract Admin".to_string()),
            email: "admin@contract.test".to_string(),
            password: "$scrypt$n=16384,r=8,p=1$c2FsdA$aGFzaA".to_string(),
            is_active: true,
            is_admin: true,
            created_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-05 08:09:51",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste"),
            updated_at: None,
        }
    }

    #[test]
    fn the_session_user_has_no_created_at() {
        let json = serde_json::to_value(SessionUser::from(&user())).expect("serializa");

        assert_eq!(
            json,
            serde_json::json!({
                "id": 7,
                "email": "admin@contract.test",
                "fullName": "Contract Admin",
                "isActive": true,
                "isAdmin": true
            })
        );
    }

    #[test]
    fn the_current_user_has_created_at() {
        let json = serde_json::to_value(CurrentUser::from(&user())).expect("serializa");

        assert_eq!(json["createdAt"], "2026-08-05T08:09:51.000");
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(6));
    }

    #[test]
    fn no_view_ever_carries_the_password() {
        // A coluna existe no `Model`; um `#[derive(Serialize)]` na entidade a
        // levaria junto. As views sao o que impede isso.
        for json in [
            serde_json::to_value(SessionUser::from(&user())).expect("serializa"),
            serde_json::to_value(CurrentUser::from(&user())).expect("serializa"),
        ] {
            let rendered = json.to_string();
            assert!(!rendered.contains("password"), "vazou a chave: {rendered}");
            assert!(!rendered.contains("$scrypt$"), "vazou o hash: {rendered}");
        }
    }

    #[test]
    fn the_token_type_is_the_literal_bearer() {
        let json =
            serde_json::to_value(Session::new("oat_MQ.YWJj".to_string(), &user())).expect("ok");

        assert_eq!(json["type"], "bearer");
        assert_eq!(json["token"], "oat_MQ.YWJj");
    }

    #[test]
    fn a_user_without_a_name_serializes_null() {
        let mut model = user();
        model.full_name = None;

        let json = serde_json::to_value(SessionUser::from(&model)).expect("serializa");
        assert!(json["fullName"].is_null());
    }
}
