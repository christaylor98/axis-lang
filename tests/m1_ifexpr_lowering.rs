// M1 end-to-end IfExpr lowering — AXLANG_M1_IFEXPR_LOWERING_FIX_V1
//
// Proves that an IfExpr-bearing M1 program lowers end-to-end to a Core IR
// CIf with cond, then_, and else_ all populated.  This test did NOT exist
// before the fix and would fail if m1-ast.yaml reverted to the old
// then_expr/else_expr names while the updated m1-normalize.yaml pattern
// already expects then_branch/else_branch.
//
// Pipeline: M1 source → lex → parse → schema project (m1-ast.yaml)
//           → normalize (m1-normalize.yaml) → lower (nf_lowering / m1-lowering.yaml)
//
// The source uses VarRef branches (not literal values) so it avoids the
// BoolLit/IntLit normalization gap that is a separate, pre-existing
// limitation of m1-normalize.yaml.

use axis_lang::frontend::schema_ast::SchemaValue;
use axis_lang::ir::core_ir_05::Node;
use axis_lang::lowering::nf_lowering::lower_nf_to_bundle_05;
use axis_lang::pipeline::{run_pipeline, PipelineConfig};
use axis_lang::registry::core05::fold::Scope;
use axis_lang::registry::Registry;
use std::path::{Path, PathBuf};

const M1_SOURCE: &str = "examples/surface-m1/ifexpr_decide.m1";
const M1_LEXER: &str = "axis-surface-m1-config/m1-lexer.yaml";
const M1_PARSER: &str = "axis-surface-m1-config/m1-parse.yaml";
const M1_SCHEMA: &str = "axis-surface-m1-config/m1-ast.yaml";
const M1_NORMALIZE: &str = "axis-surface-m1-config/m1-normalize.yaml";

fn m1_pipeline_config() -> PipelineConfig {
    PipelineConfig {
        lexer_spec: PathBuf::from(M1_LEXER),
        parser_spec: PathBuf::from(M1_PARSER),
        ast_schema: PathBuf::from(M1_SCHEMA),
        source_file: PathBuf::from(M1_SOURCE),
        entry_rule: None,
        normalize_spec: PathBuf::from(M1_NORMALIZE),
        registry: Registry::new(),
        core05_scope: None,
        parser_mode: None,
        hook_registry: None,
    }
}

// Navigate FnDecl → IfExpr from a NF root node.
// After body-fold, a zero-binding Body folds to just the result expr,
// so FnDecl.body is the IfExpr directly (no Body wrapper).
fn extract_ifexpr<'a>(
    nf_root: &'a axis_lang::frontend::schema_ast::SchemaAstNode,
) -> &'a axis_lang::frontend::schema_ast::SchemaAstNode {
    assert_eq!(nf_root.kind, "FnDecl", "NF root must be FnDecl");

    let body = match nf_root.fields.get("body") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        other => panic!("FnDecl.body must be a Node, got: {other:?}"),
    };
    assert_eq!(body.kind, "IfExpr", "FnDecl.body must be IfExpr (zero-binding body folds to result)");
    body
}

// ── 1. NF IfExpr carries canonical branch field names ────────────────────────
//
// After parse + normalize:
//   m1-ast.yaml produces IfExpr with then_branch/else_branch (canonical names).
//   m1-normalize.yaml passes them through unchanged (identity, no rename).
//
// This test fails if m1-ast.yaml reverts to then_expr/else_expr: the updated
// normalization pattern (expecting then_branch) then fails to match, and
// run_pipeline returns a NormalizationError.
#[test]
fn m1_nf_ifexpr_has_canonical_branch_names() {
    let artifacts = run_pipeline(&m1_pipeline_config())
        .unwrap_or_else(|e| panic!("M1 pipeline failed: {e}"));

    let if_expr = extract_ifexpr(&artifacts.schema_asts[0]);

    assert!(
        if_expr.fields.contains_key("then_branch"),
        "NF IfExpr must have 'then_branch' — the canonical branch field name"
    );
    assert!(
        if_expr.fields.contains_key("else_branch"),
        "NF IfExpr must have 'else_branch' — the canonical branch field name"
    );
    assert!(
        if_expr.fields.contains_key("condition"),
        "NF IfExpr must have 'condition'"
    );
    // Old names must NOT be present — no alias layer
    assert!(
        !if_expr.fields.contains_key("then_expr"),
        "NF IfExpr must NOT have 'then_expr' (old name); rename was removed"
    );
    assert!(
        !if_expr.fields.contains_key("else_expr"),
        "NF IfExpr must NOT have 'else_expr' (old name); rename was removed"
    );
}

// ── 2. NF lowering path: unbound VarRef → UnknownCallable (expected) ──────────
//
// The NF lowering path (`lower_nf_to_bundle_05`) no longer emits CVar nodes.
// VarRefs that are not in the env return UnknownCallable instead.
// Function parameters are set up as sentinel pool entries only when lowering
// via the spec-driven path (CFnDecl → lower_curried_lam_spec).
//
// This test verifies that the nf_lowering path fails predictably for free
// VarRefs, rather than silently producing CVar nodes.
#[test]
fn m1_nf_lowering_free_varref_returns_unknown_callable() {
    use axis_lang::lowering::emit_05::SchemaLower05Error;
    let artifacts = run_pipeline(&m1_pipeline_config())
        .unwrap_or_else(|e| panic!("M1 pipeline failed: {e}"));

    let if_expr = extract_ifexpr(&artifacts.schema_asts[0]);

    // Empty scope — VarRef("flag") is not in env → UnknownCallable.
    let scope = Scope::new();
    let err = lower_nf_to_bundle_05(if_expr, &scope)
        .unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(_)),
        "free VarRef with empty env must yield UnknownCallable, got: {err:?}"
    );
}

// ── 3. NF path: FnDecl containing IfExpr → CIf via canonical lowering ────────
//
// Lowers the full FnDecl (with `flag` param producing a Param-typed pool
// entry) and asserts the body's IfExpr lowers to a CIf node. Replaces the
// prior spec_driven path test now that lowering has consolidated to one
// hardcoded path.
//
// Fails if:
//   - the IfExpr field names (condition/then_branch/else_branch) don't match
//     the surface expected by the lowering
//   - the body lowering can't resolve VarRef("flag") via the param-slot env
#[test]
fn m1_ifexpr_lowers_to_cif_via_nf_path() {
    let artifacts = run_pipeline(&m1_pipeline_config())
        .unwrap_or_else(|e| panic!("M1 pipeline failed: {e}"));

    // Lower the FnDecl through nf_lowering — param slots are interned as
    // Param-typed pool entries; VarRef("flag") resolves to that slot via env.
    let scope = Scope::new();
    let bundle = lower_nf_to_bundle_05(&artifacts.schema_asts[0], &scope)
        .unwrap_or_else(|e| panic!("NF lowering of FnDecl failed: {e}"));

    assert!(
        !bundle.nodes.is_empty(),
        "bundle must have at least one node"
    );
    assert!(
        bundle.nodes.iter().any(|n| matches!(n, Node::CIf { .. })),
        "lowering must produce a CIf"
    );
}
