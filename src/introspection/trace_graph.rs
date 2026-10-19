// Wave B: Trace Graph Construction
//
// Links tokens → AST → Schema AST → Core IR
// - Immutable once constructed
// - Out-of-band (not embedded in Core IR)
// - Tooling-only
// - Preserves spans and annotations

use crate::frontend::token::{Span, Token};
use crate::introspection::provenance::ProvenanceAnnotationSet;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// TRACE NODE IDS
// ═══════════════════════════════════════════════════════════════════════════

/// Unique identifier for a trace node
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceNodeId(u64);

impl TraceNodeId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// ID counter for generating unique trace node IDs
#[derive(Debug, Clone)]
pub struct TraceIdGenerator {
    next_id: u64,
}

impl TraceIdGenerator {
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    pub fn next(&mut self) -> TraceNodeId {
        let id = TraceNodeId(self.next_id);
        self.next_id += 1;
        id
    }
}

impl Default for TraceIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TRACE NODES
// ═══════════════════════════════════════════════════════════════════════════

/// Trace node - represents a node in the pipeline with full provenance
#[derive(Debug, Clone)]
pub struct TraceNode {
    /// Unique ID for this trace node
    pub id: TraceNodeId,
    /// Type of node
    pub kind: TraceNodeKind,
    /// Source span (if available)
    pub span: Option<Span>,
    /// Provenance annotations
    pub annotations: ProvenanceAnnotationSet,
    /// Links to parent nodes (sources)
    pub parents: Vec<TraceNodeId>,
}

/// Kind of trace node
#[derive(Debug, Clone)]
pub enum TraceNodeKind {
    /// Token from lexer
    Token { token: Token },
    /// Generic AST node from parser
    GenericAst { kind: String, child_count: usize },
    /// Schema AST node from schema projection
    SchemaAst { kind: String, field_count: usize },
    /// Core IR node from lowering
    CoreIr {
        term_kind: String,
        node_id: Option<u64>,
    },
}

impl TraceNode {
    /// Create a new trace node
    pub fn new(
        id: TraceNodeId,
        kind: TraceNodeKind,
        span: Option<Span>,
        annotations: ProvenanceAnnotationSet,
        parents: Vec<TraceNodeId>,
    ) -> Self {
        Self {
            id,
            kind,
            span,
            annotations,
            parents,
        }
    }

    /// Get a human-readable description of this node
    pub fn description(&self) -> String {
        match &self.kind {
            TraceNodeKind::Token { token } => {
                format!("Token({:?} '{}')", token.kind, token.lexeme)
            }
            TraceNodeKind::GenericAst { kind, .. } => {
                format!("GenericAST({})", kind)
            }
            TraceNodeKind::SchemaAst { kind, .. } => {
                format!("SchemaAST({})", kind)
            }
            TraceNodeKind::CoreIr { term_kind, node_id } => {
                if let Some(id) = node_id {
                    format!("CoreIR({} #{})", term_kind, id)
                } else {
                    format!("CoreIR({})", term_kind)
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TRACE LINKS
// ═══════════════════════════════════════════════════════════════════════════

/// Link between trace nodes
#[derive(Debug, Clone)]
pub struct TraceLink {
    /// Source node
    pub from: TraceNodeId,
    /// Target node
    pub to: TraceNodeId,
    /// Link kind
    pub kind: TraceLinkKind,
}

/// Kind of trace link
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceLinkKind {
    /// Token contributed to Generic AST node
    TokenToGenericAst,
    /// Generic AST contributed to Schema AST node
    GenericAstToSchemaAst,
    /// Schema AST contributed to Core IR node
    SchemaAstToCoreIr,
    /// Core IR node contributed to another Core IR node (composition)
    CoreIrToCoreIr,
}

// ═══════════════════════════════════════════════════════════════════════════
// TRACE GRAPH
// ═══════════════════════════════════════════════════════════════════════════

/// Trace graph - complete traceability graph for pipeline execution
///
/// This structure is IMMUTABLE once constructed and is OUT-OF-BAND
/// (not embedded in Core IR). It provides full traceability from
/// tokens to Core IR.
#[derive(Debug, Clone)]
pub struct TraceGraph {
    /// All trace nodes, indexed by ID
    nodes: HashMap<TraceNodeId, TraceNode>,
    /// All trace links
    links: Vec<TraceLink>,
    /// Index: Core IR node ID -> trace node ID
    core_ir_index: HashMap<u64, TraceNodeId>,
}

impl TraceGraph {
    // fn new() deleted Bundle 8 (2026-06-08): zero callers; TraceGraphBuilder::build() is the
    // only constructor path.

    /// Get a trace node by ID
    pub fn get_node(&self, id: TraceNodeId) -> Option<&TraceNode> {
        self.nodes.get(&id)
    }

    /// Get trace node by Core IR node ID
    pub fn get_node_by_core_ir_id(&self, core_ir_id: u64) -> Option<&TraceNode> {
        self.core_ir_index
            .get(&core_ir_id)
            .and_then(|id| self.nodes.get(id))
    }

    /// Get all nodes
    pub fn all_nodes(&self) -> Vec<&TraceNode> {
        self.nodes.values().collect()
    }

    /// Get all links
    pub fn all_links(&self) -> &[TraceLink] {
        &self.links
    }

    /// Get all links from a specific node
    pub fn links_from(&self, id: TraceNodeId) -> Vec<&TraceLink> {
        self.links.iter().filter(|l| l.from == id).collect()
    }

    /// Get all links to a specific node
    pub fn links_to(&self, id: TraceNodeId) -> Vec<&TraceLink> {
        self.links.iter().filter(|l| l.to == id).collect()
    }

    /// Get all parent nodes (following links backwards)
    pub fn get_parents(&self, id: TraceNodeId) -> Vec<&TraceNode> {
        self.nodes
            .get(&id)
            .map(|node| {
                node.parents
                    .iter()
                    .filter_map(|parent_id| self.nodes.get(parent_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Trace back to all source tokens for a given node
    pub fn trace_to_tokens(&self, id: TraceNodeId) -> Vec<&TraceNode> {
        let mut tokens = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = vec![id];

        while let Some(current_id) = queue.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id);

            if let Some(node) = self.nodes.get(&current_id) {
                match &node.kind {
                    TraceNodeKind::Token { .. } => {
                        tokens.push(node);
                    }
                    _ => {
                        // Continue tracing back
                        queue.extend(&node.parents);
                    }
                }
            }
        }

        tokens
    }

    /// Get all source spans contributing to a node
    pub fn get_source_spans(&self, id: TraceNodeId) -> Vec<Span> {
        self.trace_to_tokens(id)
            .iter()
            .filter_map(|node| node.span.clone())
            .collect()
    }

    /// Count nodes by kind
    pub fn count_by_kind(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for node in self.nodes.values() {
            let kind_name = match &node.kind {
                TraceNodeKind::Token { .. } => "Token",
                TraceNodeKind::GenericAst { .. } => "GenericAst",
                TraceNodeKind::SchemaAst { .. } => "SchemaAst",
                TraceNodeKind::CoreIr { .. } => "CoreIr",
            };
            *counts.entry(kind_name.to_string()).or_insert(0) += 1;
        }
        counts
    }

    /// Verify trace graph invariants (for testing)
    pub fn verify_invariants(&self) -> Result<(), String> {
        // Every Core IR node must be traceable to at least one source
        for (core_ir_id, trace_id) in &self.core_ir_index {
            let tokens = self.trace_to_tokens(*trace_id);
            if tokens.is_empty() {
                return Err(format!(
                    "Core IR node {} (trace {}) has no source tokens",
                    core_ir_id, trace_id.0
                ));
            }
        }

        // All parent references must be valid
        for node in self.nodes.values() {
            for parent_id in &node.parents {
                if !self.nodes.contains_key(parent_id) {
                    return Err(format!(
                        "Node {} references non-existent parent {}",
                        node.id.0, parent_id.0
                    ));
                }
            }
        }

        // All links must reference valid nodes
        for link in &self.links {
            if !self.nodes.contains_key(&link.from) {
                return Err(format!(
                    "Link references non-existent source node {}",
                    link.from.0
                ));
            }
            if !self.nodes.contains_key(&link.to) {
                return Err(format!(
                    "Link references non-existent target node {}",
                    link.to.0
                ));
            }
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TRACE GRAPH BUILDER
// ═══════════════════════════════════════════════════════════════════════════

/// Builder for constructing trace graphs
///
/// This is the ONLY way to create a trace graph. Once built, the graph is immutable.
#[derive(Debug)]
pub struct TraceGraphBuilder {
    id_gen: TraceIdGenerator,
    nodes: HashMap<TraceNodeId, TraceNode>,
    links: Vec<TraceLink>,
    core_ir_index: HashMap<u64, TraceNodeId>,
}

impl TraceGraphBuilder {
    /// Create a new trace graph builder
    pub fn new() -> Self {
        Self {
            id_gen: TraceIdGenerator::new(),
            nodes: HashMap::new(),
            links: Vec::new(),
            core_ir_index: HashMap::new(),
        }
    }

    /// Add a token node
    pub fn add_token(&mut self, token: Token, annotations: ProvenanceAnnotationSet) -> TraceNodeId {
        let id = self.id_gen.next();
        let span = Some(token.span.clone());
        let node = TraceNode::new(
            id,
            TraceNodeKind::Token { token },
            span,
            annotations,
            Vec::new(),
        );
        self.nodes.insert(id, node);
        id
    }

    /// Add a generic AST node
    pub fn add_generic_ast(
        &mut self,
        kind: String,
        child_count: usize,
        span: Option<Span>,
        annotations: ProvenanceAnnotationSet,
        parents: Vec<TraceNodeId>,
    ) -> TraceNodeId {
        let id = self.id_gen.next();
        let node = TraceNode::new(
            id,
            TraceNodeKind::GenericAst { kind, child_count },
            span,
            annotations,
            parents,
        );
        self.nodes.insert(id, node);
        id
    }

    /// Add a schema AST node
    pub fn add_schema_ast(
        &mut self,
        kind: String,
        field_count: usize,
        span: Option<Span>,
        annotations: ProvenanceAnnotationSet,
        parents: Vec<TraceNodeId>,
    ) -> TraceNodeId {
        let id = self.id_gen.next();
        let node = TraceNode::new(
            id,
            TraceNodeKind::SchemaAst { kind, field_count },
            span,
            annotations,
            parents,
        );
        self.nodes.insert(id, node);
        id
    }

    /// Add a Core IR node
    pub fn add_core_ir(
        &mut self,
        term_kind: String,
        node_id: Option<u64>,
        span: Option<Span>,
        annotations: ProvenanceAnnotationSet,
        parents: Vec<TraceNodeId>,
    ) -> TraceNodeId {
        let id = self.id_gen.next();
        let node = TraceNode::new(
            id,
            TraceNodeKind::CoreIr { term_kind, node_id },
            span,
            annotations,
            parents,
        );
        self.nodes.insert(id, node);

        // Index by Core IR node ID if available
        if let Some(core_ir_id) = node_id {
            self.core_ir_index.insert(core_ir_id, id);
        }

        id
    }

    /// Add a link between nodes
    pub fn add_link(&mut self, from: TraceNodeId, to: TraceNodeId, kind: TraceLinkKind) {
        self.links.push(TraceLink { from, to, kind });
    }

    /// Build the immutable trace graph
    pub fn build(self) -> TraceGraph {
        TraceGraph {
            nodes: self.nodes,
            links: self.links,
            core_ir_index: self.core_ir_index,
        }
    }
}

impl Default for TraceGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::token::TokenKind;

    #[test]
    fn test_trace_id_generator() {
        let mut gen = TraceIdGenerator::new();
        let id1 = gen.next();
        let id2 = gen.next();
        assert_eq!(id1.as_u64(), 0);
        assert_eq!(id2.as_u64(), 1);
    }

    #[test]
    fn test_trace_graph_builder() {
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

        builder.add_link(token_id, ast_id, TraceLinkKind::TokenToGenericAst);

        let graph = builder.build();

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.links.len(), 1);
    }

    #[test]
    fn test_trace_to_tokens() {
        let mut builder = TraceGraphBuilder::new();

        let token1 = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token2 = Token::new(TokenKind::Ident, "bar".to_string(), Span::new(4, 7));

        let token_id1 = builder.add_token(token1, ProvenanceAnnotationSet::new());
        let token_id2 = builder.add_token(token2, ProvenanceAnnotationSet::new());

        let ast_id = builder.add_generic_ast(
            "call".to_string(),
            2,
            Some(Span::new(0, 7)),
            ProvenanceAnnotationSet::new(),
            vec![token_id1, token_id2],
        );

        let graph = builder.build();

        let tokens = graph.trace_to_tokens(ast_id);
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_core_ir_index() {
        let mut builder = TraceGraphBuilder::new();

        let core_ir_id = builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(42),
            None,
            ProvenanceAnnotationSet::new(),
            Vec::new(),
        );

        let graph = builder.build();

        let node = graph.get_node_by_core_ir_id(42);
        assert!(node.is_some());
        assert_eq!(node.unwrap().id, core_ir_id);
    }

    #[test]
    fn test_verify_invariants() {
        let mut builder = TraceGraphBuilder::new();

        let token = Token::new(TokenKind::Ident, "foo".to_string(), Span::new(0, 3));
        let token_id = builder.add_token(token, ProvenanceAnnotationSet::new());

        let _core_ir_id = builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            vec![token_id],
        );

        let graph = builder.build();

        assert!(graph.verify_invariants().is_ok());
    }

    #[test]
    fn test_verify_invariants_fails_for_orphan_core_ir() {
        let mut builder = TraceGraphBuilder::new();

        // Core IR node with no parents (orphan)
        builder.add_core_ir(
            "CUnitLit".to_string(),
            Some(1),
            None,
            ProvenanceAnnotationSet::new(),
            Vec::new(),
        );

        let graph = builder.build();

        // Should fail because Core IR node has no source tokens
        assert!(graph.verify_invariants().is_err());
    }
}
