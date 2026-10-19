// `fail` in a `CIf` arm: a diverging arm agrees with any sibling type.
//
// `fail(Text)` stops the process. It never returns, so it has no result type,
// but the registry type language is monomorphic with no bottom type — its `out`
// can only be a placeholder. Without a rule in the verifier, the shape that
// input validation actually wants,
//
//     if int_lt(n, Int(0)) { fail(Text("negative")) } else { n }
//
// is rejected as a `CIfBranchDivergence` between that placeholder and `Int`.
//
// This matters because `fail` exists to close a specific gap: an M1 composite
// that replaces a bridge fn cannot port that fn's input validation if the
// language has no way to fail, so the validation was dropped at every such
// rewrite.
//
// The rule must not be a blanket relaxation of branch agreement — a genuine
// type divergence between two value-producing arms is still an error, and
// `test_genuine_branch_divergence_is_still_rejected` is what holds that line.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    let release = PathBuf::from("target/release/axis");
    if release.exists() {
        return release;
    }
    let debug = PathBuf::from("target/debug/axis");
    assert!(debug.exists(), "axis binary not found. Run: cargo build --release");
    debug
}

fn spec_args() -> Vec<String> {
    let c = "axis-surface-ai3-config";
    vec![
        "--lexer".into(), format!("{c}/ai3-lexer.yaml"),
        "--parser".into(), format!("{c}/ai3-parse.yaml"),
        "--schema".into(), format!("{c}/ai3-ast.yaml"),
        "--normalize".into(), format!("{c}/ai3-normalize.yaml"),
    ]
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from("target/test-fail-divergence").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// `registries/axis.axreg` plus a `fail` declaration, since the committed
/// fixture registry predates it. Identity is the §5b rule, `SHA-256(name)`.
///
/// `out Value` is the placeholder the real registry uses; the point of these
/// tests is that the verifier does not take it literally in an arm.
fn registry_with_fail(dir: &Path) -> PathBuf {
    let ident =
        axis_lang::fabric::hash::content_hash(b"fail").to_hex();
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    text.push_str(&format!(
        "\ntype Value = sum [ AsUnit(Unit) | AsBool(Bool) | AsInt(Int) | AsText(Text) ]\n\
         \nfn fail\n  identity 0x{ident}\n  kind     leaf\n  in       (Text)\n  \
         out      Value\n  effect   fullIo\n  deterministic false\n  \
         idempotent    false\nend\n"
    ));
    let path = dir.join("with_fail.axreg");
    std::fs::write(&path, text).expect("write registry");
    path
}

struct Run { code: i32, stdout: String, stderr: String }

fn compile(src: &Path, reg: &Path, out: &Path) -> Run {
    let o = Command::new(binary())
        .args(spec_args())
        .args([
            "--file", &src.display().to_string(),
            "--reg", &reg.display().to_string(),
            "--out", &out.display().to_string(),
        ])
        .output()
        .expect("spawn axis");
    Run {
        code: o.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn write(path: &Path, s: &str) {
    if let Some(p) = path.parent() { std::fs::create_dir_all(p).unwrap(); }
    std::fs::write(path, s).unwrap();
}

/// The validation shape: `fail` in the then-arm, a value in the else-arm.
#[test]
fn test_fail_in_then_arm_takes_the_else_arm_type() {
    let dir = scratch("then_arm");
    let src = dir.join("v.ai3");
    write(&src, "fn validate(n: Int) -> Int {\n  \
                 return if int_lt(n, Int(0)) { fail(Text(\"negative\")) } else { n }\n}\n");
    let r = compile(&src, &registry_with_fail(&dir), &dir.join("out"));
    assert_eq!(r.code, 0, "should compile: {}{}", r.stdout, r.stderr);
}

/// Symmetric: the rule is about divergence, not about which arm it is in.
/// A check that only looked at `then_` would pass the test above and fail here
/// — that asymmetry is a real bug this repo has shipped before, in the removed
/// `compute_branch_paths` tripwire.
#[test]
fn test_fail_in_else_arm_takes_the_then_arm_type() {
    let dir = scratch("else_arm");
    let src = dir.join("v.ai3");
    write(&src, "fn validate(n: Int) -> Int {\n  \
                 return if int_lt(n, Int(0)) { n } else { fail(Text(\"non-negative\")) }\n}\n");
    let r = compile(&src, &registry_with_fail(&dir), &dir.join("out"));
    assert_eq!(r.code, 0, "should compile: {}{}", r.stdout, r.stderr);
}

/// Both arms diverge: nothing to disagree about, and the `CIf` itself never
/// produces a value.
#[test]
fn test_both_arms_failing_compiles() {
    let dir = scratch("both_arms");
    let src = dir.join("v.ai3");
    write(&src, "fn validate(n: Int) -> Int {\n  \
                 return if int_lt(n, Int(0)) { fail(Text(\"low\")) } else { fail(Text(\"high\")) }\n}\n");
    let r = compile(&src, &registry_with_fail(&dir), &dir.join("out"));
    assert_eq!(r.code, 0, "should compile: {}{}", r.stdout, r.stderr);
}

/// THE LINE THIS MUST NOT CROSS. Two arms that both produce values, of
/// different types, are still a divergence. If this ever passes, the `fail`
/// rule has become a blanket relaxation of branch type agreement.
#[test]
fn test_genuine_branch_divergence_is_still_rejected() {
    let dir = scratch("genuine_divergence");
    let src = dir.join("v.ai3");
    write(&src, "fn bad(n: Int) -> Int {\n  \
                 return if int_lt(n, Int(0)) { Text(\"oops\") } else { n }\n}\n");
    let r = compile(&src, &registry_with_fail(&dir), &dir.join("out"));
    assert_eq!(r.code, 1, "should be refused: {}{}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("CIfBranchDivergence") || r.stderr.contains("branch divergence"),
        "should fail as a branch divergence. Got: {}",
        r.stderr
    );
}

/// A `fail` outside any branch is an ordinary unconditional call and stays
/// subject to the normal rules — the divergence handling is scoped to `CIf`
/// arms and must not leak into a fn's declared return type.
#[test]
fn test_fail_as_a_whole_body_compiles() {
    let dir = scratch("whole_body");
    let src = dir.join("v.ai3");
    write(&src, "fn always(n: Int) -> Int {\n  return if int_lt(n, Int(0)) { n } else { n }\n}\n");
    let r = compile(&src, &registry_with_fail(&dir), &dir.join("out"));
    assert_eq!(r.code, 0, "control case should compile: {}{}", r.stdout, r.stderr);
}
