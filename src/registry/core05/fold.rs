//! The Scope / fold resolver (Registry Format §7, Registry §"Resolution and the
//! Active Registry").
//!
//! A `Scope` is a set of records indexed by identity. Resolution asks one
//! question: "does this scope hold the definition for this identity?" Yes →
//! resolve. No → the **UNKNOWN gate** — a hard halt, never a default, never a
//! guess. There is no name in the canonical form to resolve against; the
//! `names` table is **projection only** and is checked at scope-composition
//! time for uniqueness (a duplicate name is a hard error).

use std::collections::HashMap;

use crate::fabric::hash::Hash256;

use super::codec::{encode_fn_genesis, record_content_hash};
use super::record::{
    CompositeBody, ContractAppend, EffectClass, FnGenesis, FnKind, Implementation, LeafFacts,
    Record, TypeDef,
};

/// Declared signature of a higher-order function's callback parameter.
///
/// `Fn` is a single opaque primitive (`PrimCode::Fn`): every fn-ref pool entry
/// carries the same `def_hash` and differs only in its 32-byte payload, so an
/// `in (..., Fn)` clause on its own says nothing about what the referenced fn
/// must accept or return. Without that, the verifier's per-argument check
/// admits ANY fn-ref into ANY `Fn` slot — a callee of the wrong arity, the
/// wrong return type, or the wrong argument types passes verification and
/// panics inside generated code instead.
///
/// A `CallbackContract` supplies the missing half. It is authored per arg
/// position with the registry's `callback` directive:
///
/// ```text
/// fn map
///   in (ValueList, Fn)
///   out ValueList
///   callback 1 (Value) -> Value
/// end
/// ```
///
/// and read back by the verifier, which resolves the fn-ref's payload to its
/// `FnGenesis` and checks that signature against this contract.
///
/// Scope-only, exactly like the projection `names` table: a contract is not
/// one of the six canonical record kinds, so declaring one changes no
/// registry's canonical byte stream and no fn identity.
///
/// Distinct from [`Scope::hof_callback_params`], which records arg positions
/// carrying a callback *result* value for effect attribution. These describe
/// the *fn-ref* in the slot; those describe a value already computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackContract {
    /// 0-based position of the callback in the HOF's `in (...)` clause.
    pub position: u32,
    /// Type identities the HOF will apply the referenced fn to, in order.
    /// The referenced fn's own declared inputs must admit each of these.
    pub inputs: Vec<Hash256>,
    /// Type identity the HOF requires the referenced fn to produce.
    pub output: Hash256,
}

/// Hard halts from the fold. `Unknown(id)` is the UNKNOWN gate — distinct
/// from a soft "not found"; callers MUST NOT default on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// Identity not present in the active scope.
    Unknown(Hash256),
    /// Two records with the same identity disagree (e.g. two different
    /// `FnGenesis` records with the same minted identity).
    Conflict {
        identity: Hash256,
        what: &'static str,
    },
    /// The `ContractAppend` chain has a gap, a duplicate generation, or a
    /// `prev` mismatch.
    BrokenChain { identity: Hash256, reason: String },
    /// Two records under the same projection name in the active scope.
    DuplicateName(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Unknown(h) => write!(f, "UNKNOWN gate: {} not in active scope", h),
            ResolveError::Conflict { identity, what } => {
                write!(f, "conflicting {} for identity {}", what, identity)
            }
            ResolveError::BrokenChain { identity, reason } => {
                write!(f, "broken contract chain for {}: {}", identity, reason)
            }
            ResolveError::DuplicateName(n) => write!(f, "duplicate projection name '{}'", n),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Facts attached to a resolved function.
///
/// Composite facts are **derived** from the body graph (Registry Format §6:
/// "A composite's facts are derived from its body's components"). Phase 1
/// stores `CompositeBody` but cannot walk Core IR 0.5 graphs yet, so composite
/// resolution returns [`Facts::PendingDerivation`] — never a guessed default.
/// Conservative ceiling values may be returned **only** behind this explicit
/// flag (intent: `emit "placeholder/guessed composite fact values" forbidden`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Facts {
    Leaf(LeafFacts),
    PendingDerivation {
        body: Hash256,
    },
}

/// The resolved view of a function identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFn {
    pub genesis: FnGenesis,
    /// Highest generation present in the chain (0 = genesis only, no appends).
    pub contract_generation: u64,
    /// Union of all clauses across the chain (predicate-function identities),
    /// in chain order (genesis appends first, then each `ContractAppend`).
    pub contract_clauses: Vec<Hash256>,
    /// Highest oracle reference along the chain (or ZERO if none).
    pub oracle: Hash256,
    pub facts: Facts,
    pub implementations: Vec<Implementation>,
}

/// The active registry scope: a hash-resolution set of records, with a
/// projection-only name table.
#[derive(Debug, Default, Clone)]
pub struct Scope {
    pub types: HashMap<Hash256, TypeDef>,
    pub fns: HashMap<Hash256, FnGenesis>,
    pub contract_appends: HashMap<Hash256, Vec<ContractAppend>>,
    pub leaf_facts: HashMap<Hash256, LeafFacts>,
    pub composite_bodies: HashMap<Hash256, CompositeBody>,
    pub implementations: HashMap<Hash256, Vec<Implementation>>,
    /// Projection-only name → identity binding. Names within a single scope
    /// MUST be unique (Registry §"Resolution and the Active Registry" rule 3).
    pub names: HashMap<String, Hash256>,
    /// HOF callback parameter positions: fn_id → [arg_positions].
    ///
    /// Each listed arg position in a CCall to that fn carries a function-typed
    /// argument whose effect must be unioned into the HOF's derived effect at
    /// the call site. The HOF body itself cannot express dynamic dispatch
    /// (Tier 3 forbidden), so callback effects propagate via this annotation
    /// rather than through the body walk.
    pub hof_callback_params: HashMap<Hash256, Vec<u32>>,
    /// HOF callback signature contracts: fn_id → one [`CallbackContract`] per
    /// `Fn`-typed arg position, authored with the registry's `callback`
    /// directive.
    ///
    /// The verifier requires a contract for EVERY `Fn`-typed parameter of a
    /// called fn and fails closed without one: an `Fn` slot with no contract
    /// is uncheckable, and silently admitting whatever fn-ref is handed to it
    /// is the defect this table exists to prevent.
    pub hof_callback_contracts: HashMap<Hash256, Vec<CallbackContract>>,
}

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a record into the scope. Detects identity conflicts (e.g. two
    /// distinct `FnGenesis` records under the same minted identity).
    pub fn insert(&mut self, rec: Record) -> Result<(), ResolveError> {
        match rec {
            Record::Type(t) => {
                let id = super::codec::type_identity(&t);
                if let Some(existing) = self.types.get(&id) {
                    if existing != &t {
                        // Cannot happen if hashing is sound — two TypeDefs with
                        // the same identity are by construction equal canonical
                        // bytes — but check defensively.
                        return Err(ResolveError::Conflict {
                            identity: id,
                            what: "TypeDef",
                        });
                    }
                }
                self.types.insert(id, t);
                Ok(())
            }
            Record::FnGenesis(f) => {
                if let Some(existing) = self.fns.get(&f.identity) {
                    if existing != &f {
                        return Err(ResolveError::Conflict {
                            identity: f.identity,
                            what: "FnGenesis",
                        });
                    }
                }
                self.fns.insert(f.identity, f);
                Ok(())
            }
            Record::ContractAppend(c) => {
                self.contract_appends
                    .entry(c.identity)
                    .or_default()
                    .push(c);
                Ok(())
            }
            Record::LeafFacts(l) => {
                if let Some(existing) = self.leaf_facts.get(&l.identity) {
                    if existing != &l {
                        return Err(ResolveError::Conflict {
                            identity: l.identity,
                            what: "LeafFacts",
                        });
                    }
                }
                self.leaf_facts.insert(l.identity, l);
                Ok(())
            }
            Record::CompositeBody(c) => {
                if let Some(existing) = self.composite_bodies.get(&c.identity) {
                    if existing != &c {
                        return Err(ResolveError::Conflict {
                            identity: c.identity,
                            what: "CompositeBody",
                        });
                    }
                }
                self.composite_bodies.insert(c.identity, c);
                Ok(())
            }
            Record::Implementation(i) => {
                self.implementations
                    .entry(i.identity)
                    .or_default()
                    .push(i);
                Ok(())
            }
        }
    }

    /// Bind a projection-only name to an identity. Duplicate names in one scope
    /// are a hard error (Registry §"Resolution..." rule 3). The bound identity
    /// is NOT required to already be present — the binding is text-side, and
    /// the underlying record may arrive later in the same scope-composition
    /// step. Resolution by name through the scope still gates on presence.
    pub fn bind_name(&mut self, name: String, identity: Hash256) -> Result<(), ResolveError> {
        if let Some(existing) = self.names.get(&name) {
            if *existing != identity {
                return Err(ResolveError::DuplicateName(name));
            }
            return Ok(());
        }
        self.names.insert(name, identity);
        Ok(())
    }

    pub fn lookup_name(&self, name: &str) -> Option<Hash256> {
        self.names.get(name).copied()
    }

    // ─────────────────── resolution (UNKNOWN gate) ──────────────────────────

    pub fn resolve_type(&self, id: Hash256) -> Result<&TypeDef, ResolveError> {
        self.types.get(&id).ok_or(ResolveError::Unknown(id))
    }

    pub fn resolve_fn(&self, id: Hash256) -> Result<ResolvedFn, ResolveError> {
        let genesis = self
            .fns
            .get(&id)
            .ok_or(ResolveError::Unknown(id))?
            .clone();

        // Fold the contract chain.
        let (contract_generation, contract_clauses, oracle) = self.fold_contract(&genesis)?;

        let facts = match genesis.kind {
            FnKind::Leaf => match self.leaf_facts.get(&id) {
                Some(lf) => Facts::Leaf(lf.clone()),
                // Declared-but-missing leaf facts is an UNKNOWN gate (the leaf
                // is incompletely admitted).
                None => return Err(ResolveError::Unknown(id)),
            },
            FnKind::Composite => match self.composite_bodies.get(&id) {
                Some(cb) => Facts::PendingDerivation { body: cb.body },
                None => return Err(ResolveError::Unknown(id)),
            },
        };

        let implementations = self
            .implementations
            .get(&id)
            .cloned()
            .unwrap_or_default();

        Ok(ResolvedFn {
            genesis,
            contract_generation,
            contract_clauses,
            oracle,
            facts,
            implementations,
        })
    }

    /// Walk `ContractAppend` records for `genesis.identity` in generation order,
    /// verifying the `prev`-hash chain. Returns (highest_generation, union of
    /// clauses in chain order, last non-zero oracle).
    fn fold_contract(
        &self,
        genesis: &FnGenesis,
    ) -> Result<(u64, Vec<Hash256>, Hash256), ResolveError> {
        let id = genesis.identity;
        let mut appends = self
            .contract_appends
            .get(&id)
            .cloned()
            .unwrap_or_default();
        if appends.is_empty() {
            return Ok((0, Vec::new(), Hash256::ZERO));
        }
        appends.sort_by_key(|a| a.generation);

        // Verify chain: generations must be strictly 1, 2, 3, …
        // and `prev` must match the prior record's content hash (the FnGenesis
        // record itself for gen 1).
        let genesis_hash =
            crate::fabric::hash::content_hash(&encode_fn_genesis(genesis));
        let mut expected_prev = genesis_hash;
        let mut expected_gen: u64 = 1;
        let mut clauses: Vec<Hash256> = Vec::new();
        let mut oracle = Hash256::ZERO;

        for ap in &appends {
            if ap.generation != expected_gen {
                return Err(ResolveError::BrokenChain {
                    identity: id,
                    reason: format!(
                        "expected generation {}, found {}",
                        expected_gen, ap.generation
                    ),
                });
            }
            if ap.prev != expected_prev {
                return Err(ResolveError::BrokenChain {
                    identity: id,
                    reason: format!("prev hash mismatch at generation {}", ap.generation),
                });
            }
            clauses.extend_from_slice(&ap.added);
            if !ap.oracle.is_zero() {
                oracle = ap.oracle;
            }
            expected_prev = record_content_hash(&Record::ContractAppend(ap.clone()));
            expected_gen += 1;
        }

        Ok((expected_gen - 1, clauses, oracle))
    }
}

// ─────────────────────── conservative ceilings (advisory) ───────────────────
//
// Phase 1 does NOT derive composite facts (no Core IR walker yet) — see
// `Facts::PendingDerivation`. The conservative ceiling values below are
// documented here for the day a walker lands, NOT applied silently.
//
//   * effect_class:  max over components  (worst-case: FullIo)
//   * deterministic: conjunction          (conservative default: false)
//   * idempotent:    conservatively false unless provably derivable
//
// Exposed as a pure helper that takes the derived component facts as input;
// the registry does not invoke it until a graph walker can supply those inputs.

/// Combine a set of component leaf-fact summaries by the rules in Registry
/// §"Bridge Facts (Leaf Declarations)". The caller is responsible for
/// recursively expanding composites first; this helper does not walk.
pub fn combine_component_facts<'a>(
    components: impl IntoIterator<Item = (EffectClass, bool, bool)>,
) -> (EffectClass, bool, bool) {
    let mut effect = EffectClass::Pure;
    let mut deterministic = true;
    let idempotent = false; // conservative
    let mut any = false;
    for (e, d, _i) in components {
        any = true;
        if e.as_varint() > effect.as_varint() {
            effect = e;
        }
        deterministic &= d;
        // idempotency is not composed monotonically here — left conservative.
    }
    if !any {
        // empty composite: pure, deterministic, not-idempotent (conservative).
        return (EffectClass::Pure, true, false);
    }
    (effect, deterministic, idempotent)
}
