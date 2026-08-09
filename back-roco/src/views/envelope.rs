//! Os envelopes de sucesso da API.
//!
//! O contrato tem tres formas, e a escolha entre elas nao e' livre — cada rota
//! usa a sua, e trocar uma pela outra quebra o cliente:
//!
//! ```jsonc
//! { "success": true, "data": … }                       // a maioria
//! { "success": true, "message": "…" }                   // logout, register pendente
//! { "success": true, "message": "…", "data": … }        // toggle de status
//! ```
//!
//! Os erros tem envelopes proprios, com outra logica — ver
//! [`crate::views::errors`].

use serde::Serialize;

/// `{ "success": true, "data": … }`
#[derive(Debug, Clone, Serialize)]
pub struct Data<T> {
    pub success: bool,
    pub data: T,
}

impl<T> Data<T> {
    pub const fn new(data: T) -> Self {
        Self {
            success: true,
            data,
        }
    }
}

/// `{ "success": true, "message": "…" }`
#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub success: bool,
    pub message: String,
}

impl Message {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }
}

/// `{ "success": true, "message": "…", "data": … }`
#[derive(Debug, Clone, Serialize)]
pub struct MessageWithData<T> {
    pub success: bool,
    pub message: String,
    pub data: T,
}

impl<T> MessageWithData<T> {
    pub fn new(message: impl Into<String>, data: T) -> Self {
        Self {
            success: true,
            message: message.into(),
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_envelopes_carry_exactly_their_keys() {
        let data = serde_json::to_value(Data::new(42)).expect("serializa");
        let message = serde_json::to_value(Message::new("ok")).expect("serializa");
        let both = serde_json::to_value(MessageWithData::new("ok", 42)).expect("serializa");

        assert_eq!(data, serde_json::json!({ "success": true, "data": 42 }));
        assert_eq!(
            message,
            serde_json::json!({ "success": true, "message": "ok" })
        );
        assert_eq!(
            both,
            serde_json::json!({ "success": true, "message": "ok", "data": 42 })
        );
    }

    #[test]
    fn success_is_always_true() {
        // O envelope de sucesso nunca carrega `false` — quem falha usa
        // `ApiError`, que tem outro shape.
        assert!(Data::new(0).success);
        assert!(Message::new("x").success);
        assert!(MessageWithData::new("x", 0).success);
    }
}
