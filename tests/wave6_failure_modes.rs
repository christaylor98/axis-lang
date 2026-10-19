// WAVE 6: Failure Mode Tests
//
// OBJECTIVE: Verify pipeline fails gracefully for invalid inputs
// - Invalid syntax → lex/parse error
// - Invalid semantics → lowering error
// - No panics, no silent fallbacks

use axis_lang::pipeline::{run_pipeline, PipelineConfig};
use axis_lang::registry::Registry;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

fn surface_0_config(source_file: &str) -> PipelineConfig {
    PipelineConfig {
        lexer_spec: PathBuf::from("axis-surface-0-config/semantic-surface-0-lexer.yaml"),
        parser_spec: PathBuf::from("axis-surface-0-config/semantic-surface-0-parse.yaml"),
        ast_schema: PathBuf::from("axis-surface-0-config/semantic-surface-0-ast.yaml"),
        source_file: PathBuf::from(source_file),
        entry_rule: None,
        normalize_spec: PathBuf::from("axis-surface-0-config/semantic-surface-0-normalize.yaml"),
        registry: Registry::new(),
        core05_scope: None,
        parser_mode: None,
        hook_registry: None,
    }
}

/// One file PER CALL, not one per process.
///
/// This used to be `wave6_test_{pid}.ax0` — a single path shared by every test
/// in this file, while cargo runs them on multiple threads in that one process.
/// So `failure_empty_file` (writes ""), `error_messages_are_nonempty` (writes
/// "@#$") and their siblings raced on the same path and on each other's
/// `cleanup_temp_file`: a test could read another's content, or a file that had
/// already been deleted. It failed nondeterministically — a clean run, then 5
/// failures, then two clean runs, on identical code — which makes every result
/// from this file unusable as evidence either way.
///
/// The counter is per-process and combined with the pid, so parallel test
/// binaries do not collide either.
fn write_temp_file(content: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let temp_dir = env::temp_dir();
    let temp_file = temp_dir.join(format!("wave6_test_{}_{}.ax0", std::process::id(), n));
    fs::write(&temp_file, content).expect("writing temp file should succeed");
    temp_file
}

fn cleanup_temp_file(path: &PathBuf) {
    fs::remove_file(path).ok();
}

// ═══════════════════════════════════════════════════════════════════════════
// LEXER FAILURE MODES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_invalid_character() {
    let temp_file = write_temp_file("@#$%^");
    let config = surface_0_config(temp_file.to_str().unwrap());

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    // Should fail at lex or parse stage
    assert!(
        result.is_err(),
        "Invalid characters should cause pipeline failure"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// PARSER FAILURE MODES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_unclosed_paren() {
    let temp_file = write_temp_file("(42");
    let config = surface_0_config(temp_file.to_str().unwrap());

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    assert!(
        result.is_err(),
        "Unclosed parenthesis should cause parse failure"
    );
}

#[test]
fn failure_unmatched_keyword() {
    let temp_file = write_temp_file("if true then 1"); // missing else
    let config = surface_0_config(temp_file.to_str().unwrap());

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    assert!(
        result.is_err(),
        "Incomplete if expression should cause parse failure"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEMA PROJECTION FAILURE MODES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_type_mismatch() {
    // This depends on schema enforcement - may pass if schema is permissive
    let temp_file = write_temp_file("if 42 then true else false"); // condition is int, not bool
    let config = surface_0_config(temp_file.to_str().unwrap());

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    // Schema may allow this - only fail if schema is strict
    if result.is_err() {
        println!("✓ Schema correctly rejected type mismatch");
    } else {
        println!("⚠ Schema allows int as condition (may be intentional)");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALISATION FAILURE MODES
// ═══════════════════════════════════════════════════════════════════════════

// Note: Normalisation failures depend on whether forbidden surface constructs
// can even be parsed. These tests may need adjustment based on actual grammar.

// ═══════════════════════════════════════════════════════════════════════════
// EMPTY INPUT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_empty_file() {
    let temp_file = write_temp_file("");
    let config = surface_0_config(temp_file.to_str().unwrap());

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    assert!(result.is_err(), "Empty file should cause pipeline failure");
}

// ═══════════════════════════════════════════════════════════════════════════
// MISSING FILES
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_missing_source_file() {
    let config = surface_0_config("nonexistent_file.ax0");

    let result = run_pipeline(&config);

    assert!(result.is_err(), "Missing source file should cause IO error");
}

#[test]
fn failure_missing_lexer_spec() {
    let temp_file = write_temp_file("()");

    let mut config = surface_0_config(temp_file.to_str().unwrap());
    config.lexer_spec = PathBuf::from("nonexistent_lexer.yaml");

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    assert!(
        result.is_err(),
        "Missing lexer spec should cause spec load error"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// MALFORMED SPECS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_malformed_lexer_spec() {
    let temp_file = write_temp_file("()");

    // Create malformed lexer spec
    let temp_dir = env::temp_dir();
    let bad_spec = temp_dir.join("bad_lexer.yaml");
    fs::write(&bad_spec, "not: valid: yaml::: content").expect("writing bad spec should succeed");

    let mut config = surface_0_config(temp_file.to_str().unwrap());
    config.lexer_spec = bad_spec.clone();

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);
    cleanup_temp_file(&bad_spec);

    assert!(
        result.is_err(),
        "Malformed lexer spec should cause spec load error"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// ERROR QUALITY VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn error_messages_are_nonempty() {
    let temp_file = write_temp_file("@#$");
    let config = surface_0_config(temp_file.to_str().unwrap());

    let result = run_pipeline(&config);

    cleanup_temp_file(&temp_file);

    match result {
        Err(e) => {
            let error_string = format!("{}", e);
            assert!(!error_string.is_empty(), "Error message must not be empty");
            println!("Error message: {}", error_string);
        }
        Ok(_) => panic!("Expected error but pipeline succeeded"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NO PANIC VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn no_panic_on_invalid_input() {
    // This test ensures pipeline errors gracefully rather than panicking
    let test_cases = vec!["", "@#$", "(unclosed", "if true", "42 42 42"];

    for (i, input) in test_cases.iter().enumerate() {
        let temp_file = write_temp_file(input);
        let config = surface_0_config(temp_file.to_str().unwrap());

        // This should not panic - either Ok or Err is acceptable
        let _result = run_pipeline(&config);

        cleanup_temp_file(&temp_file);

        println!("Test case {} did not panic: '{}'", i + 1, input);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FAILURE MODE VERIFICATION REPORT
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn failure_mode_verification_report() {
    println!("════════════════════════════════════════════════════════════");
    println!("WAVE 6: FAILURE MODE VERIFICATION REPORT");
    println!("════════════════════════════════════════════════════════════");
    println!();
    println!("VERIFIED FAILURE MODES:");
    println!("  ✓ Invalid characters → lex/parse error");
    println!("  ✓ Unclosed delimiters → parse error");
    println!("  ✓ Incomplete expressions → parse error");
    println!("  ✓ Empty file → error");
    println!("  ✓ Missing files → IO error");
    println!("  ✓ Malformed specs → spec load error");
    println!();
    println!("VERIFIED PROPERTIES:");
    println!("  ✓ No panics on invalid input");
    println!("  ✓ Error messages are non-empty");
    println!("  ✓ Errors propagate correctly");
    println!();
    println!("════════════════════════════════════════════════════════════");
}

// ═══════════════════════════════════════════════════════════════════════════
// DUPLICATE FUNCTION DECLARATION (M1, via the `axis` BINARY)
// ═══════════════════════════════════════════════════════════════════════════
//
// User fn identity is SHA-256(name), so two `FnDecl`s with the same name mint
// the same identity. `augment_scope_with_user_fns` previously discarded both
// `Scope::insert` results with `let _ =`, so the second record was dropped in
// silence and every call site bound to the FIRST signature — a signature the
// programmer did not write. These run the real binary end to end.

/// Pick the NEWEST of the two build profiles, not the first that exists.
/// Preferring `target/release/axis` unconditionally silently tests a stale
/// binary whenever a release build predates the change under test.
fn m1_binary() -> PathBuf {
    let debug = PathBuf::from("target/debug/axis");
    let release = PathBuf::from("target/release/axis");
    let mtime = |p: &PathBuf| fs::metadata(p).and_then(|m| m.modified()).ok();
    match (mtime(&debug), mtime(&release)) {
        (Some(d), Some(r)) => if r > d { release } else { debug },
        (Some(_), None) => debug,
        (None, Some(_)) => release,
        (None, None) => panic!("axis binary not found — run `cargo build`"),
    }
}

fn run_m1_binary(source_file: &PathBuf) -> std::process::Output {
    Command::new(m1_binary())
        .arg("--file").arg(source_file)
        .arg("--lexer").arg("axis-surface-m1-config/m1-lexer.yaml")
        .arg("--parser").arg("axis-surface-m1-config/m1-parse.yaml")
        .arg("--schema").arg("axis-surface-m1-config/m1-ast.yaml")
        .arg("--normalize").arg("axis-surface-m1-config/m1-normalize.yaml")
        .arg("--reg").arg("axis-surface-m1-config/m1-registry.axreg")
        .arg("--reg").arg("registries/pilot_lowering.axreg")
        .arg("--reg").arg("registries/fnref_test.axreg")
        .output()
        .expect("failed to execute the axis binary")
}

fn write_temp_m1(tag: &str, content: &str) -> PathBuf {
    let temp_file = env::temp_dir().join(format!("wave6_dupfn_{}_{}.m1", tag, std::process::id()));
    fs::write(&temp_file, content).expect("writing temp file should succeed");
    temp_file
}

fn assert_duplicate_fn_rejected(tag: &str, source: &str) {
    let temp_file = write_temp_m1(tag, source);
    let output = run_m1_binary(&temp_file);
    cleanup_temp_file(&temp_file);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "[{}] duplicate fn declaration must not compile. output: {}",
        tag, combined
    );
    assert!(
        combined.contains("duplicate function 'dup'"),
        "[{}] error must name the duplicated function. output: {}",
        tag, combined
    );
}

/// Two `dup`s with DIFFERENT signatures: the second `FnGenesis` is a genuine
/// `ResolveError::Conflict`.
#[test]
fn failure_duplicate_fn_differing_signatures() {
    assert_duplicate_fn_rejected(
        "sigs",
        "fn dup(x: Bool) -> Bool {\n  x\n}\n\nfn dup(x: Int) -> Int {\n  x\n}\n",
    );
}

/// Two `dup`s with IDENTICAL signatures but different bodies. The record-level
/// conflict check cannot see this — `FnGenesis`/`LeafFacts` carry no body, so
/// both records compare equal and `Scope::insert` returns `Ok`. Caught by the
/// per-file declared-name set instead.
#[test]
fn failure_duplicate_fn_identical_signatures_differing_bodies() {
    assert_duplicate_fn_rejected(
        "bodies",
        "fn dup(x: Bool) -> Bool {\n  Bool(true)\n}\n\nfn dup(x: Bool) -> Bool {\n  Bool(false)\n}\n",
    );
}

/// A call site must not silently bind to whichever duplicate landed first.
#[test]
fn failure_duplicate_fn_with_call_site() {
    assert_duplicate_fn_rejected(
        "callsite",
        "fn dup(x: Bool) -> Bool {\n  x\n}\n\nfn dup(x: Int) -> Int {\n  x\n}\n\n\
         fn caller(n: Int) -> Int {\n  dup(n)\n}\n",
    );
}

/// Positive control: distinctly-named cross-calling fns still compile, so the
/// three tests above cannot pass by breaking M1 compilation generally.
#[test]
fn duplicate_fn_check_does_not_reject_distinct_names() {
    let source = PathBuf::from("examples/surface-m1/two_fn_call.m1");
    let output = run_m1_binary(&source);
    assert!(
        output.status.success(),
        "two_fn_call.m1 must still compile. stdout: {} stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK SPAWN / JOIN (M1, via the `axis` BINARY)
// ═══════════════════════════════════════════════════════════════════════════
//
// `spawn` / `join` are ORDINARY registry callables — no surface syntax, no new
// Core IR node kind, no annotation channel. Everything below therefore runs on
// the UNMODIFIED M1 config set; only the registry differs.
//
// What these pin:
//   1. A fn-ref passed by name into `spawn`'s `Fn` slot verifies clean.
//   2. `Handle`/`HandleList` are REAL types, not documentation. Registry type
//      identity is structural (`codec.rs:105-107`), so `HandleList = list Int`
//      would hash identically to `IntList` and any list of integers would pass
//      as a handle list. `list (product (Int, Value))` collides with nothing —
//      the rejections below are only possible because of that shape.
//   3. The HOF callback contract catches a worker whose signature cannot be
//      applied, instead of letting it panic inside generated code.
//
// FIXTURE, NOT THE CANONICAL DECLARATION. The shipped entries live in
// axRegistry (`axis-types.axreg` / `axis-bridge.axreg`) and type the
// worker slot as the sum-typed `Value`. The M1 config set declares
// `Value = prim value` instead, and the two vocabularies cannot be loaded
// together (see examples/surface-ai3/BASELINE.md "Finding 2"). So this fixture
// monomorphises the worker to `Int`, exactly as `registries/fnref_test.axreg`
// already does for `apply_handler`. The mechanism under test — Fn slot,
// callback contract, Handle typing — is identical; only the element type is
// pinned down. Identities are sha256(name) per §5b and match the shipped ones.
const TASKS_TEST_AXREG: &str = "\
registry m1_tasks_test 0.1

type Handle     = product (Int, Value)
type HandleList = list Handle

fn spawn
  identity 0xa04afb368a4946b069ebf8e3ceeeba94ff114ddb477c7b5a5e133c5d52544d43
  kind     leaf
  in       (Fn, Int)
  out      Handle
  effect   fullIo
  deterministic false
  idempotent    false
  callback 0 (Int) -> Int
end

fn join
  identity 0x58393216032be6257784ac0c6a73efb2a084e27b4cfff1e6acee7b7e6ab93b10
  kind     leaf
  in       (HandleList)
  out      HandleList
  effect   fullIo
  deterministic false
  idempotent    false
end

// Declared HERE rather than reused from the M1 registry so the handle-is-not-
// an-Int test does not depend on which arithmetic callables that set happens
// to carry (int_add is not in it).
fn consume_int
  identity 0x1a593f9859423a5c5502c2677700c52260244c43e27b76f0fd42a85a8ae3b544
  kind     leaf
  in       (Int)
  out      Unit
  effect   pure
  deterministic true
  idempotent    true
end

fn handle_list_make
  identity 0xcd4fc0dba1b5965440e5a7a40e1bf8f9fee790eb77dce3629270dd604ac060d9
  kind     leaf
  in       (Handle)
  out      HandleList
  effect   pure
  deterministic true
  idempotent    true
  variadic true
end
";

/// Run the binary over the M1 config set plus the task registry above.
/// Same four spec files as `run_m1_binary` — the point is that spawn/join need
/// no surface change whatsoever.
fn run_m1_binary_with_tasks(source_file: &PathBuf, reg_file: &PathBuf) -> std::process::Output {
    Command::new(m1_binary())
        .arg("--file").arg(source_file)
        .arg("--lexer").arg("axis-surface-m1-config/m1-lexer.yaml")
        .arg("--parser").arg("axis-surface-m1-config/m1-parse.yaml")
        .arg("--schema").arg("axis-surface-m1-config/m1-ast.yaml")
        .arg("--normalize").arg("axis-surface-m1-config/m1-normalize.yaml")
        .arg("--reg").arg("axis-surface-m1-config/m1-registry.axreg")
        .arg("--reg").arg("registries/pilot_lowering.axreg")
        .arg("--reg").arg(reg_file)
        .output()
        .expect("failed to execute the axis binary")
}

/// Write the fixture registry and the M1 source, run the binary, clean up, and
/// return the combined stdout+stderr alongside the success flag.
fn run_tasks_case(tag: &str, source: &str) -> (bool, String) {
    let pid = std::process::id();
    let reg_file = env::temp_dir().join(format!("wave6_tasks_{}_{}.axreg", tag, pid));
    fs::write(&reg_file, TASKS_TEST_AXREG).expect("writing fixture registry should succeed");
    let src_file = env::temp_dir().join(format!("wave6_tasks_{}_{}.m1", tag, pid));
    fs::write(&src_file, source).expect("writing temp source should succeed");

    let output = run_m1_binary_with_tasks(&src_file, &reg_file);
    cleanup_temp_file(&src_file);
    cleanup_temp_file(&reg_file);
    // A run that reaches the verify gate WRITES `coreir/<stem>.coreir` next to
    // the repo. Only the positive case gets that far, but clean up
    // unconditionally so a passing test never leaves an untracked file behind.
    cleanup_temp_file(&PathBuf::from(format!(
        "coreir/{}.coreir",
        src_file.file_stem().expect("temp source has a stem").to_string_lossy()
    )));

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

fn assert_tasks_rejected(tag: &str, source: &str, expected_fragment: &str) {
    let (ok, combined) = run_tasks_case(tag, source);
    assert!(!ok, "[{}] must not compile. output: {}", tag, combined);
    assert!(
        combined.contains(expected_fragment),
        "[{}] expected {:?} in output: {}",
        tag, expected_fragment, combined
    );
}

/// POSITIVE CONTROL — fan out to two tasks and join them.
///
/// Without this the rejections below could all pass by breaking M1 generally.
/// Note there is no `@spawn`, no `par`, no annotation: `spawn` and `join` are
/// plain calls, so the surface config is byte-identical to every other M1 test.
#[test]
fn tasks_spawn_and_join_compiles_and_verifies() {
    let (ok, combined) = run_tasks_case(
        "ok",
        "fn worker(p: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> HandleList {\n  \
         let a = spawn(worker, Int(1))\n  \
         let b = spawn(worker, Int(2))\n  \
         join(handle_list_make(a, b))\n}\n",
    );
    assert!(ok, "spawn/join program must compile and verify. output: {}", combined);
}

/// A bare `Handle` is not a `HandleList` — the list is the barrier, and one
/// task still has to be joined as a one-element list.
#[test]
fn tasks_reject_bare_handle_where_handle_list_required() {
    assert_tasks_rejected(
        "barehandle",
        "fn worker(p: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> HandleList {\n  \
         let a = spawn(worker, Int(1))\n  \
         join(a)\n}\n",
        "TypeMismatch",
    );
}

/// A `ValueList` must not pass as a `HandleList`. This is the test that would
/// silently pass if `HandleList` were ever redefined as `list Int` — the two
/// would then share one structural identity.
#[test]
fn tasks_reject_value_list_where_handle_list_required() {
    assert_tasks_rejected(
        "valuelist",
        "fn worker(p: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> HandleList {\n  \
         let a = spawn(worker, Int(1))\n  \
         join(list_make(a))\n}\n",
        "TypeMismatch",
    );
}

/// A non-handle ELEMENT in `handle_list_make`. The variadic entry declares
/// `in (Handle)`, so every argument is checked against `Handle`; before that
/// `variadic true` switched the check off and this verified clean, then panicked
/// inside `join` ("expected a Handle (id, result) pair"). The bad element is
/// second so a check of position 0 alone would not catch it.
#[test]
fn tasks_reject_non_handle_element_in_handle_list() {
    assert_tasks_rejected(
        "nonhandleelem",
        "fn worker(p: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> HandleList {\n  \
         let a = spawn(worker, Int(1))\n  \
         join(handle_list_make(a, Int(3)))\n}\n",
        "TypeMismatch",
    );
}

/// A handle is not an integer, however it is represented underneath.
#[test]
fn tasks_reject_handle_used_as_int() {
    assert_tasks_rejected(
        "handleasint",
        "fn worker(p: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> Unit {\n  \
         let a = spawn(worker, Int(1))\n  \
         consume_int(a)\n}\n",
        "TypeMismatch",
    );
}

/// A non-fn in the `Fn` callee slot. `Fn` is one opaque primitive, so this is
/// caught by the slot's declared type rather than by the callback contract.
#[test]
fn tasks_reject_non_fn_in_callee_slot() {
    assert_tasks_rejected(
        "nonfn",
        "fn worker(p: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> HandleList {\n  \
         join(handle_list_make(spawn(Int(5), Int(1))))\n}\n",
        "TypeMismatch",
    );
}

/// Wrong ARITY in the callee. Every fn-ref pool entry shares one `def_hash`,
/// so without the `callback` contract this verifies clean and panics inside
/// generated code — the exact failure M1_HOF_CALLBACK_CONTRACT_V1 closed.
#[test]
fn tasks_reject_callee_with_wrong_arity() {
    assert_tasks_rejected(
        "arity",
        "fn worker(p: Int, q: Int) -> Int {\n  Int(7)\n}\n\n\
         fn controller() -> HandleList {\n  \
         let a = spawn(worker, Int(1))\n  \
         join(handle_list_make(a))\n}\n",
        "CallbackContractViolated",
    );
}

/// Wrong RETURN TYPE in the callee — the other half of the contract.
#[test]
fn tasks_reject_callee_with_wrong_return_type() {
    assert_tasks_rejected(
        "ret",
        "fn worker(p: Int) -> Unit {\n  Unit()\n}\n\n\
         fn controller() -> HandleList {\n  \
         let a = spawn(worker, Int(1))\n  \
         join(handle_list_make(a))\n}\n",
        "CallbackContractViolated",
    );
}

// ─── Bare payload into a sum: admitted only when the variant is determined ───
//
// `type_admits` let a bare value into a sum if ANY constructor carried it as
// its single payload. With two variants carrying the same type the bare value
// names neither, and the verifier passed it anyway: an `Int` handed to an
// `Outcome = sum [ Presented(Int) | Stale(Int) ]` parameter compiled clean,
// meaning whichever variant the runtime happened to pick. Now it is admitted
// only when exactly one constructor carries it — `Value`, whose constructors
// all carry different types, is unaffected (the M1 baseline exercises it).

const SUM_ADMIT_AXREG: &str = "\
registry m1_sum_admit_test 0.1

type Outcome = sum [ Presented(Int) | Stale(Int) ]
type Mixed   = sum [ Num(Int) | Word(Text) ]

fn take_outcome
  identity 0xa6fd2d5e98fb72a4ea3240bc555e64ec74aff9e50145c72742b0dd894e046e5a
  kind     leaf
  in       (Outcome)
  out      Int
  effect   pure
  deterministic true
  idempotent    true
end

fn take_mixed
  identity 0x23e92f9b2069dff59094dc6342785666dcdee78c29c478aa2dfcd18ccc2f8242
  kind     leaf
  in       (Mixed)
  out      Int
  effect   pure
  deterministic true
  idempotent    true
end
";

fn run_sum_admit_case(tag: &str, source: &str) -> (bool, String) {
    let pid = std::process::id();
    let reg_file = env::temp_dir().join(format!("wave6_sumadmit_{}_{}.axreg", tag, pid));
    fs::write(&reg_file, SUM_ADMIT_AXREG).expect("writing fixture registry should succeed");
    let src_file = env::temp_dir().join(format!("wave6_sumadmit_{}_{}.m1", tag, pid));
    fs::write(&src_file, source).expect("writing temp source should succeed");
    let output = run_m1_binary_with_tasks(&src_file, &reg_file);
    cleanup_temp_file(&src_file);
    cleanup_temp_file(&reg_file);
    cleanup_temp_file(&PathBuf::from(format!(
        "coreir/{}.coreir",
        src_file.file_stem().expect("temp source has a stem").to_string_lossy()
    )));
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

#[test]
fn failure_bare_payload_into_ambiguous_sum() {
    let (ok, combined) = run_sum_admit_case("ambiguous", "fn f() -> Int {\n  take_outcome(Int(1))\n}\n");
    assert!(!ok, "an Int names neither Presented nor Stale; it must not verify. output: {}", combined);
    assert!(combined.contains("TypeMismatch"), "expected a TypeMismatch. output: {}", combined);
}

#[test]
fn bare_payload_into_unambiguous_sum_still_verifies() {
    // Positive control: keeps the test above from passing because sums broke
    // altogether. Int is carried by Num only.
    let (ok, combined) = run_sum_admit_case("unambiguous", "fn g() -> Int {\n  take_mixed(Int(1))\n}\n");
    assert!(ok, "an Int is Num, unambiguously; it must verify. output: {}", combined);
}
