//! Dominio de `users`: credenciais, cadastro e listagem administrativa.
//!
//! O que o Adonis resolve com o mixin `withAuthFinder` e o `SimplePaginator`
//! mora aqui, para que os controllers so' traduzam HTTP. Duas regras merecem
//! destaque porque sao faceis de "melhorar" por engano durante o porte:
//!
//! 1. **`is_active` nao entra na verificacao de credenciais.** O Adonis valida
//!    a senha primeiro e so' depois olha o `isActive`, devolvendo mensagens
//!    diferentes para "senha errada" (400) e "conta pendente" (401). Juntar as
//!    duas coisas mudaria os dois status.
//! 2. **O e-mail e' normalizado antes de qualquer consulta**, com o mesmo
//!    algoritmo do VineJS — ver [`crate::models::email`].

use loco_rs::prelude::*;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationErrors};

use crate::models::email;
use crate::models::password::PasswordHasher;
use crate::models::validation::{finish, required_email, required_text};
use crate::views::pagination::PageRequest;

pub use super::_entities::users::{ActiveModel, Column, Entity, Model};

impl ActiveModelBehavior for ActiveModel {}

/// Hash usado quando o e-mail nao existe, para igualar o tempo de resposta.
///
/// Sem ele, "usuario inexistente" volta na hora e "senha errada" leva o tempo
/// de um scrypt — a diferenca e' medivel e transforma o login num oraculo de
/// quais e-mails estao cadastrados. E' o mesmo motivo do `hash.verify` sobre um
/// hash falso que o `withAuthFinder` do Adonis executa.
///
/// E' derivado no primeiro uso, e nao escrito a mao: um PHC invalido faria o
/// `verify` sair antes de derivar, e a equalizacao deixaria de existir sem
/// nenhum sinal.
static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    PasswordHasher::default()
        .hash("timing-equalisation-placeholder")
        .unwrap_or_default()
});

/// Corpo de `POST /api/auth/register`, na forma em que chega do cliente.
///
/// Todos os campos sao `Option` de proposito: um corpo sem `email` precisa
/// virar `{"field":"email","rule":"required"}`, e nao um 400 de desserializacao
/// sem detalhe nenhum.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RegisterParams {
    #[serde(default, rename = "fullName")]
    pub full_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default, rename = "bootstrapToken")]
    pub bootstrap_token: Option<String>,
}

/// Corpo de `POST /api/auth/login`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LoginParams {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

impl Validate for RegisterParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if let Some(full_name) = self.full_name.as_ref() {
            required_text(&mut errors, "fullName", Some(full_name), 0, 100);
        }
        required_email(&mut errors, self.email.as_ref());
        required_text(&mut errors, "password", self.password.as_ref(), 8, 32);
        if let Some(token) = self.bootstrap_token.as_ref() {
            required_text(&mut errors, "bootstrapToken", Some(token), 0, 255);
        }

        finish(errors)
    }
}

impl Validate for LoginParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        required_email(&mut errors, self.email.as_ref());
        // Sem limite de tamanho: o `loginValidator` do Adonis so' exige que a
        // senha exista. Recusar por tamanho aqui trocaria um 400 de credencial
        // invalida por um 422, e quebraria quem tem senha antiga fora da faixa.
        required_text(
            &mut errors,
            "password",
            self.password.as_ref(),
            1,
            usize::MAX,
        );

        finish(errors)
    }
}

impl RegisterParams {
    /// E-mail normalizado, pronto para consultar o banco.
    pub fn normalized_email(&self) -> String {
        self.email
            .as_deref()
            .map(email::normalize)
            .unwrap_or_default()
    }
}

impl LoginParams {
    pub fn normalized_email(&self) -> String {
        self.email
            .as_deref()
            .map(email::normalize)
            .unwrap_or_default()
    }
}

/// Dados ja' validados para criar um usuario.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub full_name: Option<String>,
    pub email: String,
    pub password: String,
    pub is_active: bool,
    pub is_admin: bool,
}

impl Model {
    /// Quantos usuarios existem. Alimenta `GET /api/auth/status` e a regra do
    /// primeiro administrador.
    pub async fn count_all(db: &impl ConnectionTrait) -> Result<u64> {
        Ok(Entity::find().count(db).await?)
    }

    /// Busca pelo e-mail **ja' normalizado**.
    pub async fn find_by_email(db: &impl ConnectionTrait, email: &str) -> Result<Option<Self>> {
        Ok(Entity::find()
            .filter(Column::Email.eq(email))
            .one(db)
            .await?)
    }

    /// Confere e-mail e senha.
    ///
    /// `None` cobre os dois modos de falha — e-mail inexistente e senha errada —
    /// porque o chamador **nao pode** responder diferente para eles: a diferenca
    /// diria a um atacante quais e-mails estao cadastrados. Nao olha
    /// `is_active`; quem decide isso e' o controller, com outro status.
    pub async fn authenticate(
        db: &impl ConnectionTrait,
        email: &str,
        password: &str,
    ) -> Result<Option<Self>> {
        let hasher = PasswordHasher::default();
        let found = Self::find_by_email(db, email).await?;

        let Some(user) = found else {
            // Deriva mesmo assim, para gastar o mesmo tempo do caminho em que o
            // usuario existe.
            let _ = hasher.verify(&DUMMY_HASH, password);
            return Ok(None);
        };

        if hasher.verify(&user.password, password) {
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    /// Cria um usuario com a senha ja' derivada.
    pub async fn create(db: &impl ConnectionTrait, new_user: NewUser) -> Result<Self> {
        let hasher = PasswordHasher::default();
        let hashed = hasher
            .hash(&new_user.password)
            .map_err(|err| Error::Message(format!("failed to hash the password: {err}")))?;

        let now = chrono::Utc::now().naive_utc();

        Ok(ActiveModel {
            full_name: Set(new_user.full_name),
            email: Set(new_user.email),
            password: Set(hashed),
            is_active: Set(new_user.is_active),
            is_admin: Set(new_user.is_admin),
            created_at: Set(now),
            updated_at: Set(Some(now)),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }

    /// Uma pagina da listagem administrativa.
    ///
    /// Ordena por `created_at desc` como o Adonis, **mais** `id desc` como
    /// desempate. O Adonis nao tem o desempate (achado da Fase 2): usuarios
    /// criados no mesmo segundo saem em ordem arbitraria, e a mesma pessoa pode
    /// aparecer na pagina 1 e na 2. O desempate nao muda a ordenacao
    /// contratada, so' a torna estavel.
    pub async fn list_page(
        db: &impl ConnectionTrait,
        page: PageRequest,
        is_active: Option<bool>,
    ) -> Result<(Vec<Self>, u64)> {
        let filter = Condition::all().add_option(is_active.map(|value| Column::IsActive.eq(value)));

        let total = Entity::find().filter(filter.clone()).count(db).await?;

        let rows = Entity::find()
            .filter(filter)
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .offset(page.offset())
            .limit(page.per_page)
            .all(db)
            .await?;

        Ok((rows, total))
    }

    /// Inverte o `is_active` e devolve o registro atualizado.
    pub async fn toggle_active(self, db: &impl ConnectionTrait) -> Result<Self> {
        let flipped = !self.is_active;
        let mut active: ActiveModel = self.into();

        active.is_active = Set(flipped);
        active.updated_at = Set(Some(chrono::Utc::now().naive_utc()));

        Ok(active.update(db).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors_of(params: &impl Validate) -> Vec<(String, String)> {
        let Err(errors) = Validate::validate(params) else {
            return Vec::new();
        };

        let mut collected: Vec<(String, String)> = errors
            .field_errors()
            .iter()
            .flat_map(|(field, list)| {
                list.iter()
                    .map(move |error| ((*field).to_string(), error.code.to_string()))
            })
            .collect();
        collected.sort();
        collected
    }

    fn valid_register() -> RegisterParams {
        RegisterParams {
            full_name: Some("Contract Admin".to_string()),
            email: Some("admin@contract.test".to_string()),
            password: Some("senha-de-teste".to_string()),
            bootstrap_token: None,
        }
    }

    #[test]
    fn accepts_a_complete_registration() {
        assert!(Validate::validate(&valid_register()).is_ok());
    }

    #[test]
    fn full_name_and_bootstrap_token_are_optional() {
        let params = RegisterParams {
            full_name: None,
            bootstrap_token: None,
            ..valid_register()
        };
        assert!(Validate::validate(&params).is_ok());
    }

    #[test]
    fn requires_email_and_password() {
        let params = RegisterParams::default();

        assert_eq!(
            errors_of(&params),
            vec![
                ("email".to_string(), "required".to_string()),
                ("password".to_string(), "required".to_string()),
            ]
        );
    }

    #[test]
    fn enforces_the_password_length_range() {
        for (password, expected) in [("1234567", "minLength"), (&"a".repeat(33), "maxLength")] {
            let params = RegisterParams {
                password: Some(password.to_string()),
                ..valid_register()
            };
            assert_eq!(
                errors_of(&params),
                vec![("password".to_string(), expected.to_string())],
                "senha {password:?}"
            );
        }
    }

    #[test]
    fn the_login_password_has_no_length_limit() {
        // Uma senha antiga fora da faixa de 8..32 precisa continuar podendo ser
        // digitada — quem recusa e' a verificacao, com 400, nao a validacao.
        let params = LoginParams {
            email: Some("admin@contract.test".to_string()),
            password: Some("a".to_string()),
        };
        assert!(Validate::validate(&params).is_ok());
    }

    #[test]
    fn login_still_requires_both_fields() {
        assert_eq!(
            errors_of(&LoginParams::default()),
            vec![
                ("email".to_string(), "required".to_string()),
                ("password".to_string(), "required".to_string()),
            ]
        );
    }

    #[test]
    fn normalizes_the_email_before_the_query() {
        let params = LoginParams {
            email: Some(" J.O.A.O+erp@GMail.com ".to_string()),
            password: Some("x".to_string()),
        };
        assert_eq!(params.normalized_email(), "joao@gmail.com");
    }

    #[test]
    fn the_dummy_hash_is_shaped_like_a_real_one() {
        // Se ele nao passar pelo parser do PHC, o `verify` sai antes de derivar
        // e a equalizacao de tempo deixa de existir — em silencio.
        assert!(PasswordHasher::is_valid_hash(&DUMMY_HASH));
        assert!(!PasswordHasher::default().verify(&DUMMY_HASH, "qualquer-senha"));
    }
}
