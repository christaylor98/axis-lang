# Phase 6b Gate Result: Composite Derivation — STILL DEFERRED

**Gate checked:** 2026-06-06  
**Outcome:** GATE FAILS — composite bodies are not resolvable  
**Code changes:** NONE (correct outcome per spec)

---

## Gate Question

Does a `CompositeBody.body` hash resolve to a real `CoreBundle` in the active scope?

## Verdict: NO

The only body-resolution path in the codebase is:

```
Scope::resolve_fn(identity)
  -> ResolvedFn { facts: Facts::PendingDerivation { body: Hash256 } }
```

This yields a `Hash256` — the hash stored in `CompositeBody`. It does **not** yield a `CoreBundle`. There is no `HashMap<Hash256, CoreBundle>` or equivalent body store anywhere:

- Not in `Scope` (`src/registry/core05/fold.rs`)
- Not in `PipelineConfig` or `PipelineArtifacts` (`src/pipeline/mod.rs`)
- Not passed to `verify_bundle` (`src/validation/core_ir.rs:118`)
- Not introduced by Phase 6 (bridge) work

The `verify_bundle` signature confirms this:

```rust
pub fn verify_bundle(
    bundle: &CoreBundle,
    scope: &Scope,
    max_allowed_effect: EffectClass,
) -> Result<VerifyPass, VerifyError>
```

No body store is accepted. Even if one existed, the verifier has no channel to receive it.

The `axis.axreg` registry (the project's primary registry) contains only leaf functions — no composites are registered at all.

`fold.rs` lines 60–65 document this explicitly:

> "Phase 1 stores `CompositeBody` but cannot walk Core IR 0.5 graphs yet, so composite resolution returns `Facts::PendingDerivation` — never a guessed default."

---

## Consequence

The fail-closed branch in `src/validation/core_ir.rs:180–192` is **correct as written**:

```rust
Facts::PendingDerivation { body } => {
    return Err(VerifyError::CompositeEffectUnderivable {
        node_index: i,
        target: *target_identity,
        reason: format!(
            "cannot derive effect class: composite body {} not resolvable",
            body.to_hex()
        ),
    });
}
```

Two tests already guard this:
- `fail_composite_effect_underivable` (line 581)
- `fail_composite_fails_closed_not_assumed_pure` (line 601)

No change is warranted.

---

## What Is Required to Unblock This Phase

Three coordinated pieces must exist before proceeding. None may be fabricated.

**1. A body store**  
A `HashMap<Hash256, CoreBundle>` (or equivalent) populated when a `CoreBundle` is lowered. The key must be `bundle_identity(bundle)` from `src/ir/core_ir_05.rs`. The store must be accessible at `verify_bundle` call sites.

**2. Interface change to `verify_bundle`**  
The store must be passed into the verifier so the `PendingDerivation` branch can attempt a lookup:

```rust
pub fn verify_bundle(
    bundle:             &CoreBundle,
    scope:              &Scope,
    body_store:         &HashMap<Hash256, CoreBundle>,  // new
    max_allowed_effect: EffectClass,
) -> Result<VerifyPass, VerifyError>
```

**3. Real composite registrations**  
At least one composite fn must appear in a loaded registry with its `CompositeBody.body` hash equal to the `bundle_identity` of a `CoreBundle` that is present in the body store. Phase 1 stubs (`body: ZERO` or arbitrary sentinel hashes) do not satisfy this — they are not in any store.

---

## Algorithm (ready when gate unblocks)

The derivation algorithm is already designed — see `intent-phase6b-composite-derivation.lisp` (working context) and `fold.rs::combine_component_facts` (the combination step is already implemented):

```
derive(identity, scope, body_store, seen):
  if identity in seen      -> CYCLE: CompositeEffectUnderivable
  resolved = scope.resolve_fn(identity)
  Leaf(lf)                -> return lf.effect_class
  PendingDerivation{body} ->
    bundle = body_store.get(body)
             or CompositeEffectUnderivable (unresolvable body)
    components = CCall target_identities in bundle.nodes
    return MAX over { derive(t, scope, body_store, seen ∪ {identity}) }

deterministic = AND over components
idempotent    = conservatively false
```

`combine_component_facts` in `fold.rs:318` already implements the `MAX / AND / conservative-false` combination. The only missing pieces are the store and the interface wiring.
