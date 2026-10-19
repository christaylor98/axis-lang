// Wave 4 Usage Example: Schema-Driven AST Projection
//
// This example demonstrates how to use the Wave 4 schema projection
// to transform a Generic AST into a typed Schema AST.

use axis_lang::frontend::{
    ast_builder::build_generic_ast,
    parser_runtime::{ParseNode, ParseTree},
    schema_ast::{project_schema_ast, AstSchema, FieldExtraction, SchemaNodeDef, SpanRule},
    token::{Span, Token, TokenKind},
};
use std::collections::HashMap;

fn main() {
    println!("Wave 4 Schema AST Projection Example\n");

    // ═════════════════════════════════════════════════════════════════
    // Step 1: Build a Generic AST (from Wave 3)
    // ═════════════════════════════════════════════════════════════════

    // Example: if x then 1 else 2
    // Generic AST: If(kw_if, condition, then_branch, kw_else, else_branch)

    let kw_if = Token {
        kind: TokenKind::Keyword("if".to_string()),
        lexeme: "if".to_string(),
        span: Span::new(0, 2),
    };

    let tok_x = Token {
        kind: TokenKind::Ident,
        lexeme: "x".to_string(),
        span: Span::new(3, 4),
    };

    let kw_then = Token {
        kind: TokenKind::Keyword("then".to_string()),
        lexeme: "then".to_string(),
        span: Span::new(5, 9),
    };

    let tok_1 = Token {
        kind: TokenKind::IntLit,
        lexeme: "1".to_string(),
        span: Span::new(10, 11),
    };

    let kw_else = Token {
        kind: TokenKind::Keyword("else".to_string()),
        lexeme: "else".to_string(),
        span: Span::new(12, 16),
    };

    let tok_2 = Token {
        kind: TokenKind::IntLit,
        lexeme: "2".to_string(),
        span: Span::new(17, 18),
    };

    // Build parse tree
    let condition = ParseTree {
        rule: "Atom".to_string(),
        children: vec![ParseNode::Terminal(tok_x)],
        span: Span::new(3, 4),
    };

    let then_branch = ParseTree {
        rule: "Atom".to_string(),
        children: vec![ParseNode::Terminal(tok_1)],
        span: Span::new(10, 11),
    };

    let else_branch = ParseTree {
        rule: "Atom".to_string(),
        children: vec![ParseNode::Terminal(tok_2)],
        span: Span::new(17, 18),
    };

    let if_tree = ParseTree {
        rule: "If".to_string(),
        children: vec![
            ParseNode::Terminal(kw_if),
            ParseNode::Rule(condition),
            ParseNode::Terminal(kw_then),
            ParseNode::Rule(then_branch),
            ParseNode::Terminal(kw_else),
            ParseNode::Rule(else_branch),
        ],
        span: Span::new(0, 18),
    };

    // Build Generic AST
    let generic_ast = build_generic_ast(&if_tree).expect("Failed to build Generic AST");

    println!("✓ Built Generic AST:");
    println!("  kind: {}", generic_ast.kind);
    println!("  children: {}", generic_ast.children.len());
    println!(
        "  span: {}..{}\n",
        generic_ast.span.start, generic_ast.span.end
    );

    // ═════════════════════════════════════════════════════════════════
    // Step 2: Define Schema
    // ═════════════════════════════════════════════════════════════════

    let mut schema = AstSchema {
        nodes: HashMap::new(),
        unhandled_behavior:
            axis_lang::frontend::schema_ast::UnhandledNodeBehavior::Reject,
    };

    // Define Atom node schema
    let mut atom_fields = HashMap::new();
    atom_fields.insert("tok".to_string(), FieldExtraction::Child(0));

    schema.nodes.insert(
        "Atom".to_string(),
        SchemaNodeDef {
            match_kind: "Atom".to_string(),
            fields: atom_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    // Define If node schema
    let mut if_fields = HashMap::new();
    if_fields.insert("condition".to_string(), FieldExtraction::Child(1));
    if_fields.insert("then_branch".to_string(), FieldExtraction::Child(3));
    if_fields.insert("else_branch".to_string(), FieldExtraction::Child(5));

    schema.nodes.insert(
        "If".to_string(),
        SchemaNodeDef {
            match_kind: "If".to_string(),
            fields: if_fields,
            span_rule: Some(SpanRule::Derived),
            annotations: None,
        },
    );

    println!("✓ Defined Schema:");
    println!("  nodes: {}", schema.nodes.len());
    println!("  - Atom (extracts child 0)");
    println!("  - If (extracts condition, then_branch, else_branch)\n");

    // ═════════════════════════════════════════════════════════════════
    // Step 3: Project to Schema AST
    // ═════════════════════════════════════════════════════════════════

    let schema_ast =
        project_schema_ast(&generic_ast, &schema).expect("Failed to project Schema AST");

    println!("✓ Projected to Schema AST:");
    println!("  kind: {}", schema_ast.kind);
    println!("  fields: {}", schema_ast.fields.len());
    println!(
        "  span: {}..{}\n",
        schema_ast.span.start, schema_ast.span.end
    );

    // ═════════════════════════════════════════════════════════════════
    // Step 4: Access Semantic Fields
    // ═════════════════════════════════════════════════════════════════

    use axis_lang::frontend::schema_ast::SchemaValue;

    println!("✓ Accessing semantic fields:");

    if let Some(SchemaValue::Node(condition_node)) = schema_ast.fields.get("condition") {
        println!("  condition: {}", condition_node.kind);
        println!(
            "    span: {}..{}",
            condition_node.span.start, condition_node.span.end
        );
    }

    if let Some(SchemaValue::Node(then_node)) = schema_ast.fields.get("then_branch") {
        println!("  then_branch: {}", then_node.kind);
        println!("    span: {}..{}", then_node.span.start, then_node.span.end);
    }

    if let Some(SchemaValue::Node(else_node)) = schema_ast.fields.get("else_branch") {
        println!("  else_branch: {}", else_node.kind);
        println!("    span: {}..{}", else_node.span.start, else_node.span.end);
    }

    println!("\n✓ Wave 4 Schema Projection Complete!");
    println!("\nKey Properties:");
    println!("  • All transformations declared in schema");
    println!("  • Named semantic fields (not indexed children)");
    println!("  • Span preservation and derivation");
    println!("  • Explicit error handling");
    println!("  • No silent inference or defaults");
}
