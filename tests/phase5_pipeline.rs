// Phase 5 integration tests: end-to-end pipeline + verify + write.
//
// Tests the wired pipeline: source → NF → CoreBundle → Verifier gate → .coreir.
// Verifier-fail cases are exercised directly (without spawning the CLI binary)
// since the surface-0 syntax only produces UnitLit / IfExpr constructs at the
// NF level; CCall-based programs are tested through the direct Verifier API.

use axis_lang::ir::core_ir_05::{
    bundle_identity, write_axbi, serialize_canonical, Builder, CoreBundle, Node,
};
use axis_lang::lowering::emit_05::intern_unit;
use axis_lang::lowering::nf_lowering::lower_nf_to_bundle_05;
use axis_lang::pipeline::{run_pipeline, PipelineConfig};
use axis_lang::registry::core05::fold::Scope;
use axis_lang::registry::core05::record::EffectClass;
use axis_lang::registry::core05::text;
use axis_lang::registry::Registry;
use axis_lang::validation::core_ir::{verify_bundle, VerifyError};
use std::collections::HashMap;
use std::path::PathBuf;

const AXIS_AXREG: &str = include_str!("../registries/axis.axreg");
const PILOT_AXREG: &str = include_str!("../registries/pilot_lowering.axreg");

fn axis_scope() -> Scope {
    text::lower(AXIS_AXREG)
        .expect("axis.axreg must load cleanly")
        .scope
}

fn pilot_scope() -> Scope {
    text::lower(PILOT_AXREG)
        .expect("pilot_lowering.axreg must load cleanly")
        .scope
}

fn surface_0_config_with_scope(source_file: &str, scope: Scope) -> PipelineConfig {
    PipelineConfig {
        lexer_spec: PathBuf::from("axis-surface-0-config/semantic-surface-0-lexer.yaml"),
        parser_spec: PathBuf::from("axis-surface-0-config/semantic-surface-0-parse.yaml"),
        ast_schema: PathBuf::from("axis-surface-0-config/semantic-surface-0-ast.yaml"),
        source_file: PathBuf::from(source_file),
        entry_rule: None,
        normalize_spec: PathBuf::from(
            "axis-surface-0-config/semantic-surface-0-normalize.yaml",
        ),
        registry: Registry::new(),
        core05_scope: Some(scope),
        parser_mode: None,
        hook_registry: None,
    }
}

// ── 1. axis.axreg loads and produces a valid scope ──────────────────────────

#[test]
fn axis_registry_loads_cleanly() {
    let scope = axis_scope();
    // All nine functions should be resolvable by name
    for name in &[
        "unit_id", "seq_unit", "bool_not", "bool_and", "bool_or",
        "int_add", "int_sub", "int_mul", "int_eq", "int_lt",
    ] {
        assert!(
            scope.lookup_name(name).is_some(),
            "axis.axreg should contain '{}'",
            name
        );
    }
}

// ── 2. Verify the registry §5b identities are stable SHA-256 values ─────────

#[test]
fn axis_registry_identities_are_stable() {
    let scope = axis_scope();
    // int_add identity = SHA-256("int_add")
    let id = scope.lookup_name("int_add").expect("int_add must be in scope");
    assert_eq!(
        id.to_hex(),
        "e75ea99b4ee3797ecd1dbc78deed3baa952651bcae06d74458068fa4c5395ed0",
        "int_add must have its §5b bootstrap identity"
    );
}

// ── 3. Full pipeline: unit literal source compiles to a CoreBundle ───────────

#[test]
fn pipeline_unit_literal_produces_bundle() {
    let scope = axis_scope();
    let config = surface_0_config_with_scope("samples/pipeline_test.ax", scope);
    let artifacts = run_pipeline(&config).expect("pipeline must succeed");
    let bundle = artifacts
        .core_bundle
        .expect("core_bundle must be present when scope is set");
    assert_eq!(bundle.constant_pool.len(), 1, "unit literal → one pool entry");
    assert_eq!(bundle.nodes.len(), 0, "unit literal → no nodes");
}

// ── 4. Bundle identity is stable across two runs ─────────────────────────────

#[test]
fn bundle_identity_stable() {
    let mk = || {
        let scope = axis_scope();
        let config = surface_0_config_with_scope("samples/pipeline_test.ax", scope);
        let artifacts = run_pipeline(&config).unwrap();
        bundle_identity(artifacts.core_bundle.as_ref().unwrap())
    };
    assert_eq!(mk(), mk(), "identity must be deterministic");
}

// ── 5. Verifier passes on a valid unit-literal bundle ────────────────────────

#[test]
fn verifier_passes_unit_literal() {
    let scope = axis_scope();
    let config = surface_0_config_with_scope("samples/pipeline_test.ax", scope.clone());
    let artifacts = run_pipeline(&config).unwrap();
    let bundle = artifacts.core_bundle.unwrap();
    verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Pure)
        .expect("unit literal must pass verification");
}

// ── 6. Verifier GATE: unknown CCall target → hard FAIL ───────────────────────

#[test]
fn verifier_rejects_unknown_target() {
    use axis_lang::fabric::hash::mint_identity;

    let scope = Scope::new(); // empty — no functions known
    let unknown_id = mint_identity(); // fresh entropy, guaranteed unknown

    let mut builder = Builder::new().with_scope(&scope);
    let unit_ref = intern_unit(&mut builder);
    // Force-push a CCall without going through the builder's UNKNOWN gate:
    // we manually drop the scope check by building without_scope and then
    // verifying externally.
    let _ = unit_ref;
    drop(builder);

    // Build WITHOUT scope (builder doesn't gate) to get a CCall with unknown target
    let mut builder2 = Builder::new();
    let u = intern_unit(&mut builder2);
    let call_ref = builder2
        .push_node(Node::CCall {
            target_name: "unknown".to_string(),
            target_identity: unknown_id,
            args: vec![u],
        })
        .expect("builder without scope does not gate identity");
    let bundle = builder2.finish(call_ref).unwrap();

    // Verifier must reject it
    let result = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo);
    assert!(
        matches!(result, Err(VerifyError::UnknownTarget { .. })),
        "expected UnknownTarget, got {:?}",
        result
    );
}

// ── 7. Canonical serialization is stable ─────────────────────────────────────

#[test]
fn canonical_bytes_stable() {
    let scope = axis_scope();
    let config1 = surface_0_config_with_scope("samples/pipeline_test.ax", scope.clone());
    let config2 = surface_0_config_with_scope("samples/pipeline_test.ax", scope);
    let b1 = run_pipeline(&config1).unwrap().core_bundle.unwrap();
    let b2 = run_pipeline(&config2).unwrap().core_bundle.unwrap();
    assert_eq!(serialize_canonical(&b1), serialize_canonical(&b2));
}

// ── 8. write_axbi produces non-empty bytes and decode round-trips ──────────

#[test]
fn axbi_encode_decode_roundtrip() {
    use axis_lang::ir::core_ir_05::read_axbi;

    let scope = pilot_scope();
    let mut builder = Builder::new().with_scope(&scope);
    let u = intern_unit(&mut builder);
    let bundle = builder.finish(u).unwrap();

    let bytes = write_axbi(&bundle);
    assert!(!bytes.is_empty(), "encoded bytes must not be empty");

    let decoded = read_axbi(&bytes).expect("read_axbi must succeed");
    assert_eq!(bundle, decoded, "round-trip must preserve the bundle");
}

// ── 9. wave3 gold: const_unit call verifies as Pure ──────────────────────────

#[test]
fn wave3_gold_const_unit_call_verifies_pure() {
    use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
    use axis_lang::frontend::token::{Span, Token, TokenKind};

    let scope = pilot_scope();
    let span = Span::new(0, 10);

    let unit_node = SchemaAstNode {
        kind: "UnitLit".to_string(),
        fields: HashMap::new(),
        annotations: Default::default(),
        span: span.clone(),
    };
    // ArgList wraps the arg expressions — matches the shape produced by the real pipeline.
    let mut arglist_fields = HashMap::new();
    arglist_fields.insert("args".to_string(), SchemaValue::Nodes(vec![unit_node]));
    let arglist_node = SchemaAstNode {
        kind: "ArgList".to_string(),
        fields: arglist_fields,
        annotations: Default::default(),
        span: span.clone(),
    };
    let name_tok = Token {
        kind: TokenKind::Ident,
        lexeme: "const_unit".to_string(),
        span: span.clone(),
    };
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), SchemaValue::Token(name_tok));
    fields.insert("args".to_string(), SchemaValue::Node(Box::new(arglist_node)));
    let call_node = SchemaAstNode {
        kind: "CallExpr".to_string(),
        fields,
        annotations: Default::default(),
        span,
    };

    let bundle = lower_nf_to_bundle_05(&call_node, &scope).expect("lowering must succeed");
    verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Pure)
        .expect("const_unit(()) must verify as Pure");
}

// ── 10. No legacy symbols: CoreBundle has no 0.3 CoreTerm references ─────────
// (compile-time guard — if Phase-3d retired types accidentally reappear,
//  references to them here would fail to compile, surfacing the regression)
#[test]
fn no_legacy_types_in_bundle() {
    // CoreBundle is a 0.5 type. If 0.3 CoreTerm were present this module
    // would need to import crate::ir::core_ir which Phase 3d purged.
    let _: CoreBundle = {
        let scope = Scope::new();
        let mut b = Builder::new().with_scope(&scope);
        let u = intern_unit(&mut b);
        b.finish(u).unwrap()
    };
}

// ─── `callback` DIRECTIVE (HOF callback contracts) ──────────────────────────
//
// An `in (..., Fn)` clause names an opaque primitive: every fn-ref pool entry
// carries the same def_hash, so the slot alone cannot say what the referenced
// fn must accept or return. The `callback` directive supplies that half, and
// the verifier fails closed on an `Fn` slot that lacks one. These tests cover
// the loader half — that a contract parses, resolves, and rejects nonsense.

/// A one-file registry declaring `Fn`, `Int`, `Unit` and a HOF over them, with
/// whatever `callback` lines the caller wants spliced into the fn block.
fn hof_registry(callback_lines: &str) -> String {
    format!(
        "registry cb_test 0.1\n\
         \n\
         type Unit = prim unit\n\
         type Int  = prim int\n\
         type Fn   = prim fn\n\
         \n\
         fn apply_it\n\
        \x20 identity 0x00000000000000000000000000000000000000000000000000000000000000a1\n\
        \x20 kind leaf\n\
        \x20 in (Fn)\n\
        \x20 out Unit\n\
        \x20 effect pure\n\
        \x20 deterministic true\n\
        \x20 idempotent true\n\
         {}\
         end\n",
        callback_lines
    )
}

#[test]
fn callback_directive_resolves_into_the_scope() {
    let lowered = text::lower(&hof_registry("  callback 0 (Int) -> Unit\n"))
        .expect("a well-formed callback contract must load");
    let hof = *lowered
        .scope
        .names
        .get("apply_it")
        .expect("apply_it must be bound");
    let contracts = lowered
        .scope
        .hof_callback_contracts
        .get(&hof)
        .expect("the callback contract must reach the scope");
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].position, 0);
    assert_eq!(contracts[0].inputs.len(), 1, "contract takes one argument");
    assert_eq!(
        contracts[0].output,
        *lowered.scope.names.get("Unit").unwrap(),
        "contract returns Unit"
    );
}

#[test]
fn callback_contracts_do_not_change_canonical_bytes() {
    // Contracts are scope-only, like the projection name table. Declaring one
    // must not perturb the canonical record stream — otherwise annotating an
    // existing registry would silently re-identify every fn in it.
    let without = text::lower(&hof_registry("")).unwrap();
    let with = text::lower(&hof_registry("  callback 0 (Int) -> Unit\n")).unwrap();
    assert_eq!(
        without.canonical_bytes(),
        with.canonical_bytes(),
        "a callback contract must leave canonical bytes untouched"
    );
}

#[test]
fn callback_on_a_non_fn_parameter_is_rejected() {
    // `out Unit` is position-less; position 0 here is the Fn slot, so ask for
    // a contract on a slot that does not exist as an Fn.
    let src = hof_registry("").replace("  in (Fn)", "  in (Int)");
    let src = src.replace("  out Unit", "  out Unit\n  callback 0 (Int) -> Unit");
    let err = text::lower(&src).expect_err("a contract on a non-Fn slot must be rejected");
    let msg = format!("{}", err);
    assert!(
        msg.contains("not an `Fn` parameter"),
        "error should say the slot is not Fn: {}",
        msg
    );
}

#[test]
fn callback_position_out_of_range_is_rejected() {
    let err = text::lower(&hof_registry("  callback 3 (Int) -> Unit\n"))
        .expect_err("an out-of-range callback position must be rejected");
    let msg = format!("{}", err);
    assert!(
        msg.contains("out of range"),
        "error should say the position is out of range: {}",
        msg
    );
}

#[test]
fn callback_with_an_unknown_type_is_rejected() {
    let err = text::lower(&hof_registry("  callback 0 (Nonesuch) -> Unit\n"))
        .expect_err("an unknown type in a contract must be rejected");
    let msg = format!("{}", err);
    assert!(
        msg.contains("Nonesuch"),
        "error should name the unknown type: {}",
        msg
    );
}

#[test]
fn duplicate_callback_for_one_position_is_rejected() {
    let err = text::lower(&hof_registry(
        "  callback 0 (Int) -> Unit\n  callback 0 (Unit) -> Unit\n",
    ))
    .expect_err("two contracts for one position must be rejected");
    let msg = format!("{}", err);
    assert!(
        msg.contains("duplicate callback"),
        "error should name the duplicate: {}",
        msg
    );
}
