// WAVE 3: Desugaring Unit Tests
//
// Tests that desugaring transformations work correctly

use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use axis_lang::frontend::token::{Span, Token, TokenKind};
use axis_lang::normalize::desugaring::desugar_node;
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

#[allow(dead_code)]
fn make_token(name: &str) -> Token {
    Token {
        kind: TokenKind::Ident,
        lexeme: name.to_string(),
        span: Span::new(0, name.len()),
    }
}

fn make_node_with_fields(kind: &str, fields: HashMap<String, SchemaValue>) -> SchemaAstNode {
    SchemaAstNode {
        kind: kind.to_string(),
        fields,
        annotations: Vec::new(),
        span: Span::new(0, 10),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DESUGARING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_loop_expr_desugars_to_lambda() {
    // LoopExpr should desugar to LamExpr
    let body = make_node("UnitLit");
    let mut fields = HashMap::new();
    fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));

    let loop_expr = make_node_with_fields("LoopExpr", fields);
    let result = desugar_node(&loop_expr);

    assert_eq!(result.kind, "LamExpr", "LoopExpr should desugar to LamExpr");
    assert!(
        result.fields.contains_key("param"),
        "LamExpr should have param field"
    );
    assert!(
        result.fields.contains_key("body"),
        "LamExpr should have body field"
    );
}

#[test]
fn test_while_expr_desugars_to_lambda_with_if() {
    // WhileExpr should desugar to LamExpr containing IfExpr
    let condition = make_node("BoolLit");
    let body = make_node("UnitLit");

    let mut fields = HashMap::new();
    fields.insert(
        "condition".to_string(),
        SchemaValue::Node(Box::new(condition)),
    );
    fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));

    let while_expr = make_node_with_fields("WhileExpr", fields);
    let result = desugar_node(&while_expr);

    assert_eq!(
        result.kind, "LamExpr",
        "WhileExpr should desugar to LamExpr"
    );

    // Check that body contains IfExpr
    if let Some(SchemaValue::Node(lam_body)) = result.fields.get("body") {
        assert_eq!(lam_body.kind, "IfExpr", "LamExpr body should be IfExpr");
    } else {
        panic!("LamExpr should have body field");
    }
}

#[test]
fn test_wrapper_unwrapping() {
    // Item wrapper should be unwrapped
    let inner = make_node("IntLit");
    let mut fields = HashMap::new();
    fields.insert("variant".to_string(), SchemaValue::Node(Box::new(inner)));

    let wrapper = make_node_with_fields("Item", fields);
    let result = desugar_node(&wrapper);

    assert_eq!(result.kind, "IntLit", "Item wrapper should be unwrapped");
}

#[test]
fn test_expr_wrapper_unwrapping() {
    // Expr wrapper should be unwrapped
    let inner = make_node("VarRef");
    let mut fields = HashMap::new();
    fields.insert("variant".to_string(), SchemaValue::Node(Box::new(inner)));

    let wrapper = make_node_with_fields("Expr", fields);
    let result = desugar_node(&wrapper);

    assert_eq!(result.kind, "VarRef", "Expr wrapper should be unwrapped");
}

#[test]
fn test_desugared_node_passes_nf_validation() {
    // A desugared LoopExpr should pass NF validation
    let body = make_node("UnitLit");
    let mut fields = HashMap::new();
    fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));

    let loop_expr = make_node_with_fields("LoopExpr", fields);
    let desugared = desugar_node(&loop_expr);

    let validation_result = validate_nf(&desugared);
    assert!(
        validation_result.is_ok(),
        "Desugared LoopExpr should pass NF validation, got: {:?}",
        validation_result
    );
}

#[test]
fn test_nf_admissible_nodes_unchanged() {
    // Nodes already in NF should pass through unchanged
    let let_expr = make_node("LetExpr");
    let result = desugar_node(&let_expr);

    assert_eq!(result.kind, "LetExpr", "NF nodes should pass through");
}

#[test]
fn test_recursive_desugaring() {
    // Nested forbidden constructs should be fully desugared
    let inner_loop = make_node_with_fields("LoopExpr", {
        let mut f = HashMap::new();
        f.insert(
            "body".to_string(),
            SchemaValue::Node(Box::new(make_node("UnitLit"))),
        );
        f
    });

    let outer_loop = make_node_with_fields("LoopExpr", {
        let mut f = HashMap::new();
        f.insert("body".to_string(), SchemaValue::Node(Box::new(inner_loop)));
        f
    });

    let result = desugar_node(&outer_loop);

    // Both loops should be desugared to nested lambdas
    assert_eq!(result.kind, "LamExpr", "Outer loop should be LamExpr");

    if let Some(SchemaValue::Node(outer_body)) = result.fields.get("body") {
        assert_eq!(
            outer_body.kind, "LamExpr",
            "Inner loop should also be desugared to LamExpr"
        );
    } else {
        panic!("Nested desugaring failed");
    }
}
