// AST Projection Schema-Exact Tests
// Tests verify that AST projection is SCHEMA-EXACT:
// - No implicit defaults
// - No automatic terminal skipping
// - No semantic interpretation
// - Hard errors on schema violations

use axis_lang::frontend::ast_builder::build_generic_ast;
use axis_lang::frontend::parser_runtime::{ParseNode, ParseTree};
use axis_lang::frontend::schema_ast::{
    project_schema_ast, AstSchema, FieldExtraction, SchemaNodeDef, SchemaValue, SpanRule,
    UnhandledNodeBehavior,
};
use axis_lang::frontend::token::{Span, Token, TokenKind};
use std::collections::HashMap;

fn token(kind: TokenKind, lexeme: &str, start: usize, end: usize) -> Token {
    Token {
        kind,
        lexeme: lexeme.to_string(),
        span: Span::new(start, end),
    }
}

fn parse_tree(rule: &str, children: Vec<ParseNode>, start: usize, end: usize) -> ParseTree {
    ParseTree {
        rule: rule.to_string(),
        children,
        span: Span::new(start, end),
    }
}

fn minimal_schema() -> AstSchema {
    AstSchema {
        nodes: HashMap::new(),
        unhandled_behavior: UnhandledNodeBehavior::Reject,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TERMINAL IN CHILDREN() RANGE → HARD ERROR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_children_with_terminal_fails() {
    // CST: Parent(Child, terminal("x"), Child, terminal("y"))
    // Schema: items from children(0..) which includes terminals → HARD ERROR

    let tok1 = token(TokenKind::IntLit, "1", 0, 1);
    let tok_x = token(TokenKind::Ident, "x", 2, 3);
    let tok2 = token(TokenKind::IntLit, "2", 4, 5);
    let tok_y = token(TokenKind::Ident, "y", 6, 7);

    let child1 = parse_tree("Child", vec![ParseNode::Terminal(tok1)], 0, 1);
    let child2 = parse_tree("Child", vec![ParseNode::Terminal(tok2)], 4, 5);

    let parent_tree = parse_tree(
        "Parent",
        vec![
            ParseNode::Rule(child1),
            ParseNode::Terminal(tok_x.clone()),
            ParseNode::Rule(child2),
            ParseNode::Terminal(tok_y.clone()),
        ],
        0,
        7,
    );

    let ast = build_generic_ast(&parent_tree).expect("AST build failed");

    let mut schema = minimal_schema();

    // Child schema
    let mut child_fields = HashMap::new();
    child_fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );
    schema.nodes.insert(
        "Child".to_string(),
        SchemaNodeDef {
            match_kind: "Child".to_string(),
            fields: child_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Parent schema with children(0..) which will hit terminals
    let mut parent_fields = HashMap::new();
    parent_fields.insert("items".to_string(), FieldExtraction::Children(0, None));

    schema.nodes.insert(
        "Parent".to_string(),
        SchemaNodeDef {
            match_kind: "Parent".to_string(),
            fields: parent_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project should FAIL because children(0..) includes terminals
    let result = project_schema_ast(&ast, &schema);
    assert!(
        result.is_err(),
        "Expected error when children() includes terminal"
    );

    let err = result.unwrap_err();
    assert!(
        err.message.contains("terminal") && err.message.contains("children"),
        "Error should mention terminal in children range, got: {}",
        err.message
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// MISSING SCHEMA NODE → HARD ERROR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_missing_schema_node_fails() {
    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("UnknownNode", vec![ParseNode::Terminal(tok)], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let schema = minimal_schema(); // Empty schema

    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err(), "Expected error for unhandled node");

    let err = result.unwrap_err();
    assert!(
        err.message.contains("unhandled") && err.message.contains("UnknownNode"),
        "Error should mention unhandled node, got: {}",
        err.message
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TOKEN NOT FOUND → HARD ERROR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_token_not_found_fails() {
    // CST has IntLit but schema expects Ident → HARD ERROR
    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("TestNode", vec![ParseNode::Terminal(tok)], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), FieldExtraction::Token(TokenKind::Ident));

    schema.nodes.insert(
        "TestNode".to_string(),
        SchemaNodeDef {
            match_kind: "TestNode".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err(), "Expected error when token not found");

    let err = result.unwrap_err();
    assert!(
        err.message.contains("token") && err.message.contains("not found"),
        "Error should mention token not found, got: {}",
        err.message
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// CHILD INDEX OUT OF BOUNDS → HARD ERROR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_child_index_out_of_bounds_fails() {
    // CST has 1 child, schema requests child(5) → HARD ERROR
    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let child = parse_tree("Child", vec![ParseNode::Terminal(tok)], 0, 2);
    let tree = parse_tree("Parent", vec![ParseNode::Rule(child)], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();

    // Child schema
    let mut child_fields = HashMap::new();
    child_fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );
    schema.nodes.insert(
        "Child".to_string(),
        SchemaNodeDef {
            match_kind: "Child".to_string(),
            fields: child_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Parent schema with out-of-bounds child index
    let mut parent_fields = HashMap::new();
    parent_fields.insert("item".to_string(), FieldExtraction::Child(5));

    schema.nodes.insert(
        "Parent".to_string(),
        SchemaNodeDef {
            match_kind: "Parent".to_string(),
            fields: parent_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let result = project_schema_ast(&ast, &schema);
    assert!(
        result.is_err(),
        "Expected error for out of bounds child index"
    );

    let err = result.unwrap_err();
    assert!(
        err.message.contains("out of bounds") || err.message.contains("child(5)"),
        "Error should mention out of bounds, got: {}",
        err.message
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// CHILDREN RANGE INVALID → HARD ERROR
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_children_range_invalid_fails() {
    // CST has 2 children, schema requests children(5..10) → HARD ERROR
    let tok1 = token(TokenKind::IntLit, "1", 0, 1);
    let tok2 = token(TokenKind::IntLit, "2", 2, 3);
    let child1 = parse_tree("Child", vec![ParseNode::Terminal(tok1)], 0, 1);
    let child2 = parse_tree("Child", vec![ParseNode::Terminal(tok2)], 2, 3);

    let tree = parse_tree(
        "Parent",
        vec![ParseNode::Rule(child1), ParseNode::Rule(child2)],
        0,
        3,
    );
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();

    // Child schema
    let mut child_fields = HashMap::new();
    child_fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );
    schema.nodes.insert(
        "Child".to_string(),
        SchemaNodeDef {
            match_kind: "Child".to_string(),
            fields: child_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Parent schema with invalid children range
    let mut parent_fields = HashMap::new();
    parent_fields.insert("items".to_string(), FieldExtraction::Children(5, Some(10)));

    schema.nodes.insert(
        "Parent".to_string(),
        SchemaNodeDef {
            match_kind: "Parent".to_string(),
            fields: parent_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err(), "Expected error for invalid children range");

    let err = result.unwrap_err();
    assert!(
        err.message.contains("invalid") || err.message.contains("children"),
        "Error should mention invalid range, got: {}",
        err.message
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// HAPPY PATH: VALID SCHEMA PROJECTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_valid_projection_succeeds() {
    // CST: IntLit(token("42"))
    // Schema: value from token(INT_LIT) → SUCCESS

    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("IntLit", vec![ParseNode::Terminal(tok.clone())], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );

    schema.nodes.insert(
        "IntLit".to_string(),
        SchemaNodeDef {
            match_kind: "IntLit".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let schema_ast = project_schema_ast(&ast, &schema).expect("Projection should succeed");

    assert_eq!(schema_ast.kind, "IntLit");
    assert_eq!(schema_ast.fields.len(), 1);

    match &schema_ast.fields["value"] {
        SchemaValue::Token(t) => {
            assert_eq!(t.lexeme, "42");
            assert_eq!(t.kind, TokenKind::IntLit);
        }
        _ => panic!("Expected Token value"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHILDREN() ONLY NODES, NO TERMINALS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_children_only_nodes_succeeds() {
    // CST: Parent(Child, Child, Child) - no terminals in range
    // Schema: items from children(0..) → SUCCESS

    let tok1 = token(TokenKind::IntLit, "1", 0, 1);
    let tok2 = token(TokenKind::IntLit, "2", 2, 3);
    let tok3 = token(TokenKind::IntLit, "3", 4, 5);

    let child1 = parse_tree("Child", vec![ParseNode::Terminal(tok1)], 0, 1);
    let child2 = parse_tree("Child", vec![ParseNode::Terminal(tok2)], 2, 3);
    let child3 = parse_tree("Child", vec![ParseNode::Terminal(tok3)], 4, 5);

    let tree = parse_tree(
        "Parent",
        vec![
            ParseNode::Rule(child1),
            ParseNode::Rule(child2),
            ParseNode::Rule(child3),
        ],
        0,
        5,
    );
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();

    // Child schema
    let mut child_fields = HashMap::new();
    child_fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );
    schema.nodes.insert(
        "Child".to_string(),
        SchemaNodeDef {
            match_kind: "Child".to_string(),
            fields: child_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Parent schema
    let mut parent_fields = HashMap::new();
    parent_fields.insert("items".to_string(), FieldExtraction::Children(0, None));

    schema.nodes.insert(
        "Parent".to_string(),
        SchemaNodeDef {
            match_kind: "Parent".to_string(),
            fields: parent_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let schema_ast = project_schema_ast(&ast, &schema).expect("Projection should succeed");

    assert_eq!(schema_ast.kind, "Parent");
    assert_eq!(schema_ast.fields.len(), 1);

    match &schema_ast.fields["items"] {
        SchemaValue::Nodes(nodes) => {
            assert_eq!(nodes.len(), 3, "Should extract all 3 child nodes");
            for node in nodes {
                assert_eq!(node.kind, "Child");
            }
        }
        _ => panic!("Expected Nodes value"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHILD() CAN EXTRACT TERMINAL AS TOKEN
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_child_extracts_terminal_as_token() {
    // CST: Parent(token("42"))
    // Schema: value from child(0) where child is terminal → Token value

    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("Parent", vec![ParseNode::Terminal(tok.clone())], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert("value".to_string(), FieldExtraction::Child(0));

    schema.nodes.insert(
        "Parent".to_string(),
        SchemaNodeDef {
            match_kind: "Parent".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let schema_ast = project_schema_ast(&ast, &schema).expect("Projection should succeed");

    assert_eq!(schema_ast.kind, "Parent");
    match &schema_ast.fields["value"] {
        SchemaValue::Token(t) => {
            assert_eq!(t.lexeme, "42");
        }
        _ => panic!("Expected Token value when child(0) extracts terminal"),
    }
}
