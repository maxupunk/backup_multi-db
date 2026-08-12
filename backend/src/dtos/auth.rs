//! Respostas de `/api/auth/*`.
//!
//! O usuário sai daqui na mesma forma que sai de `/api/users`: [`User`], de
//! [`crate::dtos::users`]. Havia três recortes diferentes do mesmo registro —
//! um com `createdAt`, outro sem, outro com tudo —, e a diferença não
//! correspondia a nenhuma regra: era só o que cada rota tinha à mão. Um tipo só
//! dá ao frontend um `User` que serve em qualquer tela.
//!
//! Nenhuma resposta expõe `password`, `api_key`, `pid` ou os campos de
//! recuperação de senha.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::dtos::users::User;
use crate::models::_entities::users;

/// Estado do sistema para a tela de primeiro acesso.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct SystemStatus {
    pub has_users: bool,
    /// `true` só quando **não há nenhum usuário** e o ambiente é produção.
    ///
    /// É o que faz a tela de cadastro pedir o token de bootstrap. Fora de
    /// produção o primeiro administrador é criado sem token, para não travar o
    /// desenvolvimento.
    pub requires_bootstrap_token: bool,
}

/// Corpo de `register` e `login` bem-sucedidos.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../frontend/src/bindings/")]
pub struct Session {
    /// Sempre `"bearer"` — o cliente só repassa o valor no cabeçalho
    /// `Authorization`. Vai ao TypeScript como tipo literal, e não como
    /// `string`, para que um `Authorization: ${type}` errado não compile.
    #[serde(rename = "type")]
    #[ts(rename = "type", type = "\"bearer\"")]
    pub token_type: String,
    pub token: String,
    pub user: User,
}

impl Session {
    pub fn new(token: String, user: &users::Model) -> Self {
        Self {
            token_type: "bearer".to_string(),
            token,
            user: User::from(user),
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
            password: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".to_string(),
            api_key: "bk_test".to_string(),
            pid: uuid::Uuid::nil(),
            reset_token: None,
            reset_sent_at: None,
            email_verification_token: None,
            email_verification_sent_at: None,
            email_verified_at: None,
            is_active: true,
            is_admin: true,
            created_at: chrono::NaiveDateTime::parse_from_str(
                "2026-08-05 08:09:51",
                "%Y-%m-%d %H:%M:%S",
            )
            .expect("data de teste")
            .and_utc()
            .fixed_offset(),
            updated_at: chrono::DateTime::UNIX_EPOCH.fixed_offset(),
        }
    }

    #[test]
    fn the_session_carries_the_same_user_shape_as_the_listing() {
        let json = serde_json::to_value(Session::new("a.jwt.token".to_string(), &user()))
            .expect("serializa");

        assert_eq!(
            json["user"],
            serde_json::to_value(User::from(&user())).expect("serializa")
        );
    }

    #[test]
    fn the_timestamp_is_an_instant_not_a_wall_clock() {
        // Com o deslocamento: o navegador recebe um instante, e não uma hora
        // local para a qual ele teria de adivinhar o fuso.
        let json = serde_json::to_value(User::from(&user())).expect("serializa");

        assert_eq!(json["createdAt"], "2026-08-05T08:09:51Z");
    }

    #[test]
    fn no_response_ever_carries_a_secret() {
        // As colunas existem no `Model`; um `#[derive(Serialize)]` na entidade
        // as levaria junto. Os DTOs são o que impede isso.
        let rendered = serde_json::to_value(Session::new("a.jwt.token".to_string(), &user()))
            .expect("serializa")
            .to_string();

        for secret in [
            "password",
            "$argon2",
            "apiKey",
            "api_key",
            "resetToken",
            "pid",
        ] {
            assert!(!rendered.contains(secret), "vazou {secret}: {rendered}");
        }
    }

    #[test]
    fn the_token_type_is_the_literal_bearer() {
        let json = serde_json::to_value(Session::new("a.jwt.token".to_string(), &user()))
            .expect("serializa");

        assert_eq!(json["type"], "bearer");
        assert_eq!(json["token"], "a.jwt.token");
    }

    #[test]
    fn a_user_without_a_name_serialises_null() {
        let mut model = user();
        model.full_name = None;

        let json = serde_json::to_value(User::from(&model)).expect("serializa");
        assert!(json["fullName"].is_null());
    }
}
