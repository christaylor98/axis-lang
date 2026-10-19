// Wave B: Provenance Annotation System
//
// System-generated annotations (distinct from user annotations):
// - Namespaced to prevent collision with user annotations
// - Attached during parsing, schema projection, and lowering
// - Immutable once attached
// - Ignored by execution

use crate::frontend::token::Span;
use crate::frontend::schema_ast::{Annotation, AnnotationValue};

// ═══════════════════════════════════════════════════════════════════════════
// PROVENANCE NAMESPACES
// ═══════════════════════════════════════════════════════════════════════════

/// Provenance annotation namespace
///
/// System-generated annotations are namespaced to prevent collision with user annotations.
/// User annotations MUST NOT use these reserved namespaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProvenanceNamespace {
    /// Source location annotations (@source.*)
    Source,
    /// Schema projection annotations (@schema.*)
    Schema,
    /// Lowering annotations (@lowering.*)
    Lowering,
    /// Registry annotations (@registry.*)
    Registry,
    /// Pipeline annotations (@pipeline.*)
    Pipeline,
}

impl ProvenanceNamespace {
    /// Get the string prefix for this namespace
    pub fn prefix(&self) -> &'static str {
        match self {
            ProvenanceNamespace::Source => "source",
            ProvenanceNamespace::Schema => "schema",
            ProvenanceNamespace::Lowering => "lowering",
            ProvenanceNamespace::Registry => "registry",
            ProvenanceNamespace::Pipeline => "pipeline",
        }
    }

    /// Check if a key belongs to this namespace
    pub fn matches(&self, key: &str) -> bool {
        key.starts_with(self.prefix()) && key.chars().nth(self.prefix().len()) == Some('.')
    }

    /// Create a qualified key for this namespace
    pub fn qualify(&self, name: &str) -> String {
        format!("{}.{}", self.prefix(), name)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVENANCE ANNOTATIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Provenance annotation - system-generated annotation with namespace
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceAnnotation {
    pub namespace: ProvenanceNamespace,
    pub key: String,
    pub value: AnnotationValue,
}

impl ProvenanceAnnotation {
    /// Create a new provenance annotation
    pub fn new(
        namespace: ProvenanceNamespace,
        name: impl Into<String>,
        value: AnnotationValue,
    ) -> Self {
        let name = name.into();
        let key = namespace.qualify(&name);
        Self {
            namespace,
            key,
            value,
        }
    }

    /// Convert to Core IR annotation
    pub fn to_core_ir_annotation(&self) -> Annotation {
        Annotation {
            key: self.key.clone(),
            value: self.value.clone(),
        }
    }

    /// Check if an annotation is a provenance annotation
    pub fn is_provenance(annotation: &Annotation) -> bool {
        Self::namespace_of(&annotation.key).is_some()
    }

    /// Get the namespace of a provenance annotation key
    pub fn namespace_of(key: &str) -> Option<ProvenanceNamespace> {
        if ProvenanceNamespace::Source.matches(key) {
            Some(ProvenanceNamespace::Source)
        } else if ProvenanceNamespace::Schema.matches(key) {
            Some(ProvenanceNamespace::Schema)
        } else if ProvenanceNamespace::Lowering.matches(key) {
            Some(ProvenanceNamespace::Lowering)
        } else if ProvenanceNamespace::Registry.matches(key) {
            Some(ProvenanceNamespace::Registry)
        } else if ProvenanceNamespace::Pipeline.matches(key) {
            Some(ProvenanceNamespace::Pipeline)
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVENANCE ANNOTATION CONSTRUCTORS
// ═══════════════════════════════════════════════════════════════════════════

/// Source location provenance annotations
pub mod source {
    use super::*;

    /// Attach source span information
    pub fn span(span: &Span) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Source,
            "span",
            AnnotationValue::String(format!("{}..{}", span.start, span.end)),
        )
    }

    /// Attach source file information
    pub fn file(path: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Source,
            "file",
            AnnotationValue::String(path.into()),
        )
    }

    /// Attach source text information
    pub fn text(text: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Source,
            "text",
            AnnotationValue::String(text.into()),
        )
    }
}

/// Schema projection provenance annotations
pub mod schema {
    use super::*;

    /// Attach schema node kind information
    pub fn node(kind: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Schema,
            "node",
            AnnotationValue::String(kind.into()),
        )
    }

    /// Attach schema rule information
    pub fn rule(rule: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Schema,
            "rule",
            AnnotationValue::String(rule.into()),
        )
    }

    /// Attach schema field information
    pub fn field(field: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Schema,
            "field",
            AnnotationValue::String(field.into()),
        )
    }
}

/// Lowering provenance annotations
pub mod lowering {
    use super::*;

    /// Attach lowering rule information
    pub fn rule(rule: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Lowering,
            "rule",
            AnnotationValue::String(rule.into()),
        )
    }

    /// Attach lowering phase information
    pub fn phase(phase: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Lowering,
            "phase",
            AnnotationValue::String(phase.into()),
        )
    }

    /// Attach lowering source node ID
    pub fn source_node_id(id: u64) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Lowering,
            "source_node_id",
            AnnotationValue::Int(id as i64),
        )
    }
}

/// Registry provenance annotations
pub mod registry {
    use super::*;

    /// Attach registry ID information
    pub fn id(id: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Registry,
            "id",
            AnnotationValue::String(id.into()),
        )
    }

    /// Attach registry version information
    pub fn version(version: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Registry,
            "version",
            AnnotationValue::String(version.into()),
        )
    }
}

/// Pipeline provenance annotations
pub mod pipeline {
    use super::*;

    /// Attach pipeline phase information
    pub fn phase(phase: impl Into<String>) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Pipeline,
            "phase",
            AnnotationValue::String(phase.into()),
        )
    }

    /// Attach pipeline timestamp
    pub fn timestamp(ts: u64) -> ProvenanceAnnotation {
        ProvenanceAnnotation::new(
            ProvenanceNamespace::Pipeline,
            "timestamp",
            AnnotationValue::Int(ts as i64),
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PROVENANCE ANNOTATION SETS
// ═══════════════════════════════════════════════════════════════════════════

/// Set of provenance annotations organized by namespace
#[derive(Debug, Clone, Default)]
pub struct ProvenanceAnnotationSet {
    annotations: Vec<ProvenanceAnnotation>,
}

impl ProvenanceAnnotationSet {
    /// Create an empty annotation set
    pub fn new() -> Self {
        Self {
            annotations: Vec::new(),
        }
    }

    /// Add a provenance annotation
    pub fn add(&mut self, annotation: ProvenanceAnnotation) {
        self.annotations.push(annotation);
    }

    /// Get all annotations
    pub fn all(&self) -> &[ProvenanceAnnotation] {
        &self.annotations
    }

    /// Get annotations in a specific namespace
    pub fn in_namespace(&self, namespace: &ProvenanceNamespace) -> Vec<&ProvenanceAnnotation> {
        self.annotations
            .iter()
            .filter(|a| &a.namespace == namespace)
            .collect()
    }

    /// Convert to Core IR annotations
    pub fn to_core_ir_annotations(&self) -> Vec<Annotation> {
        self.annotations
            .iter()
            .map(|a| a.to_core_ir_annotation())
            .collect()
    }

    /// Check if this set contains a specific key
    pub fn contains_key(&self, key: &str) -> bool {
        self.annotations.iter().any(|a| a.key == key)
    }

    /// Get annotation by key
    pub fn get(&self, key: &str) -> Option<&ProvenanceAnnotation> {
        self.annotations.iter().find(|a| a.key == key)
    }

    /// Merge another annotation set into this one
    pub fn merge(&mut self, other: &ProvenanceAnnotationSet) {
        for annotation in &other.annotations {
            if !self.contains_key(&annotation.key) {
                self.annotations.push(annotation.clone());
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_qualify() {
        assert_eq!(ProvenanceNamespace::Source.qualify("span"), "source.span");
        assert_eq!(ProvenanceNamespace::Schema.qualify("node"), "schema.node");
        assert_eq!(
            ProvenanceNamespace::Lowering.qualify("rule"),
            "lowering.rule"
        );
    }

    #[test]
    fn test_namespace_matches() {
        assert!(ProvenanceNamespace::Source.matches("source.span"));
        assert!(!ProvenanceNamespace::Source.matches("schema.node"));
        assert!(!ProvenanceNamespace::Source.matches("sourcecode")); // No dot
    }

    #[test]
    fn test_namespace_detection() {
        assert_eq!(
            ProvenanceAnnotation::namespace_of("source.span"),
            Some(ProvenanceNamespace::Source)
        );
        assert_eq!(
            ProvenanceAnnotation::namespace_of("schema.node"),
            Some(ProvenanceNamespace::Schema)
        );
        assert_eq!(ProvenanceAnnotation::namespace_of("user.annotation"), None);
    }

    #[test]
    fn test_provenance_annotation_creation() {
        let ann = source::span(&Span::new(0, 10));
        assert_eq!(ann.namespace, ProvenanceNamespace::Source);
        assert_eq!(ann.key, "source.span");
    }

    #[test]
    fn test_annotation_set() {
        let mut set = ProvenanceAnnotationSet::new();
        set.add(source::span(&Span::new(0, 10)));
        set.add(schema::node("IfExpr"));

        assert_eq!(set.all().len(), 2);
        assert_eq!(set.in_namespace(&ProvenanceNamespace::Source).len(), 1);
        assert_eq!(set.in_namespace(&ProvenanceNamespace::Schema).len(), 1);
        assert!(set.contains_key("source.span"));
        assert!(set.contains_key("schema.node"));
    }

    #[test]
    fn test_annotation_set_merge() {
        let mut set1 = ProvenanceAnnotationSet::new();
        set1.add(source::span(&Span::new(0, 10)));

        let mut set2 = ProvenanceAnnotationSet::new();
        set2.add(schema::node("IfExpr"));

        set1.merge(&set2);
        assert_eq!(set1.all().len(), 2);
    }

    #[test]
    fn test_no_duplicate_keys_on_merge() {
        let mut set1 = ProvenanceAnnotationSet::new();
        set1.add(source::span(&Span::new(0, 10)));

        let mut set2 = ProvenanceAnnotationSet::new();
        set2.add(source::span(&Span::new(5, 15))); // Same key, different value

        set1.merge(&set2);
        assert_eq!(set1.all().len(), 1); // Should not duplicate
    }
}
