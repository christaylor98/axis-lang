// WAVE 1: Normal Form Validation Tests
//
// Tests for NF validation in the pipeline

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

fn make_node_with_child(kind: &str, field_name: &str, child: SchemaAstNode) -> SchemaAstNode {
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
// UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nf_validation_passes_for_admissible_nodes() {
    // All these nodes should be in the admissible set
    let test_cases = vec![
        "Program", "LetExpr", "IfExpr", "LamExpr", "AppExpr", "VarRef", "IntLit", "BoolLit",
        "UnitLit",
    ];

    for kind in test_cases {
        let node = make_node(kind);
        let result = validate_nf(&node);
        assert!(
            result.is_ok(),
            "Node kind '{}' should pass NF validation",
            kind
        );
    }
}

#[test]
fn test_nf_validation_fails_for_forbidden_nodes() {
    // These are explicitly forbidden
    let test_cases = vec!["ForExpr", "LoopExpr", "WhileExpr", "MatchExpr", "Pattern"];

    for kind in test_cases {
        let node = make_node(kind);
        let result = validate_nf(&node);
        assert!(
            result.is_err(),
            "Forbidden node kind '{}' should fail NF validation",
            kind
        );
        if let Err(e) = result {
            assert!(
                e.message.contains("forbidden"),
                "Error for '{}' should mention 'forbidden'",
                kind
            );
        }
    }
}

#[test]
fn test_nf_validation_fails_for_unknown_nodes() {
    let unknown_nodes = vec!["UnknownNode", "SomeRandomNode", "NotInSpec"];

    for kind in unknown_nodes {
        let node = make_node(kind);
        let result = validate_nf(&node);
        assert!(
            result.is_err(),
            "Unknown node kind '{}' should fail NF validation",
            kind
        );
    }
}

#[test]
fn test_nf_validation_recursive() {
    // Valid parent with valid child should pass
    let child = make_node("VarRef");
    let parent = make_node_with_child("LetExpr", "value", child);
    assert!(validate_nf(&parent).is_ok());
}

#[test]
fn test_nf_validation_fails_on_nested_forbidden() {
    // Valid parent with forbidden child should fail
    let forbidden_child = make_node("ForExpr");
    let parent = make_node_with_child("LetExpr", "value", forbidden_child);
    let result = validate_nf(&parent);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.node_kind, "ForExpr");
        assert_eq!(e.parent_kind, Some("LetExpr".to_string()));
    }
}

#[test]
fn test_wrapper_nodes_allowed() {
    // Wrappers are transparent and allowed
    let expr = make_node("Expr");
    assert!(validate_nf(&expr).is_ok());

    let atomic = make_node("AtomicExpr");
    assert!(validate_nf(&atomic).is_ok());
}

#[test]
fn test_semantic_surface_0_nodes_pass() {
    // These are the actual nodes from Semantic-Surface-0
    let semantic_nodes = vec![
        "IntLit",
        "BoolLit",
        "UnitLit",
        "VarRef",
        "LetExpr",
        "LamExpr",
        "IfExpr",
        "AppExpr",
        "Expr",
        "AtomicExpr",
    ];

    for kind in semantic_nodes {
        let node = make_node(kind);
        let result = validate_nf(&node);
        assert!(
            result.is_ok(),
            "Semantic-Surface-0 node '{}' should pass NF validation",
            kind
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ERROR REPORTING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_error_includes_span() {
    let node = SchemaAstNode {
        kind: "ForExpr".to_string(),
        fields: HashMap::new(),
        annotations: Vec::new(),
        span: Span::new(42, 100),
    };

    let result = validate_nf(&node);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.span.start, 42);
        assert_eq!(e.span.end, 100);
    }
}

#[test]
fn test_error_includes_node_kind() {
    let node = make_node("LoopExpr");
    let result = validate_nf(&node);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.node_kind, "LoopExpr");
    }
}

#[test]
fn test_error_message_quality() {
    let node = make_node("ForExpr");
    let result = validate_nf(&node);
    assert!(result.is_err());
    if let Err(e) = result {
        let msg = e.message.to_lowercase();
        assert!(
            msg.contains("forexpr") || msg.contains("for"),
            "Error message should mention the forbidden node"
        );
        assert!(
            msg.contains("forbidden") || msg.contains("not"),
            "Error message should indicate it's forbidden"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// INTEGRATION TESTS (require full pipeline)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_pipeline_rejects_non_nf_ast() {
    // This would require a full pipeline test with a source file
    // that produces non-NF AST. Skipped for now since we don't have
    // normalisation yet (WAVE 1 scope is validation only).
    // TODO: Add in WAVE 2 when normalisation is implemented
}

#[test]
fn test_inspection_shows_nf_status() {
    // Test that --inspect nf shows validation results
    // This would require running the binary and parsing output
    // Skipped for unit tests - covered by manual testing
    // TODO: Add as integration test if needed
}
