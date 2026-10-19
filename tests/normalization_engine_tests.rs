// Normalization Engine Tests
//
// SCOPE:
// - Test normalization rule application
// - Test NF boundary enforcement
// - Test error handling

use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use axis_lang::frontend::token::{Span, Token, TokenKind};
use axis_lang::normalize::{NormalizationContext, NormalizationError};
use std::collections::HashMap;
use std::path::Path;

fn make_token(kind: TokenKind, lexeme: &str) -> Token {
    Token {
        kind,
        lexeme: lexeme.to_string(),
        span: Span {
            start: 0,
            end: lexeme.len(),
        },
    }
}

fn make_int_lit_ast(value: i64) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        SchemaValue::Token(make_token(TokenKind::IntLit, &value.to_string())),
    );
    SchemaAstNode {
        kind: "IntLit".to_string(),
        fields,
        annotations: vec![],
        span: Span {
            start: 0,
            end: value.to_string().len(),
        },
    }
}

fn make_var_ref_ast(name: &str) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(
        "name".to_string(),
        SchemaValue::Token(make_token(TokenKind::Ident, name)),
    );
    SchemaAstNode {
        kind: "VarRef".to_string(),
        fields,
        annotations: vec![],
        span: Span {
            start: 0,
            end: name.len(),
        },
    }
}

fn make_let_expr_ast(name: &str, value: SchemaAstNode, body: SchemaAstNode) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(
        "name".to_string(),
        SchemaValue::Token(make_token(TokenKind::Ident, name)),
    );
    fields.insert("value".to_string(), SchemaValue::Node(Box::new(value)));
    fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));

    let span_end = 10 + name.len();
    SchemaAstNode {
        kind: "LetExpr".to_string(),
        fields,
        annotations: vec![],
        span: Span {
            start: 0,
            end: span_end,
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AI2 NORMALIZATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_ai2_normalize_int_literal() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load AI2 normalization context");

    let ast = make_int_lit_ast(42);
    let result = ctx.normalize(ast);

    assert!(
        result.is_ok(),
        "AI2 IntLit normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "IntLit");
}

#[test]
fn test_ai2_normalize_var_ref() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load AI2 normalization context");

    let ast = make_var_ref_ast("x");
    let result = ctx.normalize(ast);

    assert!(
        result.is_ok(),
        "AI2 VarRef normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "VarRef");
}

#[test]
fn test_ai2_normalize_let_expr() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load AI2 normalization context");

    let value = make_int_lit_ast(5);
    let body = make_var_ref_ast("x");
    let ast = make_let_expr_ast("x", value, body);

    let result = ctx.normalize(ast);
    assert!(
        result.is_ok(),
        "AI2 LetExpr normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "LetExpr");
}

#[test]
fn test_ai2_unknown_node_kind_fails() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load AI2 normalization context");

    let ast = SchemaAstNode {
        kind: "UnknownNode".to_string(),
        fields: HashMap::new(),
        annotations: vec![],
        span: Span { start: 0, end: 10 },
    };

    let result = ctx.normalize(ast);
    assert!(result.is_err(), "Should fail on unknown node kind");

    match result {
        Err(NormalizationError::RewriteError { message, .. }) => {
            assert!(message.contains("No normalization rule matches"));
        }
        _ => panic!("Expected RewriteError on unknown node kind"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AI1 NORMALIZATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_ai1_normalize_int_literal() {
    let rules_path = Path::new("axis-surface-ai1-config/ai1-rpn-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load AI1 normalization context");

    let ast = make_int_lit_ast(100);
    let result = ctx.normalize(ast);

    assert!(
        result.is_ok(),
        "AI1 IntLit normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "IntLit");
}

#[test]
fn test_ai1_normalize_var_ref() {
    let rules_path = Path::new("axis-surface-ai1-config/ai1-rpn-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load AI1 normalization context");

    let ast = make_var_ref_ast("foo");
    let result = ctx.normalize(ast);

    assert!(
        result.is_ok(),
        "AI1 VarRef normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "VarRef");
}

// ═══════════════════════════════════════════════════════════════════════════
// SURFACE-0 NORMALIZATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_surface0_normalize_int_literal() {
    let rules_path = Path::new("axis-surface-0-config/surface-0-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx = NormalizationContext::load(rules_path)
        .expect("Failed to load Surface-0 normalization context");

    let ast = make_int_lit_ast(999);
    let result = ctx.normalize(ast);

    assert!(
        result.is_ok(),
        "Surface-0 IntLit normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "IntLit");
}

// ═══════════════════════════════════════════════════════════════════════════
// H1 NORMALIZATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_h1_normalize_int_literal() {
    let rules_path = Path::new("axis-surface-h1-config/surface-h1-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx =
        NormalizationContext::load(rules_path).expect("Failed to load H1 normalization context");

    let ast = make_int_lit_ast(777);
    let result = ctx.normalize(ast);

    assert!(
        result.is_ok(),
        "H1 IntLit normalization failed: {:?}",
        result.err()
    );

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.node().kind, "IntLit");
}

// ═══════════════════════════════════════════════════════════════════════════
// NF VALIDATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nf_validation_rejects_forbidden_node() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let ctx = NormalizationContext::load(rules_path).expect("Failed to load normalization context");

    // Create a node with a forbidden kind (not in NF admissible set)
    let ast = SchemaAstNode {
        kind: "ForbiddenNodeKind".to_string(),
        fields: HashMap::new(),
        annotations: vec![],
        span: Span { start: 0, end: 10 },
    };

    let result = ctx.normalize(ast);

    // Should fail either at rule matching or NF validation
    assert!(result.is_err(), "Should reject forbidden node kind");
}
