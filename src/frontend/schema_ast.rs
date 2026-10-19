// Wave 4: Schema-Driven AST Projection (Explicit Semantics)
//
// This module transforms Generic AST → Schema AST using ONLY
// explicit, declared transformations in ast_schema.yaml.
//
// SCHEMA-EXACT PROJECTION RULES:
// 1. Every Generic AST node kind MUST have a schema entry (or unhandled behavior set)
// 2. Every schema field MUST be extractable from Generic AST
// 3. child(N) extracts child at index N (node or terminal)
// 4. children(A..B) extracts ONLY nodes in range [A,B), HARD ERROR on terminals
// 5. token(KIND) extracts first matching token, HARD ERROR if not found
// 6. tokens(KIND) extracts all matching tokens (may be empty list)
// 7. Missing fields → HARD ERROR
// 8. Out of bounds indices → HARD ERROR
// 9. Wrong token kind → HARD ERROR
// 10. Terminals in children() range → HARD ERROR
//
// FORBIDDEN:
// - Inferring field meaning
// - Auto-flattening lists
// - Skipping tokens silently
// - Renaming without schema instruction
// - Adding semantic defaults
// - Modifying Generic AST
// - Default field values
// - Partial projection
//
// If the schema is incomplete, the projection MUST fail.

use crate::frontend::ast_builder::{ASTChild, ASTNode};
use crate::frontend::token::{Span, Token, TokenKind};
use std::collections::HashMap;
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════════
// ANNOTATION MODEL (moved here from ir::core_ir for Phase 3d)
// ═══════════════════════════════════════════════════════════════════════════

/// Annotation attached to AST/Core IR nodes
///
/// Annotations are data, not behavior. They must never affect semantics or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub key: String,
    pub value: AnnotationValue,
}

/// Value types for annotations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationValue {
    Bool(bool),
    Int(i64),
    String(String),
    Symbol(String),
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════════

/// Schema-driven AST Node - typed, semantic AST
///
/// This represents the output of schema projection:
/// - Node kinds are semantic (e.g., "IfExpr", not generic "expr")
/// - Fields are named and extracted according to schema
/// - Structure is cleaned up as per schema rules
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaAstNode {
    /// Semantic node kind (from schema)
    pub kind: String,
    /// Named fields extracted from generic AST
    pub fields: HashMap<String, SchemaValue>,
    /// Annotations extracted from schema (explicit only)
    pub annotations: Vec<Annotation>,
    /// Span (derived from contributing children/tokens or explicit in schema)
    pub span: Span,
}

/// Value stored in a schema AST field
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaValue {
    /// Single child node
    Node(Box<SchemaAstNode>),
    /// Multiple child nodes
    Nodes(Vec<SchemaAstNode>),
    /// Single token
    Token(Token),
    /// Multiple tokens
    Tokens(Vec<Token>),
}

/// Schema AST projection error
#[derive(Debug, Clone)]
pub struct SchemaAstError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for SchemaAstError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Schema AST error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for SchemaAstError {}

// ═══════════════════════════════════════════════════════════════════════════
// SCHEMA MODEL
// ═══════════════════════════════════════════════════════════════════════════

/// AST Schema - defines transformations from Generic AST to Schema AST
#[derive(Debug, Clone)]
pub struct AstSchema {
    /// Node definitions: kind -> schema node
    pub nodes: HashMap<String, SchemaNodeDef>,
    /// Behavior for unhandled nodes (default: reject)
    pub unhandled_behavior: UnhandledNodeBehavior,
}

/// Behavior when encountering a Generic AST node with no schema entry
#[derive(Debug, Clone, PartialEq)]
pub enum UnhandledNodeBehavior {
    /// Reject with error (default, strict mode)
    Reject,
    /// Pass through unchanged (permissive mode)
    PassThrough,
}

/// Schema node definition
#[derive(Debug, Clone)]
pub struct SchemaNodeDef {
    /// Which Generic AST node kind this applies to
    pub match_kind: String,
    /// Field extraction rules
    pub fields: HashMap<String, FieldExtraction>,
    /// Annotation extraction rule (explicit only)
    pub annotations: Option<AnnotationExtraction>,
    /// Optional explicit span rule
    pub span_rule: Option<SpanRule>,
}

/// Annotation extraction rule
#[derive(Debug, Clone)]
pub enum AnnotationExtraction {
    /// Extract annotations from tokens of specific kind
    FromTokens(TokenKind),
}

/// Field extraction rule
#[derive(Debug, Clone)]
pub enum FieldExtraction {
    /// Extract child at specific index
    Child(usize),
    /// Extract children in range (start..end, exclusive end)
    /// If end is None, extract all children from start to end of list
    Children(usize, Option<usize>),
    /// Extract children from start to len-1 (exclude last child)
    /// Useful for patterns like "items+ <TERMINAL>" where last child is terminal
    ChildrenExcludingLast(usize),
    /// Extract the last child
    /// Symmetric counterpart to ChildrenExcludingLast — use in schema as "last_child"
    LastChild,
    /// Extract token from child node at specific index
    /// Used for postfix parser output where literal nodes have token children
    ChildToken(usize),
    /// Extract token of specific kind
    Token(TokenKind),
    /// Extract all tokens of specific kind
    Tokens(TokenKind),
    /// Extract the first terminal token of any kind (used when the kind varies by grammar alternative)
    FirstToken,
}

/// Span rule for schema node
#[derive(Debug, Clone)]
pub enum SpanRule {
    /// Explicit span (start, end)
    Explicit(usize, usize),
    /// Derive from contributing children/tokens (default)
    Derived,
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Project Generic AST to Schema AST using schema rules
///
/// This function applies ONLY transformations declared in the schema.
/// No inference, no defaults, no silent semantics.
///
/// Errors indicate:
/// - Missing schema entries
/// - Failed field extractions
/// - Index out of bounds
/// - Token not found
pub fn project_schema_ast(
    ast: &ASTNode,
    schema: &AstSchema,
) -> Result<SchemaAstNode, SchemaAstError> {
    project_node(ast, schema)
}

// ═══════════════════════════════════════════════════════════════════════════
// INTERNAL IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════

/// Recursively project a Generic AST node to Schema AST
fn project_node(ast: &ASTNode, schema: &AstSchema) -> Result<SchemaAstNode, SchemaAstError> {
    // Look up schema definition for this node kind
    let schema_node = match schema.nodes.get(&ast.kind) {
        Some(node_def) => node_def,
        None => {
            // No schema entry - check unhandled behavior
            match schema.unhandled_behavior {
                UnhandledNodeBehavior::Reject => {
                    return Err(SchemaAstError {
                        message: format!("unhandled AST node '{}'", ast.kind),
                        span: ast.span.clone(),
                    });
                }
                UnhandledNodeBehavior::PassThrough => {
                    // Pass through as-is: create schema node with generic structure
                    return passthrough_node(ast, schema);
                }
            }
        }
    };

    // Verify match
    if schema_node.match_kind != ast.kind {
        return Err(SchemaAstError {
            message: format!(
                "schema node match mismatch: expected '{}', got '{}'",
                schema_node.match_kind, ast.kind
            ),
            span: ast.span.clone(),
        });
    }

    // Extract fields according to schema
    let mut fields = HashMap::new();
    for (field_name, extraction) in &schema_node.fields {
        let value = extract_field(ast, extraction, schema)?;
        fields.insert(field_name.clone(), value);
    }

    // SCHEMA COMPLETENESS CHECK: all declared fields must be present
    if fields.len() != schema_node.fields.len() {
        return Err(SchemaAstError {
            message: format!(
                "schema field extraction incomplete for node '{}': expected {} fields, got {}",
                ast.kind,
                schema_node.fields.len(),
                fields.len()
            ),
            span: ast.span.clone(),
        });
    }

    // Extract annotations if specified
    let annotations = if let Some(ref ann_extraction) = schema_node.annotations {
        extract_annotations(ast, ann_extraction)?
    } else {
        Vec::new()
    };

    // Determine span
    let span = match &schema_node.span_rule {
        Some(SpanRule::Explicit(start, end)) => Span::new(*start, *end),
        Some(SpanRule::Derived) | None => {
            // Derive span from fields
            derive_span_from_fields(&fields).unwrap_or_else(|| ast.span.clone())
        }
    };

    Ok(SchemaAstNode {
        kind: ast.kind.clone(),
        fields,
        annotations,
        span,
    })
}

/// Extract a field value from Generic AST according to extraction rule
fn extract_field(
    ast: &ASTNode,
    extraction: &FieldExtraction,
    schema: &AstSchema,
) -> Result<SchemaValue, SchemaAstError> {
    match extraction {
        FieldExtraction::Child(index) => {
            // Extract child at specific index
            if *index >= ast.children.len() {
                return Err(SchemaAstError {
                    message: format!(
                        "child({}) out of bounds: node '{}' has only {} children",
                        index,
                        ast.kind,
                        ast.children.len()
                    ),
                    span: ast.span.clone(),
                });
            }

            match &ast.children[*index] {
                ASTChild::Node(child_node) => {
                    // Recursively project child node
                    let projected = project_node(child_node, schema)?;
                    Ok(SchemaValue::Node(Box::new(projected)))
                }
                ASTChild::Terminal(token) => {
                    // ALLOW: child() can extract terminal as token
                    // This is used when schema expects token at specific position
                    Ok(SchemaValue::Token(token.clone()))
                }
            }
        }

        FieldExtraction::Children(start, end) => {
            // Determine actual end index (use child count for open-ended ranges)
            let actual_end = end.unwrap_or(ast.children.len());

            // For open-ended ranges (children(N..)), return empty list if N >= len.
            // This allows binary expression nodes with zero ops to produce empty lists.
            if end.is_none() && *start >= ast.children.len() {
                return Ok(SchemaValue::Nodes(vec![]));
            }

            // Extract children in range
            if *start >= ast.children.len()
                || actual_end > ast.children.len()
                || start >= &actual_end
            {
                let end_display = end.map(|e| e.to_string()).unwrap_or_else(|| "".to_string());
                return Err(SchemaAstError {
                    message: format!(
                        "children({}..{}) invalid: node '{}' has {} children",
                        start,
                        end_display,
                        ast.kind,
                        ast.children.len()
                    ),
                    span: ast.span.clone(),
                });
            }

            let mut nodes = Vec::new();
            for (idx, child) in ast.children[*start..actual_end].iter().enumerate() {
                match child {
                    ASTChild::Node(child_node) => {
                        let projected = project_node(child_node, schema)?;
                        nodes.push(projected);
                    }
                    ASTChild::Terminal(token) => {
                        // HARD ERROR: terminals in range must be explicitly handled
                        return Err(SchemaAstError {
                            message: format!(
                                "children({}..{}) at index {} contains terminal '{}' (kind: {:?}) - terminals must be extracted with token() or tokens(), not children()",
                                start,
                                end.map(|e| e.to_string()).unwrap_or_else(|| "".to_string()),
                                start + idx,
                                token.lexeme,
                                token.kind
                            ),
                            span: token.span.clone(),
                        });
                    }
                }
            }

            Ok(SchemaValue::Nodes(nodes))
        }

        FieldExtraction::ChildrenExcludingLast(start) => {
            // Extract children from start to len-1 (exclude last child)
            // Used for patterns like "items+ <TERMINAL>" where grammar has trailing terminal

            if ast.children.is_empty() {
                return Err(SchemaAstError {
                    message: format!(
                        "children_excluding_last({}) invalid: node '{}' has no children",
                        start, ast.kind
                    ),
                    span: ast.span.clone(),
                });
            }

            let actual_end = ast.children.len() - 1;

            // start > actual_end means start is past the valid range — genuine error.
            // start == actual_end means zero items before the last child — return empty list.
            // This is correct for patterns like Body where zero let-bindings is valid.
            if *start > actual_end {
                return Err(SchemaAstError {
                    message: format!(
                        "children_excluding_last({}) invalid: node '{}' has {} children, start is out of range",
                        start, ast.kind, ast.children.len()
                    ),
                    span: ast.span.clone(),
                });
            }

            if *start == actual_end {
                return Ok(SchemaValue::Nodes(vec![]));
            }

            let mut nodes = Vec::new();
            for (idx, child) in ast.children[*start..actual_end].iter().enumerate() {
                match child {
                    ASTChild::Node(child_node) => {
                        let projected = project_node(child_node, schema)?;
                        nodes.push(projected);
                    }
                    ASTChild::Terminal(token) => {
                        // HARD ERROR: terminals must be explicitly extracted
                        return Err(SchemaAstError {
                            message: format!(
                                "children_excluding_last({}) at index {} contains terminal '{}' (kind: {:?}) - terminals must be extracted with token() or tokens()",
                                start,
                                start + idx,
                                token.lexeme,
                                token.kind
                            ),
                            span: token.span.clone(),
                        });
                    }
                }
            }

            Ok(SchemaValue::Nodes(nodes))
        }

        FieldExtraction::LastChild => {
            // Extract the last child node
            if ast.children.is_empty() {
                return Err(SchemaAstError {
                    message: format!(
                        "last_child invalid: node '{}' has no children",
                        ast.kind
                    ),
                    span: ast.span.clone(),
                });
            }
            match ast.children.last().unwrap() {
                ASTChild::Node(child_node) => {
                    let projected = project_node(child_node, schema)?;
                    Ok(SchemaValue::Node(Box::new(projected)))
                }
                ASTChild::Terminal(token) => Err(SchemaAstError {
                    message: format!(
                        "last_child of '{}' is a terminal '{}' — use token() instead",
                        ast.kind, token.lexeme
                    ),
                    span: token.span.clone(),
                }),
            }
        }

        FieldExtraction::ChildToken(index) => {
            // Extract token from child node at specific index
            // Used for postfix parser output where literal nodes have token children
            if *index >= ast.children.len() {
                return Err(SchemaAstError {
                    message: format!(
                        "child_token({}) out of bounds: node '{}' has only {} children",
                        index,
                        ast.kind,
                        ast.children.len()
                    ),
                    span: ast.span.clone(),
                });
            }

            match &ast.children[*index] {
                ASTChild::Node(child_node) => {
                    // Extract first terminal from child node
                    for child in &child_node.children {
                        if let ASTChild::Terminal(token) = child {
                            return Ok(SchemaValue::Token(token.clone()));
                        }
                    }
                    Err(SchemaAstError {
                        message: format!(
                            "child_token({}) failed: child node '{}' has no terminal children",
                            index, child_node.kind
                        ),
                        span: child_node.span.clone(),
                    })
                }
                ASTChild::Terminal(token) => {
                    // Child is already a terminal - return it directly
                    Ok(SchemaValue::Token(token.clone()))
                }
            }
        }

        FieldExtraction::Token(kind) => {
            // Find first token of specific kind
            for child in &ast.children {
                if let ASTChild::Terminal(token) = child {
                    if tokens_match(&token.kind, kind) {
                        return Ok(SchemaValue::Token(token.clone()));
                    }
                }
            }

            Err(SchemaAstError {
                message: format!("token({:?}) not found in node '{}'", kind, ast.kind),
                span: ast.span.clone(),
            })
        }

        FieldExtraction::Tokens(kind) => {
            // Find all tokens of specific kind
            let mut tokens = Vec::new();
            for child in &ast.children {
                if let ASTChild::Terminal(token) = child {
                    if tokens_match(&token.kind, kind) {
                        tokens.push(token.clone());
                    }
                }
            }

            Ok(SchemaValue::Tokens(tokens))
        }

        FieldExtraction::FirstToken => {
            for child in &ast.children {
                if let ASTChild::Terminal(token) = child {
                    return Ok(SchemaValue::Token(token.clone()));
                }
            }
            Err(SchemaAstError {
                message: format!("first_token: no terminal found in node '{}'", ast.kind),
                span: ast.span.clone(),
            })
        }
    }
}

/// Check if two token kinds match
fn tokens_match(actual: &TokenKind, expected: &TokenKind) -> bool {
    match (actual, expected) {
        (TokenKind::Keyword(a), TokenKind::Keyword(e)) => a == e,
        (TokenKind::Punct(a), TokenKind::Punct(e)) => a == e,
        (TokenKind::Ident, TokenKind::Ident) => true,
        (TokenKind::IntLit, TokenKind::IntLit) => true,
        (TokenKind::BoolLit, TokenKind::BoolLit) => true,
        (TokenKind::StringLit, TokenKind::StringLit) => true,
        (TokenKind::UnitLit, TokenKind::UnitLit) => true,
        (TokenKind::Whitespace, TokenKind::Whitespace) => true,
        (TokenKind::Comment, TokenKind::Comment) => true,
        (TokenKind::Eof, TokenKind::Eof) => true,
        _ => false,
    }
}

/// Extract annotations from Generic AST according to annotation extraction rule
fn extract_annotations(
    ast: &ASTNode,
    extraction: &AnnotationExtraction,
) -> Result<Vec<Annotation>, SchemaAstError> {
    match extraction {
        AnnotationExtraction::FromTokens(kind) => {
            // Find all tokens of specific kind and parse as annotations
            let mut annotations = Vec::new();
            for child in &ast.children {
                if let ASTChild::Terminal(token) = child {
                    if tokens_match(&token.kind, kind) {
                        // Parse annotation from token lexeme
                        if let Some(ann) = parse_annotation(&token.lexeme) {
                            annotations.push(ann);
                        }
                    }
                }
            }
            Ok(annotations)
        }
    }
}

/// Parse annotation from string (e.g., "@inline" or "@doc(\"text\")")
///
/// For now, supports simple formats:
/// - @key            -> Annotation { key, value: Bool(true) }
/// - @key=true/false -> Annotation { key, value: Bool(value) }
/// - @key=123        -> Annotation { key, value: Int(value) }
/// - @key="string"   -> Annotation { key, value: String(value) }
fn parse_annotation(s: &str) -> Option<Annotation> {
    let s = s.trim();
    if !s.starts_with('@') {
        return None;
    }

    let s = &s[1..]; // Remove '@'

    // Check for key=value format
    if let Some(eq_pos) = s.find('=') {
        let key = s[..eq_pos].trim().to_string();
        let value_str = s[eq_pos + 1..].trim();

        // Try parsing as bool
        if value_str == "true" {
            return Some(Annotation {
                key,
                value: AnnotationValue::Bool(true),
            });
        }
        if value_str == "false" {
            return Some(Annotation {
                key,
                value: AnnotationValue::Bool(false),
            });
        }

        // Try parsing as int
        if let Ok(i) = value_str.parse::<i64>() {
            return Some(Annotation {
                key,
                value: AnnotationValue::Int(i),
            });
        }

        // Try parsing as string (quoted)
        if value_str.starts_with('"') && value_str.ends_with('"') && value_str.len() >= 2 {
            let string_value = value_str[1..value_str.len() - 1].to_string();
            return Some(Annotation {
                key,
                value: AnnotationValue::String(string_value),
            });
        }

        // Otherwise treat as symbol
        return Some(Annotation {
            key,
            value: AnnotationValue::Symbol(value_str.to_string()),
        });
    }

    // No '=' found - simple flag annotation
    Some(Annotation {
        key: s.to_string(),
        value: AnnotationValue::Bool(true),
    })
}

/// Pass through a node unchanged (for permissive mode)
fn passthrough_node(ast: &ASTNode, schema: &AstSchema) -> Result<SchemaAstNode, SchemaAstError> {
    // Create a schema node that preserves generic structure
    let mut fields = HashMap::new();

    // Extract all children as "children" field
    let mut child_nodes = Vec::new();
    for child in &ast.children {
        if let ASTChild::Node(child_node) = child {
            let projected = project_node(child_node, schema)?;
            child_nodes.push(projected);
        }
    }

    if !child_nodes.is_empty() {
        fields.insert("children".to_string(), SchemaValue::Nodes(child_nodes));
    }

    // Extract all terminals as "tokens" field
    let mut tokens = Vec::new();
    for child in &ast.children {
        if let ASTChild::Terminal(token) = child {
            tokens.push(token.clone());
        }
    }

    if !tokens.is_empty() {
        fields.insert("tokens".to_string(), SchemaValue::Tokens(tokens));
    }

    Ok(SchemaAstNode {
        kind: ast.kind.clone(),
        fields,
        annotations: Vec::new(), // Passthrough nodes have no annotations
        span: ast.span.clone(),
    })
}

/// Derive span from schema fields
fn derive_span_from_fields(fields: &HashMap<String, SchemaValue>) -> Option<Span> {
    let mut min_start = usize::MAX;
    let mut max_end = 0;

    for value in fields.values() {
        match value {
            SchemaValue::Node(node) => {
                min_start = min_start.min(node.span.start);
                max_end = max_end.max(node.span.end);
            }
            SchemaValue::Nodes(nodes) => {
                for node in nodes {
                    min_start = min_start.min(node.span.start);
                    max_end = max_end.max(node.span.end);
                }
            }
            SchemaValue::Token(token) => {
                min_start = min_start.min(token.span.start);
                max_end = max_end.max(token.span.end);
            }
            SchemaValue::Tokens(tokens) => {
                for token in tokens {
                    min_start = min_start.min(token.span.start);
                    max_end = max_end.max(token.span.end);
                }
            }
        }
    }

    if min_start != usize::MAX && max_end > 0 {
        Some(Span::new(min_start, max_end))
    } else {
        None
    }
}
