// Normal Form (NF) Validation for H1
//
// This module implements structural validation of Schema AST against the
// NF-H1 specification (core_spec/NORMAL_FORM_H1.md).
//
// SCOPE (WAVE 1):
// - Validate node kinds against admissible set
// - Fail hard on forbidden constructs
// - Provide clear error messages
// - No normalisation (transformation is separate)
// - No semantics (lowering's responsibility)
// - No registry access
//
// INVARIANTS:
// - Validation is deterministic
// - Validation is total (no panic, always returns Result)
// - Validation is pure (no side effects, no AST modification)
// - Validation is recursive (checks entire tree)

use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use crate::frontend::token::{Span, TokenKind};
use std::collections::HashSet;
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// NF validation error
#[derive(Debug, Clone)]
pub struct NfValidationError {
    pub message: String,
    pub span: Span,
    pub node_kind: String,
    pub parent_kind: Option<String>,
}

impl fmt::Display for NfValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NF Validation Error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for NfValidationError {}

/// Validation result - Ok means NF-compliant
pub type NfValidationResult = Result<(), NfValidationError>;

/// Validate that a Schema AST conforms to NF-H1
///
/// This is the main entry point for NF validation.
///
/// Checks:
/// - All node kinds are in the admissible set
/// - No forbidden constructs present
/// - Structural validity (recursive)
///
/// Does NOT check:
/// - Type correctness
/// - Name resolution
/// - Semantic validity
///
/// Returns:
/// - Ok(()) if AST is NF-compliant
/// - Err(NfValidationError) if any violation is found
///
/// DETERMINISTIC: Same AST always produces same result
/// TOTAL: Never panics, always returns Result
/// PURE: No side effects, does not modify AST
pub fn validate_nf(node: &SchemaAstNode) -> NfValidationResult {
    validate_node(node, None)
}

/// Enforce single-assignment: no variable name may be bound more than once
/// in the same linear LetExpr chain within a function body.
///
/// Called after normalization, so AssignStmts have been folded into LetExprs.
/// Only scans the outermost LetExpr spine — nested IfExpr branches are not
/// sub-scopes in H1 (flat scope model), but inner assignments are a deferred gap.
pub fn validate_h1_single_assignment(node: &SchemaAstNode) -> NfValidationResult {
    let mut bound: HashSet<String> = HashSet::new();
    scan_let_chain(node, &mut bound)
}

fn scan_let_chain(node: &SchemaAstNode, bound: &mut HashSet<String>) -> NfValidationResult {
    if node.kind != "LetExpr" {
        return Ok(());
    }
    if let Some(SchemaValue::Token(tok)) = node.fields.get("name") {
        let name = tok.lexeme.clone();
        if !bound.insert(name.clone()) {
            return Err(NfValidationError {
                message: format!(
                    "single-assignment violation: '{}' is already bound in this scope",
                    name
                ),
                span: node.span.clone(),
                node_kind: "LetExpr".to_string(),
                parent_kind: None,
            });
        }
    }
    if let Some(SchemaValue::Node(body)) = node.fields.get("body") {
        scan_let_chain(body, bound)?;
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// NF-H1 ADMISSIBLE SET (Normative)
// ═══════════════════════════════════════════════════════════════════════════

/// NF-H1 Admissible Node Kinds
///
/// Source: core_spec/NORMAL_FORM_H1.md § 3
///
/// These are the ONLY node kinds permitted in NF-H1.
/// Any other node kind is a hard error.
///
/// WAVE 1 NOTE: This set is aligned with Semantic-Surface-0 lowering targets.
/// The generic names in the spec (e.g., LambdaExpr) map to surface names (e.g., LamExpr).
/// WAVE 3 NOTE: H1-specific wrapper nodes added to allow testing forbidden constructs.
const NF_ADMISSIBLE_NODES: &[&str] = &[
    // Program structure
    "Program",
    "FunctionDecl",
    "FnDecl", // H1 function declaration
    "Item",   // H1 top-level item wrapper
    // Binding and scope
    "LetExpr",
    "LetBinding",   // H1 intermediate (AssignStmt desugared, consumed by fold)
    "AssignStmt",   // H1 assignment statement (present pre-fold)
    "Block",
    "BlockContent", // H1 block content wrapper
    "Stmt",         // H1 statement wrapper
    "ExprStmt",     // H1 expression statement
    // Control flow
    "IfExpr",
    // Computation
    "LamExpr",  // Lambda abstraction (spec: LambdaExpr)
    "AppExpr",  // Application (spec: CallExpr for non-curried)
    "CallExpr", // Direct call (alternative to AppExpr)
    // Values
    "VarRef",  // Variable reference (spec: Ident)
    "Ident",   // Identifier (alternative to VarRef)
    "IntLit",            // Integer literal constructor Int(...)
    "FloatLit",          // Float literal constructor Float(...)
    "DecimalLit",        // Decimal literal constructor Decimal(...)
    "TextLit",           // Text literal constructor Text(...)
    "BoolLit",           // Boolean literal constructor Bool(...)
    "UnitLit",           // Unit literal constructor Unit()
    "ValueConstructExpr", // Value(...) compound constructor
    "ListConstructExpr",  // List(...) constructor
    "Literal",           // Generic literal (legacy; kept for backward compat)
    "Unit",              // Unit value
    // H1 structural nodes (to be normalized away)
    "ParamList",
    "Param",
    "Primary",
    "UnaryExpr",
    "MulExpr",
    "AddExpr",
    "RelExpr",
    "EqualityExpr",
    "ArgList",
    "CallSuffix",
    // Structural wrappers (transparent, stripped at lowering boundary)
    "Expr",
    "AtomicExpr",
];

/// NF-M1 Admissible Node Kinds
///
/// Surface: surface-m1 (Mechanical 1)
///
/// Node kinds that M1 normalization (m1-normalize.yaml) produces in its
/// output and which are not already in NF_ADMISSIBLE_NODES.
///
/// Derived from m1-normalize.yaml: every rule whose nf_node is "identity",
/// an explicit kind name, or which passes through a structural node. Transparent
/// rules (nf_node: "transparent") and unwrap rules ("unwrap_program_items")
/// eliminate their input node kind and are not listed.
///
/// Shared kinds already in NF_ADMISSIBLE_NODES (FnDecl, IfExpr,
/// CallExpr, VarRef, Literal, ArgList) are omitted here.
const NF_M1_ADMISSIBLE_NODES: &[&str] = &[
    // Body and let-bindings
    "Body",
    "LetBinding",
    "LetDecl",
    // Typed parameter nodes (surface-only; type annotation stripped but node kept)
    "TypedParamList",
    "TypedParamRest",
    "TypedParam",
    // Type expression nodes (surface-only; kept for FnDecl return_type field)
    "PrimTypeExpr",
    "ValueTypeExpr",
    "ValueListTypeExpr",
    "TypeExprRest",
    "ValueTypeArgs",
    // Core expression forms
    "DetermineExpr",
    "ValueConstructExpr",
    "ValueListConstructExpr",
    // Typed literal constructors (M1 constructor syntax; replaces bare Literal)
    "TextLit",
    // Arg helpers
    "ArgListRest",
    // Sentinels
    "Empty",
];

/// NF-H1 Forbidden Constructs (Explicit)
///
/// Source: core_spec/NORMAL_FORM_H1.md § 4
///
/// These node kinds MUST NOT appear in NF.
/// They indicate failed or incomplete normalisation.
const NF_FORBIDDEN_NODES: &[&str] = &[
    // Surface control flow
    "ForExpr",
    "LoopExpr",
    "WhileExpr",
    // Surface pattern matching
    "MatchExpr",
    "Pattern",
    "PatternArm",
    // Implicit constructs
    "ImplicitReturn",
    "ImplicitSequence",
];

// ═══════════════════════════════════════════════════════════════════════════
// VALIDATION IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════

/// Validate a single node and recurse into children
fn validate_node(node: &SchemaAstNode, parent_kind: Option<&str>) -> NfValidationResult {
    let kind = node.kind.as_str();

    // Check if node kind is explicitly forbidden
    if is_forbidden_node(kind) {
        return Err(NfValidationError {
            message: format!(
                "forbidden node kind '{}' (surface construct must be normalized away)",
                kind
            ),
            span: node.span.clone(),
            node_kind: kind.to_string(),
            parent_kind: parent_kind.map(|s| s.to_string()),
        });
    }

    // Check if node kind is in admissible set
    if !is_admissible_node(kind) {
        return Err(NfValidationError {
            message: format!(
                "unknown or non-NF node kind '{}' (not in admissible set)",
                kind
            ),
            span: node.span.clone(),
            node_kind: kind.to_string(),
            parent_kind: parent_kind.map(|s| s.to_string()),
        });
    }

    // Recursively validate all child nodes
    for (_field_name, field_value) in &node.fields {
        validate_field(field_value, Some(kind))?;
    }

    Ok(())
}

/// Validate a field value (may contain nodes)
fn validate_field(value: &SchemaValue, parent_kind: Option<&str>) -> NfValidationResult {
    match value {
        SchemaValue::Node(child_node) => {
            validate_node(child_node, parent_kind)?;
        }
        SchemaValue::Nodes(child_nodes) => {
            for child_node in child_nodes {
                validate_node(child_node, parent_kind)?;
            }
        }
        SchemaValue::Token(_) | SchemaValue::Tokens(_) => {
            // Tokens are always valid in NF
        }
    }
    Ok(())
}

/// Check if a node kind is in the admissible set (H1 or M1)
fn is_admissible_node(kind: &str) -> bool {
    NF_ADMISSIBLE_NODES.contains(&kind) || NF_M1_ADMISSIBLE_NODES.contains(&kind)
}

/// Check if a node kind is explicitly forbidden
fn is_forbidden_node(kind: &str) -> bool {
    NF_FORBIDDEN_NODES.contains(&kind)
}

// ═══════════════════════════════════════════════════════════════════════════
/// Collect all node kinds in an AST (for debugging/inspection)
pub fn collect_node_kinds(node: &SchemaAstNode) -> Vec<String> {
    let mut kinds = Vec::new();
    collect_node_kinds_recursive(node, &mut kinds);
    kinds.sort();
    kinds.dedup();
    kinds
}

fn collect_node_kinds_recursive(node: &SchemaAstNode, kinds: &mut Vec<String>) {
    kinds.push(node.kind.clone());

    for (_field_name, field_value) in &node.fields {
        match field_value {
            SchemaValue::Node(child) => {
                collect_node_kinds_recursive(child, kinds);
            }
            SchemaValue::Nodes(children) => {
                for child in children {
                    collect_node_kinds_recursive(child, kinds);
                }
            }
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// VALUE CONSTRUCTION TYPE CHECK (G2 gate)
// ═══════════════════════════════════════════════════════════════════════════

/// Validate type-tag/literal compatibility in every ValueConstructExpr and
/// ValueListConstructExpr in the tree. Called pre-lower; the IR never sees
/// ill-typed constructions.
///
/// Checked per element: if both the type arg (tag) is a known primitive AND
/// the value arg is a literal, their primitive types must agree.
/// VarRefs and compound type args are skipped (no full type inference here).
///
/// Also checked: arity — count(type_args) == count(value_args) for
/// ValueConstructExpr.
pub fn validate_value_construct_types(node: &SchemaAstNode) -> NfValidationResult {
    check_vc_types_recursive(node)
}

fn check_vc_types_recursive(node: &SchemaAstNode) -> NfValidationResult {
    match node.kind.as_str() {
        "ValueConstructExpr" => check_value_construct_expr(node)?,
        "ValueListConstructExpr" => check_value_list_construct_expr(node)?,
        _ => {}
    }
    for (_, val) in &node.fields {
        match val {
            SchemaValue::Node(child) => check_vc_types_recursive(child)?,
            SchemaValue::Nodes(children) => {
                for child in children {
                    check_vc_types_recursive(child)?;
                }
            }
            SchemaValue::Token(_) | SchemaValue::Tokens(_) => {}
        }
    }
    Ok(())
}

fn check_value_construct_expr(node: &SchemaAstNode) -> NfValidationResult {
    let type_args_node = match node.fields.get("type_args") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        _ => return Ok(()),
    };
    let arglist = match node.fields.get("args") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        _ => return Ok(()),
    };
    let type_args = collect_value_type_args(type_args_node);
    let value_args = collect_arglist_nodes(arglist);

    if type_args.len() != value_args.len() {
        return Err(NfValidationError {
            message: format!(
                "Value construction arity mismatch: {} type args but {} value args",
                type_args.len(),
                value_args.len()
            ),
            span: node.span.clone(),
            node_kind: node.kind.clone(),
            parent_kind: None,
        });
    }
    for (i, (&targ, &varg)) in type_args.iter().zip(value_args.iter()).enumerate() {
        check_prim_tag_vs_literal(targ, varg, i, &node.kind, node)?;
    }
    Ok(())
}

fn check_value_list_construct_expr(node: &SchemaAstNode) -> NfValidationResult {
    let element_type = match node.fields.get("element_type") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        _ => return Ok(()),
    };
    let arglist = match node.fields.get("args") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        _ => return Ok(()),
    };
    let value_args = collect_arglist_nodes(arglist);
    for (i, &varg) in value_args.iter().enumerate() {
        check_prim_tag_vs_literal(element_type, varg, i, &node.kind, node)?;
    }
    Ok(())
}

fn check_prim_tag_vs_literal(
    tag: &SchemaAstNode,
    val: &SchemaAstNode,
    idx: usize,
    parent_kind: &str,
    parent_node: &SchemaAstNode,
) -> NfValidationResult {
    let Some(tag_prim) = prim_name_of_type_node(tag) else {
        return Ok(());
    };
    let Some(lit_prim) = prim_name_of_literal_node(val) else {
        return Ok(());
    };
    if tag_prim != lit_prim {
        return Err(NfValidationError {
            message: format!(
                "Value construction type mismatch at element {}: tag declares {} but literal is {}",
                idx, tag_prim, lit_prim
            ),
            span: val.span.clone(),
            node_kind: val.kind.clone(),
            parent_kind: Some(parent_kind.to_string()),
        });
    }
    let _ = parent_node;
    Ok(())
}

/// Collect type-arg nodes from a ValueTypeArgs node.
/// First item is direct (PrimTypeExpr/ValueTypeExpr/…); rest are under TypeExprRest.type_expr.
fn collect_value_type_args(type_args_node: &SchemaAstNode) -> Vec<&SchemaAstNode> {
    let types = match type_args_node.fields.get("types") {
        Some(SchemaValue::Nodes(ns)) => ns,
        _ => return vec![],
    };
    let mut out = Vec::new();
    for n in types {
        match n.kind.as_str() {
            "TypeExprRest" => {
                if let Some(SchemaValue::Node(inner)) = n.fields.get("type_expr") {
                    out.push(inner.as_ref());
                }
            }
            "Empty" => {}
            _ => out.push(n),
        }
    }
    out
}

/// Collect value-arg nodes from an ArgList node.
/// First item is direct; rest are under ArgListRest.arg.
fn collect_arglist_nodes(arglist: &SchemaAstNode) -> Vec<&SchemaAstNode> {
    let args = match arglist.fields.get("args") {
        Some(SchemaValue::Nodes(ns)) => ns,
        _ => return vec![],
    };
    let mut out = Vec::new();
    for n in args {
        match n.kind.as_str() {
            "ArgListRest" => {
                if let Some(SchemaValue::Node(inner)) = n.fields.get("arg") {
                    out.push(inner.as_ref());
                }
            }
            "Empty" => {}
            _ => out.push(n),
        }
    }
    out
}

/// If `node` is a PrimTypeExpr, return the declared primitive type name ("Int", "Text", …).
fn prim_name_of_type_node(node: &SchemaAstNode) -> Option<&str> {
    if node.kind != "PrimTypeExpr" {
        return None;
    }
    match node.fields.get("name") {
        Some(SchemaValue::Token(tok)) => Some(&tok.lexeme),
        _ => None,
    }
}

/// If `node` is a typed literal constructor, return the primitive type name it encodes.
fn prim_name_of_literal_node(node: &SchemaAstNode) -> Option<&'static str> {
    match node.kind.as_str() {
        "IntLit" => Some("Int"),
        "TextLit" => Some("Text"),
        "BoolLit" => Some("Bool"),
        "UnitLit" => Some("Unit"),
        // Legacy bare Literal — present in H1 surface, absent from M1.
        "Literal" => match node.fields.get("value") {
            Some(SchemaValue::Token(tok)) => match &tok.kind {
                TokenKind::IntLit => Some("Int"),
                TokenKind::StringLit => Some("Text"),
                TokenKind::Keyword(kw) if kw == "true" || kw == "false" => Some("Bool"),
                TokenKind::Keyword(kw) if kw == "unit" => Some("Unit"),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Helper to make test nodes
    fn make_node(kind: &str, span: Span) -> SchemaAstNode {
        SchemaAstNode {
            kind: kind.to_string(),
            fields: HashMap::new(),
            annotations: Vec::new(),
            span,
        }
    }

    fn make_node_with_child(kind: &str, child_field: &str, child: SchemaAstNode) -> SchemaAstNode {
        let mut fields = HashMap::new();
        fields.insert(child_field.to_string(), SchemaValue::Node(Box::new(child)));
        SchemaAstNode {
            kind: kind.to_string(),
            fields,
            annotations: Vec::new(),
            span: Span::new(0, 10),
        }
    }

    #[test]
    fn test_admissible_nodes_pass() {
        // All admissible nodes should pass validation
        for &kind in NF_ADMISSIBLE_NODES {
            let node = make_node(kind, Span::new(0, 10));
            assert!(
                validate_nf(&node).is_ok(),
                "Admissible node '{}' should pass validation",
                kind
            );
        }
    }

    #[test]
    fn test_forbidden_nodes_fail() {
        // All forbidden nodes should fail validation
        for &kind in NF_FORBIDDEN_NODES {
            let node = make_node(kind, Span::new(0, 10));
            let result = validate_nf(&node);
            assert!(
                result.is_err(),
                "Forbidden node '{}' should fail validation",
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
    fn test_unknown_node_fails() {
        let node = make_node("UnknownNode", Span::new(0, 10));
        let result = validate_nf(&node);
        assert!(result.is_err(), "Unknown node should fail validation");
        if let Err(e) = result {
            assert!(e.message.contains("unknown") || e.message.contains("not in admissible set"));
        }
    }

    #[test]
    fn test_recursive_validation() {
        // Valid parent with valid child should pass
        let child = make_node("Ident", Span::new(5, 8));
        let parent = make_node_with_child("LetExpr", "value", child);
        assert!(validate_nf(&parent).is_ok());
    }

    #[test]
    fn test_recursive_validation_fails_on_forbidden_child() {
        // Valid parent with forbidden child should fail
        let forbidden_child = make_node("ForExpr", Span::new(5, 8));
        let parent = make_node_with_child("Block", "stmt", forbidden_child);
        let result = validate_nf(&parent);
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.node_kind, "ForExpr");
            assert_eq!(e.parent_kind, Some("Block".to_string()));
        }
    }

    #[test]
    fn test_collect_node_kinds() {
        let child = make_node("Ident", Span::new(5, 8));
        let parent = make_node_with_child("LetExpr", "value", child);
        let kinds = collect_node_kinds(&parent);
        assert_eq!(kinds, vec!["Ident".to_string(), "LetExpr".to_string()]);
    }

    #[test]
    fn test_wrapper_nodes_allowed() {
        // Wrappers are in admissible set (transparent at lowering boundary)
        let expr = make_node("Expr", Span::new(0, 10));
        assert!(validate_nf(&expr).is_ok());

        let atomic = make_node("AtomicExpr", Span::new(0, 10));
        assert!(validate_nf(&atomic).is_ok());
    }
}
