//! The six registry record kinds for Core IR 0.5 (Registry Format §2).
//!
//! Definitions only — encoding lives in [`super::codec`]. Records are immutable
//! and append-only; the contract grows via [`ContractAppend`] chained by
//! `prev` content hash (Registry §"The Contract").

use crate::fabric::hash::Hash256;

// ─────────────────────────── record kind tags ───────────────────────────────
//
// The leading varint of every canonical record (Format §2).

pub const TAG_TYPE_DEF: u64 = 0x01;
pub const TAG_FN_GENESIS: u64 = 0x02;
pub const TAG_CONTRACT_APPEND: u64 = 0x03;
pub const TAG_LEAF_FACTS: u64 = 0x04;
pub const TAG_COMPOSITE_BODY: u64 = 0x05;
pub const TAG_IMPLEMENTATION: u64 = 0x06;

// ─────────────────────────── primitive vocabulary ───────────────────────────
//
// "The canonical primitive code" (Format §3, primitive shape). The numeric
// vocabulary is defined here so it is stable across encode/decode; the codes
// align with the payload encoders in `fabric::codec` so a `TypeDef` schema and
// its payload encoder agree.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimCode {
    Unit = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    Bytes = 4,
    Text = 5,
    Value = 6,
    Dec = 7,
    /// Fn reference. Payload is the 32-byte minted identity hash of a registry
    /// function. See axis_core_ir_0_5.capnp lines 50-58, 178-186.
    Fn = 8,
    /// Parameter slot. Payload is a varint encoding the parameter index of the
    /// enclosing fn (0-based, matching the position in the registry entry's
    /// `in` clause). The IR has no Lam / parameter-binding construct; instead,
    /// a `kind composite` fn's body bundle stores parameter references as pool
    /// entries with this type. The bridge substitutes each Param-typed pool
    /// entry with the corresponding caller argument when emitting a CCall to
    /// that composite — uniform with how `kind leaf` calls already work; no
    /// new node kinds added (Core IR 0.5 stays at exactly three kinds).
    Param = 9,
}

impl PrimCode {
    pub fn from_varint(v: u64) -> Option<Self> {
        match v {
            0 => Some(PrimCode::Unit),
            1 => Some(PrimCode::Bool),
            2 => Some(PrimCode::Int),
            3 => Some(PrimCode::Float),
            4 => Some(PrimCode::Bytes),
            5 => Some(PrimCode::Text),
            6 => Some(PrimCode::Value),
            7 => Some(PrimCode::Dec),
            8 => Some(PrimCode::Fn),
            9 => Some(PrimCode::Param),
            _ => None,
        }
    }

    pub fn as_varint(self) -> u64 {
        self as u64
    }
}

// ─────────────────────────────── TypeDef (0x01) ──────────────────────────────
//
// Structural. Identity == content_hash of the canonical TypeDef encoding.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeShape {
    /// shape_kind = 0
    Primitive(PrimCode),
    /// shape_kind = 1; positional, ordered field TYPE identities.
    Product(Vec<Hash256>),
    /// shape_kind = 2; ordered constructors; each is an ordered list of field
    /// type identities.
    Sum(Vec<Vec<Hash256>>),
    /// shape_kind = 3; homogeneous list of the given element type identity.
    List(Hash256),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef {
    pub shape: TypeShape,
}

// ─────────────────────────────── FnGenesis (0x02) ────────────────────────────
//
// Nominal. `identity` is a MINTED token stored in the record; it is NOT derived
// from the record's content. The content hash of the record is used for chain
// pinning (e.g. `ContractAppend.prev` for generation 1) and is distinct from
// `identity` (Format §4).

/// `record_kind` discriminator: 0 = leaf, 1 = composite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    Leaf = 0,
    Composite = 1,
}

impl FnKind {
    pub fn from_varint(v: u64) -> Option<Self> {
        match v {
            0 => Some(FnKind::Leaf),
            1 => Some(FnKind::Composite),
            _ => None,
        }
    }
    pub fn as_varint(self) -> u64 {
        self as u64
    }
}

/// Advisory execution disposition (Registry §"Advisory Execution Disposition").
/// 0 = unspecified, 1 = interactive, 2 = batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Unspecified = 0,
    Interactive = 1,
    Batch = 2,
}

impl Disposition {
    pub fn from_varint(v: u64) -> Option<Self> {
        match v {
            0 => Some(Disposition::Unspecified),
            1 => Some(Disposition::Interactive),
            2 => Some(Disposition::Batch),
            _ => None,
        }
    }
    pub fn as_varint(self) -> u64 {
        self as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnGenesis {
    /// Minted token, opaque, stored — NOT a hash of this record.
    pub identity: Hash256,
    /// Ordered input TYPE identities (the frozen signature genesis clause).
    pub inputs: Vec<Hash256>,
    /// Output TYPE identity.
    pub output: Hash256,
    pub kind: FnKind,
    pub disposition: Disposition,
    /// Relaxes the verifier's arity/type check; what is relaxed depends on `inputs`:
    /// empty — any count, unchecked; one type `T` — any count, every arg a `T`;
    /// two or more — exact count, types unchecked. See `validation/core_ir.rs`
    /// step 2a. Expressed in the registry with `variadic true`; default false.
    pub variadic: bool,
}

// ─────────────────────────── ContractAppend (0x03) ───────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAppend {
    /// The function whose contract this extends.
    pub identity: Hash256,
    /// Content hash of the prior generation record (the `FnGenesis` for gen 1).
    pub prev: Hash256,
    /// Monotonic; `prev.generation + 1`.
    pub generation: u64,
    /// Added clauses: each is a predicate-function identity that must hold over
    /// `(inputs, output)`.
    pub added: Vec<Hash256>,
    /// Content-addressed characterization corpus, or `Hash256::ZERO` if none.
    pub oracle: Hash256,
}

// ─────────────────────────── LeafFacts (0x04) ────────────────────────────────

/// An obligation set carried by a function. Each bit is an independent
/// obligation; obligations compose by bitwise OR without loss.
///
/// Bit layout:
///   bit 0 (0x01) — `Reads`             reads external state
///   bit 1 (0x02) — `Writes`            writes external state
///   bit 2 (0x04) — `Nondeterministic`  non-reproducible outputs
///   bit 3 (0x08) — `FullIo`            unrestricted I/O channel ownership
///   bit 4 (0x10) — `Irreversible`      side-effect that cannot be undone
///
/// `join(Reads, Writes) = EffectClass(0x03)` — a DISTINCT value from
/// `FullIo = EffectClass(0x08)`. No single named top collapses the set.
///
/// Varint codes: `Pure`=0, `Reads`=1, `Writes`=2, `Nondeterministic`=4,
/// `FullIo`=8, `Irreversible`=16. Multi-obligation sets (e.g. Reads|Writes=3)
/// survive serialization because `from_varint`/`as_varint` round-trip any `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EffectClass(pub u8);

#[allow(non_upper_case_globals)]
impl EffectClass {
    pub const Pure: EffectClass = EffectClass(0);
    pub const Reads: EffectClass = EffectClass(1);
    pub const Writes: EffectClass = EffectClass(2);
    pub const Nondeterministic: EffectClass = EffectClass(4);
    pub const FullIo: EffectClass = EffectClass(8);
    /// An action whose effects cannot be undone (e.g. sending a message,
    /// charging a card, launching a missile). Carries the Irreversible
    /// obligation bit independently of Writes — a write may be reversible;
    /// an Irreversible action may not write external state but still be
    /// unretractable. The gate-domination check (Phase 5) requires any node
    /// carrying this obligation to be control-flow dominated by a determinacy
    /// discharge gate on every path to it.
    pub const Irreversible: EffectClass = EffectClass(0x10);
    /// Permissive ceiling: every defined obligation bit set. Use as the
    /// `max_allowed_effect` ceiling when imposing no effect restriction —
    /// `derived.is_contained_by(ANY)` holds for every valid `derived`.
    pub const ANY: EffectClass = EffectClass(0x1F);

    /// Decode from a registry varint. Accepts any value that fits in a `u8`.
    pub fn from_varint(v: u64) -> Option<Self> {
        if v <= 0xFF { Some(EffectClass(v as u8)) } else { None }
    }

    /// Encode for registry serialization. Inverse of `from_varint`.
    pub fn as_varint(self) -> u64 { self.0 as u64 }

    /// Obligation union: set union over the independent obligation bits.
    ///
    /// `union(Reads, Nondeterministic)` = `EffectClass(0x05)` — distinct from
    /// `union(Writes, Nondeterministic)` = `EffectClass(0x06)`. No single top
    /// element can fake this identity.
    pub fn union(self, other: EffectClass) -> EffectClass {
        EffectClass(self.0 | other.0)
    }

    pub fn has_reads(self) -> bool { self.0 & 0x01 != 0 }
    pub fn has_writes(self) -> bool { self.0 & 0x02 != 0 }
    pub fn has_nondeterministic(self) -> bool { self.0 & 0x04 != 0 }
    pub fn has_irreversible(self) -> bool { self.0 & 0x10 != 0 }

    /// True when every obligation bit in `self` is present in `ceiling`.
    ///
    /// The correct ceiling check: `derived.is_contained_by(max_allowed)` passes
    /// iff the bundle carries no obligation the consumer has not permitted.
    /// Equivalent to `(self.0 & !ceiling.0) == 0`.
    pub fn is_contained_by(self, ceiling: EffectClass) -> bool {
        (self.0 & !ceiling.0) == 0
    }

    /// Derived tier view: answers the gate's subsumption question
    /// "is this effect set at or above the Irreversible tier?"
    ///
    /// This is a read-only view over the existing bitset. EffectClass storage,
    /// union, containment, and 3b derivation are unchanged — tier() derives
    /// from the bits, it does not add any new storage.
    ///
    /// Gate-domination (Phase 5) uses this to decide whether a node requires
    /// a dominating determinacy discharge gate before it may execute.
    pub fn tier(self) -> EffectTier {
        if self.has_irreversible() {
            EffectTier::IrreversibleOrAbove
        } else {
            EffectTier::BelowIrreversible
        }
    }
}

/// Effect tier: derived view answering the gate-domination query
/// "is this effect set at or above the Irreversible tier?"
///
/// This is a VIEW over EffectClass; it adds no storage. The underlying
/// bitset (EffectClass.0) is the source of truth; EffectTier is derived
/// by reading the Irreversible bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectTier {
    /// No Irreversible obligation: only reversible or non-Irreversible effects.
    /// A node at this tier does NOT require a dominating determinacy gate.
    BelowIrreversible,
    /// Carries the Irreversible obligation: at or above the Irreversible tier.
    /// A node at this tier MUST be control-flow dominated by a determinacy
    /// discharge gate on every path to it, or verification fails.
    IrreversibleOrAbove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafFacts {
    pub identity: Hash256,
    pub effect_class: EffectClass,
    pub deterministic: bool,
    pub idempotent: bool,
}

// ─────────────────────────── CompositeBody (0x05) ────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeBody {
    pub identity: Hash256,
    /// A Core IR graph identity. Phase 1 stores it; fact derivation lands when
    /// the graph is walkable.
    pub body: Hash256,
}

// ─────────────────────────── Implementation (0x06) ───────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Implementation {
    pub identity: Hash256,
    /// Contract generation this implementation satisfies.
    pub satisfies: u64,
    /// Content-addressed native bridge (leaf), or for a composite equal to its body.
    pub artifact: Hash256,
}

// ─────────────────────────── Record (sum) ───────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Record {
    Type(TypeDef),
    FnGenesis(FnGenesis),
    ContractAppend(ContractAppend),
    LeafFacts(LeafFacts),
    CompositeBody(CompositeBody),
    Implementation(Implementation),
}
