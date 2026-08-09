//! Cross-language compatibility for the opaque access token format.
//!
//! Decision D1 keeps the AdonisJS token so that sessions survive the cutover.
//! That holds only if Rust can decode and verify a token **Node** issued —
//! the property this file checks.
//!
//! The vectors in `tests/fixtures/access_token_vectors.json` came from
//! `@adonisjs/auth`'s own `AccessToken.createTransientToken`.

use back_roco::models::access_token::AccessToken;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorFile {
    prefix: String,
    hash_algorithm: String,
    vectors: Vec<Vector>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    identifier: String,
    seed_length: usize,
    seed: String,
    checksum: String,
    secret: String,
    token_value: String,
    hash: String,
}

fn load() -> VectorFile {
    let raw = include_str!("../fixtures/access_token_vectors.json");
    serde_json::from_str(raw).expect("access_token_vectors.json is valid JSON")
}

#[test]
fn fixture_declares_the_expected_format() {
    let file = load();

    assert_eq!(file.prefix, "oat_");
    assert_eq!(file.hash_algorithm, "sha256-hex");
    assert!(!file.vectors.is_empty());
}

/// The gate: a token Node issued decodes into the identifier and secret Node
/// recorded, and its secret matches the digest Node stored.
#[test]
fn decodes_and_verifies_tokens_issued_by_node() {
    let file = load();

    for vector in &file.vectors {
        let decoded = AccessToken::decode(&vector.token_value)
            .unwrap_or_else(|| panic!("failed to decode the token for id {}", vector.identifier));

        assert_eq!(decoded.identifier, vector.identifier);
        assert_eq!(decoded.secret, vector.secret);

        assert!(
            AccessToken::verify(&vector.hash, &decoded.secret),
            "secret did not verify against the stored hash for id {}",
            vector.identifier
        );
    }
}

/// The digest function has to be the same one, not merely one that agrees on
/// the recorded pairs.
#[test]
fn reproduces_the_stored_digest() {
    let file = load();

    for vector in &file.vectors {
        assert_eq!(
            AccessToken::hash_secret(&vector.secret),
            vector.hash,
            "digest mismatch for id {}",
            vector.identifier
        );
    }
}

/// A wrong secret must not verify, or the test above would pass against an
/// implementation that always returns `true`.
#[test]
fn rejects_wrong_secrets_against_node_hashes() {
    let file = load();

    for vector in &file.vectors {
        for wrong in [
            format!("{}x", vector.secret),
            vector.secret[..vector.secret.len() - 1].to_string(),
            vector.seed.clone(), // seed without the checksum
            String::new(),
        ] {
            assert!(
                !AccessToken::verify(&vector.hash, &wrong),
                "accepted a wrong secret for id {}",
                vector.identifier
            );
        }
    }
}

/// Node builds the secret as `seed || crc32(seed)` in decimal. Rust must agree
/// on the checksum, otherwise tokens it mints have a shape Node's tooling would
/// not recognise — and the CRC-32 variant (IEEE, as `crc32fast` implements) is
/// exactly the kind of detail that silently differs between libraries.
#[test]
fn agrees_with_node_on_the_seed_checksum() {
    let file = load();

    for vector in &file.vectors {
        assert_eq!(
            vector.seed.len(),
            vector.seed_length,
            "fixture is inconsistent for id {}",
            vector.identifier
        );
        assert_eq!(format!("{}{}", vector.seed, vector.checksum), vector.secret);
        assert_eq!(
            crc32fast::hash(vector.seed.as_bytes()).to_string(),
            vector.checksum,
            "CRC-32 differs from Node's for id {}",
            vector.identifier
        );
    }
}

/// The reverse direction: a token minted in Rust has to be one Node can decode
/// and verify during the shadow-traffic window (roadmap 12.13).
///
/// Asserts the structural properties Node's `decode` relies on — prefix,
/// base64url segments, `.` separator — plus that the digest round-trips.
#[test]
fn mints_tokens_in_the_format_node_accepts() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
    use base64::Engine as _;

    let file = load();

    for identifier in ["1", "42", "123456"] {
        let token = AccessToken::generate(identifier).unwrap();

        let body = token
            .value
            .strip_prefix(&file.prefix)
            .expect("value carries the oat_ prefix");
        let (id_b64, secret_b64) = body.split_once('.').expect("value has one separator");

        assert_eq!(
            String::from_utf8(B64URL.decode(id_b64).unwrap()).unwrap(),
            identifier
        );

        let secret = String::from_utf8(B64URL.decode(secret_b64).unwrap()).unwrap();
        assert_eq!(AccessToken::hash_secret(&secret), token.hash);
        assert!(AccessToken::verify(&token.hash, &secret));
    }
}
