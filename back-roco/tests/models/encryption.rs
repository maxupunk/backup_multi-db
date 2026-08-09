//! Cross-language compatibility for the credential encryption format.
//!
//! The unit tests inside `src/models/encryption.rs` prove the Rust
//! implementation is self-consistent. That is not the property that matters
//! here: the migration only works if Rust can read what **Node** wrote.
//!
//! The vectors in `tests/fixtures/encryption_vectors.json` were produced by the
//! AdonisJS `EncryptionService` itself. If this file fails, decision D3 in the
//! roadmap is broken and every stored credential is unreadable — it is the
//! go/no-go gate for the whole port, not a nice-to-have.
//!
//! Regenerate the fixture from `backend/` with:
//!
//! ```sh
//! node -e "…"  # see ROADMAP_BACK_ROCO.md, task 3.2
//! ```

use back_roco::models::encryption::EncryptionService;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorFile {
    key_hex: String,
    iv_length: usize,
    auth_tag_length: usize,
    vectors: Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
    name: String,
    plaintext: String,
    encrypted: String,
}

fn load() -> VectorFile {
    let raw = include_str!("../fixtures/encryption_vectors.json");
    serde_json::from_str(raw).expect("encryption_vectors.json is valid JSON")
}

/// The recorded parameters must stay what the port was built against. A fixture
/// regenerated with different sizes would otherwise pass silently while
/// describing a format the production rows do not use.
#[test]
fn fixture_declares_the_expected_parameters() {
    let file = load();

    assert_eq!(file.iv_length, 16, "the Node original uses a 16-byte IV");
    assert_eq!(file.auth_tag_length, 16);
    assert_eq!(file.key_hex.len(), 64);
    assert!(!file.vectors.is_empty());
}

/// The gate: every ciphertext produced by Node decrypts to its original
/// plaintext in Rust.
#[test]
fn decrypts_every_payload_produced_by_node() {
    let file = load();
    let svc = EncryptionService::from_hex_key(&file.key_hex).expect("fixture key is valid");

    for vector in &file.vectors {
        let decrypted = svc
            .decrypt(&vector.encrypted)
            .unwrap_or_else(|e| panic!("vector {:?} failed to decrypt: {e}", vector.name));

        assert_eq!(
            decrypted, vector.plaintext,
            "vector {:?} decrypted to the wrong plaintext",
            vector.name
        );
    }
}

/// The reverse direction. Node has to keep reading what Rust writes for as long
/// as both run side by side during the shadow-traffic window (roadmap 12.13).
///
/// Rust cannot verify Node's side on its own, so this asserts the property that
/// makes it possible: a Rust ciphertext round-trips through the exact wire
/// format Node parses — three colon-separated base64 fields, with a 16-byte IV
/// and a 16-byte tag.
#[test]
fn emits_the_wire_format_node_can_parse() {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;

    let file = load();
    let svc = EncryptionService::from_hex_key(&file.key_hex).unwrap();

    for vector in &file.vectors {
        let encrypted = svc.encrypt(&vector.plaintext).unwrap();
        let parts: Vec<&str> = encrypted.split(':').collect();

        assert_eq!(
            parts.len(),
            3,
            "vector {:?}: expected iv:tag:data",
            vector.name
        );
        assert_eq!(BASE64.decode(parts[0]).unwrap().len(), file.iv_length);
        assert_eq!(BASE64.decode(parts[1]).unwrap().len(), file.auth_tag_length);
        assert!(BASE64.decode(parts[2]).is_ok());

        assert_eq!(svc.decrypt(&encrypted).unwrap(), vector.plaintext);
    }
}

/// A payload cannot be read with the wrong key, no matter which side wrote it.
#[test]
fn rejects_node_payloads_under_a_different_key() {
    let file = load();
    let wrong = EncryptionService::from_hex_key(&"7f".repeat(32)).unwrap();

    for vector in &file.vectors {
        assert!(
            wrong.decrypt(&vector.encrypted).is_err(),
            "vector {:?} decrypted under the wrong key",
            vector.name
        );
    }
}

/// Decrypts the **real** ciphertexts from the running AdonisJS database.
///
/// The recorded vectors prove the algorithm matches. This proves the algorithm
/// matches *the rows that actually have to survive the migration* — the fixture
/// could have been generated with parameters that drifted from production.
///
/// Ignored by default because it needs the production key and database, neither
/// of which is versioned. Run it before signing off on Fase 4 (data migration):
///
/// ```sh
/// export $(grep DB_ENCRYPTION_KEY ../.env | xargs)
/// export ROCO_REAL_CIPHERTEXTS="$(sqlite3 ../backend/storage/database/app.sqlite3 \
///   'SELECT password_encrypted FROM connections UNION ALL SELECT config_encrypted FROM storage_destinations;')"
/// cargo test --test mod decrypts_real_production_rows -- --ignored --nocapture
/// ```
///
/// Never asserts on, prints, or otherwise surfaces the plaintext — these are
/// live database credentials.
#[test]
#[ignore = "needs the production key and database; see the doc comment"]
fn decrypts_real_production_rows() {
    let Ok(key) = std::env::var("DB_ENCRYPTION_KEY") else {
        panic!("set DB_ENCRYPTION_KEY (see the doc comment on this test)");
    };
    let Ok(rows) = std::env::var("ROCO_REAL_CIPHERTEXTS") else {
        panic!("set ROCO_REAL_CIPHERTEXTS (see the doc comment on this test)");
    };

    let svc = EncryptionService::from_hex_key(&key).expect("production key is 64 hex chars");

    let rows: Vec<&str> = rows
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert!(!rows.is_empty(), "no ciphertexts supplied");

    for (i, encrypted) in rows.iter().enumerate() {
        assert!(
            EncryptionService::is_encrypted(encrypted),
            "row {i} is not in the expected wire format"
        );

        let plaintext = svc
            .decrypt(encrypted)
            .unwrap_or_else(|e| panic!("row {i} failed to decrypt: {e}"));

        assert!(
            !plaintext.is_empty(),
            "row {i} decrypted to an empty string"
        );

        // Length only — the plaintext is a live credential.
        println!("row {i}: ok ({} bytes)", plaintext.len());
    }

    println!("{} production ciphertext(s) decrypted", rows.len());
}
