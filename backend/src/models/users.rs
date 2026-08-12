//! `users` domain: credentials, registration, recovery and the admin listing.
//!
//! Three rules here are easy to "improve" by accident:
//!
//! 1. **`is_active` takes no part in credential checking.** The password is
//!    verified first and only then is the flag consulted, so "wrong password"
//!    (400) and "account pending approval" (401) stay distinct. Folding them
//!    together would tell an attacker which e-mails exist.
//! 2. **The e-mail is normalised before every query** — see
//!    [`crate::models::email`].
//! 3. **Recovery never confirms an address.** `forgot` is a no-op for an
//!    unknown e-mail and the controller answers the same either way.

use loco_rs::hash;
use loco_rs::prelude::*;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::{Validate, ValidationErrors};

use crate::models::email;
use crate::models::validation::{finish, required_email, required_text};
use crate::views::pagination::PageRequest;

pub use super::_entities::users::{ActiveModel, Column, Entity, Model};

/// How long a `reset_token` stays usable, in hours.
///
/// Short enough that a link sitting in a mailbox stops being a standing key to
/// the account, long enough to survive a message that took a while to arrive.
/// The value is repeated in the e-mail body, so change both together.
const RESET_TOKEN_TTL_HOURS: i64 = 4;

/// Length of the generated `api_key`, in characters.
const API_KEY_LENGTH: usize = 32;

/// Length of the generated `reset_token`, in characters.
const RESET_TOKEN_LENGTH: usize = 32;

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> std::result::Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert && self.updated_at.is_unchanged() {
            let mut this = self;
            this.updated_at = Set(chrono::Utc::now().into());
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

#[async_trait::async_trait]
impl Authenticable for Model {
    async fn find_by_api_key(db: &DatabaseConnection, api_key: &str) -> ModelResult<Self> {
        Entity::find()
            .filter(Column::ApiKey.eq(api_key))
            .one(db)
            .await?
            .ok_or(ModelError::EntityNotFound)
    }

    /// Resolves the `pid` carried by a JWT.
    ///
    /// The claim is a UUID string; anything else is a forged or stale token, so
    /// a parse failure is reported as "no such user" rather than as an internal
    /// error.
    async fn find_by_claims_key(db: &DatabaseConnection, claims_key: &str) -> ModelResult<Self> {
        let pid = Uuid::parse_str(claims_key).map_err(|_| ModelError::EntityNotFound)?;

        Entity::find()
            .filter(Column::Pid.eq(pid))
            .one(db)
            .await?
            .ok_or(ModelError::EntityNotFound)
    }
}

/// Hash used when the e-mail does not exist, to equalise response time.
///
/// Without it, "unknown user" returns immediately while "wrong password" pays
/// for a full argon2 derivation. The difference is measurable and turns login
/// into an oracle for which addresses are registered.
///
/// It is derived on first use rather than written by hand: an invalid PHC
/// string would make `verify` bail out before deriving, and the equalisation
/// would silently stop existing.
static DUMMY_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    hash::hash_password("timing-equalisation-placeholder").unwrap_or_default()
});

/// Body of `POST /api/auth/register`, as it arrives from the client.
///
/// Every field is `Option` on purpose: a body without `email` has to become
/// `{"field":"email","rule":"required"}`, not a bare deserialisation failure
/// with no detail.
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

/// Body of `POST /api/auth/login`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LoginParams {
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// Body of `POST /api/auth/forgot`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ForgotParams {
    #[serde(default)]
    pub email: Option<String>,
}

/// Body of `POST /api/auth/reset`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ResetParams {
    #[serde(default)]
    pub token: Option<String>,
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
        // No length bound: rejecting by length here would turn a wrong
        // credential (400) into a validation failure (422), and would lock out
        // anyone whose password predates the current range.
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

impl Validate for ForgotParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        required_email(&mut errors, self.email.as_ref());
        finish(errors)
    }
}

impl Validate for ResetParams {
    fn validate(&self) -> std::result::Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        required_text(&mut errors, "token", self.token.as_ref(), 1, 255);
        // Same range the registration enforces: a reset must not be a way to
        // set a password the sign-up form would have refused.
        required_text(&mut errors, "password", self.password.as_ref(), 8, 32);

        finish(errors)
    }
}

impl RegisterParams {
    /// Normalised e-mail, ready to query with.
    #[must_use]
    pub fn normalized_email(&self) -> String {
        self.email
            .as_deref()
            .map(email::normalize)
            .unwrap_or_default()
    }
}

impl LoginParams {
    #[must_use]
    pub fn normalized_email(&self) -> String {
        self.email
            .as_deref()
            .map(email::normalize)
            .unwrap_or_default()
    }
}

impl ForgotParams {
    #[must_use]
    pub fn normalized_email(&self) -> String {
        self.email
            .as_deref()
            .map(email::normalize)
            .unwrap_or_default()
    }
}

/// Validated data for creating a user.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub full_name: Option<String>,
    pub email: String,
    pub password: String,
    pub is_active: bool,
    pub is_admin: bool,
}

impl Model {
    /// How many users exist. Feeds `GET /api/auth/status` and the first-admin
    /// rule.
    ///
    /// # Errors
    /// Fails when the query cannot be executed.
    pub async fn count_all(db: &impl ConnectionTrait) -> Result<u64> {
        Ok(Entity::find().count(db).await?)
    }

    /// Looks a user up by an **already normalised** e-mail.
    ///
    /// # Errors
    /// Fails when the query cannot be executed.
    pub async fn find_by_email(db: &impl ConnectionTrait, email: &str) -> Result<Option<Self>> {
        Ok(Entity::find()
            .filter(Column::Email.eq(email))
            .one(db)
            .await?)
    }

    /// Looks a user up by the public identifier carried in the JWT.
    ///
    /// # Errors
    /// Fails when the query cannot be executed.
    pub async fn find_by_pid(db: &impl ConnectionTrait, pid: &Uuid) -> Result<Option<Self>> {
        Ok(Entity::find().filter(Column::Pid.eq(*pid)).one(db).await?)
    }

    /// Looks a reset token up, ignoring the ones that have expired.
    ///
    /// Expiry is enforced here and not at the call site so that no future
    /// caller can forget it.
    ///
    /// # Errors
    /// Fails when the query cannot be executed.
    pub async fn find_by_reset_token(
        db: &impl ConnectionTrait,
        token: &str,
    ) -> Result<Option<Self>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(RESET_TOKEN_TTL_HOURS);

        Ok(Entity::find()
            .filter(Column::ResetToken.eq(token))
            .filter(Column::ResetSentAt.gte(chrono::DateTime::<chrono::FixedOffset>::from(cutoff)))
            .one(db)
            .await?)
    }

    /// Checks an e-mail and password pair.
    ///
    /// `None` covers both failure modes — unknown e-mail and wrong password —
    /// because the caller **must not** answer differently for them.
    ///
    /// # Errors
    /// Fails when the query cannot be executed.
    pub async fn authenticate(
        db: &impl ConnectionTrait,
        email: &str,
        password: &str,
    ) -> Result<Option<Self>> {
        let found = Self::find_by_email(db, email).await?;

        let Some(user) = found else {
            // Derive anyway, so the miss costs the same as the hit.
            let _ = hash::verify_password(password, &DUMMY_HASH);
            return Ok(None);
        };

        if hash::verify_password(password, &user.password) {
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }

    /// Creates a user with the password already derived.
    ///
    /// # Errors
    /// Fails when the password cannot be hashed or the insert is rejected —
    /// a duplicate e-mail among them.
    pub async fn create(db: &impl ConnectionTrait, new_user: NewUser) -> Result<Self> {
        let hashed = hash::hash_password(&new_user.password)?;

        Ok(ActiveModel {
            pid: Set(Uuid::new_v4()),
            api_key: Set(format!("bk_{}", hash::random_string(API_KEY_LENGTH))),
            full_name: Set(new_user.full_name),
            email: Set(new_user.email),
            password: Set(hashed),
            is_active: Set(new_user.is_active),
            is_admin: Set(new_user.is_admin),
            ..Default::default()
        }
        .insert(db)
        .await?)
    }

    /// Issues a JWT naming this user by `pid`.
    ///
    /// # Errors
    /// Fails when the configured secret is not valid base64.
    pub fn generate_jwt(&self, secret: &str, expiration_seconds: u64) -> Result<String> {
        loco_rs::auth::jwt::JWT::new(secret)
            .generate_token(
                expiration_seconds,
                self.pid.to_string(),
                serde_json::Map::new(),
            )
            .map_err(|err| Error::Message(format!("failed to sign the session token: {err}")))
    }

    /// Stamps a fresh reset token and returns the updated user.
    ///
    /// Overwriting an existing token is deliberate: the newest link is the only
    /// one that works, so a leaked older mail stops being useful.
    ///
    /// # Errors
    /// Fails when the update is rejected.
    pub async fn start_password_reset(self, db: &impl ConnectionTrait) -> Result<Self> {
        let mut active: ActiveModel = self.into();

        active.reset_token = Set(Some(hash::random_string(RESET_TOKEN_LENGTH)));
        active.reset_sent_at = Set(Some(chrono::Utc::now().into()));

        Ok(active.update(db).await?)
    }

    /// Replaces the password and burns the reset token.
    ///
    /// # Errors
    /// Fails when the password cannot be hashed or the update is rejected.
    pub async fn finish_password_reset(
        self,
        db: &impl ConnectionTrait,
        password: &str,
    ) -> Result<Self> {
        let hashed = hash::hash_password(password)?;
        let mut active: ActiveModel = self.into();

        active.password = Set(hashed);
        // Clearing both is what makes the link single-use.
        active.reset_token = Set(None);
        active.reset_sent_at = Set(None);

        Ok(active.update(db).await?)
    }

    /// One page of the admin listing.
    ///
    /// Ordered by `created_at desc` with `id desc` as the tie-break: users
    /// created within the same second would otherwise come back in an arbitrary
    /// order, and the same person could show up on both page 1 and page 2.
    ///
    /// # Errors
    /// Fails when either query cannot be executed.
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

    /// Flips `is_active` and returns the updated record.
    ///
    /// # Errors
    /// Fails when the update is rejected.
    pub async fn toggle_active(self, db: &impl ConnectionTrait) -> Result<Self> {
        let flipped = !self.is_active;
        let mut active: ActiveModel = self.into();

        active.is_active = Set(flipped);

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
    fn a_reset_requires_a_token_and_a_password_in_range() {
        assert_eq!(
            errors_of(&ResetParams::default()),
            vec![
                ("password".to_string(), "required".to_string()),
                ("token".to_string(), "required".to_string()),
            ]
        );

        let short = ResetParams {
            token: Some("t".to_string()),
            password: Some("1234567".to_string()),
        };
        assert_eq!(
            errors_of(&short),
            vec![("password".to_string(), "minLength".to_string())]
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
        // If it does not parse as PHC, `verify` bails out before deriving and
        // the timing equalisation silently stops existing.
        assert!(DUMMY_HASH.starts_with("$argon2"));
        assert!(!hash::verify_password("qualquer-senha", &DUMMY_HASH));
    }
}
