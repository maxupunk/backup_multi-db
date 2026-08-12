//! Authenticated encryption for stored credentials.
//!
//! Covers `connections.password_encrypted` and
//! `storage_destinations.config_encrypted` — the two columns that hold secrets
//! the application must be able to read back, so hashing is not an option.
//!
//! Wire format: `v1.base64url(nonce).base64url(ciphertext||tag)`
//!
//! Three properties are deliberate, and each replaces a shortcut the previous
//! implementation took:
//!
//! * **Stock `Aes256Gcm`**, which means a **12-byte** nonce. GCM's counter
//!   block is built directly from a 12-byte nonce; any other length is folded
//!   through GHASH first, which is legal but off the beaten path and rules out
//!   the audited default type.
//! * **The key is derived, never used raw.** `DB_ENCRYPTION_KEY` goes through
//!   HKDF-SHA256 with a fixed, versioned `info` label. Domain separation is the
//!   real win: the same configured secret can later feed a second purpose
//!   without the two sharing a key.
//! * **The payload names its own version.** A `v1.` prefix means the next
//!   format change can recognise what it is looking at instead of guessing from
//!   field count.
//!
//! There is **no** reader for any older format. A fallback path would be the
//! legacy coming back through the side door; existing ciphertexts are
//! unreadable by design and the credentials behind them are re-entered.
//!
//! It lives under `models/` because the two models that own encrypted columns
//! are its only callers, and their `ActiveModelBehavior` hooks are where
//! encryption happens.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64;
use base64::Engine as _;
use hkdf::Hkdf;
use sha2::Sha256;

/// Prefix every payload carries, so a future format is told apart by reading
/// rather than by guessing.
const VERSION: &str = "v1";

/// Nonce length of the stock `Aes256Gcm`, in bytes.
const NONCE_LENGTH: usize = 12;

/// Length of the raw key, in bytes (AES-256).
const KEY_LENGTH: usize = 32;

/// HKDF `info` label. Versioned with the wire format: a new format gets a new
/// label, and therefore a different key, without touching the configuration.
const KDF_INFO: &[u8] = b"backup-multi-db/column-encryption/v1";

/// Failure modes of the encrypt/decrypt round trip.
///
/// Deliberately opaque about *why* a decryption failed: telling "wrong key"
/// apart from "tampered ciphertext" is an oracle, and neither is actionable to
/// a caller.
#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("DB_ENCRYPTION_KEY must be 64 hexadecimal characters (32 bytes)")]
    InvalidKey,

    #[error("invalid encrypted payload format, expected v1.nonce.data")]
    InvalidFormat,

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("plaintext for encryption must not be empty")]
    EmptyPlaintext,

    #[error("decrypted payload is not valid UTF-8")]
    InvalidUtf8,
}

/// Encrypts and decrypts credential blobs under a key derived from the
/// configured secret.
///
/// The derived key is held for the lifetime of the value. Build one per
/// application boot and share it through `AppContext` rather than re-deriving
/// per request — HKDF is cheap, but not free, and doing it once keeps the raw
/// secret out of every call frame.
#[derive(Clone)]
pub struct EncryptionService {
    key: [u8; KEY_LENGTH],
}

impl std::fmt::Debug for EncryptionService {
    /// Never renders the key, so a stray `{:?}` in a log or an error chain
    /// cannot exfiltrate it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionService").finish_non_exhaustive()
    }
}

impl EncryptionService {
    /// Builds a service from the 64-character hex `DB_ENCRYPTION_KEY`.
    ///
    /// The hex value is the *input* to the KDF, not the cipher key. It is still
    /// required to be a full 32 bytes: HKDF spreads entropy, it does not create
    /// it, and a short secret would stay short.
    ///
    /// # Errors
    /// Returns [`EncryptionError::InvalidKey`] when the value is not exactly 64
    /// hex characters.
    pub fn from_hex_key(key_hex: &str) -> Result<Self, EncryptionError> {
        if key_hex.len() != KEY_LENGTH * 2 {
            return Err(EncryptionError::InvalidKey);
        }

        let mut secret = [0u8; KEY_LENGTH];
        hex::decode_to_slice(key_hex, &mut secret).map_err(|_| EncryptionError::InvalidKey)?;

        let mut key = [0u8; KEY_LENGTH];
        // No salt: there is exactly one input secret and it is already 256 bits
        // of configured entropy, so the salt would be a constant with nothing
        // to separate. `info` carries the domain separation instead.
        Hkdf::<Sha256>::new(None, &secret)
            .expand(KDF_INFO, &mut key)
            .map_err(|_| EncryptionError::InvalidKey)?;

        Ok(Self { key })
    }

    /// Encrypts `plaintext` into `v1.base64url(nonce).base64url(ciphertext||tag)`.
    ///
    /// The nonce is freshly random per call — reusing one under the same key
    /// breaks GCM outright. Empty input is rejected rather than stored: an empty
    /// credential is always a bug upstream, and a ciphertext that decrypts to
    /// nothing hides it.
    ///
    /// # Errors
    /// Returns [`EncryptionError::EmptyPlaintext`] for empty input, or
    /// [`EncryptionError::DecryptionFailed`] when the OS entropy source or the
    /// cipher fails.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        if plaintext.is_empty() {
            return Err(EncryptionError::EmptyPlaintext);
        }

        let mut nonce = [0u8; NONCE_LENGTH];
        getrandom::fill(&mut nonce).map_err(|_| EncryptionError::DecryptionFailed)?;

        let sealed = self
            .cipher()
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: &[],
                },
            )
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        Ok(format!(
            "{VERSION}.{}.{}",
            BASE64.encode(nonce),
            BASE64.encode(&sealed)
        ))
    }

    /// Decrypts a payload produced by [`Self::encrypt`].
    ///
    /// # Errors
    /// Returns [`EncryptionError::InvalidFormat`] for anything that is not a
    /// `v1` payload, [`EncryptionError::DecryptionFailed`] when authentication
    /// fails, and [`EncryptionError::InvalidUtf8`] when the plaintext is not
    /// text.
    pub fn decrypt(&self, encrypted: &str) -> Result<String, EncryptionError> {
        let parsed = Parsed::parse(encrypted)?;

        let plaintext = self
            .cipher()
            .decrypt(
                &Nonce::from(parsed.nonce),
                Payload {
                    msg: &parsed.sealed,
                    aad: &[],
                },
            )
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        String::from_utf8(plaintext).map_err(|_| EncryptionError::InvalidUtf8)
    }

    /// Reports whether `text` is shaped like one of our payloads.
    ///
    /// Used by the model hooks to avoid re-encrypting a value that is already
    /// encrypted. It is a shape check, not an authenticity check.
    #[must_use]
    pub fn is_encrypted(text: &str) -> bool {
        Parsed::parse(text).is_ok()
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new(&Key::<Aes256Gcm>::from(self.key))
    }
}

/// The decoded fields of the wire format.
struct Parsed {
    nonce: [u8; NONCE_LENGTH],
    /// Ciphertext with the GCM tag appended, which is the layout the `aead`
    /// API expects.
    sealed: Vec<u8>,
}

impl Parsed {
    /// Splits `v1.nonce.data` and validates the fixed-size field.
    ///
    /// A fourth segment is rejected rather than ignored: base64url never emits
    /// `.`, so an extra separator means the value is not one of ours.
    fn parse(encrypted: &str) -> Result<Self, EncryptionError> {
        let mut parts = encrypted.split('.');
        let (Some(version), Some(nonce_b64), Some(data_b64), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(EncryptionError::InvalidFormat);
        };

        if version != VERSION {
            return Err(EncryptionError::InvalidFormat);
        }

        let nonce: [u8; NONCE_LENGTH] = BASE64
            .decode(nonce_b64)
            .map_err(|_| EncryptionError::InvalidFormat)?
            .try_into()
            .map_err(|_| EncryptionError::InvalidFormat)?;

        let sealed = BASE64
            .decode(data_b64)
            .map_err(|_| EncryptionError::InvalidFormat)?;

        // Shorter than the tag means there is no ciphertext at all.
        if sealed.len() <= 16 {
            return Err(EncryptionError::InvalidFormat);
        }

        Ok(Self { nonce, sealed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn service() -> EncryptionService {
        EncryptionService::from_hex_key(TEST_KEY).expect("test key is valid")
    }

    #[test]
    fn round_trips_plaintext() {
        let svc = service();
        let encrypted = svc.encrypt("senha123").unwrap();
        assert_eq!(svc.decrypt(&encrypted).unwrap(), "senha123");
    }

    #[test]
    fn the_payload_announces_its_version() {
        assert!(service().encrypt("senha123").unwrap().starts_with("v1."));
    }

    #[test]
    fn the_nonce_is_twelve_bytes_and_fresh_per_call() {
        let svc = service();
        let first = svc.encrypt("same").unwrap();
        let second = svc.encrypt("same").unwrap();

        assert_ne!(first, second);

        let nonce = BASE64
            .decode(first.split('.').nth(1).expect("nonce field"))
            .expect("base64");
        assert_eq!(nonce.len(), NONCE_LENGTH);
    }

    #[test]
    fn the_cipher_key_is_not_the_configured_secret() {
        // The whole point of the KDF: the value in the environment must not be
        // usable as an AES key on its own.
        let mut raw = [0u8; KEY_LENGTH];
        hex::decode_to_slice(TEST_KEY, &mut raw).expect("hex");

        assert_ne!(service().key, raw);
    }

    #[test]
    fn derivation_is_deterministic() {
        // Two boots have to reach the same key, or every restart would orphan
        // the stored credentials.
        assert_eq!(service().key, service().key);
    }

    #[test]
    fn rejects_a_key_of_the_wrong_length() {
        assert!(EncryptionService::from_hex_key("abcd").is_err());
        assert!(EncryptionService::from_hex_key(&"z".repeat(64)).is_err());
    }

    #[test]
    fn rejects_empty_plaintext() {
        assert!(matches!(
            service().encrypt(""),
            Err(EncryptionError::EmptyPlaintext)
        ));
    }

    #[test]
    fn rejects_malformed_payloads() {
        let svc = service();
        for bad in ["", "no-dots", "v1.abc", "v1.a.b.c", "v1.!!!.!!!", "v2.a.b"] {
            assert!(svc.decrypt(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_the_previous_wire_format() {
        // `iv:tag:ciphertext`. No fallback path exists on purpose — this is the
        // test that fails if somebody adds one.
        let svc = service();
        assert!(matches!(
            svc.decrypt("AAAAAAAAAAAAAAAAAAAAAA==:AAAAAAAAAAAAAAAAAAAAAA==:AAAA"),
            Err(EncryptionError::InvalidFormat)
        ));
    }

    #[test]
    fn rejects_a_tampered_ciphertext() {
        let svc = service();
        let encrypted = svc.encrypt("senha123").unwrap();

        let mut parts: Vec<&str> = encrypted.split('.').collect();
        let mangled = {
            let data = parts[2];
            let (head, tail) = data.split_at(data.len() - 1);
            let flipped = if tail == "A" { "B" } else { "A" };
            format!("{head}{flipped}")
        };
        parts[2] = &mangled;

        assert!(matches!(
            svc.decrypt(&parts.join(".")),
            Err(EncryptionError::DecryptionFailed)
        ));
    }

    #[test]
    fn rejects_a_payload_encrypted_under_a_different_key() {
        let other = EncryptionService::from_hex_key(&"ab".repeat(32)).unwrap();
        let encrypted = other.encrypt("senha123").unwrap();

        assert!(matches!(
            service().decrypt(&encrypted),
            Err(EncryptionError::DecryptionFailed)
        ));
    }

    #[test]
    fn recognises_its_own_output_as_encrypted() {
        let encrypted = service().encrypt("senha123").unwrap();

        assert!(EncryptionService::is_encrypted(&encrypted));
        assert!(!EncryptionService::is_encrypted("senha123"));
        assert!(!EncryptionService::is_encrypted(""));
        // Right shape, wrong field sizes.
        assert!(!EncryptionService::is_encrypted("v1.AAAA.BBBB"));
    }

    #[test]
    fn debug_never_renders_the_key() {
        let rendered = format!("{:?}", service());
        assert!(!rendered.contains("key"), "got: {rendered}");
        assert!(!rendered.contains('0'), "got: {rendered}");
    }
}
