//! Authenticated encryption for stored credentials.
//!
//! This is a byte-for-byte port of the AdonisJS `EncryptionService`
//! (`backend/app/services/encryption_service.ts`). The stored ciphertexts in
//! `connections.password_encrypted` and `storage_destinations.config_encrypted`
//! were produced by that implementation and must stay readable after the
//! migration — the format is a compatibility contract, not a design choice.
//!
//! Wire format: `base64(iv):base64(auth_tag):base64(ciphertext)`
//!
//! Two details of the original diverge from the usual AES-GCM defaults and are
//! the reason this cannot use the stock `Aes256Gcm` alias:
//!
//! * The IV is **16 bytes**, not the 12-byte nonce that `Aes256Gcm` hardcodes.
//! * `DB_ENCRYPTION_KEY` (64 hex chars) is used **directly** as the 32-byte key.
//!   There is no KDF, no salt, no stretching.
//!
//! It lives under `models/` because the two models that own encrypted columns
//! are its only callers, and their `ActiveModelBehavior` hooks are where
//! encryption happens.

use aes::Aes256;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{AesGcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use typenum::U16;

/// AES-256-GCM with a 16-byte IV and a 16-byte auth tag.
///
/// `Aes256Gcm` is `AesGcm<Aes256, U12>`; the Node implementation passes a
/// 16-byte IV to `createCipheriv`, which GCM accepts by hashing it through
/// GHASH instead of using it as a counter block directly. Using U12 here would
/// silently fail to decrypt every existing row.
type Aes256GcmIv16 = AesGcm<Aes256, U16>;

/// Length of the IV, in bytes. Matches `IV_LENGTH` in the TypeScript original.
const IV_LENGTH: usize = 16;

/// Length of the GCM authentication tag, in bytes.
const AUTH_TAG_LENGTH: usize = 16;

/// Length of the raw key, in bytes (AES-256).
const KEY_LENGTH: usize = 32;

/// Failure modes of the encrypt/decrypt round trip.
///
/// Deliberately opaque about *why* a decryption failed: distinguishing "wrong
/// key" from "tampered ciphertext" leaks an oracle, and neither is actionable
/// to a caller.
#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("DB_ENCRYPTION_KEY must be 64 hexadecimal characters (32 bytes)")]
    InvalidKey,

    #[error("invalid encrypted payload format, expected iv:authTag:data")]
    InvalidFormat,

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("plaintext for encryption must not be empty")]
    EmptyPlaintext,

    #[error("decrypted payload is not valid UTF-8")]
    InvalidUtf8,
}

/// Encrypts and decrypts credential blobs with a caller-supplied key.
///
/// The key is held as raw bytes for the lifetime of the value. Callers should
/// build one per application boot and share it through `AppContext`, rather
/// than re-deriving it per request — `Buffer.from(hex)` per call was measurable
/// enough in the Node original to warrant memoization there.
#[derive(Clone)]
pub struct EncryptionService {
    key: [u8; KEY_LENGTH],
}

impl std::fmt::Debug for EncryptionService {
    /// Never renders the key, so an accidental `{:?}` in a log or an error
    /// chain cannot exfiltrate it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionService").finish_non_exhaustive()
    }
}

impl EncryptionService {
    /// Builds a service from the 64-character hex key used by `DB_ENCRYPTION_KEY`.
    pub fn from_hex_key(key_hex: &str) -> Result<Self, EncryptionError> {
        if key_hex.len() != KEY_LENGTH * 2 {
            return Err(EncryptionError::InvalidKey);
        }

        let mut key = [0u8; KEY_LENGTH];
        hex::decode_to_slice(key_hex, &mut key).map_err(|_| EncryptionError::InvalidKey)?;

        Ok(Self { key })
    }

    /// Encrypts `plaintext`, returning `base64(iv):base64(tag):base64(ciphertext)`.
    ///
    /// The IV is freshly random per call. Rejecting empty input mirrors the
    /// original, which threw rather than storing a ciphertext that decrypts to
    /// nothing — an empty stored credential is always a bug upstream.
    pub fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        if plaintext.is_empty() {
            return Err(EncryptionError::EmptyPlaintext);
        }

        let mut iv = [0u8; IV_LENGTH];
        getrandom::fill(&mut iv).map_err(|_| EncryptionError::DecryptionFailed)?;

        self.encrypt_with_iv(plaintext, &iv)
    }

    /// Encrypts with a caller-chosen IV.
    ///
    /// Only for reproducing the recorded cross-language test vectors. Reusing an
    /// IV with the same key breaks GCM outright, so this stays private.
    fn encrypt_with_iv(
        &self,
        plaintext: &str,
        iv: &[u8; IV_LENGTH],
    ) -> Result<String, EncryptionError> {
        let cipher = self.cipher();
        let nonce = Nonce::<U16>::from(*iv);

        let sealed = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: &[],
                },
            )
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        // `aead` appends the tag to the ciphertext; the Node format keeps them
        // in separate fields, so split it back out.
        let split = sealed
            .len()
            .checked_sub(AUTH_TAG_LENGTH)
            .ok_or(EncryptionError::DecryptionFailed)?;
        let (ciphertext, tag) = sealed.split_at(split);

        Ok(format!(
            "{}:{}:{}",
            BASE64.encode(iv),
            BASE64.encode(tag),
            BASE64.encode(ciphertext)
        ))
    }

    /// Decrypts a payload produced by either this service or the Node original.
    pub fn decrypt(&self, encrypted: &str) -> Result<String, EncryptionError> {
        let payload = Payload_::parse(encrypted)?;

        // The tag trails the ciphertext in the `aead` API's buffer layout.
        let mut sealed = payload.ciphertext;
        sealed.extend_from_slice(&payload.tag);

        let cipher = self.cipher();
        let nonce = Nonce::<U16>::from(payload.iv);

        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &sealed,
                    aad: &[],
                },
            )
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        String::from_utf8(plaintext).map_err(|_| EncryptionError::InvalidUtf8)
    }

    /// Reports whether `text` is shaped like one of our payloads.
    ///
    /// Used by the model hooks to avoid double-encrypting a value that was
    /// already encrypted — the same guard `isEncrypted` provides in the
    /// original. It is a shape check, not an authenticity check.
    pub fn is_encrypted(text: &str) -> bool {
        Payload_::parse(text).is_ok()
    }

    fn cipher(&self) -> Aes256GcmIv16 {
        Aes256GcmIv16::new(&Key::<Aes256GcmIv16>::from(self.key))
    }
}

/// The three fields of the wire format, decoded.
///
/// Named `Payload_` because `aead::Payload` is already in scope and means
/// something else entirely — the plaintext/AAD pair handed to the cipher.
struct Payload_ {
    iv: [u8; IV_LENGTH],
    tag: [u8; AUTH_TAG_LENGTH],
    ciphertext: Vec<u8>,
}

impl Payload_ {
    /// Splits `iv:tag:data` and validates the two fixed-size fields.
    ///
    /// Rejects a fourth field rather than ignoring it: base64 never emits `:`,
    /// so an extra separator means the value is not one of ours.
    fn parse(encrypted: &str) -> Result<Self, EncryptionError> {
        let mut parts = encrypted.split(':');
        let (Some(iv_b64), Some(tag_b64), Some(data_b64), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(EncryptionError::InvalidFormat);
        };

        let iv: [u8; IV_LENGTH] = BASE64
            .decode(iv_b64)
            .map_err(|_| EncryptionError::InvalidFormat)?
            .try_into()
            .map_err(|_| EncryptionError::InvalidFormat)?;

        let tag: [u8; AUTH_TAG_LENGTH] = BASE64
            .decode(tag_b64)
            .map_err(|_| EncryptionError::InvalidFormat)?
            .try_into()
            .map_err(|_| EncryptionError::InvalidFormat)?;

        let ciphertext = BASE64
            .decode(data_b64)
            .map_err(|_| EncryptionError::InvalidFormat)?;

        Ok(Self {
            iv,
            tag,
            ciphertext,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same fixed key used to generate `tests/fixtures/encryption_vectors.json`.
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
    fn produces_a_fresh_iv_per_call() {
        let svc = service();
        assert_ne!(svc.encrypt("same").unwrap(), svc.encrypt("same").unwrap());
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
        for bad in ["", "no-colons", "a:b", "a:b:c:d", "!!!:!!!:!!!"] {
            assert!(svc.decrypt(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_a_tampered_ciphertext() {
        let svc = service();
        let encrypted = svc.encrypt("senha123").unwrap();

        // Flip the last base64 character of the ciphertext field.
        let mut parts: Vec<&str> = encrypted.split(':').collect();
        let mangled = {
            let data = parts[2];
            let (head, tail) = data.split_at(data.len() - 1);
            let flipped = if tail == "A" { "B" } else { "A" };
            format!("{head}{flipped}")
        };
        parts[2] = &mangled;

        assert!(matches!(
            svc.decrypt(&parts.join(":")),
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
        assert!(!EncryptionService::is_encrypted("AAAA:BBBB:CCCC"));
    }

    #[test]
    fn debug_never_renders_the_key() {
        let rendered = format!("{:?}", service());
        assert!(!rendered.contains("key"), "got: {rendered}");
        assert!(!rendered.contains('0'), "got: {rendered}");
    }
}
