// Wave S4: bundle-id and verify Happy-Path Integration Tests
//
// OBJECTIVE: Prove that `axis bundle-id` and `axis verify` work correctly
// on valid Core IR 0.5 bundles. Complements the error-path coverage in
// wave_s3_spec_authority.rs (which already tests both subcommands for failure
// modes: no args, corrupt input, missing --reg, etc.).
//
// COVERAGE:
//   axis bundle-id:
//     - valid .coreir → exit 0, stdout is a 64-char lowercase hex identity hash
//     - corrupt bytes (non-axbi) → exit 3
//     - non-existent file path → exit 1
//
//   axis verify:
//     - valid .coreir + minimal registry → exit 0, stdout is JSON with effect_class
//
// FIXTURES:
//   build/ai2/program.coreir  — compiled Core IR 0.5 bundle (tracked in git)
//   registries/minimal.axreg  — minimal scope registry (tracked in git)
//
// Written in Bundle 5 (2026-06-08).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn get_binary_path() -> PathBuf {
    let release = PathBuf::from("target/release/axis");
    if release.exists() {
        return release;
    }
    PathBuf::from("target/debug/axis")
}

fn ensure_binary_exists() -> PathBuf {
    let binary = get_binary_path();
    assert!(
        binary.exists(),
        "axis binary not found at {:?}. Run: cargo build --release",
        binary
    );
    binary
}

/// Path to the committed Core IR 0.5 fixture.
fn valid_bundle() -> PathBuf {
    PathBuf::from("build/ai2/program.coreir")
}

/// Path to the minimal committed registry.
fn minimal_reg() -> PathBuf {
    PathBuf::from("registries/minimal.axreg")
}

// ═══════════════════════════════════════════════════════════════════════════
// bundle-id: happy path
// ═══════════════════════════════════════════════════════════════════════════

/// `axis bundle-id <valid.coreir>` exits 0 and prints a 64-char hex hash.
#[test]
fn test_bundle_id_valid_coreir_exits_zero() {
    let binary = ensure_binary_exists();
    let bundle = valid_bundle();

    assert!(
        bundle.exists(),
        "Fixture {:?} not found — was `cargo build` run?",
        bundle
    );

    let output = Command::new(&binary)
        .arg("bundle-id")
        .arg(&bundle)
        .output()
        .expect("failed to execute axis binary");

    assert!(
        output.status.success(),
        "axis bundle-id on valid .coreir should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The identity hash printed to stdout must be exactly 64 lowercase hex chars.
#[test]
fn test_bundle_id_valid_coreir_output_is_hex() {
    let binary = ensure_binary_exists();
    let bundle = valid_bundle();

    assert!(bundle.exists(), "Fixture {:?} not found", bundle);

    let output = Command::new(&binary)
        .arg("bundle-id")
        .arg(&bundle)
        .output()
        .expect("failed to execute axis binary");

    assert!(output.status.success(), "Expected exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let hash = stdout.trim();

    assert_eq!(
        hash.len(),
        64,
        "Identity hash must be exactly 64 chars. Got {:?} (len={})",
        hash,
        hash.len()
    );
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "Identity hash must be lowercase hex. Got: {:?}",
        hash
    );
}

/// The same bundle produces the same identity hash on repeated calls
/// (determinism invariant).
#[test]
fn test_bundle_id_is_deterministic() {
    let binary = ensure_binary_exists();
    let bundle = valid_bundle();

    assert!(bundle.exists(), "Fixture {:?} not found", bundle);

    let run = |b: &PathBuf| {
        let out = Command::new(&binary)
            .arg("bundle-id")
            .arg(b)
            .output()
            .expect("failed to execute axis binary");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let first = run(&bundle);
    let second = run(&bundle);

    assert_eq!(
        first, second,
        "axis bundle-id must be deterministic: got different hashes on repeated calls"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// bundle-id: error paths (complement to wave_s3 test 8)
// ═══════════════════════════════════════════════════════════════════════════

/// Non-capnp binary data must exit 3 (format-mismatch).
#[test]
fn test_bundle_id_corrupt_file_exits_3() {
    let binary = ensure_binary_exists();

    // Write junk bytes that are not a valid Cap'n Proto message.
    let corrupt = PathBuf::from("target/s4_corrupt.coreir");
    fs::write(&corrupt, b"this is not an axbi bundle").expect("write failed");

    let output = Command::new(&binary)
        .arg("bundle-id")
        .arg(&corrupt)
        .output()
        .expect("failed to execute axis binary");

    let _ = fs::remove_file(&corrupt);

    assert_eq!(
        output.status.code(),
        Some(3),
        "Corrupt (non-axbi) input must exit 3. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Non-existent file path must exit 1.
#[test]
fn test_bundle_id_missing_file_exits_1() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("bundle-id")
        .arg("no-such-file-xyz-abc.coreir")
        .output()
        .expect("failed to execute axis binary");

    assert_eq!(
        output.status.code(),
        Some(1),
        "Missing file must exit 1. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot read"),
        "Expected 'cannot read' in stderr. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// verify: happy path
// ═══════════════════════════════════════════════════════════════════════════

/// `axis verify <valid.coreir> --reg <minimal.axreg>` exits 0 and emits a
/// JSON object with an "effect_class" key to stdout.
#[test]
fn test_verify_valid_bundle_exits_zero() {
    let binary = ensure_binary_exists();
    let bundle = valid_bundle();
    let reg = minimal_reg();

    assert!(bundle.exists(), "Fixture {:?} not found", bundle);
    assert!(reg.exists(), "Fixture {:?} not found", reg);

    let output = Command::new(&binary)
        .arg("verify")
        .arg(&bundle)
        .arg("--reg")
        .arg(&reg)
        .output()
        .expect("failed to execute axis binary");

    assert!(
        output.status.success(),
        "axis verify on valid bundle+registry should exit 0. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The JSON output from a passing verify must include "effect_class".
#[test]
fn test_verify_output_contains_effect_class() {
    let binary = ensure_binary_exists();
    let bundle = valid_bundle();
    let reg = minimal_reg();

    assert!(bundle.exists(), "Fixture {:?} not found", bundle);
    assert!(reg.exists(), "Fixture {:?} not found", reg);

    let output = Command::new(&binary)
        .arg("verify")
        .arg(&bundle)
        .arg("--reg")
        .arg(&reg)
        .output()
        .expect("failed to execute axis binary");

    assert!(output.status.success(), "Expected exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("effect_class"),
        "verify stdout must contain 'effect_class'. Got: {}",
        stdout
    );
}
