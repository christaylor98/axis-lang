//! Shared Core IR 0.5 emission helpers (Phase 3c).
//!
//! All five remaining lowering paths MUST use these functions.
//! Copy-pasting the 3a/3b builder pattern into each path is a defect.
//!
//! # Shared here
//! - `unit_type_id()` / `intern_unit()` — the only literal type proven by the Phase-3a pilot.
//! - `SchemaLower05Error` — shared error for all SchemaAstNode → 0.5 paths.
//! - `schema_field_node` / `schema_field_nodes` / `schema_field_token` — shared field extraction.

use crate::fabric::hash::Hash256;
use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use crate::frontend::token::Token;
use crate::ir::core_ir_05::{Builder, BuilderError, NodeRef};
use crate::registry::core05::{
    codec::type_identity,
    record::{PrimCode, TypeDef, TypeShape},
};
use std::fmt;

// ─── UNIT TYPE IDENTITY ─────────────────────────────────────────────────────

/// Type identity for the `unit` primitive (the `def_hash` for all unit constants).
/// Computed from the canonical TypeDef; never invented.
pub fn unit_type_id() -> Hash256 {
    type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Unit) })
}

/// Intern a unit constant into the builder's constant pool. Deduplicated automatically.
pub fn intern_unit(builder: &mut Builder<'_>) -> NodeRef {
    builder.intern_constant(unit_type_id(), vec![])
}

// ─── FN TYPE IDENTITY ───────────────────────────────────────────────────────

/// Type identity for the `Fn` primitive (PrimCode::Fn = 8).
pub fn fn_type_id() -> Hash256 {
    type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Fn) })
}

/// Intern a fn-ref constant: def_hash = fn_type_id(), payload = 32-byte identity.
pub fn intern_fn_ref(builder: &mut Builder<'_>, fn_identity: Hash256) -> NodeRef {
    builder.intern_constant(fn_type_id(), fn_identity.as_bytes().to_vec())
}

// ─── PARAM SLOT IDENTITY ────────────────────────────────────────────────────

/// Type identity for the `Param` primitive (PrimCode::Param = 9).
///
/// A pool entry with this def_hash represents a parameter slot in a `kind
/// composite` fn's body bundle. Its payload is a varint encoding the
/// 0-based parameter index. At eject, when the bridge emits a CCall to a
/// composite, it walks the body bundle and substitutes every Param-typed
/// pool entry with the corresponding caller-arg expression.
pub fn param_type_id() -> Hash256 {
    type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Param) })
}

/// Intern a parameter-slot pool entry: def_hash = param_type_id(), payload =
/// varint(slot_index). Distinct slot indices produce distinct payloads, so
/// multiple Param entries never alias in the constant pool (a real bug in
/// the prior empty-payload sentinel: two same-typed params interned to the
/// same pool slot and the second one's references silently aliased the
/// first).
pub fn intern_param_slot(builder: &mut Builder<'_>, slot_index: u32) -> NodeRef {
    let mut payload = Vec::new();
    crate::fabric::codec::write_varint(&mut payload, slot_index as u64);
    builder.intern_constant(param_type_id(), payload)
}

/// Try to decode a constant pool entry as a parameter slot.
/// Returns `Some(slot_index)` if `def_hash == param_type_id()` and payload
/// decodes as a single varint; otherwise `None`.
pub fn decode_param_slot(entry_def_hash: &Hash256, payload: &[u8]) -> Option<u32> {
    if *entry_def_hash != param_type_id() {
        return None;
    }
    let mut r = crate::fabric::codec::Reader::new(payload);
    let v = crate::fabric::codec::read_varint(&mut r).ok()?;
    u32::try_from(v).ok()
}

// ─── SHARED ERROR TYPE ──────────────────────────────────────────────────────

/// Shared error for all SchemaAstNode → Core IR 0.5 lowering paths (Phase 3c).
#[derive(Debug, PartialEq, Eq)]
pub enum SchemaLower05Error {
    /// Node kind has no defined 0.5 lowering — STOP-and-report, never improvise.
    UnsupportedNode(String),
    /// Callable name not found in the active scope — UNKNOWN gate hard halt.
    UnknownCallable(String),
    /// Required field is missing from the node.
    MissingField { node_kind: String, field: String },
    /// Two `FnDecl`s claim the same name, or a user `FnDecl` collides with a
    /// name already bound in the active registry scope. User fn identity is
    /// `SHA-256(name)`, so a repeated name is a repeated identity: the second
    /// record is a genuine `ResolveError::Conflict`, not a benign re-insert.
    DuplicateFn(String),
    /// A user `FnDecl` collides with a name the merged registry scope already
    /// holds, in a way re-blessing cannot explain — the registry record is a
    /// leaf (so this file is not its body), or it is a composite whose declared
    /// signature disagrees with the one written here.
    ///
    /// Kept apart from [`DuplicateFn`] because the two want opposite fixes and
    /// the old shared wording ("already bound in the active scope") named
    /// neither the registry nor the file, which is the whole of what the author
    /// needs to know.
    RegistryFnCollision { name: String, detail: String },
    /// The builder rejected a node (forward edge, pool OOB, or UNKNOWN gate).
    Builder(BuilderError),
}

impl fmt::Display for SchemaLower05Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaLower05Error::UnsupportedNode(kind) =>
                write!(f, "no 0.5 lowering for node kind '{}' — STOP", kind),
            SchemaLower05Error::UnknownCallable(name) =>
                write!(f, "UNKNOWN gate: callable '{}' not found in scope", name),
            SchemaLower05Error::MissingField { node_kind, field } =>
                write!(f, "missing field '{}' in node '{}'", field, node_kind),
            SchemaLower05Error::DuplicateFn(name) =>
                write!(f, "duplicate function '{}' — declared twice in this file", name),
            SchemaLower05Error::RegistryFnCollision { name, detail } =>
                write!(f, "function '{}' collides with a registry entry of the same name: {}", name, detail),
            SchemaLower05Error::Builder(e) =>
                write!(f, "builder error: {}", e),
        }
    }
}

impl std::error::Error for SchemaLower05Error {}

impl From<BuilderError> for SchemaLower05Error {
    fn from(e: BuilderError) -> Self {
        SchemaLower05Error::Builder(e)
    }
}

// ─── SCHEMA AST FIELD EXTRACTION ────────────────────────────────────────────

/// Extract a single child node from a SchemaAstNode field.
pub fn schema_field_node<'a>(
    node: &'a SchemaAstNode,
    field: &str,
) -> Result<&'a SchemaAstNode, SchemaLower05Error> {
    node.fields
        .get(field)
        .and_then(|v| match v {
            SchemaValue::Node(n) => Some(n.as_ref()),
            _ => None,
        })
        .ok_or_else(|| SchemaLower05Error::MissingField {
            node_kind: node.kind.clone(),
            field: field.to_string(),
        })
}

/// Extract a list of child nodes from a SchemaAstNode field.
pub fn schema_field_nodes<'a>(
    node: &'a SchemaAstNode,
    field: &str,
) -> Result<Vec<&'a SchemaAstNode>, SchemaLower05Error> {
    node.fields
        .get(field)
        .and_then(|v| match v {
            SchemaValue::Nodes(ns) => Some(ns.iter().collect()),
            _ => None,
        })
        .ok_or_else(|| SchemaLower05Error::MissingField {
            node_kind: node.kind.clone(),
            field: field.to_string(),
        })
}

/// Extract a token's lexeme from a SchemaAstNode field.
pub fn schema_field_token(
    node: &SchemaAstNode,
    field: &str,
) -> Result<String, SchemaLower05Error> {
    node.fields
        .get(field)
        .and_then(|v| match v {
            SchemaValue::Token(t) => Some(t.lexeme.clone()),
            _ => None,
        })
        .ok_or_else(|| SchemaLower05Error::MissingField {
            node_kind: node.kind.clone(),
            field: field.to_string(),
        })
}

/// Extract the full Token (kind + lexeme) from a SchemaAstNode field.
pub fn schema_field_full_token<'a>(
    node: &'a SchemaAstNode,
    field: &str,
) -> Result<&'a Token, SchemaLower05Error> {
    node.fields
        .get(field)
        .and_then(|v| match v {
            SchemaValue::Token(t) => Some(t),
            _ => None,
        })
        .ok_or_else(|| SchemaLower05Error::MissingField {
            node_kind: node.kind.clone(),
            field: field.to_string(),
        })
}
