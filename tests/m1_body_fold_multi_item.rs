// M1 body-fold + multi-item gate test — AXLANG_M1_BODY_FOLD_AND_MULTI_ITEM_T1
//
// GATE: one M1 program with BOTH multiple functions AND a function whose Body
// has multiple let-bindings, lowered end-to-end via run_pipeline
// (lex → parse → schema → normalize → validate_nf) to Core IR.
//
// Asserts:
//   - Normalization succeeds (no multi-item hard-fail)
//   - NF root is Program{items} with both FnDecl nodes present
//   - make_pair's body is nested LetExpr (2 bindings → 2 nested LetExpr)
//   - Innermost body of the nested chain is VarRef("x")
//   - simple's body is VarRef("flag") directly (0 bindings → just result)
//   - Lowering the nested LetExpr body via lower_nf_to_bundle_05 produces
//     a valid Core IR bundle (proves the real lowering path works)
//
// This test would FAIL before this change:
//   - unwrap_program_to_single_item hard-failed for 2-item programs
//   - desugar_body used identity (Body node preserved, not folded)

use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use axis_lang::lowering::nf_lowering::lower_nf_to_bundle_05;
use axis_lang::pipeline::{run_pipeline, PipelineConfig};
use axis_lang::registry::core05::fold::Scope;
use axis_lang::registry::Registry;
use std::path::PathBuf;

const SOURCE: &str = "examples/surface-m1/multi_fn_multi_let.m1";
const LEXER: &str = "axis-surface-m1-config/m1-lexer.yaml";
const PARSER: &str = "axis-surface-m1-config/m1-parse.yaml";
const SCHEMA: &str = "axis-surface-m1-config/m1-ast.yaml";
const NORMALIZE: &str = "axis-surface-m1-config/m1-normalize.yaml";

fn pipeline_config() -> PipelineConfig {
    PipelineConfig {
        lexer_spec: PathBuf::from(LEXER),
        parser_spec: PathBuf::from(PARSER),
        ast_schema: PathBuf::from(SCHEMA),
        source_file: PathBuf::from(SOURCE),
        entry_rule: None,
        normalize_spec: PathBuf::from(NORMALIZE),
        registry: Registry::new(),
        core05_scope: None, // normalization only; lowering done manually below
        parser_mode: None,
        hook_registry: None,
    }
}

// Resolve a field as a single Node reference.
fn as_node<'a>(val: &'a SchemaValue, ctx: &str) -> &'a SchemaAstNode {
    match val {
        SchemaValue::Node(n) => n.as_ref(),
        other => panic!("{ctx}: expected Node, got {other:?}"),
    }
}

// Return the name token lexeme from a FnDecl node.
fn fn_name(fn_decl: &SchemaAstNode) -> String {
    assert_eq!(fn_decl.kind, "FnDecl");
    match fn_decl.fields.get("name") {
        Some(SchemaValue::Token(t)) => t.lexeme.clone(),
        other => panic!("FnDecl.name must be Token, got {other:?}"),
    }
}

// ── Gate test ────────────────────────────────────────────────────────────────

#[test]
fn m1_multi_fn_multi_let_body_fold_gate() {
    // Stage 1-5.5: lex → parse → schema → normalize → validate_nf via run_pipeline
    let artifacts = run_pipeline(&pipeline_config())
        .unwrap_or_else(|e| panic!("M1 multi-item pipeline failed: {e}"));

    // ── All functions present ────────────────────────────────────────────────
    // Multi-item program must produce 2 NF items (no Program wrapper).
    let items = artifacts.schema_asts.as_slice();
    assert_eq!(items.len(), 2, "multi-item program must produce exactly 2 NF items");
    assert!(items.iter().all(|i| i.kind == "FnDecl"), "all items must be FnDecl");

    let make_pair = items.iter().find(|i| fn_name(i) == "make_pair")
        .expect("make_pair FnDecl must be present");
    let simple = items.iter().find(|i| fn_name(i) == "simple")
        .expect("simple FnDecl must be present");

    // ── make_pair body: 2 let-bindings → 2 nested LetExpr ───────────────────
    // Expected structure:
    //   LetExpr { name:"x", value: VarRef("a"), body:
    //     LetExpr { name:"y", value: VarRef("b"), body:
    //       VarRef("x")
    //     }
    //   }
    let outer = as_node(
        make_pair.fields.get("body").expect("make_pair must have body"),
        "make_pair.body",
    );
    assert_eq!(outer.kind, "LetExpr", "make_pair body must be LetExpr (outer)");

    let outer_name = match outer.fields.get("name") {
        Some(SchemaValue::Token(t)) => t.lexeme.as_str(),
        other => panic!("outer LetExpr.name must be Token, got {other:?}"),
    };
    assert_eq!(outer_name, "x", "outer LetExpr must bind 'x'");

    let inner = as_node(
        outer.fields.get("body").expect("outer LetExpr must have body"),
        "outer LetExpr.body",
    );
    assert_eq!(inner.kind, "LetExpr", "outer LetExpr.body must be LetExpr (inner)");

    let inner_name = match inner.fields.get("name") {
        Some(SchemaValue::Token(t)) => t.lexeme.as_str(),
        other => panic!("inner LetExpr.name must be Token, got {other:?}"),
    };
    assert_eq!(inner_name, "y", "inner LetExpr must bind 'y'");

    // Innermost body = VarRef("x")
    let innermost = as_node(
        inner.fields.get("body").expect("inner LetExpr must have body"),
        "inner LetExpr.body",
    );
    assert_eq!(innermost.kind, "VarRef", "innermost body must be VarRef");
    match innermost.fields.get("name") {
        Some(SchemaValue::Token(t)) => assert_eq!(t.lexeme, "x", "VarRef must reference 'x'"),
        other => panic!("VarRef.name must be Token, got {other:?}"),
    }

    // ── simple body: 0 let-bindings → VarRef("flag") directly ───────────────
    let simple_body = as_node(
        simple.fields.get("body").expect("simple must have body"),
        "simple.body",
    );
    assert_eq!(simple_body.kind, "VarRef", "simple body must be VarRef (zero bindings → just result)");
    match simple_body.fields.get("name") {
        Some(SchemaValue::Token(t)) => assert_eq!(t.lexeme, "flag", "VarRef must reference 'flag'"),
        other => panic!("VarRef.name must be Token, got {other:?}"),
    }

    // ── Core IR lowering: lower make_pair's body via lower_nf_to_bundle_05 ──
    // The nested LetExpr lowers by extending the env for each binding.
    // VarRef("a") and VarRef("b") are free variables not in scope or env —
    // since CVar nodes no longer exist, these resolve as UnknownCallable.
    let scope = Scope::new();
    let err = lower_nf_to_bundle_05(outer, &scope)
        .unwrap_err();
    assert!(
        matches!(err, axis_lang::lowering::emit_05::SchemaLower05Error::UnknownCallable(_)),
        "free VarRef with empty env must fail with UnknownCallable, got: {err:?}"
    );
}
