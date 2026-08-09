//! Password hashing, compatible with the AdonisJS `scrypt` hasher.
//!
//! Port of `@adonisjs/hash`'s scrypt driver, configured as
//! `backend/config/hash.ts` configures it. The `users.password` column already
//! holds strings this produced, and decision D2 keeps them: users must log in
//! after the migration with the password they already have, without a reset and
//! without a rehash pass.
//!
//! Stored format is a PHC string:
//!
//! ```text
//! $scrypt$n=16384,r=8,p=1$<salt>$<hash>
//! ```
//!
//! Both fields are standard-alphabet base64 **without padding**. `n`, `r` and
//! `p` are Node's `cost`, `blockSize` and `parallelization`; the `scrypt` crate
//! spells `n` as its base-2 logarithm, which is where the two APIs diverge.

use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use base64::Engine as _;
use subtle::ConstantTimeEq;

/// Work factor. Node calls it `cost`; scrypt calls it `N`.
const DEFAULT_COST: u32 = 16_384;

/// Block size (`r`).
const DEFAULT_BLOCK_SIZE: u32 = 8;

/// Parallelisation (`p`).
const DEFAULT_PARALLELIZATION: u32 = 1;

/// Salt length in bytes, matching the driver's `saltSize` default.
const DEFAULT_SALT_SIZE: usize = 16;

/// Derived key length in bytes, matching the driver's `keyLength` default.
const DEFAULT_KEY_LENGTH: usize = 64;

/// The PHC identifier this hasher accepts.
const PHC_ID: &str = "scrypt";

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("invalid PHC string")]
    InvalidPhc,

    #[error("unsupported scrypt parameters")]
    InvalidParams,

    #[error("failed to derive key")]
    DerivationFailed,
}

/// Parameters recovered from a stored PHC string.
///
/// Verification derives with **these**, never with the current configuration —
/// otherwise raising the cost factor would lock out every existing user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScryptParams {
    cost: u32,
    block_size: u32,
    parallelization: u32,
}

/// Hashes and verifies passwords in the AdonisJS scrypt format.
#[derive(Debug, Clone)]
pub struct PasswordHasher {
    params: ScryptParams,
    salt_size: usize,
    key_length: usize,
}

impl Default for PasswordHasher {
    /// The configuration in `backend/config/hash.ts`.
    fn default() -> Self {
        Self {
            params: ScryptParams {
                cost: DEFAULT_COST,
                block_size: DEFAULT_BLOCK_SIZE,
                parallelization: DEFAULT_PARALLELIZATION,
            },
            salt_size: DEFAULT_SALT_SIZE,
            key_length: DEFAULT_KEY_LENGTH,
        }
    }
}

impl PasswordHasher {
    /// Hashes `plaintext` with a fresh random salt.
    pub fn hash(&self, plaintext: &str) -> Result<String, PasswordError> {
        let mut salt = vec![0u8; self.salt_size];
        getrandom::fill(&mut salt).map_err(|_| PasswordError::DerivationFailed)?;

        let derived = derive(plaintext, &salt, self.params, self.key_length)?;

        Ok(format!(
            "${PHC_ID}$n={},r={},p={}${}${}",
            self.params.cost,
            self.params.block_size,
            self.params.parallelization,
            B64.encode(&salt),
            B64.encode(&derived)
        ))
    }

    /// Checks `plaintext` against a stored PHC string.
    ///
    /// Returns `false` for every failure mode, malformed input included. A
    /// caller cannot act differently on "wrong password" versus "corrupt hash",
    /// and distinguishing them in the response would leak which accounts exist.
    pub fn verify(&self, phc: &str, plaintext: &str) -> bool {
        let Ok(parsed) = Phc::parse(phc) else {
            return false;
        };

        let Ok(derived) = derive(plaintext, &parsed.salt, parsed.params, parsed.hash.len()) else {
            return false;
        };

        derived.ct_eq(&parsed.hash).into()
    }

    /// Reports whether a stored hash was produced with different parameters and
    /// should be re-hashed on the next successful login.
    pub fn needs_rehash(&self, phc: &str) -> bool {
        match Phc::parse(phc) {
            Ok(parsed) => parsed.params != self.params,
            // Unparseable means it is not one of ours, so it certainly needs one.
            Err(_) => true,
        }
    }

    /// Reports whether `value` is shaped like a hash this type can verify.
    pub fn is_valid_hash(value: &str) -> bool {
        Phc::parse(value).is_ok()
    }
}

/// Runs scrypt with Node's parameter spelling.
///
/// `scrypt` takes `log2(N)` where Node takes `N`, so a cost that is not a power
/// of two has no representation here — it is rejected rather than rounded.
fn derive(
    plaintext: &str,
    salt: &[u8],
    params: ScryptParams,
    key_length: usize,
) -> Result<Vec<u8>, PasswordError> {
    if !params.cost.is_power_of_two() || params.cost < 2 {
        return Err(PasswordError::InvalidParams);
    }

    let log_n = params.cost.trailing_zeros();
    let log_n = u8::try_from(log_n).map_err(|_| PasswordError::InvalidParams)?;

    // `Params::new` takes only the work factors; the output length is decided by
    // the buffer handed to `scrypt` below.
    let scrypt_params = scrypt::Params::new(log_n, params.block_size, params.parallelization)
        .map_err(|_| PasswordError::InvalidParams)?;

    let mut out = vec![0u8; key_length];
    scrypt::scrypt(plaintext.as_bytes(), salt, &scrypt_params, &mut out)
        .map_err(|_| PasswordError::DerivationFailed)?;

    Ok(out)
}

/// A parsed `$scrypt$n=..,r=..,p=..$salt$hash` string.
struct Phc {
    params: ScryptParams,
    salt: Vec<u8>,
    hash: Vec<u8>,
}

impl Phc {
    fn parse(phc: &str) -> Result<Self, PasswordError> {
        // Leading `$` makes the first split field empty; the driver's own
        // validator relies on the same five-field shape.
        let mut fields = phc.split('$');
        let (Some(""), Some(id), Some(params), Some(salt), Some(hash), None) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return Err(PasswordError::InvalidPhc);
        };

        if id != PHC_ID {
            return Err(PasswordError::InvalidPhc);
        }

        let params = Self::parse_params(params)?;
        let salt = B64.decode(salt).map_err(|_| PasswordError::InvalidPhc)?;
        let hash = B64.decode(hash).map_err(|_| PasswordError::InvalidPhc)?;

        // Mirrors the driver's `RangeValidator` bounds. A hash shorter than 64
        // bytes is not something this application ever wrote.
        if !(8..=1024).contains(&salt.len()) || !(64..=128).contains(&hash.len()) {
            return Err(PasswordError::InvalidPhc);
        }

        Ok(Self { params, salt, hash })
    }

    /// Parses `n=16384,r=8,p=1`.
    ///
    /// Order is not assumed: the PHC grammar does not fix it, and the cost of
    /// looking each key up is nothing next to the scrypt derivation that follows.
    fn parse_params(raw: &str) -> Result<ScryptParams, PasswordError> {
        let (mut cost, mut block_size, mut parallelization) = (None, None, None);

        for pair in raw.split(',') {
            let (key, value) = pair.split_once('=').ok_or(PasswordError::InvalidPhc)?;
            let value: u32 = value.parse().map_err(|_| PasswordError::InvalidPhc)?;

            match key {
                "n" => cost = Some(value),
                "r" => block_size = Some(value),
                "p" => parallelization = Some(value),
                _ => return Err(PasswordError::InvalidPhc),
            }
        }

        let (Some(cost), Some(block_size), Some(parallelization)) =
            (cost, block_size, parallelization)
        else {
            return Err(PasswordError::InvalidPhc);
        };

        Ok(ScryptParams {
            cost,
            block_size,
            parallelization,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cheap cost factor. The production 16384 takes ~100ms per derivation,
    /// which turns a handful of unit tests into a noticeable pause; the
    /// compatibility tests in `tests/models/password.rs` use the real one.
    fn fast() -> PasswordHasher {
        PasswordHasher {
            params: ScryptParams {
                cost: 1024,
                block_size: 8,
                parallelization: 1,
            },
            ..PasswordHasher::default()
        }
    }

    #[test]
    fn round_trips_a_password() {
        let hasher = fast();
        let phc = hasher.hash("senha123").unwrap();

        assert!(hasher.verify(&phc, "senha123"));
        assert!(!hasher.verify(&phc, "senha124"));
        assert!(!hasher.verify(&phc, ""));
    }

    #[test]
    fn emits_the_adonis_phc_shape() {
        let phc = fast().hash("senha123").unwrap();
        let fields: Vec<&str> = phc.split('$').collect();

        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0], "");
        assert_eq!(fields[1], "scrypt");
        assert_eq!(fields[2], "n=1024,r=8,p=1");
        assert_eq!(B64.decode(fields[3]).unwrap().len(), DEFAULT_SALT_SIZE);
        assert_eq!(B64.decode(fields[4]).unwrap().len(), DEFAULT_KEY_LENGTH);
    }

    #[test]
    fn salts_every_hash_independently() {
        let hasher = fast();
        assert_ne!(
            hasher.hash("same").unwrap(),
            hasher.hash("same").unwrap(),
            "two hashes of one password must not collide"
        );
    }

    #[test]
    fn verifies_using_the_stored_parameters_not_the_current_ones() {
        // A hash written under an old, cheaper cost must keep verifying after
        // the configured cost is raised — otherwise every user is locked out.
        let old = fast();
        let phc = old.hash("senha123").unwrap();

        let mut newer = fast();
        newer.params.cost = 2048;

        assert!(newer.verify(&phc, "senha123"));
        assert!(newer.needs_rehash(&phc));
        assert!(!old.needs_rehash(&phc));
    }

    #[test]
    fn rejects_malformed_phc_strings() {
        let hasher = fast();
        let valid = hasher.hash("senha123").unwrap();

        let bad = [
            "",
            "senha123",
            "$scrypt$n=1024,r=8,p=1$only-three-fields",
            "$argon2id$n=1024,r=8,p=1$AAAAAAAAAAAAAAAA$AAAA",
            "$scrypt$n=1024,r=8$AAAAAAAAAAAAAAAA$AAAA",
            "$scrypt$n=abc,r=8,p=1$AAAAAAAAAAAAAAAA$AAAA",
            "$scrypt$n=1024,r=8,p=1,x=9$AAAAAAAAAAAAAAAA$AAAA",
            // Right shape, hash far below the 64-byte floor.
            "$scrypt$n=1024,r=8,p=1$AAAAAAAAAAAAAAAA$AAAA",
        ];

        for value in bad {
            assert!(!hasher.verify(value, "senha123"), "accepted {value:?}");
            assert!(!PasswordHasher::is_valid_hash(value), "validated {value:?}");
        }

        assert!(PasswordHasher::is_valid_hash(&valid));
    }

    #[test]
    fn rejects_a_cost_that_is_not_a_power_of_two() {
        let mut hasher = fast();
        hasher.params.cost = 1000;

        assert!(matches!(
            hasher.hash("senha123"),
            Err(PasswordError::InvalidParams)
        ));
    }

    #[test]
    fn treats_an_unparseable_hash_as_needing_a_rehash() {
        assert!(fast().needs_rehash("not-a-phc-string"));
    }

    #[test]
    fn handles_unicode_and_long_passwords() {
        let hasher = fast();

        for password in ["ção ãe ü €", &"x".repeat(500), "  espaços  "] {
            let phc = hasher.hash(password).unwrap();
            assert!(hasher.verify(&phc, password), "failed for {password:?}");
        }
    }
}
