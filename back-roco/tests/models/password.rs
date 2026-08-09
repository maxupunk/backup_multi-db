//! Cross-language compatibility for the password hash format.
//!
//! Decision D2 keeps the AdonisJS scrypt hasher so that existing users log in
//! after the migration with the password they already have. That only holds if
//! Rust can verify a hash **Node** wrote — the property this file checks.
//!
//! The vectors in `tests/fixtures/scrypt_vectors.json` were produced by
//! `@adonisjs/hash`'s own scrypt driver, configured exactly as
//! `backend/config/hash.ts` configures it.
//!
//! These run at the production cost factor (N=16384), so each derivation costs
//! real time. That is the point: a cheaper factor would not exercise the
//! parameters the stored hashes actually use.

use back_roco::models::password::PasswordHasher;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorFile {
    cost: u32,
    block_size: u32,
    parallelization: u32,
    salt_size: usize,
    key_length: usize,
    vectors: Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
    password: String,
    phc: String,
}

fn load() -> VectorFile {
    let raw = include_str!("../fixtures/scrypt_vectors.json");
    serde_json::from_str(raw).expect("scrypt_vectors.json is valid JSON")
}

/// The fixture must describe the configuration in `backend/config/hash.ts`. A
/// fixture regenerated against different settings would pass while proving
/// nothing about the stored hashes.
#[test]
fn fixture_declares_the_production_parameters() {
    let file = load();

    assert_eq!(file.cost, 16_384);
    assert_eq!(file.block_size, 8);
    assert_eq!(file.parallelization, 1);
    assert_eq!(file.salt_size, 16);
    assert_eq!(file.key_length, 64);
    assert!(!file.vectors.is_empty());
}

/// The gate: every hash Node wrote verifies in Rust against its own password.
#[test]
fn verifies_every_hash_produced_by_node() {
    let file = load();
    let hasher = PasswordHasher::default();

    for vector in &file.vectors {
        assert!(
            hasher.verify(&vector.phc, &vector.password),
            "failed to verify the hash for {:?}",
            vector.password
        );
    }
}

/// A wrong password must not verify — otherwise the test above would pass with
/// an implementation that returns `true` unconditionally.
#[test]
fn rejects_wrong_passwords_against_node_hashes() {
    let file = load();
    let hasher = PasswordHasher::default();

    for vector in &file.vectors {
        for wrong in [
            format!("{}x", vector.password),
            vector.password.to_uppercase(),
            String::new(),
        ] {
            if wrong == vector.password {
                continue; // uppercase of a non-alphabetic password is itself
            }

            assert!(
                !hasher.verify(&vector.phc, &wrong),
                "accepted {wrong:?} for the hash of {:?}",
                vector.password
            );
        }
    }
}

/// Node's hashes are already at the configured cost, so nothing should be
/// flagged for rehashing at the moment of the cutover.
#[test]
fn does_not_flag_node_hashes_for_rehash() {
    let file = load();
    let hasher = PasswordHasher::default();

    for vector in &file.vectors {
        assert!(
            !hasher.needs_rehash(&vector.phc),
            "would rehash {:?} unnecessarily",
            vector.password
        );
        assert!(PasswordHasher::is_valid_hash(&vector.phc));
    }
}

/// Proves the KDF matches on the **exact salt and parameters** of the real
/// stored hash, without ever knowing the real password.
///
/// The recorded vectors use freshly generated salts. This derives from the salt
/// that is actually in `users.password` in production, using an arbitrary probe
/// password, and compares against what Node derived from the same two inputs.
/// If the two agree, the real hash would verify for the real password.
///
/// Ignored by default: it needs values pulled from the production database.
/// Generate the inputs from `backend/` with the snippet in the roadmap (task 3.3).
///
/// ```sh
/// export ROCO_PROBE_SALT=...      # field 4 of the stored PHC string
/// export ROCO_PROBE_EXPECTED=...  # what Node derives for ROCO_PROBE_PASSWORD
/// export ROCO_PROBE_PASSWORD=probe-cross-language-2026
/// cargo test --test mod verifies_against_the_real_production_salt -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs values pulled from the production database; see the doc comment"]
fn verifies_against_the_real_production_salt() {
    let salt = std::env::var("ROCO_PROBE_SALT").expect("set ROCO_PROBE_SALT");
    let expected = std::env::var("ROCO_PROBE_EXPECTED").expect("set ROCO_PROBE_EXPECTED");
    let password = std::env::var("ROCO_PROBE_PASSWORD").expect("set ROCO_PROBE_PASSWORD");

    // Reassemble a PHC string from the production salt and Node's derivation, so
    // the check runs through the same `verify` path the login flow uses.
    let phc = format!("$scrypt$n=16384,r=8,p=1${salt}${expected}");

    assert!(
        PasswordHasher::is_valid_hash(&phc),
        "the production salt/params do not parse"
    );
    assert!(
        PasswordHasher::default().verify(&phc, &password),
        "Rust derived a different key than Node from the same salt and parameters"
    );

    println!("scrypt matches Node on the production salt and parameters");
}

/// The reverse direction, for the shadow-traffic window (roadmap 12.13) when a
/// password changed through one backend has to verify in the other.
///
/// Rust cannot invoke Node here, so it asserts what makes that possible: its own
/// output carries the same PHC id, parameter set and field sizes Node parses.
/// The end-to-end check runs from `backend/` — see the roadmap, task 3.3.
#[test]
fn emits_hashes_in_the_format_node_parses() {
    use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
    use base64::Engine as _;

    let file = load();
    let hasher = PasswordHasher::default();

    let phc = hasher.hash("senha-de-ida-e-volta").unwrap();
    let fields: Vec<&str> = phc.split('$').collect();

    assert_eq!(fields.len(), 5, "expected $scrypt$params$salt$hash");
    assert_eq!(fields[1], "scrypt");
    assert_eq!(
        fields[2],
        format!(
            "n={},r={},p={}",
            file.cost, file.block_size, file.parallelization
        )
    );
    assert_eq!(B64.decode(fields[3]).unwrap().len(), file.salt_size);
    assert_eq!(B64.decode(fields[4]).unwrap().len(), file.key_length);

    assert!(hasher.verify(&phc, "senha-de-ida-e-volta"));
}
