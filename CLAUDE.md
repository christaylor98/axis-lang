# CLAUDE.md — Axis Lang: Invariants for AI Assistance

## ARCHITECTURE LAW: NF IS THE BOUNDARY

The pipeline has two zones divided by the **Normal Form (NF)**:

- **Above NF** (lexer, parser, AST schema, normalize): **spec-driven YAML**, one set of config files per surface.
- **At and below NF** (lowering to Core IR 0.5, verification): **fixed Rust, set once**. Not spec-driven. No YAML lowering spec exists or is to be created.

The production pipeline is:

```
axis --lexer ... --parser ... --schema ... --normalize ... --reg ...
```

The binary (`axis`, `src/main.rs`) calls `run_pipeline` → Stage 6 lowers via
`nf_lowering::lower_nf_items_to_bundle_05` (`src/lowering/nf_lowering.rs`).
This is the only lowering path. There is no `--lowering` flag, no `*-lowering.yaml`,
and no `spec_driven` lowering module. None of these will be created.

### What you must NEVER do

- **Never add a `--lowering` flag, a `*-lowering.yaml`, or a spec-driven lowering path.**
  Lowering is fixed Rust set once. If `nf_lowering` lacks an arm for a node kind, the fix
  is to extend `src/lowering/nf_lowering.rs` — never to add a YAML lowering spec or a
  second lowering path.

- **`Program` is a forbidden node kind. It must never exist.**
  The NF pipeline must never produce a `Program` root node. Multi-function
  programs are handled by running the pipeline once per top-level declaration,
  not by wrapping them in a `Program` node. If `Program` appears anywhere — in
  the NF output, in a proposed fix — that is a bug. Eliminate `Program` upstream.

- **Never describe creating a spec-driven lowering path as cleaner or lower-risk.**
  The correct path is the fixed Rust lowerer regardless of how many lines it takes.
  This has been violated before. Each violation costs 2× the tokens to undo.

### What you must always do

When `axis` fails with a lowering error, the required fix is:

1. Add a handler arm to `src/lowering/nf_lowering.rs`.
2. Run `cargo build` and the M1 end-to-end baseline to confirm no regressions.
3. Add an integration test that runs the `axis` **binary** on a real `.m1`
   (or surface-appropriate) file and asserts PASS.

The matrix tests (`tests/m1_lowering_matrix.rs`) call `run_pipeline` +
`lower_nf_to_bundle_05` directly. They exercise the real lowering code at the
integration level. But the only test of the full system — including CLI argument
handling, file I/O, registry loading, and the verify gate — is the `axis` binary
invoked on real files. Both kinds of tests matter; the binary test catches more.

### No new modules without explicit permission

Do not create new `.rs` source files, new YAML spec files, new test files, or
new modules of any kind unless the user has explicitly asked for one by name.
Fix gaps by editing existing files only. If a fix seems to require a new file,
stop and ask first.

### Known deferred work

**M1 lambda is fully eradicated (PASS 3 — AXIS_CLEANUP_PASS3_LAMBDA_ERADICATION_V1, 2026-06-15).**
Lambda syntax (`fn(...){...}`) no longer parses in M1. `validate_m1_lam_rules` (RULE 1/2)
is deleted. The LamExpr lowering arm and `extract_untyped_params` are deleted from
`src/lowering/nf_lowering.rs`. The dead cluster (`lambda_lift.rs`, `registry_lowering.rs`)
is deleted. fn-reference-by-name (`VarRef` → scope lookup → Fn-typed pool constant) is the
sole behaviour-passing mechanism and remains green.

**Program arm in nf_lowering is residual.** `run_pipeline` splits multi-item files upstream
so the `Program` arm in `lower_nf_node_05` is unreachable in practice. It is left for a
future micro-pass to remove cleanly.

### HOF callback contracts (M1_HOF_CALLBACK_CONTRACT_V1)

Every `Fn`-typed registry parameter MUST carry a `callback` line, or every call
site fails verification:

```
fn apply_handler
  in       (Fn)
  out      Unit
  callback 0 (Int) -> Unit
end
```

`Fn` is one opaque primitive — all fn-ref pool entries share a `def_hash` and
differ only in their 32-byte identity payload — so before this the verifier
admitted any callee into any `Fn` slot: wrong arity, wrong argument types and
wrong return type all verified clean and panicked inside generated code
(`int_add_vfn: expected a 2-tuple`, `flat_map: callee must return ValueList`).
Step 2c in `src/validation/core_ir.rs` resolves the fn-ref back to its
`FnGenesis` and checks it against the contract, and unions the callee's effect
into the call site so a HOF cannot launder its callback's effect.

Contracts are scope-only (`Scope::hof_callback_contracts`), like the projection
name table — adding one changes no canonical bytes and no identity. Fail-closed
is the point: an `Fn` slot with no contract is a hard failure, never a pass.

Argument checking is contravariant with one relaxation for sum-typed contract
inputs, because the projection has no type variables and `loop_while`'s
parametric `a` can only be erased to `Value`. See `callback_arg_admits` for the
exact rule and the residual gap it leaves.

### Duplicate function declarations fail closed (M1_DUPLICATE_FN_CLOSED_V1)

A user fn's identity is `SHA-256(name)` (§5b), so two `FnDecl`s with the same
name mint the SAME identity. `augment_scope_with_user_fns`
(`src/lowering/nf_lowering.rs`) used to discard both `Scope::insert` results
with `let _ =` and to write the projection name with `scope.names.insert`
rather than `bind_name`. `Scope::insert` returns `Err` ONLY when an existing
record DIFFERS — an identical re-insert is `Ok` — so there was no benign
duplicate to swallow: the second record was dropped in silence, the FIRST
survived, and every call site bound to a signature the programmer never wrote.
Two same-named fns with different types, and a caller between them, all
compiled and verified clean.

The fix is three parts, and the first two alone are NOT sufficient:

1. Propagate both `insert` results as `SchemaLower05Error::DuplicateFn(name)`.
2. Use `Scope::bind_name`, not `scope.names.insert` — `bind_name` is the
   function written to enforce "names within a single scope MUST be unique"
   (Registry §"Resolution and the Active Registry" rule 3), and the direct
   insert bypassed it. This also makes a user fn shadowing a registry callable
   a hard error rather than a silent rebind.
3. A per-file `declared` name set. **`FnGenesis` and `LeafFacts` carry no
   body**, so two same-named fns with IDENTICAL signatures and different bodies
   produce records that compare EQUAL — `insert` returns `Ok` and the
   record-level check cannot see them. Do not remove this set believing the
   conflict check subsumes it; it does not.

`augment_scope_with_user_fns` therefore returns `Result<Scope, _>`; the caller
in `src/pipeline/mod.rs` maps it to `PipelineError::Lower`.

Covered by four binary-level tests in `tests/wave6_failure_modes.rs`
(three rejections + a positive control that keeps them from passing by breaking
M1 compilation generally). M1 baseline re-run after the change: 22 PASS,
identical to the list below.

**Binary-picking footgun in tests.** `tests/wave_s4_bundle_id_tests.rs:31-36`
returns `target/release/axis` whenever it merely EXISTS, so those tests
silently exercise a stale binary on any machine with an older release build —
this produced false failures during the work above. The helper in
`wave6_failure_modes.rs` picks the NEWER of the two profiles by mtime instead.
`wave_s4` is not yet fixed.

### Registry setup

The canonical golden registry is pinned at `registry-pin/` via `.registry-pin` + `scripts/fetch-registry.sh`:

```
# Fetch / refresh the pinned registry:
bash scripts/fetch-registry.sh
```

This clones `https://github.com/christaylor98/axRegistry` at the tag in `.registry-pin`
into `registry-pin/`. The script uses `git clone --branch <tag> --depth 1` — never `git pull`.

**Classification of registry files in this repo:**
- `registry-pin/axis.axreg` — GOLDEN (canonical, pinned from axRegistry v0.5.0)
- `axis-surface-m1-config/m1-registry.axreg` — surface-specific projection; stays in surface config
- `registries/pilot_lowering.axreg` — TEST-FIXTURE (hand-authored test identities, `#[cfg(test)]` refs only)
- `registries/fnref_test.axreg` — TEST-FIXTURE (verification scaffolding)
- `registries/axtest.axreg`, `registries/minimal.axreg`, `registries/core05_example.axreg`, `registries/wave3_test.axreg` — TEST-FIXTUREs

The M1 baseline command below uses the surface-specific and test-fixture registries directly.
`registry-pin/axis.axreg` is the shared runtime callable vocabulary available to consuming projects.

---

## MANDATORY VERIFICATION: M1 END-TO-END TEST

After every change to the pipeline, lowering, verifier, registry, or normalization,
you MUST run all M1 example files through the `axis` binary and confirm the results
match the baseline below. This is the only valid test of the full system.

### Command

```
find examples/surface-m1 -name "*.m1" | sort | while read f; do
  result=$(./target/debug/axis --file "$f" \
    --lexer  axis-surface-m1-config/m1-lexer.yaml \
    --parser axis-surface-m1-config/m1-parse.yaml \
    --schema axis-surface-m1-config/m1-ast.yaml \
    --normalize axis-surface-m1-config/m1-normalize.yaml \
    --reg axis-surface-m1-config/m1-registry.axreg \
    --reg registries/pilot_lowering.axreg \
    --reg registries/fnref_test.axreg 2>&1)
  if echo "$result" | grep -q "→"; then
    echo "PASS  $f"
  else
    echo "FAIL  $f: $(echo "$result" | head -1)"
  fi
done
```

### Baseline (as of M1_BYTE_TYPE_REQUIRED — actual run, 2026-06-26)

**22 PASS** (was 21 under PASS 3; +1 for `id_bytes.m1` confirming Bytes flows through
the pipeline end-to-end). Lambda syntax removed from M1; lambda fixtures reclassified
to PARSE-rejected. fn-ref-by-name confirmed green (`fnref_positive.m1` still PASS).
`validate_m1_lam_rules` deleted; `validate_value_construct_types` intact.
Dead cluster (`lambda_lift.rs`, `registry_lowering.rs`) deleted; build green.

Pre-existing discrepancy outside this baseline's scope: `examples/surface-m1/format/`
contains two PLACEHOLDER files (`bundle_inspect.m1`, `bundle_roundtrip.m1`) added in
commit `8fb51c3` that use `--` comment syntax and intentionally do not compile;
the baseline find-loop reports them as FAIL. They are not M1 source — they document
deferred axbi format work and are not counted in any category below.

**PASS — compile + verify succeeds (22 files):**
- `examples/surface-m1/fn_identity.m1`
- `examples/surface-m1/fn_negate.m1`
- `examples/surface-m1/fnref_positive.m1`
- `examples/surface-m1/id_bytes.m1`
- `examples/surface-m1/ifexpr_decide.m1`
- `examples/surface-m1/matrix/matrix_body_1.m1`
- `examples/surface-m1/matrix/matrix_body_n.m1`
- `examples/surface-m1/matrix/matrix_bool_false.m1`
- `examples/surface-m1/matrix/matrix_bool_true.m1`
- `examples/surface-m1/matrix/matrix_determine.m1`
- `examples/surface-m1/matrix/matrix_int_lit.m1`
- `examples/surface-m1/matrix/matrix_letdecl.m1`
- `examples/surface-m1/matrix/matrix_multi_fn.m1`
- `examples/surface-m1/matrix/matrix_simple.m1`
- `examples/surface-m1/matrix/matrix_string_lit.m1`
- `examples/surface-m1/matrix/matrix_unit.m1`
- `examples/surface-m1/matrix/matrix_value_construct.m1`
- `examples/surface-m1/matrix/matrix_value_construct_nested.m1`
- `examples/surface-m1/matrix/matrix_valuelist_construct.m1`
- `examples/surface-m1/multi_fn_multi_let.m1`
- `examples/surface-m1/two_fn_call.m1`
- `examples/surface-m1/two_fns.m1`

**CORRECTLY REJECTED at parse (6 files — lambda syntax removed from M1 grammar):**
- `matrix_lam_capture.m1` — parse error: `fn(...){}` syntax no longer in M1 grammar
- `matrix_lamexpr.m1` — parse error: `fn(...){}` syntax no longer in M1 grammar
- `matrix_lam_letbound.m1` — parse error: `fn(...){}` syntax no longer in M1 grammar
- `matrix_lam_argpos.m1` — parse error: `fn(...){}` syntax no longer in M1 grammar
- `matrix_value_construct_lam.m1` — parse error: `fn(...){}` syntax no longer in M1 grammar
- `fnref_neg_lambda.m1` — parse error: `fn(...){}` syntax no longer in M1 grammar

**CORRECTLY REJECTED at NF validation (1 file — expected failure):**
- `matrix_value_construct_mistyped.m1` — type mismatch at value construction (Int literal in Text slot)

**CORRECTLY REJECTED at verification (3 files — expected failures):**
- `matrix_callexpr.m1` — TypeMismatch: Bool param passed to Unit parameter of `const_unit`
- `fnref_neg_misplaced.m1` — TypeMismatch: Fn-typed fn-ref passed to Unit parameter of `consume_unit`
- `fnref_neg_nonfn.m1` — TypeMismatch: Int literal passed to Fn parameter of `apply_handler`

**KNOWN DEFERRED GAP (1 file — fail expected until gap is resolved):**
- `example_compound.m1` — parse error: file uses lambda syntax (`fn(...){}`) which no longer
  parses in M1; previously failed at lowering (callable `text_lt` not found). Both issues remain.

### What "correct result" means

Any regression in the PASS list is a bug. Any CORRECTLY REJECTED file that starts
passing is also a bug (it means a validator or verifier check was accidentally
removed).
