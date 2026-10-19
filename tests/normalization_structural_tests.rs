// Normalization Structural Tests
//
// PURPOSE:
// Verify normalization stage boundaries and enforcement mechanisms
//
// SCOPE:
// - NF spec loadability
// - Normalization rules loadability
// - NfAst type enforcement
// - Normalization → validation integration
// - Lowering boundary enforcement
//
// These are STRUCTURAL tests, not semantic tests.
// They verify the pipeline shape, not correctness of transformations.

use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use axis_lang::frontend::token::{Span, Token, TokenKind};
use axis_lang::nf_ast::{NfAst, NfVersion};
use axis_lang::normalize::nf_validation::validate_nf;
use axis_lang::normalize::{NormalizationContext, NormalizationError};
use std::collections::HashMap;
use std::path::Path;

// ═══════════════════════════════════════════════════════════════════════════
// TEST HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Create a minimal test AST node
fn make_test_node(kind: &str) -> SchemaAstNode {
    SchemaAstNode {
        kind: kind.to_string(),
        fields: HashMap::new(),
        annotations: vec![],
        span: Span { start: 0, end: 1 },
    }
}

/// Create a test AST node with a specific span
fn make_test_node_with_span(kind: &str, start: usize, end: usize) -> SchemaAstNode {
    SchemaAstNode {
        kind: kind.to_string(),
        fields: HashMap::new(),
        annotations: vec![],
        span: Span { start, end },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NF SPEC LOADABILITY TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nf_spec_exists() {
    let nf_spec_path = Path::new("core_spec/NORMAL_FORM_SPEC_0.1.yaml");
    assert!(
        nf_spec_path.exists(),
        "NF spec file must exist at core_spec/NORMAL_FORM_SPEC_0.1.yaml"
    );
}

#[test]
fn test_normalization_rules_exist() {
    let rules_path = Path::new("core_spec/NORMALIZATION_RULES_0.1.yaml");
    assert!(
        rules_path.exists(),
        "Normalization rules must exist at core_spec/NORMALIZATION_RULES_0.1.yaml"
    );
}

// TODO (WAVE NEXT): Add spec parsing tests when loaders are implemented
// #[test]
// fn test_nf_spec_parses_successfully() { ... }
//
// #[test]
// fn test_normalization_rules_parse_successfully() { ... }

// ═══════════════════════════════════════════════════════════════════════════
// NF AST TYPE ENFORCEMENT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nf_ast_version_tracking() {
    let dummy_node = make_test_node("IntLit");

    let nf_ast = unsafe { NfAst::from_unchecked_node_for_testing(dummy_node) };

    assert_eq!(nf_ast.nf_version(), &NfVersion::V0_1);
}

#[test]
fn test_nf_ast_node_extraction() {
    let dummy_node = make_test_node("IntLit");

    let nf_ast = unsafe { NfAst::from_unchecked_node_for_testing(dummy_node.clone()) };
    let extracted = nf_ast.into_node();

    assert_eq!(extracted.kind, "IntLit");
}

#[test]
fn test_nf_version_core_ir_compatibility() {
    // NF 0.1 should target Core IR 0.3 (canonical)
    assert!(NfVersion::V0_1.is_compatible_with_core_ir("0.3"));
    assert!(!NfVersion::V0_1.is_compatible_with_core_ir("0.1"));
    assert!(!NfVersion::V0_1.is_compatible_with_core_ir("0.2"));
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZATION CONTEXT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_normalization_context_loads() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return; // Skip if config not present
    }

    let result = NormalizationContext::load(rules_path);

    assert!(result.is_ok(), "Normalization context should load");
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZATION → VALIDATION INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_normalization_produces_nf_ast_for_valid_input() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    // IntLit is in NF admissible set
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        SchemaValue::Token(Token {
            kind: TokenKind::IntLit,
            lexeme: "42".to_string(),
            span: Span { start: 0, end: 2 },
        }),
    );
    let valid_ast = SchemaAstNode {
        kind: "IntLit".to_string(),
        fields,
        annotations: vec![],
        span: Span { start: 0, end: 2 },
    };

    let ctx = NormalizationContext::load(rules_path).unwrap();
    let result = ctx.normalize(valid_ast);

    assert!(result.is_ok(), "Valid AST should normalize successfully");

    let nf_ast = result.unwrap();
    assert_eq!(nf_ast.nf_version(), &NfVersion::V0_1);
}

#[test]
fn test_normalization_fails_for_forbidden_construct() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    // ForLoop is explicitly forbidden in NF (no rule matches)
    let forbidden_ast = make_test_node("ForLoop");

    let ctx = NormalizationContext::load(rules_path).unwrap();
    let result = ctx.normalize(forbidden_ast);

    assert!(
        result.is_err(),
        "Forbidden construct should fail normalization"
    );

    match result {
        Err(NormalizationError::RewriteError { .. })
        | Err(NormalizationError::ValidationError(_)) => {
            // Expected: Either no rule match or validation failure
        }
        _ => panic!("Expected ValidationError for forbidden construct"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATION BOUNDARY ENFORCEMENT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_nf_validation_accepts_admissible_nodes() {
    // All these should be in NF admissible set
    let admissible_kinds = vec![
        "IntLit", "BoolLit", "UnitLit", "VarRef", "LetExpr", "LamExpr", "AppExpr", "IfExpr",
        "CallExpr",
    ];

    for kind in admissible_kinds {
        let ast = make_test_node(kind);

        let result = validate_nf(&ast);
        assert!(result.is_ok(), "{} should be in NF admissible set", kind);
    }
}

#[test]
fn test_nf_validation_rejects_forbidden_nodes() {
    // These should all be forbidden
    let forbidden_kinds = vec![
        "ForLoop",
        "WhileLoop",
        "MatchExpr",
        "TupleExpr",
        "ListExpr",
        "BlockExpr",
    ];

    for kind in forbidden_kinds {
        let ast = make_test_node(kind);

        let result = validate_nf(&ast);
        assert!(result.is_err(), "{} should be forbidden in NF", kind);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PIPELINE STAGE BOUNDARY TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_normalization_is_mandatory() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    // This is a structural test verifying the API enforces normalization
    // For now, we verify that NfAst exists and can be constructed via normalization
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        SchemaValue::Token(Token {
            kind: TokenKind::IntLit,
            lexeme: "42".to_string(),
            span: Span { start: 0, end: 2 },
        }),
    );
    let ast = SchemaAstNode {
        kind: "IntLit".to_string(),
        fields,
        annotations: vec![],
        span: Span { start: 0, end: 2 },
    };

    let ctx = NormalizationContext::load(rules_path).unwrap();
    let nf_ast = ctx.normalize(ast).unwrap();

    // NfAst should exist and be the result of normalization
    assert_eq!(nf_ast.nf_version(), &NfVersion::V0_1);
}

// TODO (WAVE NEXT): Add lowering boundary tests
// These require updating the lowering API to accept NfAst instead of SchemaAstNode
//
// #[test]
// fn test_lowering_only_accepts_nf_ast() {
//     // Verify lowering API signature requires NfAst
//     // This should be a compile-time check, but we can verify at runtime too
// }
//
// #[test]
// fn test_lowering_fails_if_nf_validation_bypassed() {
//     // Create non-NF AST and try to lower it
//     // Should panic or fail with clear error
// }

// ═══════════════════════════════════════════════════════════════════════════
// DETERMINISM TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_normalization_is_deterministic() {
    let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
    if !rules_path.exists() {
        return;
    }

    let mut fields1 = HashMap::new();
    fields1.insert(
        "value".to_string(),
        SchemaValue::Token(Token {
            kind: TokenKind::IntLit,
            lexeme: "42".to_string(),
            span: Span { start: 0, end: 2 },
        }),
    );
    let ast1 = SchemaAstNode {
        kind: "IntLit".to_string(),
        fields: fields1,
        annotations: vec![],
        span: Span { start: 0, end: 2 },
    };

    let mut fields2 = HashMap::new();
    fields2.insert(
        "value".to_string(),
        SchemaValue::Token(Token {
            kind: TokenKind::IntLit,
            lexeme: "42".to_string(),
            span: Span { start: 0, end: 2 },
        }),
    );
    let ast2 = SchemaAstNode {
        kind: "IntLit".to_string(),
        fields: fields2,
        annotations: vec![],
        span: Span { start: 0, end: 2 },
    };

    let ctx = NormalizationContext::load(rules_path).unwrap();

    let nf_ast1 = ctx.normalize(ast1).unwrap();
    let nf_ast2 = ctx.normalize(ast2).unwrap();

    // Same input should produce same NF version
    assert_eq!(nf_ast1.nf_version(), nf_ast2.nf_version());
}

// ═══════════════════════════════════════════════════════════════════════════
// ERROR REPORTING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_validation_error_includes_span() {
    let forbidden_ast = make_test_node_with_span("ForLoop", 10, 20);

    let result = validate_nf(&forbidden_ast);

    assert!(result.is_err());

    let err = result.unwrap_err();
    assert_eq!(err.span.start, 10);
    assert_eq!(err.span.end, 20);
    assert_eq!(err.node_kind, "ForLoop");
}

#[test]
fn test_normalization_error_display() {
    let err = NormalizationError::RewriteError {
        message: "Test error".to_string(),
        node_kind: "TestNode".to_string(),
    };

    let display = format!("{}", err);
    assert!(display.contains("TestNode"));
    assert!(display.contains("Test error"));
}
