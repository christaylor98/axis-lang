//! Phase 4: Core IR 0.5 static Verifier.
//!
//! Admission check over a [`CoreBundle`] against an active core05 registry scope.
//! No execution, no side effects. Returns PASS with derived facts, or a precise
//! hard FAIL naming the failing node/argument and reason.
//!
//! ## Steps
//! 0. Structural gate — topological order + NodeRef ranges (reuses `verify_bundle_dag_05`).
//! 1. Resolve every `CCall` target; unresolved = UNKNOWN hard FAIL.
//! 2. Type-check every `CCall` site: arg `def_hash` MUST match callee `FnGenesis.inputs`;
//!    `CIf` cond MUST be Bool; then/else branches MUST share a type.
//! 3. Derive effect class = max over `CCall` target effect profiles (leaf: declared;
//!    composite: FAIL CLOSED — never assumed pure); apply the supplied consumer policy.
//!
//! Steps 4–5 (trust gate, ledger admission): deferred — no provenance/ledger model yet.
//! `VerifyPass` carries the derived facts for a future ledger layer.

use std::collections::{HashMap, HashSet};

use crate::fabric::hash::Hash256;
use crate::ir::core_ir_05::{CoreBundle, Node, NodeRef};
use crate::lowering::emit_05::{decode_param_slot, fn_type_id};
use crate::registry::core05::{
    codec::type_identity,
    fold::{Facts, Scope},
    record::{EffectClass, EffectTier, PrimCode, TypeDef, TypeShape},
};

// ─── RESULT TYPES ────────────────────────────────────────────────────────────

/// Per-node effect annotation produced during the verifier walk.
///
/// First-order boundary:
/// - `CCall` to a leaf → `Resolved(leaf effect class)`
/// - `CIf`             → `Resolved(max of then/else branch annotations)`
/// - `CLam`            → `Resolved(body annotation)`
/// - `CVar`            → `Resolved(Pure)` (pure variable reference)
/// - `CCall` to a composite → `PendingDerivation` — composite resolution
///   requires walking the body bundle and is deferred to Phase 3. The
///   existing verifier may still derive the bundle-level effect via
///   `derive_body_effect`; the per-node annotation records what first-order
///   analysis alone can determine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeEffect {
    Resolved(EffectClass),
    /// First-order analysis cannot determine this node's effect: it calls a
    /// composite whose body has not been walked. Phase 3 deferred.
    PendingDerivation,
}

/// Successful admission verdict.
///
/// `effect_class` is DERIVED from the graph's `CCall` targets; never declared.
/// `referenced_def_hashes` contains every type identity encountered during
/// verification — returned for a future ledger/trust layer to record.
/// `node_effects` is the per-node first-order effect annotation built in the
/// same walk (index i corresponds to `bundle.nodes[i]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyPass {
    pub effect_class: EffectClass,
    pub referenced_def_hashes: HashSet<Hash256>,
    /// Per-node first-order effect annotation.
    ///
    /// On a first-order program (no composite `CCall` targets) the union of all
    /// `Resolved` entries equals `effect_class`. Any `CCall` to a composite is
    /// annotated `PendingDerivation` regardless of whether the body store
    /// supplied an answer to the bundle-level derivation.
    pub node_effects: Vec<NodeEffect>,
    /// Every type the bundle's `result` may have (more than one only when the
    /// result is a param slot shared by several fns in the file), and whether it
    /// diverges. Read by [`declared_result_admits`].
    pub result_types: Vec<Hash256>,
    pub result_diverges: bool,
}

/// RETURN_TYPE_CHECKED_V1: does the fn's declared return type admit what its
/// body produces? Same `type_admits` rule as an argument, so a `Value` return
/// stays as permissive as a `Value` parameter. A diverging result (`fail`) has
/// no type and is admitted, as a diverging CIf arm is. A ZERO declared type is
/// "unresolvable at normalize time" and fails closed.
pub fn declared_result_admits(pass: &VerifyPass, declared: Hash256, scope: &Scope) -> bool {
    if pass.result_diverges {
        return true;
    }
    declared != Hash256::ZERO && pass.result_types.iter().any(|&got| type_admits(declared, got, scope))
}

/// Hard-fail verdict. Every variant names the failing node index (into
/// `bundle.nodes`) and the precise reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// Step 0: topological order or `NodeRef` range violated.
    StructuralViolation(String),
    /// Step 1: `CCall` target identity not in the active scope (UNKNOWN gate).
    UnknownTarget { node_index: usize, target: Hash256 },
    /// Step 2: argument count does not match callee `FnGenesis.inputs` arity.
    ArityMismatch { node_index: usize, expected: usize, got: usize },
    /// Step 2: argument `def_hash` does not match callee `FnGenesis.inputs[arg_index]`.
    TypeMismatch { node_index: usize, arg_index: usize, expected: Hash256, got: Hash256 },
    /// Step 2c: an `Fn`-typed parameter of the callee was not satisfied by a
    /// fn-ref matching its declared `callback` contract — or the parameter has
    /// no contract at all, which makes the slot uncheckable.
    ///
    /// Every `Fn` pool entry shares one `def_hash`, so the Step 2b type check
    /// cannot tell one fn-ref from another; without this step a callee of any
    /// arity, argument type, or return type is admitted and fails at run
    /// instead. Fail-closed on every uncertainty.
    CallbackContractViolated { node_index: usize, arg_index: usize, reason: String },
    /// Step 2: `CIf` condition is not Bool type.
    CIfCondNotBool { node_index: usize, got: Hash256 },
    /// Step 2: `CIf` then/else branches have different types.
    CIfBranchDivergence { node_index: usize, then_type: Hash256, else_type: Hash256 },
    /// Step 3: derived effect class carries an obligation not permitted by the
    /// consumer's policy ceiling (`derived ⊄ allowed`).
    EffectPolicyViolated { derived: EffectClass, allowed: EffectClass },
    /// Step 3: composite target's effect class cannot be derived — FAIL CLOSED.
    /// Never assumed pure; never a default.
    CompositeEffectUnderivable { node_index: usize, target: Hash256, reason: String },
    /// Step 4: Irreversible (or PendingDerivation, fail-closed) node is not
    /// dominated by a CDeterminate on every control-flow path from the entry.
    /// True dominance required: CDeterminate must be a required data ancestor
    /// of the Irreversible node on every execution path, not merely present
    /// somewhere in the bundle. Undeterminable domination is treated as
    /// undominated — fail-closed.
    IrreversibleNotGated { node_index: usize },
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::StructuralViolation(msg) =>
                write!(f, "structural violation: {}", msg),
            VerifyError::UnknownTarget { node_index, target } =>
                write!(f, "UNKNOWN gate: node[{}] target {} not in scope",
                    node_index, target.to_hex()),
            VerifyError::ArityMismatch { node_index, expected, got } =>
                write!(f, "node[{}] arity mismatch: expected {} args, got {}",
                    node_index, expected, got),
            VerifyError::TypeMismatch { node_index, arg_index, expected, got } =>
                write!(f, "node[{}] arg[{}]: expected type {}, got {}",
                    node_index, arg_index, expected.to_hex(), got.to_hex()),
            VerifyError::CallbackContractViolated { node_index, arg_index, reason } =>
                write!(f, "node[{}] arg[{}]: callback contract violated: {}",
                    node_index, arg_index, reason),
            VerifyError::CIfCondNotBool { node_index, got } =>
                write!(f, "node[{}] CIf condition not Bool (got {})",
                    node_index, got.to_hex()),
            VerifyError::CIfBranchDivergence { node_index, then_type, else_type } =>
                write!(f, "node[{}] CIf branch divergence: then={} else={}",
                    node_index, then_type.to_hex(), else_type.to_hex()),
            VerifyError::EffectPolicyViolated { derived, allowed } =>
                write!(f, "effect policy violated: derived {:?} exceeds allowed {:?}",
                    derived, allowed),
            VerifyError::CompositeEffectUnderivable { node_index, target, reason } =>
                write!(f, "node[{}] composite {} effect underivable: {}",
                    node_index, target.to_hex(), reason),
            VerifyError::IrreversibleNotGated { node_index } =>
                write!(f, "node[{}] Irreversible action not dominated by CDeterminate on every path",
                    node_index),
        }
    }
}

impl std::error::Error for VerifyError {}

// ─── PRIVATE HELPERS ─────────────────────────────────────────────────────────

fn bool_type_id() -> Hash256 {
    type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) })
}

fn ref_def_hash(nr: NodeRef, bundle: &CoreBundle, result_types: &[Hash256]) -> Hash256 {
    match nr {
        NodeRef::Pool(i) => bundle.constant_pool[i as usize].def_hash,
        NodeRef::Node(j) => result_types[j as usize],
    }
}

/// Retrieve the first-order effect annotation for a `NodeRef`.
/// Pool entries are constants and are always `Resolved(Pure)`.
fn noderef_node_effect(nr: NodeRef, node_effects: &[NodeEffect]) -> NodeEffect {
    match nr {
        NodeRef::Pool(_) => NodeEffect::Resolved(EffectClass::Pure),
        NodeRef::Node(j) => node_effects[j as usize].clone(),
    }
}

/// Obligation-union join for `NodeEffect`.
///
/// `PendingDerivation` absorbs: if either operand is unknown the result is
/// unknown. For two `Resolved` values the result is the obligation union
/// (`EffectClass::union`, bitwise OR over the obligation-set encoding):
///   `join(Resolved(Reads), Resolved(Writes))` = `Resolved(EffectClass(3))`
/// The result is `EffectClass(3)`, DISTINCT from `FullIo = EffectClass(8)`.
/// Both obligations are retained individually — not collapsed to a top.
fn join_node_effect(a: NodeEffect, b: NodeEffect) -> NodeEffect {
    match (a, b) {
        (NodeEffect::PendingDerivation, _) | (_, NodeEffect::PendingDerivation) => {
            NodeEffect::PendingDerivation
        }
        (NodeEffect::Resolved(ea), NodeEffect::Resolved(eb)) => {
            NodeEffect::Resolved(ea.union(eb))
        }
    }
}

/// Type admissibility for a `CCall` argument.
///
/// The default relation is strict identity (`got == expected`) — every
/// Primitive and Product input is checked exactly. The ONE relaxation is
/// SUM MEMBERSHIP: if `expected` resolves to a `Sum` type, `got` is admitted
/// when it is the payload type of one of that Sum's constructors. This is the
/// only non-equality check in the verifier. It exists to type genuinely
/// polymorphic bridge fns honestly: `io_println` is declared `in (Value)` where
/// `Value = sum [ ...primitive variants... ]`, and the membership check admits
/// any of those exact variants — and nothing else. It is NOT a wildcard: a Sum
/// admits precisely the variants it enumerates.
/// Does a callback whose declared parameter is `accepts` satisfy a contract
/// that applies `applied` to it?
///
/// The sound direction is contravariance: the callee's parameter must admit
/// what the HOF hands it (`type_admits(accepts, applied)`).
///
/// The second, deliberately weaker direction exists because the registry has
/// no type variables. `loop_while :: a -> (a -> Bool) -> (a -> a) -> Int -> a`
/// is genuinely parametric, and the projection can only erase `a` to the `Value`
/// sum — so a working `loop_while(text, gr_more, gr_apply_step, …)` declares
/// callbacks over `Value` while passing `gr_more(state: Text) -> Bool`. Under
/// contravariance alone every such call is rejected, and they are not wrong:
/// the caller's own arg fixes `a`, the registry just cannot say so.
///
/// `type_admits(applied, accepts)` holds only when `applied` is a sum type with
/// `accepts` as a single-field constructor payload, so this relaxation reaches
/// exactly the erased slots and nothing else: a contract naming a real type
/// (`Int`, `Text`, `IntList`, the `Dec`/`Float` arms of `bridge_to_dec`) stays
/// strictly contravariant.
///
/// The residual hole is narrow and known: a `Value`-erased slot still admits a
/// callee narrower than the list it will actually be applied to — `map(int_xs,
/// text_pred)` type-checks. Closing it needs a type variable in the contract
/// syntax, unified against the caller's own argument, which is a change to the
/// projection grammar rather than to this check.
fn callback_arg_admits(accepts: Hash256, applied: Hash256, scope: &Scope) -> bool {
    type_admits(accepts, applied, scope) || type_admits(applied, accepts, scope)
}

/// Render an identity for a human: `Int (16305a6b…)` when the scope binds a
/// projection name to it, bare hex when it does not.
///
/// Only ever called on an error path. The reverse scan over `names` is linear,
/// which is irrelevant next to the failure it is describing — and a callback
/// contract violation naming `Int` and `two_arg` instead of two bare 64-char
/// hashes is the difference between a diagnostic and a puzzle.
fn describe(id: Hash256, scope: &Scope) -> String {
    let hex = id.to_hex();
    match scope.names.iter().find(|(_, &v)| v == id) {
        Some((name, _)) => format!("{} ({}…)", name, &hex[..8]),
        None => hex,
    }
}

fn type_admits(expected: Hash256, got: Hash256, scope: &Scope) -> bool {
    if got == expected {
        return true;
    }
    if let Ok(td) = scope.resolve_type(expected) {
        if let TypeShape::Sum(ctors) = &td.shape {
            // Admit a bare `got` into the sum only when EXACTLY ONE constructor
            // carries it as its single payload: then the variant it becomes is
            // determined. With two or more (`Presented(Int) | RefuseStale(Int)`)
            // the bare value names no variant, and admitting it would let the
            // verifier pass a value whose meaning the program never chose.
            // `Value` is unaffected: each of its constructors carries a
            // different type.
            return ctors.iter().filter(|fields| fields.len() == 1 && fields[0] == got).count() == 1;
        }
    }
    false
}

/// Resolve the effective type of a NodeRef, substituting param-slot pool entries
/// with their declared type from `param_types[slot_index]`.
///
/// For `NodeRef::Pool(i)` where `pool[i]` is a param slot (`def_hash ==
/// param_type_id()`), returns `param_types[slot_index]` — which may be
/// `Hash256::ZERO` if the declared type was unresolvable at normalize time.
/// Callers treat ZERO as a hard-fail signal (not an auto-pass).
/// Returns the raw `param_type_id()` when `slot_index` is out of range for
/// `param_types` (e.g. when called with `&[]` from the backward-compat wrapper).
/// For all non-param-slot refs, returns the raw `def_hash` unchanged.
fn resolve_ref_type(
    nr: NodeRef,
    bundle: &CoreBundle,
    result_types: &[Hash256],
    param_types: &[Vec<Hash256>],
) -> Hash256 {
    let cands = resolve_ref_types(nr, bundle, result_types, param_types);
    cands.first().copied().unwrap_or(Hash256::ZERO)
}

/// All types a NodeRef may legitimately have.
///
/// For a non-param ref this is the single raw `def_hash`. For a param slot it
/// is every type declared at that slot index by any fn in the file — the file's
/// functions share one pool entry per slot index, so the slot alone does not
/// identify a type (see `collect_bundle_param_types`).
///
/// An out-of-range slot, or a slot whose only candidate was unresolvable, gives
/// the raw `param_type_id()` sentinel, which matches no real type and so fails
/// closed. Callers must not treat an empty or sentinel result as an auto-pass.
fn resolve_ref_types(
    nr: NodeRef,
    bundle: &CoreBundle,
    result_types: &[Hash256],
    param_types: &[Vec<Hash256>],
) -> Vec<Hash256> {
    // Core IR 0.6: a CEnd is its block's value, so it has every type its value may have (a parameter
    // slot keeps all its candidates through the block, as it did when it was a 0.5 CIf arm)
    if let NodeRef::Node(j) = nr {
        if let Node::CEnd { value } = &bundle.nodes[j as usize] {
            return resolve_ref_types(*value, bundle, result_types, param_types);
        }
    }
    let raw = ref_def_hash(nr, bundle, result_types);
    if let NodeRef::Pool(pool_idx) = nr {
        let entry = &bundle.constant_pool[pool_idx as usize];
        if let Some(slot_idx) = decode_param_slot(&entry.def_hash, &entry.payload) {
            return match param_types.get(slot_idx as usize) {
                Some(cands) if !cands.is_empty() => cands.clone(),
                _ => vec![raw],
            };
        }
    }
    vec![raw]
}

// ─── COMPOSITE EFFECT DERIVATION ─────────────────────────────────────────────

/// Walk a composite body bundle, deriving its effect class by union over all
/// CCall targets. Recursively expands nested composites. Cycle detection via
/// `visiting` (the set of body hashes currently on the call stack). Returns
/// `CompositeEffectUnderivable` on cycle or missing body.
fn derive_body_effect(
    body_hash: Hash256,
    scope: &Scope,
    body_store: &HashMap<Hash256, CoreBundle>,
    visiting: &mut HashSet<Hash256>,
) -> Result<EffectClass, VerifyError> {
    if visiting.contains(&body_hash) {
        return Err(VerifyError::CompositeEffectUnderivable {
            node_index: 0,
            target: body_hash,
            reason: "cycle detected in composite body graph".to_string(),
        });
    }
    let bundle = match body_store.get(&body_hash) {
        Some(b) => b,
        None => {
            return Err(VerifyError::CompositeEffectUnderivable {
                node_index: 0,
                target: body_hash,
                reason: format!("body bundle {} not in store", body_hash.to_hex()),
            });
        }
    };
    visiting.insert(body_hash);
    let mut acc_effect = EffectClass::Pure;
    for node in &bundle.nodes {
        if let Node::CCall { target_identity, .. } = node {
            let resolved = scope.resolve_fn(*target_identity).map_err(|_| {
                VerifyError::CompositeEffectUnderivable {
                    node_index: 0,
                    target: *target_identity,
                    reason: format!(
                        "unresolved CCall {} in composite body",
                        target_identity.to_hex()
                    ),
                }
            })?;
            let eff = match &resolved.facts {
                Facts::Leaf(lf) => lf.effect_class,
                Facts::PendingDerivation { body: sub_body } => {
                    derive_body_effect(*sub_body, scope, body_store, visiting)?
                }
            };
            acc_effect = acc_effect.union(eff);
        }
    }
    visiting.remove(&body_hash);
    Ok(acc_effect)
}

// ─── VERIFIER ────────────────────────────────────────────────────────────────

/// Verify a [`CoreBundle`] against an active registry [`Scope`].
///
/// Backward-compatible entry point: calls [`verify_bundle_with_params`] with an
/// empty `param_types` slice. Any param-slot pool entry encountered will resolve
/// to its raw `param_type_id()` sentinel, which fails all type checks as before.
/// Use [`verify_bundle_with_params`] directly when verifying user-fn bundles
/// whose param slots must be checked against their declared types.
pub fn verify_bundle(
    bundle: &CoreBundle,
    scope: &Scope,
    body_store: &HashMap<Hash256, CoreBundle>,
    max_allowed_effect: EffectClass,
) -> Result<VerifyPass, VerifyError> {
    verify_bundle_with_params(bundle, scope, body_store, max_allowed_effect, &[])
}

/// Verify a [`CoreBundle`] against an active registry [`Scope`], resolving
/// param-slot pool entries to their declared types from `param_types`.
///
/// `param_types[i]` is the declared type hash for param slot `i` (slot_index
/// from `intern_param_slot`). When a `CIf` condition or `CCall` argument is a
/// param slot, its effective type is resolved from `param_types[slot_index]`
/// rather than reading the raw `param_type_id()` sentinel — enabling the Bool
/// check and arg type checks to succeed for correctly-typed user fn bodies.
///
/// CHECK_NOT_SKIP invariant: a non-Bool param slot condition still fails
/// `CIfCondNotBool`; an out-of-range slot_index (slot ≥ param_types.len()) or
/// a ZERO declared type (unresolvable at normalize time) also fails.
///
/// `max_allowed_effect` is the consumer's effect policy ceiling.
/// Returns `Ok(VerifyPass)` with derived facts, or `Err(VerifyError)`.
pub fn verify_bundle_with_params(
    bundle: &CoreBundle,
    scope: &Scope,
    body_store: &HashMap<Hash256, CoreBundle>,
    max_allowed_effect: EffectClass,
    param_types: &[Vec<Hash256>],
) -> Result<VerifyPass, VerifyError> {
    // Step 0: structural gate — reuses the Phase-3c flat-table verifier.
    crate::introspection::trust::verify_bundle_dag_05(bundle)
        .map_err(VerifyError::StructuralViolation)?;

    let bool_id = bool_type_id();
    // result_types[i] = the def_hash of the value produced by nodes[i].
    // Built up in index order; safe to index into since NodeRef::Node(j) requires j < i.
    let mut result_types: Vec<Hash256> = Vec::with_capacity(bundle.nodes.len());
    let mut effect = EffectClass::Pure;
    let mut referenced: HashSet<Hash256> = HashSet::new();
    // Per-node first-order effect annotation (Phase 2).
    let mut node_effects: Vec<NodeEffect> = Vec::with_capacity(bundle.nodes.len());
    // diverges[i] = true iff nodes[i] never produces a value — it calls `fail`,
    // or it is a CIf both of whose arms diverge.
    //
    // `fail` has no result type: the registry type language is monomorphic with
    // no bottom type, so its `out` is a placeholder (`Value`) and the real rule
    // is here. A diverging arm is exempt from branch-type agreement below and
    // the sibling arm's type becomes the CIf's type, which is what lets the
    // shape input validation actually wants type-check:
    //
    //   if int_lt(n, Int(0)) { fail(Text("negative")) } else { n }
    //
    // Without this it is a CIfBranchDivergence between the placeholder and Int.
    //
    // Keyed on the identity the scope resolves for the name `fail`, exactly as
    // the `seq` pass-through below is, so a registry that declares no `fail`
    // cannot have an unrelated call adopt the rule.
    let fail_identity = scope.lookup_name("fail");
    let mut diverges: Vec<bool> = Vec::with_capacity(bundle.nodes.len());
    // Whether a ref points at a diverging node. A pool constant is a value, so
    // it never diverges.
    let ref_diverges = |r: NodeRef, d: &Vec<bool>| -> bool {
        match r {
            NodeRef::Node(j) => d.get(j as usize).copied().unwrap_or(false),
            NodeRef::Pool(_) => false,
        }
    };
    // Per-node gate-domination bit (Phase 5a).
    //
    // gate_dominated[i] = true iff there exists a CDeterminate node C that is
    // guaranteed to fire on every execution path to node i. Computed bottom-up
    // in topo order using a data-flow / CIf-branch-aware rule:
    //
    //   CDeterminate itself: true (dominates itself).
    //   Non-CIf node with node-deps d1..dk: true iff any di has gate_dominated=true
    //     OR di is itself CDeterminate. (All deps are required, so any gated dep
    //     guarantees a gate fired before this node.)
    //   CIf { cond, then_, else_ }: cond is always required (its gate status propagates
    //     directly); the branches are alternative, so only gates present on BOTH
    //     branches are guaranteed. gate_dominated(K) = gd(cond) OR (gd(then_) AND gd(else_)).
    //   CVar, pool constants: false (no node-deps, not CDeterminate).
    let mut gate_dominated: Vec<bool> = Vec::with_capacity(bundle.nodes.len());

    // Seed referenced with all constant pool type identities.
    for entry in &bundle.constant_pool {
        referenced.insert(entry.def_hash);
    }

    // Helper: gate_dominated status of a NodeRef.
    // Pool constants are never CDeterminate, so always false.
    let gd = |nr: NodeRef, gd_vec: &[bool]| -> bool {
        match nr {
            NodeRef::Node(j) => gd_vec[j as usize],
            NodeRef::Pool(_) => false,
        }
    };

    for (i, node) in bundle.nodes.iter().enumerate() {
        match node {
            // ── CDeterminate ─────────────────────────────────────────────────
            // Phase 5a determinacy discharge gate. Pure marker, no operands.
            // Produces Unit type (a discharge token for type-checking purposes).
            // Gate-dominated: true (the gate dominates itself).
            Node::CDeterminate => {
                result_types.push(crate::lowering::emit_05::unit_type_id());
                node_effects.push(NodeEffect::Resolved(EffectClass::Pure));
                gate_dominated.push(true);
                diverges.push(false);
            }

            // ── CCall ────────────────────────────────────────────────────────
            Node::CCall { target_identity, args, .. } => {
                // Compute gate-domination for this node before other checks.
                // A CCall is gate-dominated iff any of its node-deps is gate-dominated
                // (i.e., CDeterminate is a guaranteed ancestor via any required dep).
                let is_gated = args.iter().any(|&nr| gd(nr, &gate_dominated));

                // Step 1: UNKNOWN gate. Any resolution failure is a hard FAIL.
                let resolved = scope
                    .resolve_fn(*target_identity)
                    .map_err(|_| VerifyError::UnknownTarget {
                        node_index: i,
                        target: *target_identity,
                    })?;

                let inputs = &resolved.genesis.inputs;

                // Step 2a: arity.
                // `in ()` alone means zero-argument. `variadic true` relaxes the
                // check, and what it relaxes is read off the `in` clause, so
                // every shape the registry can write is checked as far as it
                // can be:
                //
                //   `in ()`            any count, arguments unchecked
                //                      (value_make, list_make)
                //   `in (T)`           any count, EVERY argument must be a `T`
                //                      (handle_list_make: `in (Handle)`)
                //   `in (T1, .., Tn)`  exactly n arguments, types unchecked
                //                      (seq: its real contract is polymorphic
                //                      in both positions, which `in` cannot say)
                //
                // Before this, `variadic` switched off arity AND type checks
                // wholesale, so `handle_list_make(Int(3))` verified clean and
                // panicked inside `join` at run time, and `seq(a, b, c)` got as
                // far as rustc.
                let variadic = resolved.genesis.variadic;
                let homogeneous = variadic && inputs.len() == 1;
                let fixed_arity = !variadic || inputs.len() >= 2;

                if fixed_arity && args.len() != inputs.len() {
                    return Err(VerifyError::ArityMismatch {
                        node_index: i,
                        expected: inputs.len(),
                        got: args.len(),
                    });
                }

                // Step 2b: per-argument def_hash type-check.
                // Param-slot args are resolved to their declared type via param_types
                // before the check; the raw pool hash is kept for error reporting.
                // A homogeneous variadic checks every argument against its one
                // declared element type.
                if !variadic || homogeneous {
                    for (pos, &arg_ref) in args.iter().enumerate() {
                        let got_raw = ref_def_hash(arg_ref, bundle, &result_types);
                        // A param slot may carry several declared types (one per
                        // fn in the file sharing this slot index) — admit if any
                        // of them satisfies the contract. A non-param ref yields
                        // exactly one candidate, so this is the old check.
                        let cands = resolve_ref_types(arg_ref, bundle, &result_types, param_types);
                        let expected = if homogeneous { inputs[0] } else { inputs[pos] };
                        referenced.insert(got_raw);
                        referenced.insert(expected);
                        if !cands.iter().any(|&got| type_admits(expected, got, scope)) {
                            return Err(VerifyError::TypeMismatch {
                                node_index: i,
                                arg_index: pos,
                                expected,
                                got: got_raw,
                            });
                        }
                    }
                }

                // Step 2c: HOF callback contracts.
                //
                // `Fn` is one opaque primitive, so every fn-ref pool entry
                // carries the same def_hash and Step 2b admits ANY fn-ref into
                // ANY `Fn` slot. This step resolves the fn-ref's 32-byte
                // payload back to the callee's own FnGenesis and checks that
                // signature against the contract the registry declares for the
                // slot, closing the wrong-arity / wrong-argument /
                // wrong-return holes that otherwise surface as a panic inside
                // generated code.
                //
                // Fail-closed throughout: a slot with no contract, an arg that
                // is not a fn-ref constant, an unresolvable target, and any
                // signature divergence are all hard failures.
                //
                // Also the one place a callback's effect can be attributed to
                // its call site: the HOF applies the fn-ref, so the callee's
                // effect is part of what calling the HOF does. `callback_effect`
                // is folded into the call's effect below, which puts it under
                // the same policy ceiling and gate-domination check as any
                // directly-called effect.
                let mut callback_effect = EffectClass::Pure;
                if !variadic {
                    let fn_ty = fn_type_id();
                    let contracts = scope.hof_callback_contracts.get(target_identity);
                    for (pos, &declared) in inputs.iter().enumerate() {
                        if declared != fn_ty {
                            continue;
                        }
                        let fail = |reason: String| VerifyError::CallbackContractViolated {
                            node_index: i,
                            arg_index: pos,
                            reason,
                        };
                        let contract = contracts
                            .and_then(|cs| cs.iter().find(|c| c.position as usize == pos));
                        let contract = match contract {
                            Some(c) => c,
                            None => {
                                return Err(fail(
                                    "parameter is `Fn`-typed but the registry declares no \
                                     `callback` contract for it, so no fn-ref placed here can \
                                     be checked (add `callback <pos> (<in>) -> <out>` to the \
                                     callee's registry block)"
                                        .to_string(),
                                ))
                            }
                        };

                        // The arg must be a fn-ref pool constant. A computed
                        // node cannot be one: `Fn` values are interned
                        // constants, never produced by a call.
                        let callee_id = match args.get(pos).copied() {
                            Some(NodeRef::Pool(p)) => {
                                let entry = &bundle.constant_pool[p as usize];
                                if entry.def_hash != fn_ty {
                                    return Err(fail(format!(
                                        "expected a fn-ref constant, got a pool entry of type {}",
                                        describe(entry.def_hash, scope)
                                    )));
                                }
                                if entry.payload.len() != 32 {
                                    return Err(fail(format!(
                                        "malformed fn-ref: payload is {} bytes, expected a \
                                         32-byte identity",
                                        entry.payload.len()
                                    )));
                                }
                                let mut h = [0u8; 32];
                                h.copy_from_slice(&entry.payload);
                                Hash256::from_bytes(h)
                            }
                            Some(NodeRef::Node(j)) => {
                                return Err(fail(format!(
                                    "expected a fn-ref constant, got the result of node[{}]",
                                    j
                                )))
                            }
                            None => {
                                return Err(fail(
                                    "no argument supplied for this parameter".to_string(),
                                ))
                            }
                        };

                        let callee = scope.resolve_fn(callee_id).map_err(|_| {
                            fail(format!(
                                "fn-ref target {} is not in the active scope",
                                describe(callee_id, scope)
                            ))
                        })?;

                        // Arity, then arguments, then return.
                        //
                        // Arguments are CONTRAVARIANT: the HOF applies the
                        // callee to values of `contract.inputs[k]`, so it is
                        // the callee's own declared parameter that must admit
                        // them. The return is covariant, the ordinary
                        // direction. Both reuse `type_admits`, so a `Value`
                        // slot stays exactly as permissive here as it is for a
                        // direct call — no second subtyping rule.
                        if callee.genesis.variadic {
                            return Err(fail(format!(
                                "fn-ref target {} is variadic; a callback slot requires a \
                                 fixed, checkable signature",
                                describe(callee_id, scope)
                            )));
                        }
                        if callee.genesis.inputs.len() != contract.inputs.len() {
                            return Err(fail(format!(
                                "callback arity: contract requires a {}-argument fn, target {} \
                                 takes {}",
                                contract.inputs.len(),
                                describe(callee_id, scope),
                                callee.genesis.inputs.len()
                            )));
                        }
                        for (k, (&applied, &accepts)) in contract
                            .inputs
                            .iter()
                            .zip(callee.genesis.inputs.iter())
                            .enumerate()
                        {
                            referenced.insert(applied);
                            referenced.insert(accepts);
                            if !callback_arg_admits(accepts, applied, scope) {
                                return Err(fail(format!(
                                    "callback arg[{}]: contract applies {}, target {} accepts {}",
                                    k,
                                    describe(applied, scope),
                                    describe(callee_id, scope),
                                    describe(accepts, scope)
                                )));
                            }
                        }
                        referenced.insert(contract.output);
                        referenced.insert(callee.genesis.output);
                        if !type_admits(contract.output, callee.genesis.output, scope) {
                            return Err(fail(format!(
                                "callback return: contract requires {}, target {} returns {}",
                                describe(contract.output, scope),
                                describe(callee_id, scope),
                                describe(callee.genesis.output, scope)
                            )));
                        }

                        // Attribute the callback's effect to this call site.
                        // Composites derive from their body, fail-closed, on
                        // the same terms as a direct composite call.
                        let cb_eff = match &callee.facts {
                            Facts::Leaf(lf) => lf.effect_class,
                            Facts::PendingDerivation { body } => {
                                let mut visiting = HashSet::new();
                                derive_body_effect(*body, scope, body_store, &mut visiting)
                                    .map_err(|e| match e {
                                        VerifyError::CompositeEffectUnderivable {
                                            reason, ..
                                        } => VerifyError::CompositeEffectUnderivable {
                                            node_index: i,
                                            target: callee_id,
                                            reason,
                                        },
                                        other => other,
                                    })?
                            }
                        };
                        callback_effect = callback_effect.union(cb_eff);
                    }
                }

                // Step 3: effect derivation. For composites, derive from the
                // body bundle via recursive walk. FAIL CLOSED if body not in
                // bundle_store or a cycle is detected — never assume pure.
                //
                // Per-node annotation: leaf → Resolved(effect); composite →
                // PendingDerivation regardless of body-store availability.
                // The annotation records what first-order analysis alone sees;
                // the bundle-level accumulator uses the full derivation.
                let (target_effect, this_node_effect) = match &resolved.facts {
                    Facts::Leaf(lf) => (lf.effect_class, NodeEffect::Resolved(lf.effect_class)),
                    Facts::PendingDerivation { body } => {
                        let mut visiting = HashSet::new();
                        let base_eff =
                            derive_body_effect(*body, scope, body_store, &mut visiting).map_err(
                                |e| match e {
                                    VerifyError::CompositeEffectUnderivable { reason, .. } => {
                                        VerifyError::CompositeEffectUnderivable {
                                            node_index: i,
                                            target: *target_identity,
                                            reason,
                                        }
                                    }
                                    other => other,
                                },
                            )?;
                        // Propagate callback effects: HOFs register which arg
                        // positions carry a callback function. Each such arg's
                        // derived effect is unioned into the HOF's effect at
                        // this call site (fail-closed: pending callback → Err).
                        let cb_eff =
                            if let Some(positions) = scope.hof_callback_params.get(target_identity)
                            {
                                let mut acc = EffectClass::Pure;
                                for &pos in positions {
                                    if let Some(&nr) = args.get(pos as usize) {
                                        match noderef_node_effect(nr, &node_effects) {
                                            NodeEffect::Resolved(e) => acc = acc.union(e),
                                            NodeEffect::PendingDerivation => {
                                                return Err(
                                                    VerifyError::CompositeEffectUnderivable {
                                                        node_index: i,
                                                        target: *target_identity,
                                                        reason: format!(
                                                            "callback arg {} effect pending",
                                                            pos
                                                        ),
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                                acc
                            } else {
                                EffectClass::Pure
                            };
                        (base_eff.union(cb_eff), NodeEffect::PendingDerivation)
                    }
                };
                // A callback's effect is part of what calling the HOF does, so
                // it joins the call's own effect BEFORE the policy ceiling and
                // the gate-domination check below — an Irreversible callee
                // handed to a HOF must be gated exactly as a direct
                // Irreversible call is.
                let target_effect = target_effect.union(callback_effect);
                let this_node_effect = match this_node_effect {
                    NodeEffect::Resolved(e) => NodeEffect::Resolved(e.union(callback_effect)),
                    NodeEffect::PendingDerivation => NodeEffect::PendingDerivation,
                };
                effect = effect.union(target_effect);

                // Step 4: gate-domination check.
                // An Irreversible CCall (or PendingDerivation that derives to
                // Irreversible, fail-closed) must be dominated by a CDeterminate
                // on every control-flow path. "Dominated" means CDeterminate is
                // a required data ancestor of this node — not merely present in
                // the bundle. Undeterminable domination = ungated = fail.
                if target_effect.tier() == EffectTier::IrreversibleOrAbove && !is_gated {
                    return Err(VerifyError::IrreversibleNotGated { node_index: i });
                }

                // Step 5: result type.
                //
                // `seq` is the one callable whose result type is not its
                // declared `out`. It is compiler-injected, never written by an
                // author: `seq_scope_arm_effects` (nf_lowering.rs) wraps an
                // `if` arm that discards an effectful binding as
                // `seq(effect, arm_result)`, so the effect becomes a data
                // dependency of the arm and the emitter keeps it inside that
                // arm (BRANCH_SCOPING_V1). The runtime is the identity on its
                // second argument — `pub fn seq(_eff, result) -> result`
                // (axis-codegen-bridge-rs `runtime/arith.rs`) — so the arm's
                // type is the type of `args[1]`, unchanged by the wrap.
                //
                // The registry type language cannot express that. It is
                // monomorphic: `out` is one hash, so any declared `out` would
                // pin every seq-wrapped arm to that one type and make it
                // diverge from the sibling arm (`CIfBranchDivergence`) for
                // every result type but one. There is no type variable to
                // declare, and no Core IR sequencing node to use instead —
                // `Node` has exactly CCall, CIf, CDeterminate, so adding one
                // would move every bundle identity.
                //
                // Hence this pass-through, keyed on the identity the lowering
                // itself resolves (`scope.lookup_name("seq")`) rather than on
                // the node's `target_name` string, so a registry that declares
                // no `seq` cannot have an unrelated call silently adopt the
                // rule. `seq` is declared `variadic true` with a two-entry
                // `in`, which keeps the arity check above (exactly 2) and skips
                // only the per-argument type check — meaningless for a callable
                // polymorphic in both positions.
                let seq_identity = scope.lookup_name("seq");
                let is_seq = seq_identity == Some(*target_identity);
                let output = if is_seq && args.len() == 2 {
                    resolve_ref_type(args[1], bundle, &result_types, param_types)
                } else {
                    resolved.genesis.output
                };
                referenced.insert(output);
                result_types.push(output);
                node_effects.push(this_node_effect);
                gate_dominated.push(is_gated);
                diverges.push(fail_identity == Some(*target_identity));
            }

            // ── CBlock / CEnd (Core IR 0.6) ──────────────────────────────────
            // A CBlock opens a block and has no value of its own (Unit). A CEnd is its block's value: it
            // has the value's type, effect, gate status and divergence, so a CIf over two arm CEnds checks
            // exactly what it checked over the two values in 0.5.
            Node::CBlock => {
                result_types.push(crate::lowering::emit_05::unit_type_id());
                node_effects.push(NodeEffect::Resolved(EffectClass::Pure));
                gate_dominated.push(false);
                diverges.push(false);
            }
            Node::CEnd { value } => {
                let t = resolve_ref_type(*value, bundle, &result_types, param_types);
                referenced.insert(t);
                result_types.push(t);
                node_effects.push(noderef_node_effect(*value, &node_effects));
                gate_dominated.push(gd(*value, &gate_dominated));
                diverges.push(ref_diverges(*value, &diverges));
            }

            // ── CIf ──────────────────────────────────────────────────────────
            Node::CIf { cond, then_, else_ } => {
                // Gate-domination for CIf: cond is always required (its gate status
                // propagates directly); then_ and else_ are alternative branches —
                // only gates guaranteed on BOTH branches are guaranteed after the CIf.
                let is_gated = gd(*cond, &gate_dominated)
                    || (gd(*then_, &gate_dominated) && gd(*else_, &gate_dominated));

                // Step 2: condition must be Bool.
                // For param-slot conditions, resolve the declared type from param_types
                // before checking; raw pool hash is kept for error reporting.
                let cond_raw = ref_def_hash(*cond, bundle, &result_types);
                let cond_cands = resolve_ref_types(*cond, bundle, &result_types, param_types);
                referenced.insert(cond_raw);
                if !cond_cands.contains(&bool_id) {
                    return Err(VerifyError::CIfCondNotBool { node_index: i, got: cond_raw });
                }

                // Step 2: branches must agree on type.
                //
                // Param slots are resolved here, exactly as the condition above
                // already was. Previously these two lines read the RAW pool hash,
                // so a bare parameter reference in one arm carried the Param
                // sentinel while the other arm carried a real type, and the two
                // could never be equal:
                //
                //   fn absval(n: Int) -> Int {
                //     if int_lt(n, Int(0)) { int_sub(Int(0), n) } else { n }
                //   }
                //   CIfBranchDivergence { then_type: 16305a6b…(Int),
                //                         else_type: f9535076…(Param) }
                //
                // That rejected correct programs and forced authors to wrap a
                // bare parameter in a no-op call to disguise it. Agreement is
                // now "the two arms share at least one candidate type", which
                // for non-param refs is plain hash equality as before.
                let then_raw = ref_def_hash(*then_, bundle, &result_types);
                let else_raw = ref_def_hash(*else_, bundle, &result_types);
                let then_cands = resolve_ref_types(*then_, bundle, &result_types, param_types);
                let else_cands = resolve_ref_types(*else_, bundle, &result_types, param_types);
                referenced.insert(then_raw);
                referenced.insert(else_raw);
                // A DIVERGING ARM AGREES WITH ANYTHING.
                //
                // An arm that calls `fail` never produces a value, so it has no
                // type to agree about and the `CIf`'s type is whatever the
                // other arm yields. `fail`'s declared `out` is a placeholder —
                // the registry type language is monomorphic and has no bottom
                // type — so without this the shape input validation wants,
                //
                //   if int_lt(n, Int(0)) { fail(Text("negative")) } else { n }
                //
                // is rejected as a divergence between that placeholder and Int.
                // This is the reason `fail` can be called at all.
                //
                // Both arms diverging is fine and makes the `CIf` itself
                // diverging: control reaches no continuation either way, so
                // there is still nothing to disagree about, and the node is
                // marked so an enclosing `CIf` treats it the same way.
                let then_div = ref_diverges(*then_, &diverges);
                let else_div = ref_diverges(*else_, &diverges);
                let then_hash = if then_div && else_div {
                    then_raw
                } else if then_div {
                    if else_cands.len() == 1 { else_cands[0] } else { else_raw }
                } else if else_div {
                    if then_cands.len() == 1 { then_cands[0] } else { then_raw }
                } else {
                    let common: Vec<Hash256> = then_cands
                        .iter()
                        .filter(|t| else_cands.contains(t))
                        .copied()
                        .collect();
                    if common.is_empty() {
                        return Err(VerifyError::CIfBranchDivergence {
                            node_index: i,
                            then_type: then_raw,
                            else_type: else_raw,
                        });
                    }
                    // The CIf's own result type: the agreed type when
                    // unambiguous, else the raw then-hash (prior behaviour).
                    if common.len() == 1 { common[0] } else { then_raw }
                };

                // CIf carries no intrinsic effect; effect comes from CCall nodes.
                // Per-node annotation: max (join) of the branch annotations.
                let this_node_effect = join_node_effect(
                    noderef_node_effect(*then_, &node_effects),
                    noderef_node_effect(*else_, &node_effects),
                );
                diverges.push(then_div && else_div);
                result_types.push(then_hash);
                node_effects.push(this_node_effect);
                gate_dominated.push(is_gated);
            }

        }
    }

    // Step 3: apply consumer effect policy (bitset containment).
    // The bundle passes iff every obligation it carries is permitted by the ceiling.
    if !effect.is_contained_by(max_allowed_effect) {
        return Err(VerifyError::EffectPolicyViolated {
            derived: effect,
            allowed: max_allowed_effect,
        });
    }

    let result_types = resolve_ref_types(bundle.result, bundle, &result_types, param_types);
    let result_diverges = ref_diverges(bundle.result, &diverges);
    Ok(VerifyPass {
        effect_class: effect,
        referenced_def_hashes: referenced,
        node_effects,
        result_types,
        result_diverges,
    })
}

// ─── TESTS ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::fabric::hash::Hash256;
    use crate::ir::core_ir_05::{ConstantPoolEntry, CoreBundle, Node, NodeRef};
    use crate::lowering::emit_05::unit_type_id;
    use crate::registry::core05::{
        fold::{CallbackContract, Scope},
        record::{
            CompositeBody, Disposition, EffectClass, FnGenesis, FnKind, LeafFacts,
            Record,
        },
    };

    const PILOT_AXREG: &str = include_str!("../../registries/pilot_lowering.axreg");

    fn pilot_scope() -> Scope {
        crate::registry::core05::text::lower(PILOT_AXREG)
            .expect("pilot registry must load")
            .scope
    }

    // Pilot registry identities (from pilot_lowering.axreg).
    fn const_unit_id() -> Hash256 {
        Hash256::from_bytes([0x11; 32])
    }
    fn seq_units_id() -> Hash256 {
        Hash256::from_bytes([0x22; 32])
    }

    fn unit_entry() -> ConstantPoolEntry {
        ConstantPoolEntry { def_hash: unit_type_id(), payload: vec![] }
    }

    fn bool_entry() -> ConstantPoolEntry {
        ConstantPoolEntry { def_hash: bool_type_id(), payload: vec![0x00] }
    }

    // Build a scope with a single FullIo leaf function:
    //   writes_fn: (Unit) -> Unit
    fn writes_fn_scope() -> (Scope, Hash256) {
        let unit_id = unit_type_id();
        let fn_id = Hash256::from_bytes([0xCC; 32]);
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: fn_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: fn_id,
            effect_class: EffectClass::FullIo,
            deterministic: false,
            idempotent: false,
        })).unwrap();
        (scope, fn_id)
    }

    // Build a scope with a composite function (PendingDerivation):
    //   comp_fn: (Unit) -> Unit
    fn composite_scope() -> (Scope, Hash256) {
        let unit_id = unit_type_id();
        let fn_id = Hash256::from_bytes([0xDD; 32]);
        let body_hash = Hash256::from_bytes([0xEE; 32]);
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: fn_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Composite,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::CompositeBody(CompositeBody {
            identity: fn_id,
            body: body_hash,
        })).unwrap();
        (scope, fn_id)
    }

    // ── PASS cases ───────────────────────────────────────────────────────────

    #[test]
    fn pass_empty_bundle() {
        let scope = pilot_scope();
        // A truly empty bundle (no nodes, no pool) has no valid `result` ref
        // at all — never reachable from real lowering (every fn returns
        // something). Minimal non-empty stand-in: a bare Unit pool entry.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![],
            result: NodeRef::Pool(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Pure).unwrap();
        assert_eq!(pass.effect_class, EffectClass::Pure);
    }

    #[test]
    fn pass_single_ccall_pure() {
        let scope = pilot_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "const_unit".to_string(),
                target_identity: const_unit_id(),
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap();
        assert_eq!(pass.effect_class, EffectClass::Pure);
        assert!(pass.referenced_def_hashes.contains(&unit_type_id()));
    }

    #[test]
    fn pass_chained_ccalls() {
        let scope = pilot_scope();
        // node[0] = const_unit(Pool(0)) → Unit
        // node[1] = seq_units(Node(0), Pool(0)) → Unit
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CCall { target_name: "const_unit".to_string(), target_identity: const_unit_id(), args: vec![NodeRef::Pool(0)] },
                Node::CCall {
                    target_name: "seq_units".to_string(),
                    target_identity: seq_units_id(),
                    args: vec![NodeRef::Node(0), NodeRef::Pool(0)],
                },
            ],
            result: NodeRef::Node(1),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap();
        assert_eq!(pass.effect_class, EffectClass::Pure);
    }

    #[test]
    fn pass_cif_with_bool_cond() {
        let scope = pilot_scope();
        // Pool(0) = Bool, Pool(1) = Unit
        // node[0] = CIf { cond: Pool(0), then_: Pool(1), else_: Pool(1) } → Unit
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![bool_entry(), unit_entry()],
            nodes: vec![Node::CIf {
                cond: NodeRef::Pool(0),
                then_: NodeRef::Pool(1),
                else_: NodeRef::Pool(1),
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Pure).unwrap();
        assert_eq!(pass.effect_class, EffectClass::Pure);
    }

    // ── FAIL: structural violation ────────────────────────────────────────────

    #[test]
    fn fail_structural_forward_edge() {
        let scope = pilot_scope();
        // node[0] references node[1] — forward edge, invalid.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CIf {
                    cond: NodeRef::Pool(0),
                    then_: NodeRef::Node(1),
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
        assert!(matches!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY),
            Err(VerifyError::StructuralViolation(_))
        ));
    }

    // ── FAIL: UNKNOWN gate ────────────────────────────────────────────────────

    #[test]
    fn fail_unknown_target() {
        let scope = pilot_scope();
        let unresolved = Hash256::from_bytes([0xAA; 32]);
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: unresolved,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::UnknownTarget { node_index: 0, target: unresolved }
        );
    }

    // ── FAIL: arity mismatch ──────────────────────────────────────────────────

    #[test]
    fn fail_arity_too_few() {
        let scope = pilot_scope();
        // const_unit expects 1 arg, give 0.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![],
            nodes: vec![Node::CCall { target_name: "const_unit".to_string(), target_identity: const_unit_id(), args: vec![] }],
            result: NodeRef::Node(0),
        };
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::ArityMismatch { node_index: 0, expected: 1, got: 0 }
        );
    }

    #[test]
    fn fail_arity_too_many() {
        let scope = pilot_scope();
        // const_unit expects 1 arg, give 2.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "const_unit".to_string(),
                target_identity: const_unit_id(),
                args: vec![NodeRef::Pool(0), NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::ArityMismatch { node_index: 0, expected: 1, got: 2 }
        );
    }

    // ── variadic: what `variadic true` relaxes is read off `in` ──────────────

    // A pure leaf `vfn: (inputs...) -> Unit` declared `variadic true`.
    fn variadic_scope(inputs: Vec<Hash256>) -> (Scope, Hash256) {
        let fn_id = Hash256::from_bytes([0xAB; 32]);
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: fn_id,
            inputs,
            output: unit_type_id(),
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: true,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: fn_id,
            effect_class: EffectClass::Pure,
            deterministic: true,
            idempotent: true,
        })).unwrap();
        (scope, fn_id)
    }

    // Pool 0 is Unit, pool 1 is Bool.
    fn variadic_bundle(fn_id: Hash256, args: Vec<NodeRef>) -> CoreBundle {
        CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry(), bool_entry()],
            nodes: vec![Node::CCall { target_name: "vfn".to_string(), target_identity: fn_id, args }],
            result: NodeRef::Node(0),
        }
    }

    fn verify_variadic(inputs: Vec<Hash256>, args: Vec<NodeRef>) -> Result<(), VerifyError> {
        let (scope, fn_id) = variadic_scope(inputs);
        verify_bundle(&variadic_bundle(fn_id, args), &scope, &HashMap::new(), EffectClass::ANY)
            .map(|_| ())
    }

    /// `in ()`: any count, any types — value_make / list_make.
    #[test]
    fn variadic_empty_in_is_unchecked() {
        verify_variadic(vec![], vec![]).expect("zero args");
        verify_variadic(vec![], vec![NodeRef::Pool(1), NodeRef::Pool(0), NodeRef::Pool(1)])
            .expect("mixed types, any count");
    }

    /// `in (T)`: any count, but every argument must be a `T` —
    /// handle_list_make `in (Handle)`. The rejected element is the SECOND, so
    /// this also fails if only position 0 were checked.
    #[test]
    fn variadic_homogeneous_checks_every_element() {
        let unit = unit_type_id();
        verify_variadic(vec![unit], vec![]).expect("zero elements");
        verify_variadic(vec![unit], vec![NodeRef::Pool(0); 3]).expect("three matching elements");
        match verify_variadic(vec![unit], vec![NodeRef::Pool(0), NodeRef::Pool(1)]).unwrap_err() {
            VerifyError::TypeMismatch { node_index: 0, arg_index: 1, expected, got } => {
                assert_eq!(expected, unit);
                assert_eq!(got, bool_type_id());
            }
            other => panic!("expected TypeMismatch on element 1, got {:?}", other),
        }
    }

    /// `in (T1, T2)`: exactly two arguments, types unchecked — seq.
    #[test]
    fn variadic_multi_in_fixes_arity_not_types() {
        let unit = unit_type_id();
        verify_variadic(vec![unit, unit], vec![NodeRef::Pool(1), NodeRef::Pool(1)])
            .expect("types are not checked");
        for n in [1usize, 3] {
            assert_eq!(
                verify_variadic(vec![unit, unit], vec![NodeRef::Pool(0); n]).unwrap_err(),
                VerifyError::ArityMismatch { node_index: 0, expected: 2, got: n },
            );
        }
    }

    // ── FAIL: type mismatch ───────────────────────────────────────────────────

    #[test]
    fn fail_type_mismatch_wrong_arg() {
        let scope = pilot_scope();
        // const_unit expects Unit; Pool(0) has Bool type.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![bool_entry()],
            nodes: vec![Node::CCall {
                target_name: "const_unit".to_string(),
                target_identity: const_unit_id(),
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        match verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err() {
            VerifyError::TypeMismatch { node_index: 0, arg_index: 0, expected, got } => {
                assert_eq!(expected, unit_type_id());
                assert_eq!(got, bool_type_id());
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    // ── FAIL: CIf condition not Bool ──────────────────────────────────────────

    #[test]
    fn fail_cif_cond_not_bool() {
        let scope = pilot_scope();
        // Pool(0) has Unit type — not Bool.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CIf {
                cond: NodeRef::Pool(0),
                then_: NodeRef::Pool(0),
                else_: NodeRef::Pool(0),
            }],
            result: NodeRef::Node(0),
        };
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::CIfCondNotBool { node_index: 0, got: unit_type_id() }
        );
    }

    // ── FAIL: CIf branch type divergence ─────────────────────────────────────

    #[test]
    fn fail_cif_branch_divergence() {
        let scope = pilot_scope();
        // Pool(0) = Bool, Pool(1) = Unit.
        // then_ = Pool(1) (Unit), else_ = Pool(0) (Bool) → divergence.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![bool_entry(), unit_entry()],
            nodes: vec![Node::CIf {
                cond: NodeRef::Pool(0),
                then_: NodeRef::Pool(1),
                else_: NodeRef::Pool(0),
            }],
            result: NodeRef::Node(0),
        };
        match verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err() {
            VerifyError::CIfBranchDivergence { node_index: 0, then_type, else_type } => {
                assert_eq!(then_type, unit_type_id());
                assert_eq!(else_type, bool_type_id());
            }
            other => panic!("expected CIfBranchDivergence, got {:?}", other),
        }
    }

    // ── FAIL: effect policy violation ─────────────────────────────────────────

    #[test]
    fn fail_effect_policy_violated() {
        let (scope, fn_id) = writes_fn_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "fn".to_string(),
                target_identity: fn_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Pure).unwrap_err(),
            VerifyError::EffectPolicyViolated {
                derived: EffectClass::FullIo,
                allowed: EffectClass::Pure,
            }
        );
    }

    #[test]
    fn pass_effect_within_policy() {
        let (scope, fn_id) = writes_fn_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "fn".to_string(),
                target_identity: fn_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo).unwrap();
        assert_eq!(pass.effect_class, EffectClass::FullIo);
    }

    // ── FAIL: composite effect underivable — FAIL CLOSED ─────────────────────

    #[test]
    fn fail_composite_effect_underivable() {
        let (scope, fn_id) = composite_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "fn".to_string(),
                target_identity: fn_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        match verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err() {
            VerifyError::CompositeEffectUnderivable { node_index: 0, target, .. } => {
                assert_eq!(target, fn_id);
            }
            other => panic!("expected CompositeEffectUnderivable, got {:?}", other),
        }
    }

    // Composite FAILS CLOSED even when the policy is ANY (most permissive) — the issue is that
    // we cannot derive the effect, not that it violates policy.
    #[test]
    fn fail_composite_fails_closed_not_assumed_pure() {
        let (scope, fn_id) = composite_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "fn".to_string(),
                target_identity: fn_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        // Even with the most permissive policy, composite fails closed.
        assert!(matches!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::CompositeEffectUnderivable { .. }
        ));
    }

    // Composite PASSES when its body bundle is in the bundle_store and the
    // derived effect is within the policy ceiling.
    #[test]
    fn pass_composite_with_body_store() {
        let unit_id = unit_type_id();
        let leaf_id = Hash256::from_bytes([0xBB; 32]);
        let comp_id = Hash256::from_bytes([0xDD; 32]);

        // Body bundle: one CCall to a FullIo leaf.
        let body_bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: leaf_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let body_hash = crate::ir::core_ir_05::bundle_identity(&body_bundle);

        // Scope: leaf_id with FullIo, comp_id as composite pointing to body_hash.
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: leaf_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: crate::registry::core05::record::Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: leaf_id,
            effect_class: EffectClass::FullIo,
            deterministic: false,
            idempotent: false,
        })).unwrap();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: comp_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Composite,
            disposition: crate::registry::core05::record::Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::CompositeBody(CompositeBody {
            identity: comp_id,
            body: body_hash,
        })).unwrap();
        let mut body_store = HashMap::new();
        body_store.insert(body_hash, body_bundle);

        // Outer bundle calls the composite.
        let outer = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: comp_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&outer, &scope, &body_store, EffectClass::FullIo).unwrap();
        assert_eq!(pass.effect_class, EffectClass::FullIo);
    }

    // Composite cycle detection: composite A → body calls composite B → body
    // calls composite A → cycle → CompositeEffectUnderivable.
    #[test]
    fn fail_composite_cycle_detection() {
        let unit_id = unit_type_id();
        let comp_a = Hash256::from_bytes([0xAA; 32]);
        let comp_b = Hash256::from_bytes([0xBB; 32]);
        let body_hash_a = Hash256::from_bytes([0xA0; 32]);
        let body_hash_b = Hash256::from_bytes([0xB0; 32]);

        // Body bundle A calls composite B.
        let body_a = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: comp_b,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        // Body bundle B calls composite A → cycle.
        let body_b = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: comp_a,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };

        let mut scope = Scope::new();
        for (id, body_hash) in [(comp_a, body_hash_a), (comp_b, body_hash_b)] {
            scope.insert(Record::FnGenesis(FnGenesis {
                identity: id,
                inputs: vec![unit_id],
                output: unit_id,
                kind: FnKind::Composite,
                disposition: crate::registry::core05::record::Disposition::Unspecified,
            variadic: false,
            })).unwrap();
            scope.insert(Record::CompositeBody(CompositeBody {
                identity: id,
                body: body_hash,
            })).unwrap();
        }
        let mut body_store = HashMap::new();
        body_store.insert(body_hash_a, body_a);
        body_store.insert(body_hash_b, body_b);

        // Outer calls composite A — derivation enters cycle.
        let outer = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: comp_a,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        assert!(matches!(
            verify_bundle(&outer, &scope, &body_store, EffectClass::ANY).unwrap_err(),
            VerifyError::CompositeEffectUnderivable { .. }
        ));
    }

    // ── Per-node effect map (Phase 2 annotation) ─────────────────────────────

    // CCall to a leaf: annotation = Resolved(leaf effect), agrees with effect_class.
    #[test]
    fn node_effects_ccall_leaf_pure() {
        let scope = pilot_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "const_unit".to_string(),
                target_identity: const_unit_id(),
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap();
        assert_eq!(pass.node_effects, vec![NodeEffect::Resolved(EffectClass::Pure)]);
        assert_eq!(pass.effect_class, EffectClass::Pure);
    }

    #[test]
    fn node_effects_ccall_leaf_full_io() {
        let (scope, fn_id) = writes_fn_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "fn".to_string(),
                target_identity: fn_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo).unwrap();
        assert_eq!(pass.node_effects, vec![NodeEffect::Resolved(EffectClass::FullIo)]);
        // Annotation agrees with bundle-level effect_class on first-order programs.
        assert_eq!(pass.effect_class, EffectClass::FullIo);
    }

    // CIf: annotation = max(then annotation, else annotation).
    // node[0] = CCall(writes_fn) → FullIo
    // node[1] = CIf { then_: Pool(unit/Pure), else_: Node(0/FullIo) } → FullIo
    #[test]
    fn node_effects_cif_max_of_branches() {
        let (scope, fn_id) = writes_fn_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry(), bool_entry()],
            nodes: vec![
                Node::CCall { target_name: "fn".to_string(), target_identity: fn_id, args: vec![NodeRef::Pool(0)] },
                Node::CIf {
                    cond: NodeRef::Pool(1),
                    then_: NodeRef::Pool(0),
                    else_: NodeRef::Node(0),
                },
            ],
            result: NodeRef::Node(1),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo).unwrap();
        assert_eq!(pass.node_effects[0], NodeEffect::Resolved(EffectClass::FullIo));
        // max(Pure [Pool constant], FullIo [Node(0)]) = FullIo
        assert_eq!(pass.node_effects[1], NodeEffect::Resolved(EffectClass::FullIo));
    }

    // CIf: both branches are pool constants (Pure) → annotation = Resolved(Pure).
    #[test]
    fn node_effects_cif_pure_branches() {
        let scope = pilot_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![bool_entry(), unit_entry()],
            nodes: vec![Node::CIf {
                cond: NodeRef::Pool(0),
                then_: NodeRef::Pool(1),
                else_: NodeRef::Pool(1),
            }],
            result: NodeRef::Node(0),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Pure).unwrap();
        assert_eq!(pass.node_effects, vec![NodeEffect::Resolved(EffectClass::Pure)]);
    }

    // Composite boundary: CCall to a composite is PendingDerivation in the
    // per-node map, even when verify_bundle succeeds (body in store).
    // This tests the hard first-order boundary — Phase 3 is not entered here.
    #[test]
    fn node_effects_composite_is_pending_derivation() {
        let unit_id = unit_type_id();
        let leaf_id = Hash256::from_bytes([0xBB; 32]);
        let comp_id = Hash256::from_bytes([0xDD; 32]);

        let body_bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: leaf_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let body_hash = crate::ir::core_ir_05::bundle_identity(&body_bundle);

        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: leaf_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: crate::registry::core05::record::Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: leaf_id,
            effect_class: EffectClass::FullIo,
            deterministic: false,
            idempotent: false,
        })).unwrap();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: comp_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Composite,
            disposition: crate::registry::core05::record::Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::CompositeBody(CompositeBody {
            identity: comp_id,
            body: body_hash,
        })).unwrap();
        let mut body_store = HashMap::new();
        body_store.insert(body_hash, body_bundle);

        let outer = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: comp_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        // verify_bundle succeeds (body in store); but the per-node annotation
        // is PendingDerivation because composites are the first-order boundary.
        let pass = verify_bundle(&outer, &scope, &body_store, EffectClass::FullIo).unwrap();
        assert_eq!(
            pass.node_effects[0],
            NodeEffect::PendingDerivation,
            "CCall to composite must be PendingDerivation in the per-node map"
        );
    }

    // First-order agreement: union of all Resolved node_effects == effect_class.
    // Uses a two-node first-order program: const_unit (Pure) + writes_fn (FullIo).
    #[test]
    fn node_effects_agree_with_effect_class_on_first_order() {
        let (mut scope, writes_id) = writes_fn_scope();
        let pilot = crate::registry::core05::text::lower(PILOT_AXREG)
            .expect("pilot load")
            .scope;
        for (_id, genesis) in pilot.fns {
            scope.insert(Record::FnGenesis(genesis)).unwrap();
        }
        for (_id, lf) in pilot.leaf_facts {
            scope.insert(Record::LeafFacts(lf)).unwrap();
        }

        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CCall { target_name: "const_unit".to_string(), target_identity: const_unit_id(), args: vec![NodeRef::Pool(0)] },
                Node::CCall { target_name: "writes".to_string(), target_identity: writes_id, args: vec![NodeRef::Node(0)] },
            ],
            result: NodeRef::Node(1),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo).unwrap();

        // All annotations are Resolved on a first-order program.
        assert!(pass.node_effects.iter().all(|e| matches!(e, NodeEffect::Resolved(_))));

        // union of all Resolved annotations == effect_class.
        let derived_union = pass.node_effects.iter().fold(EffectClass::Pure, |acc, e| {
            if let NodeEffect::Resolved(eff) = e { acc.union(*eff) } else { acc }
        });
        assert_eq!(derived_union, pass.effect_class);
        assert_eq!(pass.effect_class, EffectClass::FullIo);
    }

    // ── Obligation union: Reads × Writes ─────────────────────────────────────
    //
    // Tier-shape evidence (from core05_example.axreg):
    //   `print_line` is declared `effect writes` and is an interactive output
    //   function (writes to terminal). It does NOT read external state.
    //   No axreg file declares `effect reads`.
    //   Writing to stdout and reading from stdin are independent capabilities:
    //   neither implies the other → Reads and Writes are ORTHOGONAL obligations.
    //
    // EffectClass is a genuine obligation set (P2B2). Each named constant is a
    // distinct bit; multi-obligation values are composed by bitwise OR and are
    // DISTINCT from the named single-bit constants, including FullIo.
    //
    // join(Reads=0x01, Writes=0x02) = EffectClass(0x03) — NOT FullIo(0x08).
    // The Reads and Writes obligations are independently retained.

    #[test]
    fn join_reads_writes_preserves_both_obligations() {
        use super::join_node_effect;

        let rw = join_node_effect(
            NodeEffect::Resolved(EffectClass::Reads),
            NodeEffect::Resolved(EffectClass::Writes),
        );
        let wr = join_node_effect(
            NodeEffect::Resolved(EffectClass::Writes),
            NodeEffect::Resolved(EffectClass::Reads),
        );

        // Result holds both obligations.
        let rw_ec = match rw { NodeEffect::Resolved(ec) => ec, _ => panic!("expected Resolved") };
        assert!(rw_ec.has_reads(),  "Reads obligation must be retained");
        assert!(rw_ec.has_writes(), "Writes obligation must be retained");
        // Commutative.
        assert_eq!(rw, wr);
        // Distinctness: Reads|Writes = EffectClass(3) ≠ FullIo = EffectClass(8).
        // This cannot be satisfied by top-promotion.
        assert_ne!(rw, NodeEffect::Resolved(EffectClass::FullIo));
        // Old dropped-obligation result is gone.
        assert_ne!(rw, NodeEffect::Resolved(EffectClass::Writes));
    }

    #[test]
    fn obligation_set_distinctness() {
        use super::join_node_effect;

        // join(Reads, Nondeterministic) and join(Writes, Nondeterministic) must
        // be distinct — no single top element can fake both identities.
        let reads_nd = join_node_effect(
            NodeEffect::Resolved(EffectClass::Reads),
            NodeEffect::Resolved(EffectClass::Nondeterministic),
        );
        let writes_nd = join_node_effect(
            NodeEffect::Resolved(EffectClass::Writes),
            NodeEffect::Resolved(EffectClass::Nondeterministic),
        );
        assert_ne!(reads_nd, writes_nd);

        // reads_nd has reads + nondeterministic; NOT writes.
        let reads_nd_ec = match reads_nd {
            NodeEffect::Resolved(ec) => ec,
            _ => panic!("expected Resolved"),
        };
        assert!(reads_nd_ec.has_reads());
        assert!(!reads_nd_ec.has_writes());
        assert!(reads_nd_ec.has_nondeterministic());

        // writes_nd has writes + nondeterministic; NOT reads.
        let writes_nd_ec = match writes_nd {
            NodeEffect::Resolved(ec) => ec,
            _ => panic!("expected Resolved"),
        };
        assert!(!writes_nd_ec.has_reads());
        assert!(writes_nd_ec.has_writes());
        assert!(writes_nd_ec.has_nondeterministic());

        // join(Reads, Writes) is NOT FullIo.
        let rw = join_node_effect(
            NodeEffect::Resolved(EffectClass::Reads),
            NodeEffect::Resolved(EffectClass::Writes),
        );
        assert_ne!(rw, NodeEffect::Resolved(EffectClass::FullIo));
    }

    // ── Effect-class ordering ─────────────────────────────────────────────────

    #[test]
    fn effect_class_ordering_max_is_derived() {
        // Two CCalls: const_unit (Pure) and writes_fn (FullIo).
        // Bundle effect = max = FullIo.
        let (mut scope, writes_id) = writes_fn_scope();
        // Add pilot const_unit into the writes scope.
        let pilot = crate::registry::core05::text::lower(PILOT_AXREG)
            .expect("pilot load")
            .scope;
        for (_id, genesis) in pilot.fns {
            scope.insert(Record::FnGenesis(genesis)).unwrap();
        }
        for (_id, lf) in pilot.leaf_facts {
            scope.insert(Record::LeafFacts(lf)).unwrap();
        }

        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CCall { target_name: "const_unit".to_string(), target_identity: const_unit_id(), args: vec![NodeRef::Pool(0)] },
                Node::CCall {
                    target_name: "writes".to_string(),
                    target_identity: writes_id,
                    args: vec![NodeRef::Node(0)],
                },
            ],
            result: NodeRef::Node(1),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo).unwrap();
        assert_eq!(pass.effect_class, EffectClass::FullIo);
    }

    // ── Policy ceiling: bitset containment ───────────────────────────────────
    //
    // The ceiling check is `derived ⊆ allowed` (bitset containment).
    //
    // Bidirectional proof:
    //
    // REJECT direction (under-accept bug fix):
    //   A bundle whose derived obligation has a numeric varint BELOW the
    //   ceiling value but whose obligation bit is NOT present in the ceiling
    //   set must be rejected. The old varint check (derived > allowed) passed
    //   this case because the number was smaller; containment correctly rejects
    //   it because the obligation bit is absent.
    //
    //   Concrete: derived=Writes(0x02), ceiling=Nondeterministic(0x04).
    //   Old check: 0x02 > 0x04 → false → PASS (wrong — Writes ∉ {Nd}).
    //   New check: 0x02 & !0x04 = 0x02 ≠ 0 → REJECT ✓.
    //
    // PASS direction (no over-rejection):
    //   A bundle whose every obligation bit is present in the ceiling must be
    //   admitted. Concrete: derived=Reads(0x01), ceiling=Reads|Writes(0x03).
    //   0x01 & !0x03 = 0 → PASS ✓.

    // Unit tests — exercise is_contained_by directly.
    #[test]
    fn ceiling_is_contained_by_unit() {
        // Exact match passes.
        assert!(EffectClass::Writes.is_contained_by(EffectClass::Writes));
        // Subset passes.
        assert!(EffectClass::Reads.is_contained_by(EffectClass::Reads.union(EffectClass::Writes)));
        // Superset passes (more bits in ceiling than in derived).
        let all = EffectClass::Reads.union(EffectClass::Writes).union(EffectClass::Nondeterministic);
        assert!(EffectClass::Reads.union(EffectClass::Nondeterministic).is_contained_by(all));
        // Pure is contained by everything.
        assert!(EffectClass::Pure.is_contained_by(EffectClass::Reads));
        assert!(EffectClass::Pure.is_contained_by(EffectClass::Pure));
        // Obligation absent from ceiling is rejected.
        assert!(!EffectClass::Writes.is_contained_by(EffectClass::Nondeterministic));
        assert!(!EffectClass::Reads.is_contained_by(EffectClass::Writes));
        // Varint-below-ceiling is still rejected if obligation is absent.
        // Writes=0x02 < Nondeterministic=0x04 numerically, but Writes ∉ {Nd}.
        let forbidden = EffectClass::Writes;
        let ceiling   = EffectClass::Nondeterministic;
        assert!(forbidden.as_varint() < ceiling.as_varint(), "numeric value is BELOW ceiling");
        assert!(!forbidden.is_contained_by(ceiling),         "but obligation is NOT permitted");
    }

    // Integration test: REJECT direction.
    // Bundle calls a Writes leaf; ceiling is Nondeterministic-only.
    // Derived = Writes(0x02), ceiling = Nondeterministic(0x04).
    // Old varint: 0x02 > 0x04 → false → PASSES (wrong).
    // New containment: 0x02 & !0x04 ≠ 0 → REJECTS ✓.
    #[test]
    fn ceiling_containment_rejects_forbidden_obligation_below_numeric_ceiling() {
        let unit_id = unit_type_id();
        let fn_id   = Hash256::from_bytes([0xAA; 32]);
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: fn_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: fn_id,
            effect_class: EffectClass::Writes, // derived will be Writes = 0x02
            deterministic: false,
            idempotent: false,
        })).unwrap();

        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall { target_name: "fn".to_string(), target_identity: fn_id, args: vec![NodeRef::Pool(0)] }],
            result: NodeRef::Node(0),
        };

        // Ceiling = Nondeterministic (0x04) — does NOT include Writes (0x02).
        // Varint: 0x02 > 0x04 → false — old check would PASS. Containment rejects.
        let ceiling = EffectClass::Nondeterministic;
        assert!(
            EffectClass::Writes.as_varint() < ceiling.as_varint(),
            "premise: derived varint is numerically below ceiling"
        );
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), ceiling).unwrap_err(),
            VerifyError::EffectPolicyViolated {
                derived: EffectClass::Writes,
                allowed: ceiling,
            },
            "containment must reject a forbidden obligation even when its numeric value is below the ceiling"
        );
    }

    // Integration test: PASS direction.
    // Bundle calls a Reads leaf; ceiling is Reads|Writes.
    // Derived = Reads(0x01) ⊆ ceiling(0x03) → admitted.
    #[test]
    fn ceiling_containment_admits_subset_obligation() {
        let unit_id = unit_type_id();
        let fn_id   = Hash256::from_bytes([0xAB; 32]);
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: fn_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: fn_id,
            effect_class: EffectClass::Reads,
            deterministic: false,
            idempotent: false,
        })).unwrap();

        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall { target_name: "fn".to_string(), target_identity: fn_id, args: vec![NodeRef::Pool(0)] }],
            result: NodeRef::Node(0),
        };

        let ceiling = EffectClass::Reads.union(EffectClass::Writes); // 0x03
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), ceiling).unwrap();
        assert_eq!(pass.effect_class, EffectClass::Reads);
    }

    // ── Phase 3b: composite effect derivation ────────────────────────────────

    // DISTINCTNESS: composites with different body effects yield different
    // derived effect_class. A Writes-body composite ≠ a Pure-body composite.
    #[test]
    fn composite_body_effect_distinctness() {
        let unit_id = unit_type_id();
        let writes_leaf_id = Hash256::from_bytes([0xC1; 32]);
        let pure_leaf_id   = Hash256::from_bytes([0xC2; 32]);
        let comp_writes_id = Hash256::from_bytes([0xD1; 32]);
        let comp_pure_id   = Hash256::from_bytes([0xD2; 32]);

        let body_writes = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "writes".to_string(),
                target_identity: writes_leaf_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let body_pure = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "pure".to_string(),
                target_identity: pure_leaf_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let body_writes_hash = crate::ir::core_ir_05::bundle_identity(&body_writes);
        let body_pure_hash   = crate::ir::core_ir_05::bundle_identity(&body_pure);

        let mut scope = Scope::new();
        for (id, effect) in [
            (writes_leaf_id, EffectClass::Writes),
            (pure_leaf_id,   EffectClass::Pure),
        ] {
            scope.insert(Record::FnGenesis(FnGenesis {
                identity: id,
                inputs: vec![unit_id],
                output: unit_id,
                kind: FnKind::Leaf,
                disposition: Disposition::Unspecified,
            variadic: false,
            })).unwrap();
            scope.insert(Record::LeafFacts(LeafFacts {
                identity: id,
                effect_class: effect,
                deterministic: true,
                idempotent: false,
            })).unwrap();
        }
        for (comp_id, body_hash) in [
            (comp_writes_id, body_writes_hash),
            (comp_pure_id,   body_pure_hash),
        ] {
            scope.insert(Record::FnGenesis(FnGenesis {
                identity: comp_id,
                inputs: vec![unit_id],
                output: unit_id,
                kind: FnKind::Composite,
                disposition: Disposition::Unspecified,
            variadic: false,
            })).unwrap();
            scope.insert(Record::CompositeBody(CompositeBody {
                identity: comp_id,
                body: body_hash,
            })).unwrap();
        }

        let mut body_store = HashMap::new();
        body_store.insert(body_writes_hash, body_writes);
        body_store.insert(body_pure_hash, body_pure);

        let outer_writes = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "writes".to_string(),
                target_identity: comp_writes_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let outer_pure = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "pure".to_string(),
                target_identity: comp_pure_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };

        let pass_w = verify_bundle(&outer_writes, &scope, &body_store, EffectClass::ANY).unwrap();
        let pass_p = verify_bundle(&outer_pure,   &scope, &body_store, EffectClass::ANY).unwrap();

        assert_ne!(pass_w.effect_class, pass_p.effect_class,
            "Writes-body composite and Pure-body composite must yield distinct effect_class");
        assert!(pass_w.effect_class.has_writes(),
            "Writes-body composite must derive Writes obligation");
        assert_eq!(pass_p.effect_class, EffectClass::Pure,
            "Pure-body composite must derive Pure");
    }

    // HOF SOUNDNESS: map(writes_fn, xs) includes Writes; map(pure_fn, xs) does
    // not; they differ. Callback effects propagate via hof_callback_params.
    #[test]
    fn hof_callback_effect_soundness() {
        let unit_id       = unit_type_id();
        let writes_leaf   = Hash256::from_bytes([0xC3; 32]);
        let pure_leaf     = Hash256::from_bytes([0xC4; 32]);
        let hof_id        = Hash256::from_bytes([0xF0; 32]);

        // HOF body has no intrinsic effect (Tier-3 dispatch is forbidden;
        // callback effects propagate only via hof_callback_params) — a bare
        // Unit pool entry stands in for "no nodes" (a truly nodeless AND
        // poolless bundle has no valid `result` to point at).
        let hof_body = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![],
            result: NodeRef::Pool(0),
        };
        let hof_body_hash = crate::ir::core_ir_05::bundle_identity(&hof_body);

        let mut scope = Scope::new();
        for (id, effect) in [
            (writes_leaf, EffectClass::Writes),
            (pure_leaf,   EffectClass::Pure),
        ] {
            scope.insert(Record::FnGenesis(FnGenesis {
                identity: id,
                inputs: vec![unit_id],
                output: unit_id,
                kind: FnKind::Leaf,
                disposition: Disposition::Unspecified,
            variadic: false,
            })).unwrap();
            scope.insert(Record::LeafFacts(LeafFacts {
                identity: id,
                effect_class: effect,
                deterministic: true,
                idempotent: false,
            })).unwrap();
        }
        // HOF: (callback_result: Unit, xs: Unit) -> Unit
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: hof_id,
            inputs: vec![unit_id, unit_id],
            output: unit_id,
            kind: FnKind::Composite,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::CompositeBody(CompositeBody {
            identity: hof_id,
            body: hof_body_hash,
        })).unwrap();
        // Arg 0 is the callback-result carrier; its effect is attributed to
        // the HOF call site.
        scope.hof_callback_params.insert(hof_id, vec![0]);

        let mut body_store = HashMap::new();
        body_store.insert(hof_body_hash, hof_body);

        // Outer bundle with a Writes callback:
        //   node[0] = writes_leaf(Pool(unit)) → Unit, effect = Writes
        //   node[1] = hof(Node(0), Pool(unit)) → body=Pure, cb=Writes → Writes
        let outer_writes = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CCall { target_name: "writes".to_string(), target_identity: writes_leaf, args: vec![NodeRef::Pool(0)] },
                Node::CCall {
                    target_name: "test_fn".to_string(),
                    target_identity: hof_id,
                    args: vec![NodeRef::Node(0), NodeRef::Pool(0)],
                },
            ],
            result: NodeRef::Node(1),
        };

        // Outer bundle with a Pure callback:
        //   node[0] = pure_leaf(Pool(unit)) → Unit, effect = Pure
        //   node[1] = hof(Node(0), Pool(unit)) → body=Pure, cb=Pure → Pure
        let outer_pure = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CCall { target_name: "pure".to_string(), target_identity: pure_leaf, args: vec![NodeRef::Pool(0)] },
                Node::CCall {
                    target_name: "test_fn".to_string(),
                    target_identity: hof_id,
                    args: vec![NodeRef::Node(0), NodeRef::Pool(0)],
                },
            ],
            result: NodeRef::Node(1),
        };

        let pass_w = verify_bundle(&outer_writes, &scope, &body_store, EffectClass::ANY).unwrap();
        let pass_p = verify_bundle(&outer_pure,   &scope, &body_store, EffectClass::ANY).unwrap();

        assert_ne!(pass_w.effect_class, pass_p.effect_class,
            "map(writes_fn) and map(pure_fn) must yield distinct effect_class");
        assert!(pass_w.effect_class.has_writes(),
            "map(writes_fn, xs) must carry Writes obligation");
        assert_eq!(pass_p.effect_class, EffectClass::Pure,
            "map(pure_fn, xs) must be Pure");
    }

    // FAIL-CLOSED PARTIAL UNION: a body bundle with one resolvable Writes call
    // and one unresolvable call must fail closed — never Ok with partial Writes.
    // Proves the `?` in derive_body_effect prevents partial union escape.
    #[test]
    fn fail_closed_partial_union_not_silently_dropped() {
        let unit_id      = unit_type_id();
        let writes_leaf  = Hash256::from_bytes([0xC5; 32]);
        let ghost_fn     = Hash256::from_bytes([0xC6; 32]); // intentionally NOT in scope
        let partial_comp = Hash256::from_bytes([0xC7; 32]);

        // Body calls writes_leaf (resolvable) first, then ghost_fn (unresolvable).
        // derive_body_effect accumulates Writes then hits the unresolvable call.
        // The `?` propagates CompositeEffectUnderivable — partial Writes escapes only
        // if the `?` were removed. This test proves it is not removed.
        let body = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![
                Node::CCall { target_name: "writes".to_string(), target_identity: writes_leaf, args: vec![NodeRef::Pool(0)] },
                Node::CCall { target_name: "ghost".to_string(),  target_identity: ghost_fn,    args: vec![NodeRef::Pool(0)] },
            ],
            result: NodeRef::Node(1),
        };
        let body_hash = crate::ir::core_ir_05::bundle_identity(&body);

        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: writes_leaf,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: writes_leaf,
            effect_class: EffectClass::Writes,
            deterministic: true,
            idempotent: false,
        })).unwrap();
        // ghost_fn is intentionally absent — NOT inserted into scope.
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: partial_comp,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Composite,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::CompositeBody(CompositeBody {
            identity: partial_comp,
            body: body_hash,
        })).unwrap();

        let mut body_store = HashMap::new();
        body_store.insert(body_hash, body);

        let outer = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: partial_comp,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };

        assert!(
            matches!(
                verify_bundle(&outer, &scope, &body_store, EffectClass::ANY).unwrap_err(),
                VerifyError::CompositeEffectUnderivable { .. }
            ),
            "partial body walk must fail closed, not escape with partial Writes union"
        );
    }

    // ── Permissive ceiling (ANY) admits every obligation ─────────────────────
    //
    // ANY = 0x1F = all defined obligation bits set (including Irreversible).
    // Reads(0x01) ⊆ ANY(0x1F) and Writes(0x02) ⊆ ANY(0x1F) → both admitted.
    //
    // Contrast with FullIo(0x08)-as-ceiling: Reads(0x01) ⊄ {FullIo} because
    // bit 0 is absent from 0x08, and likewise for Writes(0x02). Both would be
    // wrongly rejected under a FullIo ceiling.
    #[test]
    fn permissive_ceiling_admits_reads_and_writes() {
        let unit_id = unit_type_id();
        let reads_fn_id  = Hash256::from_bytes([0xA2; 32]);
        let writes_fn_id = Hash256::from_bytes([0xA3; 32]);

        let mut scope = Scope::new();
        for (id, effect) in [
            (reads_fn_id,  EffectClass::Reads),
            (writes_fn_id, EffectClass::Writes),
        ] {
            scope.insert(Record::FnGenesis(FnGenesis {
                identity: id,
                inputs: vec![unit_id],
                output: unit_id,
                kind: FnKind::Leaf,
                disposition: Disposition::Unspecified,
            variadic: false,
            })).unwrap();
            scope.insert(Record::LeafFacts(LeafFacts {
                identity: id,
                effect_class: effect,
                deterministic: false,
                idempotent: false,
            })).unwrap();
        }

        for fn_id in [reads_fn_id, writes_fn_id] {
            let bundle = CoreBundle {
                version: "0.5".into(),
                constant_pool: vec![unit_entry()],
                nodes: vec![Node::CCall {
                    target_name: "fn".to_string(),
                    target_identity: fn_id,
                    args: vec![NodeRef::Pool(0)],
                }],
                result: NodeRef::Node(0),
            };

            // PASS under ANY: Reads(0x01) ⊆ 0x1F and Writes(0x02) ⊆ 0x1F.
            assert!(
                verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).is_ok(),
                "ANY ceiling must admit Reads and Writes bundles"
            );

            // FAIL under FullIo(0x08): neither Reads(0x01) nor Writes(0x02) bit
            // is present in 0x08. A FullIo ceiling is NOT a permissive ceiling.
            assert!(
                matches!(
                    verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::FullIo)
                        .unwrap_err(),
                    VerifyError::EffectPolicyViolated { .. }
                ),
                "FullIo ceiling must reject Reads/Writes (their bits are absent from 0x08)"
            );
        }
    }

    // ── Irreversible obligation (P2B2-consistent, bit 4 = 0x10) ──────────────
    //
    // Irreversible is a genuine obligation bit, independent of all others.
    // Storage (EffectClass.0 bitset) is unchanged — only a new named constant
    // and the tier() view are added. Union, containment, and 3b derivation
    // are unchanged.

    #[test]
    fn irreversible_is_a_distinct_obligation_bit() {
        // Bit 4 = 0x10, distinct from all prior obligations.
        assert_eq!(EffectClass::Irreversible.0, 0x10);
        assert_ne!(EffectClass::Irreversible, EffectClass::Pure);
        assert_ne!(EffectClass::Irreversible, EffectClass::Reads);
        assert_ne!(EffectClass::Irreversible, EffectClass::Writes);
        assert_ne!(EffectClass::Irreversible, EffectClass::Nondeterministic);
        assert_ne!(EffectClass::Irreversible, EffectClass::FullIo);
    }

    #[test]
    fn irreversible_has_irreversible_predicate() {
        assert!( EffectClass::Irreversible.has_irreversible());
        assert!(!EffectClass::Pure.has_irreversible());
        assert!(!EffectClass::Reads.has_irreversible());
        assert!(!EffectClass::Writes.has_irreversible());
        assert!(!EffectClass::Nondeterministic.has_irreversible());
        assert!(!EffectClass::FullIo.has_irreversible());

        // Union with other bits must set the Irreversible bit.
        let irr_writes = EffectClass::Irreversible.union(EffectClass::Writes);
        assert!(irr_writes.has_irreversible());
        assert!(irr_writes.has_writes());
        assert!(!irr_writes.has_reads());
    }

    #[test]
    fn any_includes_irreversible_bit() {
        // ANY = 0x1F now includes Irreversible (bit 4).
        assert_eq!(EffectClass::ANY.0, 0x1F);
        assert!(EffectClass::ANY.has_irreversible());
        // Irreversible is contained by ANY.
        assert!(EffectClass::Irreversible.is_contained_by(EffectClass::ANY));
        // Irreversible is NOT contained by the old 0x0F ceiling.
        let old_any = EffectClass(0x0F);
        assert!(!EffectClass::Irreversible.is_contained_by(old_any),
            "Irreversible must not be admitted by a ceiling without bit 4 set");
    }

    #[test]
    fn irreversible_union_composition_is_bitset_union() {
        // join(Irreversible, Writes) is distinct from both; both bits present.
        let irr_w = EffectClass::Irreversible.union(EffectClass::Writes);
        assert!(irr_w.has_irreversible());
        assert!(irr_w.has_writes());
        assert!(!irr_w.has_reads());
        assert_ne!(irr_w, EffectClass::Irreversible);
        assert_ne!(irr_w, EffectClass::Writes);

        // join(Irreversible, Reads, Writes) carries three independent obligations.
        let irr_r_w = irr_w.union(EffectClass::Reads);
        assert!(irr_r_w.has_irreversible());
        assert!(irr_r_w.has_reads());
        assert!(irr_r_w.has_writes());
    }

    // ── EffectTier derived view ───────────────────────────────────────────────
    //
    // tier() is a read-only view over the bitset: it inspects the Irreversible
    // bit and returns BelowIrreversible or IrreversibleOrAbove. Storage is
    // unchanged; this is a derived predicate, not a new obligation bit.

    #[test]
    fn tier_below_irreversible_for_non_irreversible_effects() {
        use crate::registry::core05::record::EffectTier;
        for ec in [
            EffectClass::Pure,
            EffectClass::Reads,
            EffectClass::Writes,
            EffectClass::Nondeterministic,
            EffectClass::FullIo,
            EffectClass::Reads.union(EffectClass::Writes),
            EffectClass::Nondeterministic.union(EffectClass::Writes),
        ] {
            assert_eq!(
                ec.tier(),
                EffectTier::BelowIrreversible,
                "{:?} must be BelowIrreversible (no Irreversible bit)", ec
            );
        }
    }

    #[test]
    fn tier_irreversible_or_above_iff_irreversible_bit_set() {
        use crate::registry::core05::record::EffectTier;
        // Any set that includes the Irreversible bit is IrreversibleOrAbove.
        for ec in [
            EffectClass::Irreversible,
            EffectClass::Irreversible.union(EffectClass::Reads),
            EffectClass::Irreversible.union(EffectClass::Writes),
            EffectClass::Irreversible.union(EffectClass::Nondeterministic),
            EffectClass::ANY,
        ] {
            assert_eq!(
                ec.tier(),
                EffectTier::IrreversibleOrAbove,
                "{:?} must be IrreversibleOrAbove (Irreversible bit 4 set)", ec
            );
        }
    }

    #[test]
    fn tier_is_derived_not_new_storage() {
        use crate::registry::core05::record::EffectTier;
        // Verify tier() is purely derived: flipping the Irreversible bit changes
        // the tier; no other mutation occurred to EffectClass.
        let base = EffectClass::Writes;
        assert_eq!(base.tier(), EffectTier::BelowIrreversible);

        let with_irr = base.union(EffectClass::Irreversible);
        assert_eq!(with_irr.tier(), EffectTier::IrreversibleOrAbove);

        // The base is unchanged — union returns a new value.
        assert_eq!(base.tier(), EffectTier::BelowIrreversible);

        // The raw bit is what tier() reads — no hidden state.
        assert_eq!(with_irr.0, 0x12, "Writes(0x02) | Irreversible(0x10) = 0x12");
    }

    // ── Irreversible policy: ceiling must include the bit ─────────────────────
    //
    // A gated Irreversible bundle (CDeterminate dominates the Irreversible CCall)
    // still FAILS under any ceiling that does not include bit 4. The gate check
    // and the policy check are independent; both must pass. This exercises the
    // existing effect-policy machinery with a properly gated bundle.

    #[test]
    fn irreversible_bundle_rejected_without_irreversible_ceiling() {
        let unit_id = unit_type_id();
        let irr_fn_id = Hash256::from_bytes([0xB0; 32]);

        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: irr_fn_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: irr_fn_id,
            effect_class: EffectClass::Irreversible,
            deterministic: true,
            idempotent: false,
        })).unwrap();

        // Gated bundle: CDeterminate (node[0]) → irr_fn(Node(0)) (node[1]).
        // irr_fn takes Unit; CDeterminate produces Unit → type-checks.
        // node[1] has CDeterminate as a required data ancestor → gate-dominated.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![],
            nodes: vec![
                Node::CDeterminate,
                Node::CCall {
                    target_name: "irreversible".to_string(),
                    target_identity: irr_fn_id,
                    args: vec![NodeRef::Node(0)],
                },
            ],
            result: NodeRef::Node(1),
        };

        // FAIL under Pure, Reads, Writes, FullIo ceilings — bit 4 absent.
        for ceiling in [
            EffectClass::Pure,
            EffectClass::Reads,
            EffectClass::Writes,
            EffectClass::FullIo,
            EffectClass(0x0F), // old ANY before Irreversible was added
        ] {
            assert!(
                matches!(
                    verify_bundle(&bundle, &scope, &HashMap::new(), ceiling).unwrap_err(),
                    VerifyError::EffectPolicyViolated { .. }
                ),
                "Gated Irreversible bundle must be rejected by ceiling {:?} (bit 4 absent)", ceiling
            );
        }

        // PASS under ANY (0x1F) — bit 4 present, gate present.
        assert!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).is_ok(),
            "Gated Irreversible bundle must be admitted by ANY ceiling"
        );

        // PASS under Irreversible itself as ceiling — gate present.
        assert!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::Irreversible).is_ok(),
            "Gated Irreversible bundle admitted by Irreversible ceiling"
        );
    }

    // ── Phase 5a: CDeterminate gate-domination check ─────────────────────────
    //
    // These tests prove the four required properties of the domination check:
    // 1. Distinctness: same Irreversible action PASSES with a dominating gate,
    //    FAILS without one or with the gate on the wrong branch.
    // 2. Fail-closed #1: undominated Irreversible → IrreversibleNotGated.
    // 3. Fail-closed #2: PendingDerivation (undeterminable first-order) →
    //    IrreversibleNotGated when not gate-dominated.
    // 4. Wrong-branch: gate on the then-branch of CIf, Irreversible on the
    //    else-branch → IrreversibleNotGated (true dominance, not presence).
    //
    // All bundles are constructed by hand; no surface syntax or lowering used.

    // Shared helper: a scope with one Irreversible leaf (Unit → Unit).
    fn irr_fn_scope() -> (Scope, Hash256) {
        let unit_id = unit_type_id();
        let fn_id = Hash256::from_bytes([0xE0; 32]);
        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: fn_id,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: fn_id,
            effect_class: EffectClass::Irreversible,
            deterministic: true,
            idempotent: false,
        })).unwrap();
        (scope, fn_id)
    }

    // ── PASS: Irreversible CCall dominated by CDeterminate (distinctness) ─────
    //
    // node[0] = CDeterminate   (Unit type; gate_dominated = true)
    // node[1] = irr_fn(Node(0))  (Irreversible; dep on node[0] → gate_dominated = true)
    //
    // CDeterminate is a required data ancestor of node[1] on every execution
    // path → domination holds → verification passes.
    #[test]
    fn pass_irreversible_dominated_by_cdeterminate() {
        let (scope, fn_id) = irr_fn_scope();
        // irr_fn: (Unit) → Unit.  CDeterminate produces Unit → type-checks.
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![],
            nodes: vec![
                Node::CDeterminate,
                Node::CCall {
                    target_name: "irreversible".to_string(),
                    target_identity: fn_id,
                    args: vec![NodeRef::Node(0)],  // dep on CDeterminate → gate_dominated
                },
            ],
            result: NodeRef::Node(1),
        };
        let pass = verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap();
        assert_eq!(pass.effect_class, EffectClass::Irreversible);
        // CDeterminate itself contributes no effect; effect comes from irr_fn.
        assert_eq!(pass.node_effects[0], NodeEffect::Resolved(EffectClass::Pure));
        assert_eq!(pass.node_effects[1], NodeEffect::Resolved(EffectClass::Irreversible));
    }

    // ── FAIL: Irreversible CCall with no CDeterminate (fail-closed #1) ────────
    //
    // node[0] = irr_fn(Pool(unit))  (Irreversible; no CDeterminate anywhere)
    //
    // gate_dominated[0] = false → IrreversibleNotGated.
    #[test]
    fn fail_irreversible_no_gate() {
        let (scope, fn_id) = irr_fn_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "irreversible".to_string(),
                target_identity: fn_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::IrreversibleNotGated { node_index: 0 },
            "Irreversible CCall with no CDeterminate must fail with IrreversibleNotGated"
        );
    }

    // ── FAIL: wrong-branch (gate on then_, Irreversible on else_) ─────────────
    //
    // node[0] = CDeterminate             (Unit; gate_dominated = true)
    // node[1] = irr_fn(Pool(unit))       (Irreversible; NO dep on CDeterminate)
    // node[2] = CIf { cond: Pool(bool), then_: Node(0), else_: Node(1) }
    //
    // CDeterminate is in the then-branch; the Irreversible CCall is in the
    // else-branch. node[1] has gate_dominated = false (no node dep on CDeterminate)
    // → IrreversibleNotGated at node[1].
    //
    // This proves TRUE dominance: CDeterminate being present in the bundle (even
    // as a then-branch result) does NOT dominate the Irreversible node on the
    // else-branch execution path.
    #[test]
    fn fail_irreversible_wrong_branch() {
        let (scope, fn_id) = irr_fn_scope();
        let bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry(), bool_entry()],
            nodes: vec![
                Node::CDeterminate,          // node[0]: gate (then-branch)
                Node::CCall {                // node[1]: Irreversible (else-branch)
                    target_name: "irreversible".to_string(),
                    target_identity: fn_id,
                    args: vec![NodeRef::Pool(0)],  // Pool(0) = unit; no dep on CDeterminate
                },
                Node::CIf {                 // node[2]: selects between gate and irr action
                    cond: NodeRef::Pool(1), // Pool(1) = bool
                    then_: NodeRef::Node(0), // then_ = CDeterminate result
                    else_: NodeRef::Node(1), // else_ = Irreversible result
                },
            ],
            result: NodeRef::Node(2),
        };
        // node[1] has no CDeterminate in its transitive deps → IrreversibleNotGated(1).
        assert_eq!(
            verify_bundle(&bundle, &scope, &HashMap::new(), EffectClass::ANY).unwrap_err(),
            VerifyError::IrreversibleNotGated { node_index: 1 },
            "CDeterminate on then-branch must NOT dominate Irreversible on else-branch"
        );
    }

    // ── FAIL: undeterminable domination (PendingDerivation, fail-closed #2) ───
    //
    // A composite function whose body calls an Irreversible leaf. The outer
    // CCall has NodeEffect::PendingDerivation (first-order analysis cannot see
    // the body's Irreversible effect). The derived effect IS Irreversible (body
    // in store). No CDeterminate gate → IrreversibleNotGated.
    //
    // "Unknown gating is ungated." The first-order view sees PendingDerivation
    // and cannot determine if gating is needed; the full derivation confirms it
    // IS Irreversible. Either way, the gate is absent → fail-closed.
    #[test]
    fn fail_pending_derivation_irreversible_not_gated() {
        let unit_id = unit_type_id();
        let irr_leaf = Hash256::from_bytes([0xE1; 32]);
        let comp_fn  = Hash256::from_bytes([0xE2; 32]);

        // Body bundle: one Irreversible CCall.
        let body_bundle = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "irreversible".to_string(),
                target_identity: irr_leaf,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };
        let body_hash = crate::ir::core_ir_05::bundle_identity(&body_bundle);

        let mut scope = Scope::new();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: irr_leaf,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Leaf,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::LeafFacts(LeafFacts {
            identity: irr_leaf,
            effect_class: EffectClass::Irreversible,
            deterministic: true,
            idempotent: false,
        })).unwrap();
        scope.insert(Record::FnGenesis(FnGenesis {
            identity: comp_fn,
            inputs: vec![unit_id],
            output: unit_id,
            kind: FnKind::Composite,
            disposition: Disposition::Unspecified,
            variadic: false,
        })).unwrap();
        scope.insert(Record::CompositeBody(CompositeBody {
            identity: comp_fn,
            body: body_hash,
        })).unwrap();

        let mut body_store = HashMap::new();
        body_store.insert(body_hash, body_bundle);

        // Outer bundle: CCall to the Irreversible composite — no CDeterminate gate.
        let outer = CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![unit_entry()],
            nodes: vec![Node::CCall {
                target_name: "test_fn".to_string(),
                target_identity: comp_fn,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        };

        // Body IS in store (effect derivable = Irreversible); but gate is absent.
        // NodeEffect = PendingDerivation (first-order boundary); derived = Irreversible.
        // gate_dominated[0] = false → IrreversibleNotGated(0).
        assert_eq!(
            verify_bundle(&outer, &scope, &body_store, EffectClass::ANY).unwrap_err(),
            VerifyError::IrreversibleNotGated { node_index: 0 },
            "PendingDerivation composite with Irreversible body and no gate must fail"
        );
    }

    // ─── HOF CALLBACK CONTRACTS (Step 2c) ────────────────────────────────────
    //
    // `Fn` is one opaque primitive: every fn-ref pool entry carries the same
    // def_hash and differs only in its 32-byte payload. Step 2b therefore
    // cannot tell one fn-ref from another, and before Step 2c existed a callee
    // of the wrong arity, the wrong argument types, or the wrong return type
    // passed verification and panicked inside generated code instead —
    // `int_add_vfn: expected a 2-tuple`, `flat_map: callee must return
    // ValueList`, or a `Value::as_str` accessor panic. These tests pin the
    // three holes shut and pin the fail-closed default.

    fn int_type_id() -> Hash256 {
        type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Int) })
    }

    /// A leaf HOF `hof(Fn) -> Unit` plus the leaf its fn-ref points at, so the
    /// only variable under test is the callback contract. `contract` is what
    /// the HOF declares it needs; the `callee_*` arguments describe what the
    /// referenced fn actually is.
    fn callback_scope(
        callee_inputs: Vec<Hash256>,
        callee_output: Hash256,
        callee_effect: EffectClass,
        contract: Option<CallbackContract>,
    ) -> (Scope, Hash256, Hash256) {
        let unit_id = unit_type_id();
        let hof_id = Hash256::from_bytes([0xA1; 32]);
        let callee_id = Hash256::from_bytes([0xA2; 32]);
        let mut scope = Scope::new();

        for (id, inputs, output, effect) in [
            (hof_id, vec![fn_type_id()], unit_id, EffectClass::Pure),
            (callee_id, callee_inputs, callee_output, callee_effect),
        ] {
            scope.insert(Record::FnGenesis(FnGenesis {
                identity: id,
                inputs,
                output,
                kind: FnKind::Leaf,
                disposition: Disposition::Unspecified,
                variadic: false,
            })).unwrap();
            scope.insert(Record::LeafFacts(LeafFacts {
                identity: id,
                effect_class: effect,
                deterministic: true,
                idempotent: true,
            })).unwrap();
        }

        if let Some(c) = contract {
            scope.hof_callback_contracts.insert(hof_id, vec![c]);
        }
        (scope, hof_id, callee_id)
    }

    /// `hof(<fn-ref to callee>)` — one pool entry, one call.
    fn callback_bundle(hof_id: Hash256, callee_id: Hash256) -> CoreBundle {
        CoreBundle {
            version: "0.5".into(),
            constant_pool: vec![ConstantPoolEntry {
                def_hash: fn_type_id(),
                payload: callee_id.as_bytes().to_vec(),
            }],
            nodes: vec![Node::CCall {
                target_name: "hof".to_string(),
                target_identity: hof_id,
                args: vec![NodeRef::Pool(0)],
            }],
            result: NodeRef::Node(0),
        }
    }

    /// Contract for `(Int) -> Unit`, the signature the other tests diverge from.
    fn int_to_unit_contract() -> CallbackContract {
        CallbackContract {
            position: 0,
            inputs: vec![int_type_id()],
            output: unit_type_id(),
        }
    }

    #[test]
    fn callback_contract_admits_a_matching_fn_ref() {
        let (scope, hof, callee) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            Some(int_to_unit_contract()),
        );
        verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect("a fn-ref whose signature matches the contract must verify");
    }

    #[test]
    fn callback_contract_rejects_wrong_arity() {
        // The `int_add` case: a 2-ary fn handed to a 1-ary callback slot.
        // Emitted as `int_add_vfn`, which panics on a non-2-tuple at run.
        let (scope, hof, callee) = callback_scope(
            vec![int_type_id(), int_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            Some(int_to_unit_contract()),
        );
        let err = verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect_err("a 2-ary fn in a 1-ary callback slot must be rejected");
        match err {
            VerifyError::CallbackContractViolated { arg_index, reason, .. } => {
                assert_eq!(arg_index, 0);
                assert!(reason.contains("arity"), "reason should name arity: {}", reason);
            }
            other => panic!("expected CallbackContractViolated, got {:?}", other),
        }
    }

    #[test]
    fn callback_contract_rejects_wrong_return() {
        // The `flat_map(xs, is_positive)` case: the callee returns the wrong
        // type, which `flat_map` discovers only when it tries to splice it.
        let (scope, hof, callee) = callback_scope(
            vec![int_type_id()],
            int_type_id(),
            EffectClass::Pure,
            Some(int_to_unit_contract()),
        );
        let err = verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect_err("a fn returning the wrong type must be rejected");
        match err {
            VerifyError::CallbackContractViolated { reason, .. } => {
                assert!(reason.contains("return"), "reason should name the return: {}", reason);
            }
            other => panic!("expected CallbackContractViolated, got {:?}", other),
        }
    }

    #[test]
    fn callback_contract_rejects_wrong_argument_type() {
        // The `str_len` case: the HOF applies the callee to an Int, but the
        // callee accepts only Unit. `str_len_vfn`'s `as_str()` panics at run.
        let (scope, hof, callee) = callback_scope(
            vec![unit_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            Some(int_to_unit_contract()),
        );
        let err = verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect_err("a fn that cannot accept what the HOF applies it to must be rejected");
        match err {
            VerifyError::CallbackContractViolated { reason, .. } => {
                assert!(reason.contains("arg[0]"), "reason should name the argument: {}", reason);
            }
            other => panic!("expected CallbackContractViolated, got {:?}", other),
        }
    }

    #[test]
    fn fn_slot_without_a_contract_fails_closed() {
        // An `Fn` parameter with no declared contract is uncheckable. Admitting
        // it is exactly the defect Step 2c exists to prevent, so the absence of
        // a contract must fail — never silently pass.
        let (scope, hof, callee) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            None,
        );
        let err = verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect_err("an Fn slot with no contract must fail closed");
        match err {
            VerifyError::CallbackContractViolated { reason, .. } => {
                assert!(reason.contains("no `callback` contract"),
                    "reason should say the contract is missing: {}", reason);
            }
            other => panic!("expected CallbackContractViolated, got {:?}", other),
        }
    }

    #[test]
    fn callback_fn_ref_to_unknown_target_fails_closed() {
        // The pool payload is well-formed but names a fn no registry declares.
        let (scope, hof, _callee) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            Some(int_to_unit_contract()),
        );
        let stranger = Hash256::from_bytes([0xEE; 32]);
        let err = verify_bundle(
            &callback_bundle(hof, stranger),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect_err("a fn-ref to an unknown identity must fail closed");
        match err {
            VerifyError::CallbackContractViolated { reason, .. } => {
                assert!(reason.contains("not in the active scope"),
                    "reason should name the UNKNOWN gate: {}", reason);
            }
            other => panic!("expected CallbackContractViolated, got {:?}", other),
        }
    }

    #[test]
    fn callback_effect_reaches_the_call_site() {
        // The HOF itself is Pure; it applies a Writes callee. Calling it does
        // write, so the derived effect must carry Writes — otherwise a HOF
        // launders any effect its callback has.
        let contract = int_to_unit_contract();
        let (scope_w, hof_w, callee_w) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Writes,
            Some(contract.clone()),
        );
        let (scope_p, hof_p, callee_p) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            Some(contract),
        );

        let pass_w = verify_bundle(
            &callback_bundle(hof_w, callee_w), &scope_w, &HashMap::new(), EffectClass::ANY,
        ).unwrap();
        let pass_p = verify_bundle(
            &callback_bundle(hof_p, callee_p), &scope_p, &HashMap::new(), EffectClass::ANY,
        ).unwrap();

        assert!(pass_w.effect_class.has_writes(),
            "a Pure HOF applying a Writes callback must derive Writes");
        assert_eq!(pass_p.effect_class, EffectClass::Pure,
            "a Pure HOF applying a Pure callback must stay Pure");
        assert_ne!(pass_w.effect_class, pass_p.effect_class,
            "the two callbacks must be distinguishable at the call site");
    }

    #[test]
    fn callback_admits_a_narrower_param_than_an_erased_contract() {
        // The registry has no type variables, so `loop_while :: a -> (a -> Bool)
        // -> (a -> a) -> Int -> a` can only erase `a` to the `Value` sum. Real
        // callers pass monomorphic callbacks — axVerity's
        // `loop_while(text, gr_more, gr_apply_step, …)` hands it
        // `gr_more(state: Text) -> Bool`. Strict contravariance would reject
        // every one of those, and they are correct: the caller's own argument
        // fixes `a`.
        //
        // The relaxation reaches only sum-typed contract inputs, so it cannot
        // silently widen a contract that names a real type — see
        // `callback_contract_rejects_wrong_argument_type`, which still fails
        // with an Int contract against a Unit-accepting callee.
        let value_id = type_identity(&TypeDef {
            shape: TypeShape::Sum(vec![vec![int_type_id()], vec![bool_type_id()]]),
        });
        let (mut scope, hof, callee) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Pure,
            Some(CallbackContract {
                position: 0,
                inputs: vec![value_id],
                output: unit_type_id(),
            }),
        );
        scope.insert(Record::Type(TypeDef {
            shape: TypeShape::Sum(vec![vec![int_type_id()], vec![bool_type_id()]]),
        })).unwrap();

        verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect("an Int-accepting callee must satisfy a Value-erased contract");
    }

    #[test]
    fn bare_payload_admitted_into_a_sum_only_when_the_variant_is_unambiguous() {
        let text_id = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Text) });
        // `Value`-like: one constructor per payload type — a bare Int is AsInt.
        let value_like = TypeDef { shape: TypeShape::Sum(vec![vec![int_type_id()], vec![bool_type_id()]]) };
        // Every variant carries an Int: a bare Int names none of them.
        let ambiguous = TypeDef { shape: TypeShape::Sum(vec![vec![int_type_id()], vec![int_type_id()], vec![int_type_id()]]) };
        // Two Int variants and one Text variant: Text is determined, Int is not.
        let mixed = TypeDef { shape: TypeShape::Sum(vec![vec![int_type_id()], vec![int_type_id()], vec![text_id]]) };
        let ids: Vec<Hash256> = [&value_like, &ambiguous, &mixed].iter().map(|t| type_identity(t)).collect();
        let mut scope = Scope::new();
        for t in [value_like, ambiguous, mixed] {
            scope.insert(Record::Type(t)).unwrap();
        }
        assert!(type_admits(ids[0], int_type_id(), &scope), "Int into an Int|Bool sum is AsInt");
        assert!(type_admits(ids[0], bool_type_id(), &scope), "Bool into an Int|Bool sum is AsBool");
        assert!(!type_admits(ids[1], int_type_id(), &scope), "Int into Int|Int|Int names no variant");
        assert!(!type_admits(ids[2], int_type_id(), &scope), "Int is ambiguous when two variants carry it");
        assert!(type_admits(ids[2], text_id, &scope), "Text is carried by one variant only");
        assert!(!type_admits(ids[0], text_id, &scope), "a type no variant carries is not admitted");
    }

    #[test]
    fn irreversible_callback_is_gated_like_a_direct_call() {
        // An Irreversible callee handed to a HOF must face the same
        // gate-domination check a direct Irreversible call does — passing it
        // through a callback slot must not be a way around the gate.
        let (scope, hof, callee) = callback_scope(
            vec![int_type_id()],
            unit_type_id(),
            EffectClass::Irreversible,
            Some(int_to_unit_contract()),
        );
        let err = verify_bundle(
            &callback_bundle(hof, callee),
            &scope,
            &HashMap::new(),
            EffectClass::ANY,
        )
        .expect_err("an ungated Irreversible callback must be rejected");
        assert_eq!(err, VerifyError::IrreversibleNotGated { node_index: 0 });
    }
}
