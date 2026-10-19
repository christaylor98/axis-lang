// Wave 4 Integration Tests: Schema-Driven AST Projection
//
// Tests verify that Generic AST → Schema AST projection is:
// 1. Schema-driven (no inference)
// 2. Explicit (fails on missing schema)
// 3. Correct (extracts fields accurately)
// 4. Span-aware (derives or uses explicit spans)
// 5. Deterministic (same input → same output)

use axis_lang::frontend::ast_builder::build_generic_ast;
use axis_lang::frontend::parser_runtime::{ParseNode, ParseTree};
use axis_lang::frontend::schema_ast::{
    project_schema_ast, AstSchema, FieldExtraction, SchemaNodeDef, SchemaValue, SpanRule,
    UnhandledNodeBehavior,
};
use axis_lang::frontend::schema_load::load_schema_from_string;
use axis_lang::frontend::token::{Span, Token, TokenKind};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Create a test token
fn token(kind: TokenKind, lexeme: &str, start: usize, end: usize) -> Token {
    Token {
        kind,
        lexeme: lexeme.to_string(),
        span: Span::new(start, end),
    }
}

/// Helper: create a simple ParseTree for testing
fn parse_tree(rule: &str, children: Vec<ParseNode>, start: usize, end: usize) -> ParseTree {
    ParseTree {
        rule: rule.to_string(),
        children,
        span: Span::new(start, end),
    }
}

/// Create a minimal schema for testing
fn minimal_schema() -> AstSchema {
    AstSchema {
        nodes: HashMap::new(),
        unhandled_behavior: UnhandledNodeBehavior::Reject,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: HAPPY PATH - SINGLE TOKEN EXTRACTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_happy_path_token_extraction() {
    // Generic AST: IntLiteral(token("42"))
    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("Atom", vec![ParseNode::Terminal(tok.clone())], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Schema: IntLiteral matches Atom, extracts token(INT_LIT)
    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );

    schema.nodes.insert(
        "Atom".to_string(),
        SchemaNodeDef {
            match_kind: "Atom".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project
    let schema_ast = project_schema_ast(&ast, &schema).expect("projection failed");

    // Verify
    assert_eq!(schema_ast.kind, "Atom");
    assert_eq!(schema_ast.fields.len(), 1);

    match &schema_ast.fields["value"] {
        SchemaValue::Token(t) => {
            assert_eq!(t.lexeme, "42");
            assert_eq!(t.kind, TokenKind::IntLit);
        }
        _ => panic!("expected Token value"),
    }

    assert_eq!(schema_ast.span, Span::new(0, 2));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: HAPPY PATH - CHILD EXTRACTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_happy_path_child_extraction() {
    // Generic AST: If(KW_if, condition, then_branch, KW_else, else_branch)
    //   condition: Atom(x)
    //   then_branch: Atom(1)
    //   else_branch: Atom(2)

    let kw_if = token(TokenKind::Keyword("if".to_string()), "if", 0, 2);
    let kw_else = token(TokenKind::Keyword("else".to_string()), "else", 20, 24);

    let tok_x = token(TokenKind::Ident, "x", 5, 6);
    let tok_1 = token(TokenKind::IntLit, "1", 10, 11);
    let tok_2 = token(TokenKind::IntLit, "2", 28, 29);

    let condition = parse_tree("Atom", vec![ParseNode::Terminal(tok_x)], 5, 6);
    let then_branch = parse_tree("Atom", vec![ParseNode::Terminal(tok_1)], 10, 11);
    let else_branch = parse_tree("Atom", vec![ParseNode::Terminal(tok_2)], 28, 29);

    let if_tree = parse_tree(
        "If",
        vec![
            ParseNode::Terminal(kw_if),
            ParseNode::Rule(condition),
            ParseNode::Rule(then_branch),
            ParseNode::Terminal(kw_else),
            ParseNode::Rule(else_branch),
        ],
        0,
        29,
    );

    let ast = build_generic_ast(&if_tree).expect("AST build failed");

    // Schema: IfExpr extracts child(1), child(2), child(4)
    let mut schema = minimal_schema();

    // Define Atom node (for recursive projection)
    // Note: We need to handle both Ident and IntLit tokens, so we'll
    // make the schema extract the first token of any kind
    let mut atom_fields = HashMap::new();
    // For this test, we'll create separate schemas for different atoms
    // Actually, let's just use PassThrough for Atom to simplify
    atom_fields.insert(
        "tok".to_string(),
        FieldExtraction::Child(0), // Get the first child (which is a terminal)
    );
    schema.nodes.insert(
        "Atom".to_string(),
        SchemaNodeDef {
            match_kind: "Atom".to_string(),
            fields: atom_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Define If node
    let mut if_fields = HashMap::new();
    if_fields.insert("condition".to_string(), FieldExtraction::Child(1));
    if_fields.insert("then_branch".to_string(), FieldExtraction::Child(2));
    if_fields.insert("else_branch".to_string(), FieldExtraction::Child(4));

    schema.nodes.insert(
        "If".to_string(),
        SchemaNodeDef {
            match_kind: "If".to_string(),
            fields: if_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project
    let schema_ast = project_schema_ast(&ast, &schema).expect("projection failed");

    // Verify
    assert_eq!(schema_ast.kind, "If");
    assert_eq!(schema_ast.fields.len(), 3);

    // Check condition
    match &schema_ast.fields["condition"] {
        SchemaValue::Node(node) => {
            assert_eq!(node.kind, "Atom");
        }
        _ => panic!("expected Node value for condition"),
    }

    // Check then_branch
    match &schema_ast.fields["then_branch"] {
        SchemaValue::Node(node) => {
            assert_eq!(node.kind, "Atom");
        }
        _ => panic!("expected Node value for then_branch"),
    }

    // Check else_branch
    match &schema_ast.fields["else_branch"] {
        SchemaValue::Node(node) => {
            assert_eq!(node.kind, "Atom");
        }
        _ => panic!("expected Node value for else_branch"),
    }

    assert_eq!(schema_ast.span.start, 5);
    assert_eq!(schema_ast.span.end, 29);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: MISSING CHILD INDEX
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_missing_child_index() {
    // Generic AST with only 3 children, but schema asks for child(4)
    let tok1 = token(TokenKind::Ident, "a", 0, 1);
    let tok2 = token(TokenKind::Ident, "b", 2, 3);
    let tok3 = token(TokenKind::Ident, "c", 4, 5);

    let tree = parse_tree(
        "Test",
        vec![
            ParseNode::Terminal(tok1),
            ParseNode::Terminal(tok2),
            ParseNode::Terminal(tok3),
        ],
        0,
        5,
    );

    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Schema asks for child(4) which doesn't exist
    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert("missing".to_string(), FieldExtraction::Child(4));

    schema.nodes.insert(
        "Test".to_string(),
        SchemaNodeDef {
            match_kind: "Test".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project should fail
    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("child(4)"));
    assert!(err.message.contains("out of bounds"));
    assert!(err.message.contains("only 3 children"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: MISSING TOKEN
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_missing_token() {
    // Generic AST with only Ident token, but schema asks for INT_LIT
    let tok = token(TokenKind::Ident, "xyz", 0, 3);
    let tree = parse_tree("Atom", vec![ParseNode::Terminal(tok)], 0, 3);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Schema asks for INT_LIT which doesn't exist
    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );

    schema.nodes.insert(
        "Atom".to_string(),
        SchemaNodeDef {
            match_kind: "Atom".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project should fail
    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("token"));
    assert!(err.message.contains("not found"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: UNHANDLED NODE (REJECT MODE)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_unhandled_node_reject() {
    // Generic AST with node kind not in schema
    let tok = token(TokenKind::Ident, "x", 0, 1);
    let tree = parse_tree("UnknownNode", vec![ParseNode::Terminal(tok)], 0, 1);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Empty schema with Reject behavior
    let schema = minimal_schema();

    // Project should fail
    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("unhandled AST node"));
    assert!(err.message.contains("UnknownNode"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 6: UNHANDLED NODE (PASSTHROUGH MODE)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_unhandled_node_passthrough() {
    // Generic AST with node kind not in schema
    let tok = token(TokenKind::Ident, "x", 0, 1);
    let tree = parse_tree("UnknownNode", vec![ParseNode::Terminal(tok.clone())], 0, 1);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Empty schema with PassThrough behavior
    let mut schema = minimal_schema();
    schema.unhandled_behavior = UnhandledNodeBehavior::PassThrough;

    // Project should succeed
    let schema_ast = project_schema_ast(&ast, &schema).expect("projection failed");

    // Should preserve structure
    assert_eq!(schema_ast.kind, "UnknownNode");
    assert_eq!(schema_ast.span, Span::new(0, 1));

    // Should have tokens field
    match &schema_ast.fields.get("tokens") {
        Some(SchemaValue::Tokens(tokens)) => {
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].lexeme, "x");
        }
        _ => panic!("expected tokens field"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 7: SPAN CORRECTNESS
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_span_correctness() {
    // Generic AST: BinOp(left@5..6, op@10..11, right@15..16)
    let tok_left = token(TokenKind::Ident, "x", 5, 6);
    let tok_op = token(TokenKind::Punct("+".to_string()), "+", 10, 11);
    let tok_right = token(TokenKind::Ident, "y", 15, 16);

    let left = parse_tree("Atom", vec![ParseNode::Terminal(tok_left)], 5, 6);
    let right = parse_tree("Atom", vec![ParseNode::Terminal(tok_right)], 15, 16);

    let binop = parse_tree(
        "BinOp",
        vec![
            ParseNode::Rule(left),
            ParseNode::Terminal(tok_op),
            ParseNode::Rule(right),
        ],
        5,
        16,
    );

    let ast = build_generic_ast(&binop).expect("AST build failed");

    // Schema
    let mut schema = minimal_schema();

    // Atom node
    let mut atom_fields = HashMap::new();
    atom_fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::Ident),
    );
    schema.nodes.insert(
        "Atom".to_string(),
        SchemaNodeDef {
            match_kind: "Atom".to_string(),
            fields: atom_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // BinOp node
    let mut binop_fields = HashMap::new();
    binop_fields.insert("left".to_string(), FieldExtraction::Child(0));
    binop_fields.insert("right".to_string(), FieldExtraction::Child(2));

    schema.nodes.insert(
        "BinOp".to_string(),
        SchemaNodeDef {
            match_kind: "BinOp".to_string(),
            fields: binop_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project
    let schema_ast = project_schema_ast(&ast, &schema).expect("projection failed");

    // Span should be derived from left and right fields (5..16)
    assert_eq!(schema_ast.span.start, 5);
    assert_eq!(schema_ast.span.end, 16);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 8: DETERMINISM
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_determinism() {
    // Same input should produce identical output
    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("Atom", vec![ParseNode::Terminal(tok)], 0, 2);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );

    schema.nodes.insert(
        "Atom".to_string(),
        SchemaNodeDef {
            match_kind: "Atom".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project multiple times
    let schema_ast1 = project_schema_ast(&ast, &schema).expect("projection 1 failed");
    let schema_ast2 = project_schema_ast(&ast, &schema).expect("projection 2 failed");
    let schema_ast3 = project_schema_ast(&ast, &schema).expect("projection 3 failed");

    // All should be identical
    assert_eq!(schema_ast1, schema_ast2);
    assert_eq!(schema_ast2, schema_ast3);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 9: SCHEMA LOADING FROM YAML
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_schema_loading_from_yaml() {
    let yaml = r#"
nodes:
  IntLiteral:
    match: Atom
    fields:
      value:
        from: token(INT_LIT)
  IfExpr:
    match: If
    fields:
      condition:
        from: child(1)
      then_branch:
        from: child(2)
      else_branch:
        from: child(4)
"#;

    let schema = load_schema_from_string(yaml).expect("failed to load schema");

    assert_eq!(schema.nodes.len(), 2);
    assert!(schema.nodes.contains_key("IntLiteral"));
    assert!(schema.nodes.contains_key("IfExpr"));

    let int_lit = &schema.nodes["IntLiteral"];
    assert_eq!(int_lit.match_kind, "Atom");

    let if_expr = &schema.nodes["IfExpr"];
    assert_eq!(if_expr.match_kind, "If");
    assert_eq!(if_expr.fields.len(), 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 10: CHILDREN RANGE EXTRACTION
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_children_range_extraction() {
    // Generic AST: List(item0, item1, item2, item3)
    let tok0 = token(TokenKind::IntLit, "0", 0, 1);
    let tok1 = token(TokenKind::IntLit, "1", 2, 3);
    let tok2 = token(TokenKind::IntLit, "2", 4, 5);
    let tok3 = token(TokenKind::IntLit, "3", 6, 7);

    let item0 = parse_tree("Item", vec![ParseNode::Terminal(tok0)], 0, 1);
    let item1 = parse_tree("Item", vec![ParseNode::Terminal(tok1)], 2, 3);
    let item2 = parse_tree("Item", vec![ParseNode::Terminal(tok2)], 4, 5);
    let item3 = parse_tree("Item", vec![ParseNode::Terminal(tok3)], 6, 7);

    let list = parse_tree(
        "List",
        vec![
            ParseNode::Rule(item0),
            ParseNode::Rule(item1),
            ParseNode::Rule(item2),
            ParseNode::Rule(item3),
        ],
        0,
        7,
    );

    let ast = build_generic_ast(&list).expect("AST build failed");

    // Schema extracts children(1..3) - items 1 and 2
    let mut schema = minimal_schema();

    let mut item_fields = HashMap::new();
    item_fields.insert(
        "value".to_string(),
        FieldExtraction::Token(TokenKind::IntLit),
    );
    schema.nodes.insert(
        "Item".to_string(),
        SchemaNodeDef {
            match_kind: "Item".to_string(),
            fields: item_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    let mut list_fields = HashMap::new();
    list_fields.insert(
        "middle_items".to_string(),
        FieldExtraction::Children(1, Some(3)),
    );

    schema.nodes.insert(
        "List".to_string(),
        SchemaNodeDef {
            match_kind: "List".to_string(),
            fields: list_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project
    let schema_ast = project_schema_ast(&ast, &schema).expect("projection failed");

    // Check middle_items contains items 1 and 2
    match &schema_ast.fields["middle_items"] {
        SchemaValue::Nodes(nodes) => {
            assert_eq!(nodes.len(), 2);
            assert_eq!(nodes[0].kind, "Item");
            assert_eq!(nodes[1].kind, "Item");
        }
        _ => panic!("expected Nodes value"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 11: INVALID CHILDREN RANGE
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_invalid_children_range() {
    // Generic AST with 3 children, schema asks for children(2..5)
    let tok0 = token(TokenKind::Ident, "a", 0, 1);
    let tok1 = token(TokenKind::Ident, "b", 2, 3);
    let tok2 = token(TokenKind::Ident, "c", 4, 5);

    let tree = parse_tree(
        "Test",
        vec![
            ParseNode::Terminal(tok0),
            ParseNode::Terminal(tok1),
            ParseNode::Terminal(tok2),
        ],
        0,
        5,
    );

    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Schema asks for children(2..5) which is out of bounds
    let mut schema = minimal_schema();
    let mut fields = HashMap::new();
    fields.insert("items".to_string(), FieldExtraction::Children(2, Some(5)));

    schema.nodes.insert(
        "Test".to_string(),
        SchemaNodeDef {
            match_kind: "Test".to_string(),
            fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project should fail
    let result = project_schema_ast(&ast, &schema);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("children(2..5)"));
    assert!(err.message.contains("invalid"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 12: EMPTY FIELDS (EDGE CASE)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_empty_fields() {
    // Node with no fields defined in schema (valid but unusual)
    let tok = token(TokenKind::Ident, "x", 0, 1);
    let tree = parse_tree("EmptyNode", vec![ParseNode::Terminal(tok)], 0, 1);
    let ast = build_generic_ast(&tree).expect("AST build failed");

    let mut schema = minimal_schema();
    schema.nodes.insert(
        "EmptyNode".to_string(),
        SchemaNodeDef {
            match_kind: "EmptyNode".to_string(),
            fields: HashMap::new(), // No fields
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Project should succeed
    let schema_ast = project_schema_ast(&ast, &schema).expect("projection failed");

    assert_eq!(schema_ast.kind, "EmptyNode");
    assert_eq!(schema_ast.fields.len(), 0);
    assert_eq!(schema_ast.span, Span::new(0, 1));
}
