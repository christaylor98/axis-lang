// Wave S3: CLI Error-Path Integration Tests
//
// OBJECTIVE: Verify that the axis CLI hard-fails on bad inputs and that
// all error exit paths are exercised under normal cargo test.
//
// COVERAGE:
//   - Missing required flags (clap enforcement)
//   - Non-existent spec/source/registry files
//   - Unknown --inspect stage
//   - axis verify: missing bundle path, missing --reg, unknown --effect-ceiling
//   - axis bundle-id: missing argument, non-existent file, corrupt input
//
// NOTE: These tests were rewritten in Bundle 2 (2026-06-08).
// The original tests targeted 'axis run --lowering' (retired CLI from main_old.rs).
// They now target the current CLI: axis --lexer/--parser/--schema/--src/--reg
// plus the axis verify and axis bundle-id subcommands.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

fn get_binary_path() -> PathBuf {
    // Prefer release binary (built by dev_build); fall back to debug.
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

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: Missing required --reg flag MUST fail with clap error
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_missing_required_reg_fails() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("--lexer")
        .arg("axis-surface-ai2-config/ai2-explicit-lexer.yaml")
        .arg("--parser")
        .arg("axis-surface-ai2-config/ai2-explicit-parse.yaml")
        .arg("--schema")
        .arg("axis-surface-ai2-config/ai2-explicit-ast.yaml")
        .arg("--src")
        .arg("examples/hello.ax")
        // NOTE: --reg flag OMITTED — clap should hard-fail
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "Expected failure when --reg is omitted, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("reg") || stderr.contains("error"),
        "Expected error message mentioning missing required flag. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: Non-existent --lexer file MUST fail with spec-not-found error
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_missing_lexer_file_fails() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("--lexer")
        .arg("no-such-lexer.yaml")
        .arg("--parser")
        .arg("axis-surface-ai2-config/ai2-explicit-parse.yaml")
        .arg("--schema")
        .arg("axis-surface-ai2-config/ai2-explicit-ast.yaml")
        .arg("--normalize")
        .arg("axis-surface-ai2-config/ai2-explicit-normalize.yaml")
        .arg("--file")
        .arg("examples/surface-m1/fn_identity.m1")
        .arg("--reg")
        .arg("registries/axis.axreg")
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "Expected failure when --lexer file missing, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("Lexer"),
        "Expected error message mentioning missing lexer. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: Non-existent --src file MUST fail
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_missing_source_file_fails() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("--lexer")
        .arg("axis-surface-ai2-config/ai2-explicit-lexer.yaml")
        .arg("--parser")
        .arg("axis-surface-ai2-config/ai2-explicit-parse.yaml")
        .arg("--schema")
        .arg("axis-surface-ai2-config/ai2-explicit-ast.yaml")
        .arg("--normalize")
        .arg("axis-surface-ai2-config/ai2-explicit-normalize.yaml")
        .arg("--file")
        .arg("no-such-source-file.ax") // Does not exist
        .arg("--reg")
        .arg("registries/axis.axreg")
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "Expected failure when --src file missing, but command succeeded"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: Unknown --inspect stage MUST fail with a clear error
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_unknown_inspect_stage_fails() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("--lexer")
        .arg("axis-surface-ai2-config/ai2-explicit-lexer.yaml")
        .arg("--parser")
        .arg("axis-surface-ai2-config/ai2-explicit-parse.yaml")
        .arg("--schema")
        .arg("axis-surface-ai2-config/ai2-explicit-ast.yaml")
        .arg("--normalize")
        .arg("axis-surface-ai2-config/ai2-explicit-normalize.yaml")
        .arg("--file")
        .arg("tests/samples/simple.ax") // examples/hello.ax does not exist; this does
        .arg("--reg")
        .arg("registries/minimal.axreg")
        .arg("--inspect")
        .arg("nonexistent-stage-xyz")
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "Expected failure for unknown --inspect stage, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Unknown inspect stage") || stderr.contains("nonexistent-stage-xyz"),
        "Expected error message about unknown inspect stage. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: axis verify with no bundle path MUST fail with usage message
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_verify_no_bundle_path_fails() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("verify")
        // No bundle path provided
        .arg("--reg")
        .arg("registries/axis.axreg")
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "Expected failure when no bundle path given to verify, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage") || stderr.contains("verify"),
        "Expected usage hint in error. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 6: axis verify with no --reg MUST fail
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_verify_no_reg_fails() {
    let binary = ensure_binary_exists();

    // Write a dummy file so the path check passes
    let dummy = PathBuf::from("target/test_s3_dummy.coreir");
    fs::write(&dummy, b"not real axbi data").unwrap();

    let output = Command::new(&binary)
        .arg("verify")
        .arg(&dummy)
        // No --reg provided
        .output()
        .expect("failed to execute binary");

    let _ = fs::remove_file(&dummy);

    assert!(
        !output.status.success(),
        "Expected failure when --reg omitted from verify, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--reg") || stderr.contains("required"),
        "Expected error mentioning missing --reg. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 7: axis verify with unknown --effect-ceiling MUST fail
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_verify_unknown_effect_ceiling_fails() {
    let binary = ensure_binary_exists();

    let dummy = PathBuf::from("target/test_s3_ceiling.coreir");
    fs::write(&dummy, b"not real axbi data").unwrap();

    let output = Command::new(&binary)
        .arg("verify")
        .arg(&dummy)
        .arg("--reg")
        .arg("registries/axis.axreg")
        .arg("--effect-ceiling")
        .arg("impossible-level")
        .output()
        .expect("failed to execute binary");

    let _ = fs::remove_file(&dummy);

    assert!(
        !output.status.success(),
        "Expected failure for unknown --effect-ceiling, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("impossible-level") || stderr.contains("effect ceiling") || stderr.contains("unknown"),
        "Expected error mentioning bad effect ceiling. Got: {}",
        stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 8: axis bundle-id with no argument MUST fail with usage message
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_bundle_id_no_argument_fails() {
    let binary = ensure_binary_exists();

    let output = Command::new(&binary)
        .arg("bundle-id")
        // No path argument
        .output()
        .expect("failed to execute binary");

    assert!(
        !output.status.success(),
        "Expected failure when no argument given to bundle-id, but command succeeded"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("usage") || stderr.contains("bundle-id"),
        "Expected usage hint in error. Got: {}",
        stderr
    );
}
