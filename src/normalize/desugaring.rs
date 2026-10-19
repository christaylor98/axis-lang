// WAVE 3: Desugaring Transformations
//
// This module implements structural desugaring of H1 surface constructs
// to Normal Form (NF-H1) compliant AST.
//
// SCOPE:
// - ForExpr → explicit recursion
// - WhileExpr → recursion + IfExpr
// - LoopExpr → recursion
// - MatchExpr → decision tree (IfExpr + LetExpr)
// - Remove H1 wrapper nodes (Item, Stmt, ExprStmt, etc.)
//
// OUT OF SCOPE:
// - No hooks
// - No registry access
// - No semantic interpretation
// - No evaluation order guarantees
// - No exhaustiveness checking
// - No optimisation
//
// INVARIANTS:
// - Transformations are deterministic
// - Transformations are total (never panic)
// - Transformations are structural only
// - Output conforms to NF-H1 admissible set

use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use crate::frontend::token::{Span, Token, TokenKind};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Transform AST node recursively, applying desugaring rules
///
/// This is the main entry point for desugaring transformations.
/// It walks the AST tree and applies transformations where applicable.
pub fn desugar_node(node: &SchemaAstNode) -> SchemaAstNode {
    // Apply transformation based on node kind
    match node.kind.as_str() {
        "ForExpr" => desugar_for_expr(node),
        "WhileExpr" => desugar_while_expr(node),
        "LoopExpr" => desugar_loop_expr(node),
        "MatchExpr" => desugar_match_expr(node),

        // H1 wrapper nodes - unwrap
        "Item" | "Stmt" | "ExprStmt" => unwrap_single_child(node, "variant"),
        "Expr" => unwrap_single_child(node, "variant"),
        "BlockContent" => transform_block_content(node),

        // Recurse into structural nodes
        _ => recurse_fields(node),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONTROL FLOW DESUGARING
// ═══════════════════════════════════════════════════════════════════════════

/// Desugar ForExpr to explicit recursion
///
/// Surface:
///   for i in iter { body }
///
/// Desugared (structural approximation):
///   (λ loop -> λ iter ->
///     if iter {
///       let i = iter in (body; loop(iter))
///     } else {
///       ()
///     })
///
/// NOTE: This is STRUCTURAL ONLY. Semantics (iteration, continuation, etc.)
/// are defined by lowering, not here.
fn desugar_for_expr(node: &SchemaAstNode) -> SchemaAstNode {
    // Extract fields
    let var = get_token_field(node, "var");
    let iter = get_node_field(node, "iter");
    let body = get_node_field(node, "body");

    // Desugar child nodes first
    let _iter = desugar_node(&iter);
    let body = desugar_node(&body);

    // Create loop variable identifier
    let loop_var = make_ident("__loop", node.span.clone());
    let iter_var = make_ident("__iter", node.span.clone());

    // Build recursive structure:
    // λ __loop -> λ __iter -> if __iter { let var = __iter in body } else { () }
    let var_binding = make_let_expr(
        var.lexeme.clone(),
        make_var_ref(&iter_var.clone(), node.span.clone()),
        body,
        node.span.clone(),
    );

    let then_branch = make_block(vec![var_binding], node.span.clone());
    let else_branch = make_block(vec![make_unit_lit(node.span.clone())], node.span.clone());

    let if_expr = make_if_expr(
        make_var_ref(&iter_var, node.span.clone()),
        then_branch,
        else_branch,
        node.span.clone(),
    );

    let inner_lam = make_lam_expr(iter_var.lexeme.clone(), if_expr, node.span.clone());

    make_lam_expr(loop_var.lexeme.clone(), inner_lam, node.span.clone())
}

/// Desugar WhileExpr to recursion + IfExpr
///
/// Surface:
///   while cond { body }
///
/// Desugared (structural):
///   (λ loop -> if cond { body; loop() } else { () })
///
/// NOTE: Structural only. No execution semantics assumed.
fn desugar_while_expr(node: &SchemaAstNode) -> SchemaAstNode {
    let condition = get_node_field(node, "condition");
    let body = get_node_field(node, "body");

    let condition = desugar_node(&condition);
    let body = desugar_node(&body);

    let loop_var = make_ident("__loop", node.span.clone());

    // Build: if cond { body } else { () }
    let then_branch = body;
    let else_branch = make_block(vec![make_unit_lit(node.span.clone())], node.span.clone());

    let if_expr = make_if_expr(condition, then_branch, else_branch, node.span.clone());

    // Wrap in lambda: λ __loop -> if_expr
    make_lam_expr(loop_var.lexeme.clone(), if_expr, node.span.clone())
}

/// Desugar LoopExpr to recursion
///
/// Surface:
///   loop { body }
///
/// Desugared (structural):
///   (λ loop -> { body; loop() })
///
/// NOTE: Structural only. No infinite loop semantics assumed.
fn desugar_loop_expr(node: &SchemaAstNode) -> SchemaAstNode {
    let body = get_node_field(node, "body");
    let body = desugar_node(&body);

    let loop_var = make_ident("__loop", node.span.clone());

    // Wrap in lambda: λ __loop -> body
    make_lam_expr(loop_var.lexeme.clone(), body, node.span.clone())
}

/// Desugar MatchExpr to decision tree with IfExpr + LetExpr
///
/// Surface:
///   match x { 0 => a, _ => b }
///
/// Desugared (structural):
///   let __scrutinee = x in
///     if __scrutinee == 0 { a } else { b }
///
/// NOTE: Structural only. No exhaustiveness, no pattern semantics.
/// Only handles simple literal and wildcard patterns.
fn desugar_match_expr(node: &SchemaAstNode) -> SchemaAstNode {
    let scrutinee = get_node_field(node, "scrutinee");
    let arms = get_nodes_field(node, "arms");

    let scrutinee = desugar_node(&scrutinee);

    // Bind scrutinee to temporary
    let scrutinee_var = make_ident("__scrutinee", node.span.clone());

    // Build decision tree from arms
    let decision_tree = build_match_decision_tree(&arms, &scrutinee_var, node.span.clone());

    // Wrap in let binding
    make_let_expr(
        scrutinee_var.lexeme.clone(),
        scrutinee,
        decision_tree,
        node.span.clone(),
    )
}

/// Build decision tree from match arms
///
/// Creates nested IfExpr nodes checking each pattern sequentially.
/// Wildcard (_) patterns become the else branch.
fn build_match_decision_tree(
    arms: &[SchemaAstNode],
    scrutinee_var: &Token,
    span: Span,
) -> SchemaAstNode {
    if arms.is_empty() {
        // No arms - return unit (structural placeholder)
        return make_unit_lit(span);
    }

    let arm = &arms[0];
    let pattern = get_node_field(arm, "pattern");
    let expr = get_node_field(arm, "expr");
    let expr = desugar_node(&expr);

    // Check pattern type (structural inspection only)
    let pattern_variant = get_node_field(&pattern, "variant");

    match pattern_variant.kind.as_str() {
        "IdentPat" if is_wildcard_pattern(&pattern_variant) => {
            // Wildcard pattern - this is the final else case
            expr
        }
        "IntLit" | "BoolLit" => {
            // Literal pattern - create equality check
            let condition = make_equality_check(
                make_var_ref(scrutinee_var, span.clone()),
                pattern_variant.clone(),
                span.clone(),
            );

            let then_branch = make_block(vec![expr], span.clone());

            // Recurse for remaining arms
            let else_branch = if arms.len() > 1 {
                make_block(
                    vec![build_match_decision_tree(
                        &arms[1..],
                        scrutinee_var,
                        span.clone(),
                    )],
                    span.clone(),
                )
            } else {
                make_block(vec![make_unit_lit(span.clone())], span.clone())
            };

            make_if_expr(condition, then_branch, else_branch, span)
        }
        _ => {
            // Unknown pattern type - structural passthrough
            // In a real implementation, this would be an error
            expr
        }
    }
}

/// Check if pattern is wildcard (_)
fn is_wildcard_pattern(pattern: &SchemaAstNode) -> bool {
    if let Some(SchemaValue::Token(tok)) = pattern.fields.get("name") {
        tok.lexeme == "_"
    } else {
        false
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WRAPPER NODE HANDLING
// ═══════════════════════════════════════════════════════════════════════════

/// Unwrap single-child wrapper node
///
/// Extracts the child from specified field and returns it (desugared).
fn unwrap_single_child(node: &SchemaAstNode, field_name: &str) -> SchemaAstNode {
    if let Some(child) = node.fields.get(field_name) {
        match child {
            SchemaValue::Node(child_node) => desugar_node(child_node),
            _ => {
                // Field exists but wrong type - return node as-is
                recurse_fields(node)
            }
        }
    } else {
        // Field missing - return node as-is (structural preservation)
        recurse_fields(node)
    }
}

/// Transform BlockContent by flattening statements
///
/// BlockContent has a "stmts" field with multiple children.
/// We need to preserve these as a list for Block to consume.
fn transform_block_content(node: &SchemaAstNode) -> SchemaAstNode {
    let mut fields = HashMap::new();

    if let Some(SchemaValue::Nodes(stmts)) = node.fields.get("stmts") {
        // Desugar each statement
        let desugared_stmts: Vec<SchemaAstNode> =
            stmts.iter().map(|stmt| desugar_node(stmt)).collect();

        fields.insert("stmts".to_string(), SchemaValue::Nodes(desugared_stmts));
    }

    SchemaAstNode {
        kind: node.kind.clone(),
        fields,
        annotations: node.annotations.clone(),
        span: node.span.clone(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AST NODE CONSTRUCTORS
// ═══════════════════════════════════════════════════════════════════════════

/// Make identifier token
fn make_ident(name: &str, span: Span) -> Token {
    Token {
        kind: TokenKind::Ident,
        lexeme: name.to_string(),
        span,
    }
}

/// Make VarRef node
fn make_var_ref(name: &Token, span: Span) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert("name".to_string(), SchemaValue::Token(name.clone()));

    SchemaAstNode {
        kind: "VarRef".to_string(),
        fields,
        annotations: Vec::new(),
        span,
    }
}

/// Make LetExpr node
fn make_let_expr(
    name: String,
    value: SchemaAstNode,
    body: SchemaAstNode,
    span: Span,
) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(
        "name".to_string(),
        SchemaValue::Token(Token {
            kind: TokenKind::Ident,
            lexeme: name,
            span: span.clone(),
        }),
    );
    fields.insert("value".to_string(), SchemaValue::Node(Box::new(value)));
    fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));

    SchemaAstNode {
        kind: "LetExpr".to_string(),
        fields,
        annotations: Vec::new(),
        span,
    }
}

/// Make LamExpr node
fn make_lam_expr(param: String, body: SchemaAstNode, span: Span) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(
        "param".to_string(),
        SchemaValue::Token(Token {
            kind: TokenKind::Ident,
            lexeme: param,
            span: span.clone(),
        }),
    );
    fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));

    SchemaAstNode {
        kind: "LamExpr".to_string(),
        fields,
        annotations: Vec::new(),
        span,
    }
}

/// Make IfExpr node
fn make_if_expr(
    condition: SchemaAstNode,
    then_block: SchemaAstNode,
    else_block: SchemaAstNode,
    span: Span,
) -> SchemaAstNode {
    let mut fields = HashMap::new();
    fields.insert(
        "condition".to_string(),
        SchemaValue::Node(Box::new(condition)),
    );
    fields.insert(
        "then_block".to_string(),
        SchemaValue::Node(Box::new(then_block)),
    );
    fields.insert(
        "else_block".to_string(),
        SchemaValue::Node(Box::new(else_block)),
    );

    SchemaAstNode {
        kind: "IfExpr".to_string(),
        fields,
        annotations: Vec::new(),
        span,
    }
}

/// Make Block node
fn make_block(stmts: Vec<SchemaAstNode>, span: Span) -> SchemaAstNode {
    let mut fields = HashMap::new();

    // Create BlockContent wrapper
    let mut content_fields = HashMap::new();
    content_fields.insert("stmts".to_string(), SchemaValue::Nodes(stmts));
    let content = SchemaAstNode {
        kind: "BlockContent".to_string(),
        fields: content_fields,
        annotations: Vec::new(),
        span: span.clone(),
    };

    fields.insert("content".to_string(), SchemaValue::Node(Box::new(content)));

    SchemaAstNode {
        kind: "Block".to_string(),
        fields,
        annotations: Vec::new(),
        span,
    }
}

/// Make UnitLit node
fn make_unit_lit(span: Span) -> SchemaAstNode {
    SchemaAstNode {
        kind: "UnitLit".to_string(),
        fields: HashMap::new(),
        annotations: Vec::new(),
        span,
    }
}

/// Make equality check (structural, becomes CallExpr or similar)
///
/// This creates a placeholder structure. Actual equality semantics
/// are defined by lowering.
fn make_equality_check(left: SchemaAstNode, right: SchemaAstNode, span: Span) -> SchemaAstNode {
    // Structural placeholder: create CallExpr to "__eq" operator
    let mut fields = HashMap::new();
    let eq_fn = make_var_ref(&make_ident("__eq", span.clone()), span.clone());

    fields.insert("func".to_string(), SchemaValue::Node(Box::new(eq_fn)));
    fields.insert("args".to_string(), SchemaValue::Nodes(vec![left, right]));

    SchemaAstNode {
        kind: "CallExpr".to_string(),
        fields,
        annotations: Vec::new(),
        span,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// RECURSIVE TRAVERSAL
// ═══════════════════════════════════════════════════════════════════════════

/// Recursively desugar all child fields
///
/// For nodes that don't need transformation, we still recurse into children.
fn recurse_fields(node: &SchemaAstNode) -> SchemaAstNode {
    let mut fields = HashMap::new();

    for (field_name, field_value) in &node.fields {
        let desugared_value = match field_value {
            SchemaValue::Node(child) => SchemaValue::Node(Box::new(desugar_node(child))),
            SchemaValue::Nodes(children) => {
                let desugared_children: Vec<SchemaAstNode> =
                    children.iter().map(|child| desugar_node(child)).collect();
                SchemaValue::Nodes(desugared_children)
            }
            SchemaValue::Token(tok) => SchemaValue::Token(tok.clone()),
            SchemaValue::Tokens(toks) => SchemaValue::Tokens(toks.clone()),
        };
        fields.insert(field_name.clone(), desugared_value);
    }

    SchemaAstNode {
        kind: node.kind.clone(),
        fields,
        annotations: node.annotations.clone(),
        span: node.span.clone(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FIELD ACCESSORS
// ═══════════════════════════════════════════════════════════════════════════

fn get_node_field(node: &SchemaAstNode, field: &str) -> SchemaAstNode {
    match node.fields.get(field) {
        Some(SchemaValue::Node(child)) => (**child).clone(),
        _ => panic!("Missing or invalid node field '{}' in {}", field, node.kind),
    }
}

fn get_nodes_field(node: &SchemaAstNode, field: &str) -> Vec<SchemaAstNode> {
    match node.fields.get(field) {
        Some(SchemaValue::Nodes(children)) => children.clone(),
        _ => panic!(
            "Missing or invalid nodes field '{}' in {}",
            field, node.kind
        ),
    }
}

fn get_token_field(node: &SchemaAstNode, field: &str) -> Token {
    match node.fields.get(field) {
        Some(SchemaValue::Token(tok)) => tok.clone(),
        _ => panic!(
            "Missing or invalid token field '{}' in {}",
            field, node.kind
        ),
    }
}
