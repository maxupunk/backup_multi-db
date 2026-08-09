//! Opaque access tokens, compatible with `@adonisjs/auth`.
//!
//! Decision D1 keeps the AdonisJS token format so that sessions issued before
//! the cutover keep working after it. The `auth_access_tokens` rows carry a
//! SHA-256 hex digest of the token secret; nothing about the token itself is
//! stored, so this type is what turns an `Authorization: Bearer …` header back
//! into a row.
//!
//! Wire format of the value handed to the client:
//!
//! ```text
//! oat_<base64url(identifier)>.<base64url(secret)>
//! ```
//!
//! where `secret` is `<seed><crc32(seed)>` — 40 random characters followed by
//! the CRC-32 of those characters in decimal. The checksum exists so a
//! typo'd token can be rejected before touching the database; Adonis itself
//! never validates it, and neither does verification here.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine as _;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Prefix Adonis puts on every token it issues.
pub const TOKEN_PREFIX: &str = "oat_";

/// Length of the random seed, matching `tokenSecretLength`.
const SEED_LENGTH: usize = 40;

/// Alphabet the seed is drawn from — the base64url set, which is what
/// `@poppinss/utils`'s `string.random` produces.
const SEED_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// A token value split into its two parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedToken {
    /// Primary key of the `auth_access_tokens` row, as text.
    ///
    /// Kept as a string because that is what the base64 segment decodes to;
    /// parsing it to an integer is the caller's job, and a non-numeric value is
    /// simply a token that matches no row.
    pub identifier: String,

    /// The secret to hash and compare against `auth_access_tokens.hash`.
    pub secret: String,
}

/// A freshly minted token: the value to hand out, and the digest to persist.
#[derive(Debug, Clone)]
pub struct GeneratedToken {
    /// Full `oat_…` value. Shown to the client once and never stored.
    pub value: String,

    /// SHA-256 hex digest of the secret. This is what goes in the `hash` column.
    pub hash: String,
}

/// Parses, verifies and mints opaque access tokens.
pub struct AccessToken;

impl AccessToken {
    /// Splits an `oat_<id>.<secret>` value into its parts.
    ///
    /// Returns `None` for anything malformed. Callers must treat that exactly
    /// like a token that matched no row — a distinct error would tell an
    /// attacker which of their guesses were well-formed.
    pub fn decode(value: &str) -> Option<DecodedToken> {
        let body = value.strip_prefix(TOKEN_PREFIX)?;

        // Only the first `.` separates the fields; base64url never emits one,
        // but splitting from the left keeps a stray dot inside the secret from
        // silently truncating it.
        let (identifier_b64, secret_b64) = body.split_once('.')?;
        if identifier_b64.is_empty() || secret_b64.is_empty() {
            return None;
        }

        let identifier = String::from_utf8(B64URL.decode(identifier_b64).ok()?).ok()?;
        let secret = String::from_utf8(B64URL.decode(secret_b64).ok()?).ok()?;

        if identifier.is_empty() || secret.is_empty() {
            return None;
        }

        Some(DecodedToken { identifier, secret })
    }

    /// Digests a secret the way the `hash` column stores it.
    pub fn hash_secret(secret: &str) -> String {
        let digest = Sha256::digest(secret.as_bytes());
        hex::encode(digest)
    }

    /// Checks a presented secret against a stored digest, in constant time.
    ///
    /// Comparing hex strings byte-wise is enough here: both sides are
    /// fixed-length lowercase hex, so length alone leaks nothing.
    pub fn verify(stored_hash: &str, secret: &str) -> bool {
        let computed = Self::hash_secret(secret);
        computed.as_bytes().ct_eq(stored_hash.as_bytes()).into()
    }

    /// Mints a token for the row that will hold `identifier`.
    ///
    /// The caller inserts the row first to learn its primary key, then calls
    /// this — the identifier is part of the value, so it cannot be minted
    /// before the row exists.
    pub fn generate(identifier: &str) -> Result<GeneratedToken, TokenError> {
        let seed = Self::random_seed(SEED_LENGTH)?;
        let secret = format!("{seed}{}", crc32fast::hash(seed.as_bytes()));

        Ok(GeneratedToken {
            hash: Self::hash_secret(&secret),
            value: format!(
                "{TOKEN_PREFIX}{}.{}",
                B64URL.encode(identifier.as_bytes()),
                B64URL.encode(secret.as_bytes())
            ),
        })
    }

    /// Draws `length` characters uniformly from the base64url alphabet.
    ///
    /// Rejection-free because the alphabet is exactly 64 characters, so masking
    /// a random byte to 6 bits is already uniform — no modulo bias to correct.
    fn random_seed(length: usize) -> Result<String, TokenError> {
        let mut bytes = vec![0u8; length];
        getrandom::fill(&mut bytes).map_err(|_| TokenError::EntropyUnavailable)?;

        let seed = bytes
            .iter()
            .map(|b| SEED_ALPHABET[(b & 0b0011_1111) as usize] as char)
            .collect();

        Ok(seed)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("failed to read from the system random source")]
    EntropyUnavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_generated_token() {
        let token = AccessToken::generate("42").unwrap();
        let decoded = AccessToken::decode(&token.value).expect("its own token decodes");

        assert_eq!(decoded.identifier, "42");
        assert!(AccessToken::verify(&token.hash, &decoded.secret));
    }

    #[test]
    fn emits_the_adonis_token_shape() {
        let token = AccessToken::generate("1").unwrap();

        assert!(token.value.starts_with("oat_"));
        assert_eq!(token.hash.len(), 64, "sha256 in hex");
        assert!(token.hash.chars().all(|c| c.is_ascii_hexdigit()));

        let decoded = AccessToken::decode(&token.value).unwrap();
        // 40-character seed plus the CRC-32 in decimal, which is 1..=10 digits.
        assert!(
            (SEED_LENGTH + 1..=SEED_LENGTH + 10).contains(&decoded.secret.len()),
            "unexpected secret length: {}",
            decoded.secret.len()
        );
    }

    #[test]
    fn appends_the_crc32_of_the_seed() {
        let decoded = AccessToken::decode(&AccessToken::generate("1").unwrap().value).unwrap();
        let (seed, checksum) = decoded.secret.split_at(SEED_LENGTH);

        assert_eq!(checksum, crc32fast::hash(seed.as_bytes()).to_string());
    }

    #[test]
    fn draws_seeds_from_the_base64url_alphabet() {
        let decoded = AccessToken::decode(&AccessToken::generate("1").unwrap().value).unwrap();
        let seed = &decoded.secret[..SEED_LENGTH];

        assert!(
            seed.bytes().all(|b| SEED_ALPHABET.contains(&b)),
            "seed left the alphabet: {seed}"
        );
    }

    #[test]
    fn never_repeats_a_secret() {
        let a = AccessToken::generate("1").unwrap();
        let b = AccessToken::generate("1").unwrap();

        assert_ne!(a.value, b.value);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn rejects_malformed_values() {
        for bad in [
            "",
            "oat_",
            "no-prefix.abc",
            "oat_abc",     // no separator
            "oat_.abc",    // empty identifier
            "oat_abc.",    // empty secret
            "oat_!!!.abc", // identifier is not base64url
            "oat_MQ.!!!",  // secret is not base64url
            "OAT_MQ.YWJj", // prefix is case-sensitive
            "oat_MQ+YWJj", // wrong separator
        ] {
            assert!(AccessToken::decode(bad).is_none(), "accepted {bad:?}");
        }
    }

    #[test]
    fn rejects_a_secret_that_does_not_match_the_stored_hash() {
        let token = AccessToken::generate("1").unwrap();
        let other = AccessToken::generate("1").unwrap();
        let secret = AccessToken::decode(&token.value).unwrap().secret;

        assert!(!AccessToken::verify(&other.hash, &secret));
        assert!(!AccessToken::verify(&token.hash, "wrong-secret"));
        assert!(!AccessToken::verify("", &secret));
        // A stored hash of the wrong length must not compare equal.
        assert!(!AccessToken::verify(&token.hash[..32], &secret));
    }

    #[test]
    fn encodes_identifiers_that_are_not_single_digits() {
        for identifier in ["7", "123456", "9007199254740991"] {
            let token = AccessToken::generate(identifier).unwrap();
            assert_eq!(
                AccessToken::decode(&token.value).unwrap().identifier,
                identifier
            );
        }
    }
}
