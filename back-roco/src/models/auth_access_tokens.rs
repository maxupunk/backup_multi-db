//! Ciclo de vida do token opaco (tarefa 3.4 do roadmap, decisao D1).
//!
//! [`crate::models::access_token`] cuida do **formato** — decodificar
//! `oat_<id>.<secret>`, derivar o SHA-256, comparar em tempo constante. Este
//! modulo cuida do **registro**: emitir, encontrar, expirar e revogar. A
//! separacao existe porque o formato e' pura criptografia, testavel sem banco,
//! e o registro so' faz sentido com banco.
//!
//! ## Uma divergencia deliberada em relacao ao Adonis
//!
//! O `DbAccessTokensProvider.verify` do Adonis grava `last_used_at` **antes**
//! de conferir o hash: qualquer requisicao com um token bem-formado cujo `id`
//! exista provoca um `UPDATE`, mesmo com o segredo errado. Aqui a ordem e'
//! invertida — confere primeiro, grava depois. A coluna nao aparece em nenhuma
//! resposta, entao a diferenca e' invisivel para o contrato, e a ordem do
//! Adonis daria a qualquer anonimo um jeito de gerar escrita no banco em rajada.

use loco_rs::prelude::*;
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};

use crate::models::_entities::users;
use crate::models::access_token::AccessToken;

pub use super::_entities::auth_access_tokens::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

/// Valor de `auth_access_tokens.type` para token de sessao.
///
/// A coluna existe porque o Adonis permite varios provedores na mesma tabela.
/// Toda consulta filtra por ela: sem o filtro, um token de outro proposito
/// autenticaria uma sessao.
pub const TOKEN_TYPE: &str = "auth_token";

/// Abilities default do Adonis — acesso total.
const DEFAULT_ABILITIES: &str = "[\"*\"]";

/// Token recem-emitido: o valor a entregar e a linha gravada.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    /// `oat_…`. Mostrado uma vez e nunca mais recuperavel.
    pub value: String,
    pub model: Model,
}

/// Sessao autenticada: a linha do token e o usuario dono dela.
#[derive(Debug, Clone)]
pub struct VerifiedSession {
    pub token: Model,
    pub user: users::Model,
}

impl Model {
    /// Emite um token para `user_id`, valido por `ttl_seconds`.
    ///
    /// A linha e' inserida antes do valor ser montado porque o identificador
    /// faz parte do valor — nao da' para minta-lo sem saber a chave primaria.
    pub async fn issue(
        db: &impl ConnectionTrait,
        user_id: i64,
        ttl_seconds: u64,
    ) -> Result<IssuedToken> {
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);

        let row = ActiveModel {
            tokenable_id: Set(user_id),
            r#type: Set(TOKEN_TYPE.to_string()),
            name: Set(None),
            // Placeholder: o hash de verdade so' existe depois que a linha tem
            // id. Fica um valor impossivel de casar em vez de vazio, para que
            // uma falha entre o insert e o update deixe um token inutil, e nao
            // um token que qualquer segredo abre.
            hash: Set(String::new()),
            abilities: Set(DEFAULT_ABILITIES.to_string()),
            created_at: Set(Some(now.naive_utc())),
            updated_at: Set(Some(now.naive_utc())),
            last_used_at: Set(None),
            expires_at: Set(Some(expires_at.naive_utc())),
            ..Default::default()
        }
        .insert(db)
        .await?;

        let generated = AccessToken::generate(&row.id.to_string())
            .map_err(|err| Error::Message(format!("failed to mint an access token: {err}")))?;

        let mut update: ActiveModel = row.into();
        update.hash = Set(generated.hash);
        let model = update.update(db).await?;

        Ok(IssuedToken {
            value: generated.value,
            model,
        })
    }

    /// Resolve um `Authorization: Bearer …` numa sessao.
    ///
    /// `None` cobre **todos** os modos de falha — formato invalido, linha
    /// inexistente, segredo errado, token expirado e usuario apagado. Distinguir
    /// entre eles na resposta diria a quem tenta adivinhar quais identificadores
    /// existem.
    ///
    /// Nao olha `is_active`: desativar um usuario **nao** derruba a sessao dele
    /// no Adonis (achado da Fase 2). Acrescentar a checagem aqui seria uma
    /// melhoria de seguranca real, mas mudaria o comportamento observavel de
    /// `GET /api/auth/me` — fica registrado no roadmap como decisao pendente.
    pub async fn verify(
        db: &impl ConnectionTrait,
        presented: &str,
    ) -> Result<Option<VerifiedSession>> {
        let Some(decoded) = AccessToken::decode(presented) else {
            return Ok(None);
        };

        let Ok(id) = decoded.identifier.parse::<i64>() else {
            return Ok(None);
        };

        let found = Entity::find_by_id(id)
            .filter(Column::Type.eq(TOKEN_TYPE))
            .one(db)
            .await?;

        let Some(token) = found else {
            return Ok(None);
        };

        if !AccessToken::verify(&token.hash, &decoded.secret) {
            return Ok(None);
        }

        if is_expired(&token, chrono::Utc::now().naive_utc()) {
            return Ok(None);
        }

        let Some(user) = users::Entity::find_by_id(token.tokenable_id)
            .one(db)
            .await?
        else {
            // A FK e' CASCADE, entao isto so' acontece numa corrida entre o
            // delete do usuario e esta requisicao. Melhor recusar que servir
            // uma sessao sem dono.
            return Ok(None);
        };

        let token = touch(db, token).await?;

        Ok(Some(VerifiedSession { token, user }))
    }

    /// Revoga um token do usuario. Devolve quantas linhas sairam.
    ///
    /// O filtro por `tokenable_id` nao e' redundante: sem ele, saber o id de um
    /// token bastaria para desloga' um terceiro.
    pub async fn revoke(db: &impl ConnectionTrait, id: i64, user_id: i64) -> Result<u64> {
        let result = Entity::delete_many()
            .filter(Column::Id.eq(id))
            .filter(Column::TokenableId.eq(user_id))
            .filter(Column::Type.eq(TOKEN_TYPE))
            .exec(db)
            .await?;

        Ok(result.rows_affected)
    }
}

/// Um token sem `expires_at` nao expira — e' o que o Adonis faz quando
/// `expiresIn` nao esta' configurado.
fn is_expired(token: &Model, now: chrono::NaiveDateTime) -> bool {
    token.expires_at.is_some_and(|expires_at| expires_at <= now)
}

/// Marca o token como usado agora.
///
/// A falha e' engolida de proposito: `last_used_at` e' telemetria, e um erro de
/// escrita aqui nao pode derrubar uma requisicao que ja' esta' autenticada.
async fn touch(db: &impl ConnectionTrait, token: Model) -> Result<Model> {
    let id = token.id;
    let mut active: ActiveModel = token.clone().into();
    active.last_used_at = Set(Some(chrono::Utc::now().naive_utc()));

    match active.update(db).await {
        Ok(updated) => Ok(updated),
        Err(err) => {
            tracing::warn!(token_id = id, error = %err, "failed to record last_used_at");
            Ok(token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> chrono::NaiveDateTime {
        chrono::DateTime::parse_from_rfc3339(text)
            .expect("data de teste valida")
            .naive_utc()
    }

    fn token_with(expires_at: Option<chrono::NaiveDateTime>) -> Model {
        Model {
            id: 1,
            tokenable_id: 1,
            r#type: TOKEN_TYPE.to_string(),
            name: None,
            hash: "a".repeat(64),
            abilities: DEFAULT_ABILITIES.to_string(),
            created_at: None,
            updated_at: None,
            last_used_at: None,
            expires_at,
        }
    }

    #[test]
    fn a_token_without_an_expiry_never_expires() {
        assert!(!is_expired(&token_with(None), at("2099-01-01T00:00:00Z")));
    }

    #[test]
    fn expires_on_the_boundary() {
        // `<=`, e nao `<`: no instante exato do vencimento o token ja' morreu.
        // Um `<` daria uma janela de um segundo a um token vencido.
        let token = token_with(Some(at("2026-08-09T12:00:00Z")));

        assert!(is_expired(&token, at("2026-08-09T12:00:00Z")));
        assert!(is_expired(&token, at("2026-08-09T12:00:01Z")));
        assert!(!is_expired(&token, at("2026-08-09T11:59:59Z")));
    }

    #[test]
    fn the_default_abilities_match_adonis() {
        // O Adonis grava `["*"]`. Um shape diferente aqui quebraria qualquer
        // checagem de ability daqui para a frente.
        assert_eq!(
            serde_json::from_str::<Vec<String>>(DEFAULT_ABILITIES).unwrap(),
            vec!["*".to_string()]
        );
    }
}
