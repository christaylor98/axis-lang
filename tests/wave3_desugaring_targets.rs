// WAVE 3: Desugaring Target Tests
//
// Tests that ForExpr, WhileExpr, MatchExpr are properly forbidden by NF validation
// and that H1 wrapper nodes are admissible

use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use axis_lang::frontend::token::Span;
use axis_lang::normalize::nf_validation::validate_nf;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// TEST HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn make_node(kind: &str) -> SchemaAstNode {
    SchemaAstNode {
        kind: kind.to_string(),
        fields: HashMap::new(),
        annotations: Vec::new(),
        span: Span::new(0, 10),
    }
}

fn make_node_with_field(kind: &str, field_name: &str, child: SchemaAstNode) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(field_name.to_string(), SchemaValue::Node(Box::new(child)));
    SchemaAstNode {
        kind: kind.to_string(),
        fields,
        annotations: Vec::new(),
        span: Span::new(0, 20),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WAVE 3 DESUGARING TARGET TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_for_expr_is_forbidden() {
    // ForExpr MUST be rejected by NF validation
    let for_expr = make_node("ForExpr");
    let result = validate_nf(&for_expr);

    assert!(result.is_err(), "ForExpr should be forbidden in NF");

    if let Err(e) = result {
        assert!(
            e.message.contains("forbidden") || e.message.contains("ForExpr"),
            "Error should indicate ForExpr is forbidden, got: {}",
            e.message
        );
    }
}

#[test]
fn test_while_expr_is_forbidden() {
    // WhileExpr MUST be rejected by NF validation
    let while_expr = make_node("WhileExpr");
    let result = validate_nf(&while_expr);

    assert!(result.is_err(), "WhileExpr should be forbidden in NF");

    if let Err(e) = result {
        assert!(
            e.message.contains("forbidden") || e.message.contains("WhileExpr"),
            "Error should indicate WhileExpr is forbidden, got: {}",
            e.message
        );
    }
}

#[test]
fn test_loop_expr_is_forbidden() {
    // LoopExpr MUST be rejected by NF validation
    let loop_expr = make_node("LoopExpr");
    let result = validate_nf(&loop_expr);

    assert!(result.is_err(), "LoopExpr should be forbidden in NF");

    if let Err(e) = result {
        assert!(
            e.message.contains("forbidden") || e.message.contains("LoopExpr"),
            "Error should indicate LoopExpr is forbidden, got: {}",
            e.message
        );
    }
}

#[test]
fn test_match_expr_is_forbidden() {
    // MatchExpr MUST be rejected by NF validation
    let match_expr = make_node("MatchExpr");
    let result = validate_nf(&match_expr);

    assert!(result.is_err(), "MatchExpr should be forbidden in NF");

    if let Err(e) = result {
        assert!(
            e.message.contains("forbidden") || e.message.contains("MatchExpr"),
            "Error should indicate MatchExpr is forbidden, got: {}",
            e.message
        );
    }
}

#[test]
fn test_pattern_is_forbidden() {
    // Pattern MUST be rejected by NF validation
    let pattern = make_node("Pattern");
    let result = validate_nf(&pattern);

    assert!(result.is_err(), "Pattern should be forbidden in NF");

    if let Err(e) = result {
        assert!(
            e.message.contains("forbidden") || e.message.contains("Pattern"),
            "Error should indicate Pattern is forbidden, got: {}",
            e.message
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// H1 WRAPPER NODE TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_h1_wrapper_nodes_are_admissible() {
    // H1 structural nodes should be admissible (will be normalized away)
    let h1_wrappers = vec![
        "Item",
        "FnDecl",
        "AssignStmt",
        "BlockContent",
        "Stmt",
        "ExprStmt",
        "ParamList",
        "Param",
    ];

    for kind in h1_wrappers {
        let node = make_node(kind);
        let result = validate_nf(&node);
        assert!(
            result.is_ok(),
            "H1 wrapper node '{}' should be admissible in NF (to be normalized away)",
            kind
        );
    }
}

#[test]
fn test_nested_forbidden_construct_is_caught() {
    // ForExpr nested inside admissible nodes should still be caught
    let for_expr = make_node("ForExpr");
    let block = make_node_with_field("Block", "body", for_expr);
    let fn_decl = make_node_with_field("FnDecl", "body", block);

    let result = validate_nf(&fn_decl);
    assert!(
        result.is_err(),
        "Nested ForExpr should be caught by NF validation"
    );

    if let Err(e) = result {
        assert!(
            e.node_kind == "ForExpr",
            "Error should identify ForExpr as the forbidden node"
        );
    }
}
