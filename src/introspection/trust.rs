// Wave B: Trust Invariants and Verification
//
// Verify that:
// - Every Core IR node is traceable
// - No Core IR node lacks provenance
// - User annotations never override system annotations
// - Execution ignores all trace data
// - Trace graphs are deterministic

use crate::introspection::provenance::ProvenanceAnnotation;
use crate::introspection::trace_graph::TraceGraph;
use crate::frontend::schema_ast::Annotation;

// ═══════════════════════════════════════════════════════════════════════════
// TRUST INVARIANT ERRORS
// ═══════════════════════════════════════════════════════════════════════════

/// Trust invariant violation
#[derive(Debug, Clone)]
pub enum TrustInvariantError {
    /// Core IR node has no source traceability
    UntracedCoreIrNode { core_ir_id: u64 },
    /// Core IR node lacks provenance annotations
    MissingProvenance { core_ir_id: u64 },
    /// User annotation uses reserved namespace
    UserAnnotationInReservedNamespace {
        annotation_key: String,
        reserved_namespace: String,
    },
    /// Trace graph is internally inconsistent
    TraceGraphInconsistent { message: String },
    /// Two trace graphs for same input differ
    NonDeterministicTrace { message: String },
}

impl std::fmt::Display for TrustInvariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustInvariantError::UntracedCoreIrNode { core_ir_id } => {
                write!(f, "Core IR node {} has no source traceability", core_ir_id)
            }
            TrustInvariantError::MissingProvenance { core_ir_id } => {
                write!(
                    f,
                    "Core IR node {} lacks provenance annotations",
                    core_ir_id
                )
            }
            TrustInvariantError::UserAnnotationInReservedNamespace {
                annotation_key,
                reserved_namespace,
            } => {
                write!(
                    f,
                    "User annotation '{}' uses reserved namespace '{}'",
                    annotation_key, reserved_namespace
                )
            }
            TrustInvariantError::TraceGraphInconsistent { message } => {
                write!(f, "Trace graph inconsistent: {}", message)
            }
            TrustInvariantError::NonDeterministicTrace { message } => {
                write!(f, "Trace graph is non-deterministic: {}", message)
            }
        }
    }
}

impl std::error::Error for TrustInvariantError {}

// ═══════════════════════════════════════════════════════════════════════════
// INVARIANT VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════

/// Verify all trust invariants for a trace graph
///
/// This checks:
/// 1. Every Core IR node is traceable to source
/// 2. Trace graph is internally consistent
/// 3. All structural invariants hold
pub fn verify_trace_invariants(trace: &TraceGraph) -> Result<(), Vec<TrustInvariantError>> {
    let mut errors = Vec::new();

    // Check trace graph internal consistency
    if let Err(msg) = trace.verify_invariants() {
        errors.push(TrustInvariantError::TraceGraphInconsistent { message: msg });
    }

    // If there are consistency errors, return early (other checks may be invalid)
    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}

/// Verify that user annotations don't use reserved namespaces
///
/// This ensures user annotations never collide with provenance annotations.
pub fn verify_annotation_namespaces(
    annotations: &[Annotation],
) -> Result<(), Vec<TrustInvariantError>> {
    let mut errors = Vec::new();

    for annotation in annotations {
        if let Some(namespace) = ProvenanceAnnotation::namespace_of(&annotation.key) {
            errors.push(TrustInvariantError::UserAnnotationInReservedNamespace {
                annotation_key: annotation.key.clone(),
                reserved_namespace: namespace.prefix().to_string(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// verify_annotations_ignored_structurally / strip_annotations (legacy 0.3 CoreTerm path)
// removed in Phase 3d.

// ─────────────────────────────────────────────────────────────────────────────
// Core IR 0.5 flat-table invariant verifier (additive — Phase 3c)
//
// The builder already enforces these invariants at construction time. This
// verifier is an explicit, readable re-statement for testing and auditing.
// The legacy `verify_annotations_ignored_structurally` (CoreTerm tree) is untouched.
// ─────────────────────────────────────────────────────────────────────────────

/// Verify the DAG invariants of a Core IR 0.5 bundle (flat-table read path).
///
/// Checks: every NodeRef::Node(j) inside node[i] satisfies j < i (no forward
/// edges); every NodeRef::Pool(j) satisfies j < pool_len (in-range pool ref).
/// Returns `Ok(())` if all invariants hold, `Err(message)` on the first violation.
pub fn verify_bundle_dag_05(
    bundle: &crate::ir::core_ir_05::CoreBundle,
) -> Result<(), String> {
    use crate::ir::core_ir_05::{Node, NodeRef};

    let pool_len = bundle.constant_pool.len() as u32;

    let check_ref = |nr: NodeRef, node_idx: usize| -> Result<(), String> {
        match nr {
            NodeRef::Node(j) if (j as usize) >= node_idx => Err(format!(
                "node[{}] has forward edge to node[{}] — DAG invariant violated",
                node_idx, j
            )),
            NodeRef::Pool(j) if j >= pool_len => Err(format!(
                "node[{}] pool ref {} out of range (pool len {})",
                node_idx, j, pool_len
            )),
            _ => Ok(()),
        }
    };

    for (i, node) in bundle.nodes.iter().enumerate() {
        match node {
            Node::CCall { args, .. } => {
                for &arg in args {
                    check_ref(arg, i)?;
                }
            }
            Node::CIf { cond, then_, else_ } => {
                check_ref(*cond, i)?;
                check_ref(*then_, i)?;
                check_ref(*else_, i)?;
            }
            Node::CEnd { value } => check_ref(*value, i)?,
            Node::CDeterminate | Node::CBlock => {}
        }
    }
    check_ref(bundle.result, bundle.nodes.len())?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// DETERMINISM VERIFICATION
// ═══════════════════════════════════════════════════════════════════════════

/// Verify that two trace graphs are identical
///
/// This is used to verify determinism: running the pipeline twice on the
/// same input should produce identical traces.
pub fn verify_trace_determinism(
    trace1: &TraceGraph,
    trace2: &TraceGraph,
) -> Result<(), TrustInvariantError> {
    // Check node counts
    let stats1 = trace1.count_by_kind();
    let stats2 = trace2.count_by_kind();

    if stats1 != stats2 {
        return Err(TrustInvariantError::NonDeterministicTrace {
            message: format!("Node counts differ: {:?} vs {:?}", stats1, stats2),
        });
    }

    // Check link counts
    if trace1.all_links().len() != trace2.all_links().len() {
        return Err(TrustInvariantError::NonDeterministicTrace {
            message: format!(
                "Link counts differ: {} vs {}",
                trace1.all_links().len(),
                trace2.all_links().len()
            ),
        });
    }

    // Note: Full structural comparison would require stable node IDs,
    // which we don't guarantee. This is a basic sanity check.
    // For full determinism verification, use identical inputs and
    // compare serialized outputs.

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::token::{Span, Token, TokenKind};
    use crate::introspection::provenance::ProvenanceAnnotationSet;
    use crate::introspection::trace_graph::TraceGraphBuilder;
    use crate::frontend::schema_ast::AnnotationValue;

    #[test]
    fn test_verify_valid_trace_graph() {
        let mut builder = TraceGraphBuilder::new();

        let token = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token_id = builder.add_token(token, ProvenanceAnnotationSet::new());

        builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            vec![token_id],
        );

        let graph = builder.build();

        assert!(verify_trace_invariants(&graph).is_ok());
    }

    #[test]
    fn test_verify_invalid_trace_graph() {
        let mut builder = TraceGraphBuilder::new();

        // Core IR node with no source (orphan)
        builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            Vec::new(),
        );

        let graph = builder.build();

        // Should fail because Core IR has no source tokens
        assert!(verify_trace_invariants(&graph).is_err());
    }

    #[test]
    fn test_verify_annotation_namespaces() {
        let valid = vec![Annotation {
            key: "user.annotation".to_string(),
            value: AnnotationValue::String("value".to_string()),
        }];

        assert!(verify_annotation_namespaces(&valid).is_ok());

        let invalid = vec![Annotation {
            key: "source.span".to_string(), // Reserved namespace
            value: AnnotationValue::String("0..10".to_string()),
        }];

        let result = verify_annotation_namespaces(&invalid);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
    }

    // test_strip_annotations / test_verify_annotations_ignored
    // (legacy 0.3 CoreTerm path) removed in Phase 3d.

    #[test]
    fn test_verify_trace_determinism() {
        let mut builder1 = TraceGraphBuilder::new();
        let token1 = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token_id1 = builder1.add_token(token1, ProvenanceAnnotationSet::new());
        builder1.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            vec![token_id1],
        );
        let graph1 = builder1.build();

        let mut builder2 = TraceGraphBuilder::new();
        let token2 = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token_id2 = builder2.add_token(token2, ProvenanceAnnotationSet::new());
        builder2.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            vec![token_id2],
        );
        let graph2 = builder2.build();

        assert!(verify_trace_determinism(&graph1, &graph2).is_ok());
    }
}

#[cfg(test)]
mod tests_05 {
    use super::verify_bundle_dag_05;

    const PILOT_AXREG: &str = include_str!("../../registries/pilot_lowering.axreg");

    fn pilot_scope() -> crate::registry::core05::fold::Scope {
        crate::registry::core05::text::lower(PILOT_AXREG)
            .expect("pilot registry must load cleanly")
            .scope
    }

    // ── 1. Builder-produced bundle always satisfies the invariants ────────────

    #[test]
    fn valid_bundle_passes() {
        use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
        use crate::frontend::token::Span;
        use crate::lowering::nf_lowering::lower_nf_to_bundle_05;
        use std::collections::HashMap;

        let scope = pilot_scope();
        let unit = SchemaAstNode {
            kind: "UnitLit".to_string(),
            fields: HashMap::new(),
            annotations: Default::default(),
            span: Span::new(0, 2),
        };
        let mut fields = HashMap::new();
        fields.insert("condition".to_string(), SchemaValue::Node(Box::new(unit.clone())));
        fields.insert("then_branch".to_string(), SchemaValue::Node(Box::new(unit.clone())));
        fields.insert("else_branch".to_string(), SchemaValue::Node(Box::new(unit)));
        let if_node = SchemaAstNode {
            kind: "IfExpr".to_string(),
            fields,
            annotations: Default::default(),
            span: Span::new(0, 10),
        };
        let bundle = lower_nf_to_bundle_05(&if_node, &scope).unwrap();
        assert!(verify_bundle_dag_05(&bundle).is_ok(), "builder-produced bundle must satisfy DAG invariants");
    }

    // ── 2. Manually-crafted forward edge is detected ──────────────────────────

    #[test]
    fn forward_edge_detected() {
        use crate::ir::core_ir_05::{ConstantPoolEntry, CoreBundle, Node, NodeRef};
        use crate::lowering::emit_05::unit_type_id;

        let pool_entry = ConstantPoolEntry { def_hash: unit_type_id(), payload: vec![] };

        // node[0] is CIf whose then_ references node[1] (forward edge — invalid)
        let forward_bundle = CoreBundle {
            version: "0.5".to_string(),
            constant_pool: vec![pool_entry.clone()],
            nodes: vec![
                Node::CIf {
                    cond: NodeRef::Pool(0),
                    then_: NodeRef::Node(1),  // forward edge!
                    else_: NodeRef::Pool(0),
                },
                Node::CIf {
                    cond: NodeRef::Pool(0),
                    then_: NodeRef::Pool(0),
                    else_: NodeRef::Pool(0),
                },
            ],
            result: NodeRef::Node(1),
        };
        assert!(verify_bundle_dag_05(&forward_bundle).is_err(), "forward edge must be detected");
    }

    // ── 3. Minimal bundle (pool entry, no nodes) is trivially valid ──────────
    //
    // A truly EMPTY bundle (no nodes, no pool) can no longer have a valid
    // `result` ref at all — every bundle must reference something. That's
    // fine: real lowering always interns at least one pool entry (even a
    // bare `Unit()` tail does), so an all-empty bundle was never reachable
    // from a real program anyway. This test now covers the actual minimal
    // case: a bundle whose result is a bare pool entry with zero nodes.

    #[test]
    fn empty_bundle_is_valid() {
        use crate::ir::core_ir_05::{ConstantPoolEntry, CoreBundle, NodeRef};
        use crate::lowering::emit_05::unit_type_id;
        let bundle = CoreBundle {
            version: "0.5".to_string(),
            constant_pool: vec![ConstantPoolEntry { def_hash: unit_type_id(), payload: vec![] }],
            nodes: vec![],
            result: NodeRef::Pool(0),
        };
        assert!(verify_bundle_dag_05(&bundle).is_ok());
    }
}
