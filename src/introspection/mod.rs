// Wave B: Introspection, Traceability, and Trust Surfaces
//
// This module provides full introspection and trust tooling over the pipeline:
// - Provenance annotations (system-generated, namespaced)
// - Trace graph construction (tokens → AST → Schema AST → Core IR)
// - Introspection API (read-only queries)
// - Trust guarantees (determinism, traceability)
//
// FORBIDDEN:
// - Changing execution behavior
// - Changing lowering semantics
// - Changing parsing rules
// - Mutation of trace data
// - Performance optimizations that affect semantics

pub mod cli;
pub mod inspection;
pub mod inspector;
pub mod provenance;
pub mod trace_graph;
pub mod trust; // Wave 6: Core IR JSON serialization

// generate_code_trace / render_trace (legacy 0.3 Core IR path) removed in Phase 3d.
pub use inspector::Inspector;
pub use provenance::{ProvenanceAnnotation, ProvenanceNamespace};
pub use trace_graph::{TraceGraph, TraceGraphBuilder, TraceLink, TraceNode};
pub use trust::verify_trace_invariants;
