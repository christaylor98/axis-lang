//! Registry core for Core IR 0.5 — binary-canonical, identity-keyed.
//!
//! This module is the Phase-1 sibling of the legacy 0.3 registry that lives in
//! `crate::registry`; it does not replace the legacy module yet. Lowerings,
//! validation, and the bridge migrate onto it in later phases.
//!
//! Layout:
//! * [`record`] — the six record kinds and their tag/enum vocabularies.
//! * [`codec`] — canonical binary encode/decode (Registry Format §§3–6).
//! * [`fold`] — the resolver (Format §7) and the UNKNOWN gate.
//! * [`text`] — the `.axreg` projection loader (Format §8).
//!
//! Identity invariants (locked):
//!
//! * A TYPE's identity **is** the content hash of its canonical `TypeDef`
//!   bytes (structural; identical shapes deduplicate).
//! * A FUNCTION's identity is a MINTED 256-bit token stored in `FnGenesis`,
//!   never derived from the record's content.
//! * Resolution keys on identity only; an unresolved identity is a HARD HALT
//!   (the UNKNOWN gate) — never a default, never a guess.

pub mod codec;
pub mod fold;
pub mod record;
pub mod text;

pub use codec::{
    decode_record, encode_record, record_content_hash, type_identity, RecordCodecError,
};
pub use fold::{combine_component_facts, Facts, ResolveError, ResolvedFn, Scope};
pub use record::{
    CompositeBody, ContractAppend, Disposition, EffectClass, EffectTier, FnGenesis, FnKind,
    Implementation, LeafFacts, PrimCode, Record, TypeDef, TypeShape,
};
pub use text::{lower, LoadError, Lowered};
