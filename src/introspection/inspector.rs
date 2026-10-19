// Wave B: Introspection API
//
// Read-only API for querying trace data:
// - Where did this Core IR node come from?
// - Which schema rule produced it?
// - Which annotations are attached?
// - Which lowering rule emitted it?
// - What source span(s) contributed?
//
// NO MUTATION ALLOWED

use crate::frontend::token::Span;
use crate::introspection::provenance::{ProvenanceAnnotation, ProvenanceNamespace};
use crate::introspection::trace_graph::{TraceGraph, TraceNode, TraceNodeId, TraceNodeKind};
use crate::frontend::schema_ast::Annotation;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// INTROSPECTION QUERY RESULTS
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a node origin query
#[derive(Debug, Clone)]
pub struct NodeOrigin {
    /// Trace node ID
    pub trace_id: TraceNodeId,
    /// Description of the node
    pub description: String,
    /// Source spans that contributed to this node
    pub source_spans: Vec<Span>,
    /// Source file (if known)
    pub source_file: Option<String>,
    /// Parent nodes
    pub parents: Vec<NodeOrigin>,
}

/// Result of a schema rule query
#[derive(Debug, Clone)]
pub struct SchemaRuleInfo {
    /// Schema node kind
    pub node_kind: String,
    /// Schema rule name (if annotated)
    pub rule_name: Option<String>,
    /// Schema field name (if annotated)
    pub field_name: Option<String>,
}

/// Result of a lowering rule query
#[derive(Debug, Clone)]
pub struct LoweringRuleInfo {
    /// Lowering rule name (if annotated)
    pub rule_name: Option<String>,
    /// Lowering phase (if annotated)
    pub phase: Option<String>,
    /// Source schema node ID (if annotated)
    pub source_node_id: Option<u64>,
}

/// Result of an annotation query
#[derive(Debug, Clone)]
pub struct AnnotationInfo {
    /// All annotations (user + provenance)
    pub all: Vec<Annotation>,
    /// User annotations only
    pub user: Vec<Annotation>,
    /// Provenance annotations only
    pub provenance: Vec<Annotation>,
    /// Provenance annotations by namespace
    pub by_namespace: HashMap<String, Vec<Annotation>>,
}

// ═══════════════════════════════════════════════════════════════════════════
// INSPECTOR
// ═══════════════════════════════════════════════════════════════════════════

/// Inspector - read-only API for querying trace data
///
/// This is the primary API for introspection. All queries are read-only.
/// No mutation is allowed.
pub struct Inspector {
    trace: TraceGraph,
}

impl Inspector {
    /// Create a new inspector from a trace graph
    pub fn new(trace: TraceGraph) -> Self {
        Self { trace }
    }

    // ═════════════════════════════════════════════════════════════════════
    // CORE IR QUERIES
    // ═════════════════════════════════════════════════════════════════════

    /// Get the origin of a Core IR node by its node ID
    ///
    /// Returns the full trace back to source tokens.
    pub fn origin(&self, core_ir_id: u64) -> Option<NodeOrigin> {
        let node = self.trace.get_node_by_core_ir_id(core_ir_id)?;
        self.build_node_origin(node)
    }

    /// Get the schema rule that produced a Core IR node
    pub fn schema_rule(&self, core_ir_id: u64) -> Option<SchemaRuleInfo> {
        let node = self.trace.get_node_by_core_ir_id(core_ir_id)?;
        self.find_schema_info(node.id)
    }

    /// Get the lowering rule that emitted a Core IR node
    pub fn lowering_rule(&self, core_ir_id: u64) -> Option<LoweringRuleInfo> {
        let node = self.trace.get_node_by_core_ir_id(core_ir_id)?;
        self.extract_lowering_info(&node.annotations)
    }

    /// Get all source spans that contributed to a Core IR node
    pub fn source_spans(&self, core_ir_id: u64) -> Vec<Span> {
        if let Some(node) = self.trace.get_node_by_core_ir_id(core_ir_id) {
            self.trace.get_source_spans(node.id)
        } else {
            Vec::new()
        }
    }

    /// Get all annotations attached to a Core IR node
    pub fn annotations(&self, core_ir_id: u64) -> Option<AnnotationInfo> {
        let node = self.trace.get_node_by_core_ir_id(core_ir_id)?;
        Some(self.build_annotation_info(&node.annotations))
    }

    // ═════════════════════════════════════════════════════════════════════
    // GENERAL QUERIES
    // ═════════════════════════════════════════════════════════════════════

    /// Get the origin of any trace node by its trace ID
    pub fn origin_by_trace_id(&self, trace_id: TraceNodeId) -> Option<NodeOrigin> {
        let node = self.trace.get_node(trace_id)?;
        self.build_node_origin(node)
    }

    /// Find all Core IR nodes produced by a specific schema rule
    pub fn find_by_schema_rule(&self, rule_name: &str) -> Vec<u64> {
        let mut results = Vec::new();

        for node in self.trace.all_nodes() {
            if let TraceNodeKind::CoreIr {
                node_id: Some(core_ir_id),
                ..
            } = node.kind
            {
                if let Some(rule) = self.extract_schema_rule(&node.annotations) {
                    if rule == rule_name {
                        results.push(core_ir_id);
                    }
                }
            }
        }

        results
    }

    /// Find all Core IR nodes produced by a specific lowering rule
    pub fn find_by_lowering_rule(&self, rule_name: &str) -> Vec<u64> {
        let mut results = Vec::new();

        for node in self.trace.all_nodes() {
            if let TraceNodeKind::CoreIr {
                node_id: Some(core_ir_id),
                ..
            } = node.kind
            {
                if let Some(rule) = self.extract_lowering_rule(&node.annotations) {
                    if rule == rule_name {
                        results.push(core_ir_id);
                    }
                }
            }
        }

        results
    }

    /// Get statistics about the trace graph
    pub fn stats(&self) -> TraceStats {
        let counts = self.trace.count_by_kind();
        let total_nodes = self.trace.all_nodes().len();
        let total_links = self.trace.all_links().len();

        TraceStats {
            total_nodes,
            total_links,
            token_count: *counts.get("Token").unwrap_or(&0),
            generic_ast_count: *counts.get("GenericAst").unwrap_or(&0),
            schema_ast_count: *counts.get("SchemaAst").unwrap_or(&0),
            core_ir_count: *counts.get("CoreIr").unwrap_or(&0),
        }
    }

    /// Explain a Core IR node (human-readable summary)
    pub fn explain(&self, core_ir_id: u64) -> Option<String> {
        let node = self.trace.get_node_by_core_ir_id(core_ir_id)?;

        let mut explanation = String::new();
        explanation.push_str(&format!("Core IR Node #{}\n", core_ir_id));
        explanation.push_str(&format!("Description: {}\n", node.description()));

        if let Some(span) = &node.span {
            explanation.push_str(&format!("Source Span: {}..{}\n", span.start, span.end));
        }

        if let Some(schema_info) = self.schema_rule(core_ir_id) {
            explanation.push_str(&format!("Schema Node: {}\n", schema_info.node_kind));
            if let Some(rule) = schema_info.rule_name {
                explanation.push_str(&format!("Schema Rule: {}\n", rule));
            }
        }

        if let Some(lowering_info) = self.lowering_rule(core_ir_id) {
            if let Some(rule) = lowering_info.rule_name {
                explanation.push_str(&format!("Lowering Rule: {}\n", rule));
            }
            if let Some(phase) = lowering_info.phase {
                explanation.push_str(&format!("Lowering Phase: {}\n", phase));
            }
        }

        let spans = self.source_spans(core_ir_id);
        if !spans.is_empty() {
            explanation.push_str("Source Contributions:\n");
            for span in spans {
                explanation.push_str(&format!("  - {}..{}\n", span.start, span.end));
            }
        }

        Some(explanation)
    }

    // ═════════════════════════════════════════════════════════════════════
    // HELPER METHODS
    // ═════════════════════════════════════════════════════════════════════

    fn build_node_origin(&self, node: &TraceNode) -> Option<NodeOrigin> {
        let parents = self.trace.get_parents(node.id);
        let parent_origins: Vec<NodeOrigin> = parents
            .into_iter()
            .filter_map(|p| self.build_node_origin(p))
            .collect();

        // Extract source file from current node or trace up to tokens
        let source_file = self.extract_source_file(&node.annotations).or_else(|| {
            // Trace back to tokens to find source file
            let tokens = self.trace.trace_to_tokens(node.id);
            tokens
                .iter()
                .find_map(|t| self.extract_source_file(&t.annotations))
        });

        let source_spans = self.trace.get_source_spans(node.id);

        Some(NodeOrigin {
            trace_id: node.id,
            description: node.description(),
            source_spans,
            source_file,
            parents: parent_origins,
        })
    }

    fn find_schema_info(&self, trace_id: TraceNodeId) -> Option<SchemaRuleInfo> {
        // Walk backwards to find the nearest Schema AST node
        let mut current_id = trace_id;
        let mut visited = std::collections::HashSet::new();

        while !visited.contains(&current_id) {
            visited.insert(current_id);

            if let Some(node) = self.trace.get_node(current_id) {
                match &node.kind {
                    TraceNodeKind::SchemaAst { kind, .. } => {
                        return Some(SchemaRuleInfo {
                            node_kind: kind.clone(),
                            rule_name: self.extract_schema_rule(&node.annotations),
                            field_name: self.extract_schema_field(&node.annotations),
                        });
                    }
                    _ => {
                        // Continue walking backwards
                        if let Some(parent_id) = node.parents.first() {
                            current_id = *parent_id;
                        } else {
                            break;
                        }
                    }
                }
            } else {
                break;
            }
        }

        None
    }

    fn extract_lowering_info(
        &self,
        annotations: &crate::introspection::provenance::ProvenanceAnnotationSet,
    ) -> Option<LoweringRuleInfo> {
        let lowering_anns = annotations.in_namespace(&ProvenanceNamespace::Lowering);

        if lowering_anns.is_empty() {
            return None;
        }

        let rule_name = lowering_anns
            .iter()
            .find(|a| a.key.ends_with(".rule"))
            .and_then(|a| match &a.value {
                crate::frontend::schema_ast::AnnotationValue::String(s) => Some(s.clone()),
                _ => None,
            });

        let phase = lowering_anns
            .iter()
            .find(|a| a.key.ends_with(".phase"))
            .and_then(|a| match &a.value {
                crate::frontend::schema_ast::AnnotationValue::String(s) => Some(s.clone()),
                _ => None,
            });

        let source_node_id = lowering_anns
            .iter()
            .find(|a| a.key.ends_with(".source_node_id"))
            .and_then(|a| match &a.value {
                crate::frontend::schema_ast::AnnotationValue::Int(i) => Some(*i as u64),
                _ => None,
            });

        Some(LoweringRuleInfo {
            rule_name,
            phase,
            source_node_id,
        })
    }

    fn build_annotation_info(
        &self,
        annotations: &crate::introspection::provenance::ProvenanceAnnotationSet,
    ) -> AnnotationInfo {
        let all: Vec<Annotation> = annotations.to_core_ir_annotations();

        let mut provenance = Vec::new();
        let mut user = Vec::new();
        let mut by_namespace = HashMap::new();

        for ann in &all {
            if ProvenanceAnnotation::is_provenance(ann) {
                provenance.push(ann.clone());
                if let Some(ns) = ProvenanceAnnotation::namespace_of(&ann.key) {
                    by_namespace
                        .entry(ns.prefix().to_string())
                        .or_insert_with(Vec::new)
                        .push(ann.clone());
                }
            } else {
                user.push(ann.clone());
            }
        }

        AnnotationInfo {
            all,
            user,
            provenance,
            by_namespace,
        }
    }

    fn extract_source_file(
        &self,
        annotations: &crate::introspection::provenance::ProvenanceAnnotationSet,
    ) -> Option<String> {
        annotations.get("source.file").and_then(|a| match &a.value {
            crate::frontend::schema_ast::AnnotationValue::String(s) => Some(s.clone()),
            _ => None,
        })
    }

    fn extract_schema_rule(
        &self,
        annotations: &crate::introspection::provenance::ProvenanceAnnotationSet,
    ) -> Option<String> {
        annotations.get("schema.rule").and_then(|a| match &a.value {
            crate::frontend::schema_ast::AnnotationValue::String(s) => Some(s.clone()),
            _ => None,
        })
    }

    fn extract_schema_field(
        &self,
        annotations: &crate::introspection::provenance::ProvenanceAnnotationSet,
    ) -> Option<String> {
        annotations
            .get("schema.field")
            .and_then(|a| match &a.value {
                crate::frontend::schema_ast::AnnotationValue::String(s) => Some(s.clone()),
                _ => None,
            })
    }

    fn extract_lowering_rule(
        &self,
        annotations: &crate::introspection::provenance::ProvenanceAnnotationSet,
    ) -> Option<String> {
        annotations
            .get("lowering.rule")
            .and_then(|a| match &a.value {
                crate::frontend::schema_ast::AnnotationValue::String(s) => Some(s.clone()),
                _ => None,
            })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STATISTICS
// ═══════════════════════════════════════════════════════════════════════════

/// Statistics about a trace graph
#[derive(Debug, Clone)]
pub struct TraceStats {
    pub total_nodes: usize,
    pub total_links: usize,
    pub token_count: usize,
    pub generic_ast_count: usize,
    pub schema_ast_count: usize,
    pub core_ir_count: usize,
}

impl std::fmt::Display for TraceStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Trace Graph Statistics:")?;
        writeln!(f, "  Total Nodes: {}", self.total_nodes)?;
        writeln!(f, "  Total Links: {}", self.total_links)?;
        writeln!(f, "  Tokens: {}", self.token_count)?;
        writeln!(f, "  Generic AST Nodes: {}", self.generic_ast_count)?;
        writeln!(f, "  Schema AST Nodes: {}", self.schema_ast_count)?;
        writeln!(f, "  Core IR Nodes: {}", self.core_ir_count)?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::token::{Token, TokenKind};
    use crate::introspection::provenance::{lowering, schema, source, ProvenanceAnnotationSet};
    use crate::introspection::trace_graph::TraceGraphBuilder;

    #[test]
    fn test_inspector_origin() {
        let mut builder = TraceGraphBuilder::new();

        let token = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let mut annotations = ProvenanceAnnotationSet::new();
        annotations.add(source::file("test.ax"));
        let token_id = builder.add_token(token, annotations);

        let _core_ir_id = builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            Some(Span::new(0, 3)),
            ProvenanceAnnotationSet::new(),
            vec![token_id],
        );

        let graph = builder.build();
        let inspector = Inspector::new(graph);

        let origin = inspector.origin(1);
        assert!(origin.is_some());
        let origin = origin.unwrap();
        assert_eq!(origin.source_file, Some("test.ax".to_string()));
        assert_eq!(origin.source_spans.len(), 1);
    }

    #[test]
    fn test_inspector_schema_rule() {
        let mut builder = TraceGraphBuilder::new();

        let token = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token_id = builder.add_token(token, ProvenanceAnnotationSet::new());

        let mut schema_annotations = ProvenanceAnnotationSet::new();
        schema_annotations.add(schema::node("IfExpr"));
        schema_annotations.add(schema::rule("if_statement"));

        let schema_id = builder.add_schema_ast(
            "IfExpr".to_string(),
            3,
            Some(Span::new(0, 3)),
            schema_annotations,
            vec![token_id],
        );

        let _core_ir_id = builder.add_core_ir(
            "CIf".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            vec![schema_id],
        );

        let graph = builder.build();
        let inspector = Inspector::new(graph);

        let schema_info = inspector.schema_rule(1);
        assert!(schema_info.is_some());
        let schema_info = schema_info.unwrap();
        assert_eq!(schema_info.node_kind, "IfExpr");
        assert_eq!(schema_info.rule_name, Some("if_statement".to_string()));
    }

    #[test]
    fn test_inspector_lowering_rule() {
        let mut builder = TraceGraphBuilder::new();

        let mut annotations = ProvenanceAnnotationSet::new();
        annotations.add(lowering::rule("lower_if_expr"));
        annotations.add(lowering::phase("schema_lowering"));

        builder.add_core_ir(
            "CIf".to_string(),
            Some(1),
            None,
            annotations,
            Vec::new(), // Will fail invariants but ok for this test
        );

        let graph = builder.build();
        let inspector = Inspector::new(graph);

        let lowering_info = inspector.lowering_rule(1);
        assert!(lowering_info.is_some());
        let lowering_info = lowering_info.unwrap();
        assert_eq!(lowering_info.rule_name, Some("lower_if_expr".to_string()));
        assert_eq!(lowering_info.phase, Some("schema_lowering".to_string()));
    }

    #[test]
    fn test_inspector_stats() {
        let mut builder = TraceGraphBuilder::new();

        let token = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token_id = builder.add_token(token, ProvenanceAnnotationSet::new());

        let ast_id = builder.add_generic_ast(
            "ident".to_string(),
            0,
            Some(Span::new(0, 3)),
            ProvenanceAnnotationSet::new(),
            vec![token_id],
        );

        let schema_id = builder.add_schema_ast(
            "Ident".to_string(),
            1,
            Some(Span::new(0, 3)),
            ProvenanceAnnotationSet::new(),
            vec![ast_id],
        );

        builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            vec![schema_id],
        );

        let graph = builder.build();
        let inspector = Inspector::new(graph);

        let stats = inspector.stats();
        assert_eq!(stats.total_nodes, 4);
        assert_eq!(stats.token_count, 1);
        assert_eq!(stats.generic_ast_count, 1);
        assert_eq!(stats.schema_ast_count, 1);
        assert_eq!(stats.core_ir_count, 1);
    }
}
