// Normalize Module — Normalization Stage Entrypoint
//
// PURPOSE:
// This module provides the normalization stage infrastructure:
// - Loading NF target spec (NORMAL_FORM_SPEC_0.1.yaml)
// - Loading normalization rules (NORMALIZATION_RULES_0.1.yaml)
// - Orchestrating normalization (rewrite + validation)
// - Producing NfAst (guaranteed NF-compliant)
//
// SCOPE:
// - NF spec loading and validation
// - Normalization rules loading
// - Normalization entrypoint (AST → NfAst)
// - Integration with existing nf_validation
//
// OUT OF SCOPE:
// - Lowering logic (separate module)
// - Core IR generation
// - Registry interaction
// - Semantic interpretation

// nf_spec_loader deleted: was a WAVE NEXT placeholder stub (load_from_file
// silently returned an empty NfSpec with no content). Zero callers outside
// this module. Removed in Bundle 3 quality audit (2026-06-08).
//
// nf_validation migrated here from src/normalisation/ in Bundle 4 (2026-06-08).
// src/normalisation/ (Phase 3d legacy) has been deleted.
pub mod desugaring;
pub mod expand_engine;
pub mod nf_validation;
pub mod normalize_engine;
pub mod rules_loader;

use crate::frontend::schema_ast::SchemaAstNode;
use crate::nf_ast::{NfAst, NfVersion};
use crate::normalize::nf_validation::{
    validate_h1_single_assignment, validate_nf,
    validate_value_construct_types, NfValidationError,
};
use std::error::Error;
use std::fmt;
use std::path::Path;

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Normalization error (rewrite or validation failure)
#[derive(Debug)]
pub enum NormalizationError {
    /// NF spec loading failed
    SpecLoadError(String),

    /// Normalization rules loading failed
    RulesLoadError(String),

    /// Rewrite application failed
    RewriteError { message: String, node_kind: String },

    /// NF validation failed after normalization
    ValidationError(NfValidationError),

    /// Version mismatch between rules and spec
    VersionMismatch {
        rules_version: String,
        spec_version: String,
    },
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NormalizationError::SpecLoadError(msg) => {
                write!(f, "NF Spec Load Error: {}", msg)
            }
            NormalizationError::RulesLoadError(msg) => {
                write!(f, "Normalization Rules Load Error: {}", msg)
            }
            NormalizationError::RewriteError { message, node_kind } => {
                write!(f, "Rewrite Error on {}: {}", node_kind, message)
            }
            NormalizationError::ValidationError(err) => {
                write!(f, "NF Validation Error: {}", err)
            }
            NormalizationError::VersionMismatch {
                rules_version,
                spec_version,
            } => {
                write!(
                    f,
                    "Version Mismatch: rules={}, spec={}",
                    rules_version, spec_version
                )
            }
        }
    }
}

impl Error for NormalizationError {}

impl From<NfValidationError> for NormalizationError {
    fn from(err: NfValidationError) -> Self {
        NormalizationError::ValidationError(err)
    }
}

/// Normalization result
pub type NormalizationResult = Result<NfAst, NormalizationError>;

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZATION CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Normalization context (loaded specs and rules)
///
/// Created once and reused for multiple normalizations.
pub struct NormalizationContext {
    // Normalization engine
    engine: normalize_engine::NormalizationEngine,

    // NF spec version
    nf_spec_version: String,
}

impl NormalizationContext {
    /// Load normalization context from spec files
    ///
    /// # Arguments
    /// * `rules_path` - Path to normalization rules YAML
    ///
    /// # Returns
    /// Loaded context or error
    pub fn load(rules_path: &Path) -> Result<Self, NormalizationError> {
        // Load normalization rules
        let rules = rules_loader::NormalizationRules::load_from_file(rules_path)
            .map_err(|e| NormalizationError::RulesLoadError(e))?;

        // Check version compatibility
        let nf_spec_version = rules.target_nf_version.clone();
        if nf_spec_version != "0.1" {
            return Err(NormalizationError::VersionMismatch {
                rules_version: rules.version.clone(),
                spec_version: "0.1".to_string(),
            });
        }

        // Create engine with loaded rules
        let engine = normalize_engine::NormalizationEngine::new(rules);

        Ok(NormalizationContext {
            engine,
            nf_spec_version,
        })
    }

    /// Normalize AST to NF AST
    ///
    /// # Arguments
    /// * `ast` - Schema AST from projection stage
    ///
    /// # Returns
    /// NF AST (validated) or normalization error
    pub fn normalize(&self, ast: SchemaAstNode) -> NormalizationResult {
        // Apply normalization rewrites
        let normalized_ast =
            self.engine
                .normalize(ast)
                .map_err(|e| NormalizationError::RewriteError {
                    message: e.message,
                    node_kind: e.node_kind,
                })?;

        // Validate NF compliance
        validate_nf(&normalized_ast)?;
        // G2: value construction type-tag/literal compatibility (pre-lower).
        validate_value_construct_types(&normalized_ast)?;
        // H1 single-assignment: no name may be bound twice in the same LetExpr chain.
        validate_h1_single_assignment(&normalized_ast)?;

        // Construct NfAst with validation guarantee
        let nf_version = match self.nf_spec_version.as_str() {
            "0.1" => NfVersion::V0_1,
            _ => {
                return Err(NormalizationError::SpecLoadError(format!(
                    "Unknown NF spec version: {}",
                    self.nf_spec_version
                )))
            }
        };

        Ok(NfAst::from_validated_node(normalized_ast, nf_version))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::schema_ast::SchemaValue;
    use crate::frontend::token::{Span, Token, TokenKind};
    use std::collections::HashMap;

    fn make_test_token(value: &str) -> Token {
        Token {
            kind: TokenKind::Ident,
            lexeme: value.to_string(),
            span: Span {
                start: 0,
                end: value.len(),
            },
        }
    }

    fn make_int_token(value: i64) -> Token {
        Token {
            kind: TokenKind::IntLit,
            lexeme: value.to_string(),
            span: Span {
                start: 0,
                end: value.to_string().len(),
            },
        }
    }

    #[test]
    fn test_normalize_simple_literal() {
        let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
        if !rules_path.exists() {
            return; // Skip if config not present
        }

        let ctx = NormalizationContext::load(rules_path).unwrap();

        // Create IntLit AST node
        let mut fields = HashMap::new();
        fields.insert("value".to_string(), SchemaValue::Token(make_int_token(42)));

        let ast = SchemaAstNode {
            kind: "IntLit".to_string(),
            fields,
            annotations: vec![],
            span: Span { start: 0, end: 2 },
        };

        let result = ctx.normalize(ast);
        assert!(
            result.is_ok(),
            "Failed to normalize IntLit: {:?}",
            result.err()
        );

        let nf_ast = result.unwrap();
        assert_eq!(nf_ast.node().kind, "IntLit");
    }

    #[test]
    fn test_normalize_nested_let() {
        let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
        if !rules_path.exists() {
            return;
        }

        let ctx = NormalizationContext::load(rules_path).unwrap();

        // Create: let x = 5 in x
        let mut value_fields = HashMap::new();
        value_fields.insert("value".to_string(), SchemaValue::Token(make_int_token(5)));
        let value_node = SchemaAstNode {
            kind: "IntLit".to_string(),
            fields: value_fields,
            annotations: vec![],
            span: Span { start: 8, end: 9 },
        };

        let mut body_fields = HashMap::new();
        body_fields.insert("name".to_string(), SchemaValue::Token(make_test_token("x")));
        let body_node = SchemaAstNode {
            kind: "VarRef".to_string(),
            fields: body_fields,
            annotations: vec![],
            span: Span { start: 13, end: 14 },
        };

        let mut let_fields = HashMap::new();
        let_fields.insert("name".to_string(), SchemaValue::Token(make_test_token("x")));
        let_fields.insert("value".to_string(), SchemaValue::Node(Box::new(value_node)));
        let_fields.insert("body".to_string(), SchemaValue::Node(Box::new(body_node)));

        let let_ast = SchemaAstNode {
            kind: "LetExpr".to_string(),
            fields: let_fields,
            annotations: vec![],
            span: Span { start: 0, end: 14 },
        };

        let result = ctx.normalize(let_ast);
        assert!(
            result.is_ok(),
            "Failed to normalize LetExpr: {:?}",
            result.err()
        );

        let nf_ast = result.unwrap();
        assert_eq!(nf_ast.node().kind, "LetExpr");
    }

    #[test]
    fn test_no_matching_rule_fails() {
        let rules_path = Path::new("axis-surface-ai2-config/ai2-explicit-normalize.yaml");
        if !rules_path.exists() {
            return;
        }

        let ctx = NormalizationContext::load(rules_path).unwrap();

        // Create an unknown node kind
        let ast = SchemaAstNode {
            kind: "UnknownNodeKind".to_string(),
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
            _ => panic!("Expected RewriteError"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONVENIENCE API
// ═══════════════════════════════════════════════════════════════════════════

