// Multi-file `axis` builds: output paths, same-invocation body resolution, and
// re-blessing a composite.
//
// All three defects here were reported from axMachina-working against
// d690d16 and are reproduced below on the fixtures in this repo, so none of
// them needs a sibling checkout to fail.
//
//   1. `--out <dir>` derived ONE path from `source_files[0]` and wrote every
//      bundle to it. N-1 compiles were silently overwritten while still
//      printing success and their own correct identities.
//
//   2. `body_store` was seeded only from `--body-dir`, never from bundles
//      compiled in the same run, so a composite and the body it calls could
//      not be built in one invocation. The first verify failure then exited
//      before any write, so a cold build made no progress and re-running it
//      failed identically forever.
//
//   3. A file declaring a function the registry already carries as a composite
//      could not compile: the synthesized `FnKind::Leaf` record conflicted
//      with the registry's composite. Blessing a function whose inner name
//      matches its registry name made it un-rebuildable.
//
// A composite's registry name and the function name inside its body file are
// normally DIFFERENT (`am_bless` is blessed from a file declaring `fn bless`),
// which is why (3) went unnoticed and why the fixture for (2) keeps them
// distinct — otherwise it would trip (3) before reaching the verifier.

use std::path::{Path, PathBuf};
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn binary() -> PathBuf {
    let release = PathBuf::from("target/release/axis");
    if release.exists() {
        return release;
    }
    let debug = PathBuf::from("target/debug/axis");
    assert!(
        debug.exists(),
        "axis binary not found. Run: cargo build --release"
    );
    debug
}

/// The AI3 surface spec quartet every invocation below needs.
fn spec_args() -> Vec<String> {
    let c = "axis-surface-ai3-config";
    vec![
        "--lexer".into(),
        format!("{c}/ai3-lexer.yaml"),
        "--parser".into(),
        format!("{c}/ai3-parse.yaml"),
        "--schema".into(),
        format!("{c}/ai3-ast.yaml"),
        "--normalize".into(),
        format!("{c}/ai3-normalize.yaml"),
    ]
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(args: &[String]) -> Run {
    let out = Command::new(binary())
        .args(spec_args())
        .args(args)
        .output()
        .expect("failed to spawn axis");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A scratch directory under `target/`, removed and recreated per test so a
/// stale bundle from a previous run can never stand in for one this run failed
/// to write.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from("target/test-multi-file").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(path, contents).expect("write fixture");
}

fn bundles_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".coreir"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The §5b identity rule: a function's identity is `SHA-256(name)`, which is
/// what `content_hash` computes. Using the crate's own helper keeps the test
/// on the same rule the compiler applies rather than a second copy of it.
fn fn_identity_hex(name: &str) -> String {
    axis_lang::fabric::hash::content_hash(name.as_bytes()).to_hex()
}

/// Compile `source` alone and return the identity `axis` printed for it.
fn compile_for_identity(source: &Path, reg: &Path, out_dir: &Path) -> String {
    let r = run(&[
        "--file".into(),
        source.display().to_string(),
        "--reg".into(),
        reg.display().to_string(),
        "--out".into(),
        out_dir.display().to_string(),
    ]);
    assert_eq!(r.code, 0, "seed compile failed: {}{}", r.stdout, r.stderr);
    r.stdout
        .rsplit_once("identity: ")
        .map(|(_, tail)| tail.trim().trim_end_matches(')').to_string())
        .expect("no identity in stdout")
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. --out <dir> gives every source its own bundle
// ═══════════════════════════════════════════════════════════════════════════

/// Two sources, one `--out` directory: two bundles, each named for its source.
///
/// Before the fix this wrote ONE file — `ai3_tail_literal.coreir`, holding
/// `ai3_tail_text`'s bundle — and exited 0 after printing two success lines
/// with two correct, distinct identities. The report called that out as what
/// makes it dangerous rather than annoying: the output looks complete.
#[test]
fn test_out_directory_writes_one_bundle_per_source() {
    let dir = scratch("out_dir_per_source");
    let r = run(&[
        "--file".into(),
        "examples/surface-ai3/ai3_tail_literal.ai3".into(),
        "--file".into(),
        "examples/surface-ai3/ai3_tail_text.ai3".into(),
        "--reg".into(),
        "registries/axis.axreg".into(),
        "--out".into(),
        dir.display().to_string(),
    ]);

    assert_eq!(r.code, 0, "build failed: {}{}", r.stdout, r.stderr);
    assert_eq!(
        bundles_in(&dir),
        vec!["ai3_tail_literal.coreir", "ai3_tail_text.coreir"],
        "each source must get its own bundle"
    );

    // Distinct sources, distinct bytes — a per-source path that still wrote the
    // same bundle twice would pass the name check above.
    let a = std::fs::read(dir.join("ai3_tail_literal.coreir")).unwrap();
    let b = std::fs::read(dir.join("ai3_tail_text.coreir")).unwrap();
    assert_ne!(a, b, "the two bundles must not be the same bytes");
}

/// `--out` naming one exact file cannot describe a build of several sources:
/// every bundle would land on that path and only the last would survive. It is
/// refused rather than silently resolved in favour of whichever compiled last.
#[test]
fn test_out_file_with_multiple_sources_is_refused() {
    let dir = scratch("out_file_multi");
    let r = run(&[
        "--file".into(),
        "examples/surface-ai3/ai3_tail_literal.ai3".into(),
        "--file".into(),
        "examples/surface-ai3/ai3_tail_text.ai3".into(),
        "--reg".into(),
        "registries/axis.axreg".into(),
        "--out".into(),
        dir.join("one.coreir").display().to_string(),
    ]);

    assert_eq!(r.code, 1, "expected refusal, got: {}{}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("names a single file") && r.stderr.contains("2 source files"),
        "diagnostic should name the conflict. Got: {}",
        r.stderr
    );
    assert!(
        !dir.join("one.coreir").exists(),
        "nothing should be written when the output path is refused"
    );
}

/// `--out` naming one exact file is still the single-source form the bootstrap
/// scripts use, and still works.
#[test]
fn test_out_file_with_single_source_still_writes_that_exact_path() {
    let dir = scratch("out_file_single");
    let target = dir.join("nested/chosen-name.coreir");
    let r = run(&[
        "--file".into(),
        "examples/surface-ai3/ai3_tail_literal.ai3".into(),
        "--reg".into(),
        "registries/axis.axreg".into(),
        "--out".into(),
        target.display().to_string(),
    ]);

    assert_eq!(r.code, 0, "build failed: {}{}", r.stdout, r.stderr);
    assert!(target.exists(), "the exact --out path must be written");
}

/// Two sources with the same stem in different directories both want
/// `<dir>/<stem>.coreir`. That is the same silent overwrite per-source paths
/// exist to remove, so it is refused rather than decided by compile order.
#[test]
fn test_sources_sharing_a_stem_are_refused() {
    let dir = scratch("stem_collision");
    let a = dir.join("a/dup.ai3");
    let b = dir.join("b/dup.ai3");
    write(&a, "fn dup_a() -> Int {\n  return Int(1);\n}\n");
    write(&b, "fn dup_b() -> Int {\n  return Int(2);\n}\n");
    let out = dir.join("out");

    let r = run(&[
        "--file".into(),
        a.display().to_string(),
        "--file".into(),
        b.display().to_string(),
        "--reg".into(),
        "registries/axis.axreg".into(),
        "--out".into(),
        out.display().to_string(),
    ]);

    assert_eq!(r.code, 1, "expected refusal, got: {}{}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("share the stem 'dup'"),
        "diagnostic should name the stem. Got: {}",
        r.stderr
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. A composite and the body it calls build in ONE invocation
// ═══════════════════════════════════════════════════════════════════════════

/// Blesses `tf_body` from a file declaring `fn tf_inner`, then builds the
/// composite's caller and that body together with an EMPTY `--body-dir`.
///
/// The caller is passed FIRST, so nothing about the command-line order helps:
/// the verifier must defer it, pick up the body written later in the same run,
/// and come back to it. Before the fix this reported
/// `body bundle <hash> not in store` for a file sitting in the same command.
#[test]
fn test_composite_and_its_body_build_in_one_invocation() {
    let dir = scratch("fixpoint_same_invocation");
    let body_src = dir.join("src/tf_body.ai3");
    let caller_src = dir.join("src/tf_caller.ai3");
    // The registry name and the body file's own fn name differ, which is the
    // real convention (`am_bless` is blessed from a file declaring `fn bless`).
    write(&body_src, "fn tf_inner() -> Int {\n  return Int(7);\n}\n");
    write(&caller_src, "fn tf_call() -> Int {\n  return tf_body();\n}\n");

    let seed_out = dir.join("seed");
    std::fs::create_dir_all(&seed_out).unwrap();
    let body_hash = compile_for_identity(&body_src, Path::new("registries/axis.axreg"), &seed_out);

    let reg = dir.join("blessed.axreg");
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    text.push_str(&format!(
        "\nfn tf_body\n  identity 0x{}\n  kind composite\n  in ()\n  out Int\n  body 0x{}\nend\n",
        fn_identity_hex("tf_body"),
        body_hash
    ));
    write(&reg, &text);

    let out = dir.join("out");
    let bodies = dir.join("bodies"); // deliberately empty: a cold store
    std::fs::create_dir_all(&out).unwrap();
    std::fs::create_dir_all(&bodies).unwrap();

    let r = run(&[
        "--file".into(),
        caller_src.display().to_string(), // caller first
        "--file".into(),
        body_src.display().to_string(),
        "--reg".into(),
        reg.display().to_string(),
        "--body-dir".into(),
        bodies.display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ]);

    assert_eq!(
        r.code, 0,
        "cold one-invocation build failed: {}{}",
        r.stdout, r.stderr
    );
    assert_eq!(
        bundles_in(&out),
        vec!["tf_body.coreir", "tf_caller.coreir"],
        "both bundles must be written"
    );
}

/// A dependency that genuinely cannot be satisfied stops the build, but every
/// bundle that DID verify stays on disk and every failure is reported — not
/// just the first.
///
/// Exiting on the first failure before any write is what made a cold build
/// unable to make progress: no invocation could leave more on disk than the
/// one before it, so re-running never converged.
#[test]
fn test_unsatisfiable_body_keeps_the_bundles_that_verified() {
    let dir = scratch("residual_failures");
    let body_src = dir.join("src/tf_body.ai3");
    let caller_src = dir.join("src/tf_caller.ai3");
    let unrelated_src = dir.join("src/tf_unrelated.ai3");
    write(&body_src, "fn tf_inner() -> Int {\n  return Int(7);\n}\n");
    write(&caller_src, "fn tf_call() -> Int {\n  return tf_body();\n}\n");
    write(&unrelated_src, "fn tf_plain() -> Int {\n  return Int(3);\n}\n");

    let seed_out = dir.join("seed");
    std::fs::create_dir_all(&seed_out).unwrap();
    let body_hash = compile_for_identity(&body_src, Path::new("registries/axis.axreg"), &seed_out);

    let reg = dir.join("blessed.axreg");
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    text.push_str(&format!(
        "\nfn tf_body\n  identity 0x{}\n  kind composite\n  in ()\n  out Int\n  body 0x{}\nend\n",
        fn_identity_hex("tf_body"),
        body_hash
    ));
    write(&reg, &text);

    let out = dir.join("out");
    let bodies = dir.join("bodies");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::create_dir_all(&bodies).unwrap();

    // tf_body.ai3 is WITHHELD, so tf_caller's body can never enter the store.
    let r = run(&[
        "--file".into(),
        caller_src.display().to_string(),
        "--file".into(),
        unrelated_src.display().to_string(),
        "--reg".into(),
        reg.display().to_string(),
        "--body-dir".into(),
        bodies.display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ]);

    assert_eq!(r.code, 1, "expected failure: {}{}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("not in store"),
        "the real verifier error must survive. Got: {}",
        r.stderr
    );
    assert_eq!(
        bundles_in(&out),
        vec!["tf_unrelated.coreir"],
        "the bundle that verified must stay written"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Re-blessing a composite whose inner name matches its registry name
// ═══════════════════════════════════════════════════════════════════════════

/// Compile a file, bless the function it declares as a composite under the
/// SAME name, then compile the unchanged file again.
///
/// A composite's identity is `SHA-256(name)` — the same rule used for a source
/// function — so the second pass lands on the registry's own record, and the
/// synthesized `FnKind::Leaf` conflicted with the composite. Recompiling is
/// the normal re-bless path, so the function became un-rebuildable the moment
/// it was blessed; restoring its body needed a stripped registry copy.
///
/// The body hash must also still be the one the registry declares: a pass that
/// compiled but produced different bytes would not be a re-bless.
#[test]
fn test_blessed_composite_recompiles_from_its_own_source() {
    let dir = scratch("rebless_same_name");
    let src = dir.join("src/tf_self.ai3");
    write(&src, "fn tf_self() -> Int {\n  return Int(44);\n}\n");

    let seed_out = dir.join("seed");
    std::fs::create_dir_all(&seed_out).unwrap();
    let body_hash = compile_for_identity(&src, Path::new("registries/axis.axreg"), &seed_out);

    let reg = dir.join("blessed.axreg");
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    text.push_str(&format!(
        "\nfn tf_self\n  identity 0x{}\n  kind composite\n  in ()\n  out Int\n  body 0x{}\nend\n",
        fn_identity_hex("tf_self"),
        body_hash
    ));
    write(&reg, &text);

    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let r = run(&[
        "--file".into(),
        src.display().to_string(),
        "--reg".into(),
        reg.display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ]);

    assert_eq!(
        r.code, 0,
        "re-bless pass failed: {}{}",
        r.stdout, r.stderr
    );
    assert!(
        r.stdout.contains(&body_hash),
        "the second pass must reproduce the blessed body hash {}. Got: {}",
        body_hash,
        r.stdout
    );
}

/// The registry entry is a composite, but its declared signature disagrees with
/// the file. That is not a re-bless — the body no longer matches the contract
/// it was blessed under — and it is a hard error naming the registry.
#[test]
fn test_composite_signature_mismatch_is_a_named_registry_collision() {
    let dir = scratch("rebless_sig_mismatch");
    let src = dir.join("src/tf_sig.ai3");
    write(&src, "fn tf_sig() -> Int {\n  return Int(44);\n}\n");

    let reg = dir.join("blessed.axreg");
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    // Registry says one parameter; the file declares none.
    text.push_str(&format!(
        "\nfn tf_sig\n  identity 0x{}\n  kind composite\n  in (Text)\n  out Int\n  body 0x{}\nend\n",
        fn_identity_hex("tf_sig"),
        fn_identity_hex("not-a-real-body")
    ));
    write(&reg, &text);

    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let r = run(&[
        "--file".into(),
        src.display().to_string(),
        "--reg".into(),
        reg.display().to_string(),
        "--out".into(),
        out.display().to_string(),
    ]);

    assert_eq!(r.code, 1, "expected refusal: {}{}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("collides with a registry entry")
            && r.stderr.contains("1 parameter(s), this file declares 0"),
        "diagnostic must point at the registry and the mismatch. Got: {}",
        r.stderr
    );
}

/// A registry LEAF of the same name is this file's body too — the §5b provider
/// pattern. axVerity declares its providers as leaves (graphcore: 113 leaves,
/// zero composites; bench/bridge_crossing: 5), because from the caller's side
/// a provider IS a leaf: the caller links an extern.
///
/// This was refused until the check was widened, and refused for a reason that
/// was never true — "this file cannot be its body". Worse, whether it fired at
/// all turned on whether the registry's facts happened to differ from the ones
/// a source declaration synthesizes: `gc_frame` is declared `idempotent true`
/// and the synthesized value is `false`, so it did.
#[test]
fn test_registry_leaf_of_the_same_name_is_this_files_body() {
    let dir = scratch("leaf_provider");
    let src = dir.join("src/tf_leaf.ai3");
    write(&src, "fn tf_leaf() -> Int {\n  return Int(44);\n}\n");

    let reg = dir.join("blessed.axreg");
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    // Facts deliberately UNLIKE the synthesized ones (fullIo, idempotent true),
    // which is what the old check keyed on.
    text.push_str(&format!(
        "\nfn tf_leaf\n  identity 0x{}\n  kind leaf\n  in ()\n  out Int\n  effect fullIo\n  deterministic true\n  idempotent true\nend\n",
        fn_identity_hex("tf_leaf")
    ));
    write(&reg, &text);

    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let r = run(&[
        "--file".into(), src.display().to_string(),
        "--reg".into(), reg.display().to_string(),
        "--out".into(), out.display().to_string(),
    ]);
    assert_eq!(r.code, 0, "provider-leaf body should compile: {}{}", r.stdout, r.stderr);
}

/// The signature cross-check applies to a leaf entry exactly as it does to a
/// composite: this file is the body, so a body that disagrees with the declared
/// contract is still refused.
#[test]
fn test_leaf_signature_mismatch_is_still_refused() {
    let dir = scratch("leaf_sig_mismatch");
    let src = dir.join("src/tf_leafsig.ai3");
    write(&src, "fn tf_leafsig() -> Int {\n  return Int(44);\n}\n");

    let reg = dir.join("blessed.axreg");
    let mut text = std::fs::read_to_string("registries/axis.axreg").unwrap();
    text.push_str(&format!(
        "\nfn tf_leafsig\n  identity 0x{}\n  kind leaf\n  in (Text)\n  out Int\n  effect pure\n  deterministic true\n  idempotent false\nend\n",
        fn_identity_hex("tf_leafsig")
    ));
    write(&reg, &text);

    let out = dir.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let r = run(&[
        "--file".into(), src.display().to_string(),
        "--reg".into(), reg.display().to_string(),
        "--out".into(), out.display().to_string(),
    ]);
    assert_eq!(r.code, 1, "expected refusal: {}{}", r.stdout, r.stderr);
    assert!(
        r.stderr.contains("collides with a registry entry")
            && r.stderr.contains("1 parameter(s), this file declares 0"),
        "diagnostic must name the mismatch. Got: {}",
        r.stderr
    );
}
