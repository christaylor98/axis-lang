;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_4_VERIFIER
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_VERIFIER)
    (owner chris)
    (scope
      "Implement the static Core IR 0.5 Verifier: the admission check over a
       CoreBundle against an active core05 registry scope. It does not execute.
       INCLUDES: (1) resolve every CCall target — unresolved = UNKNOWN hard FAIL;
       (2) the per-argument def_hash TYPE-CHECK at every CCall site (deferred
       since Phase 2) — each argument's value def_hash MUST match the callee's
       frozen FnGenesis input signature, else hard FAIL; (3) derive the bundle's
       effect class (max over CCall target effect profiles) and apply a supplied
       consumer effect policy — denied class = hard FAIL; (4) re-validate the
       structural invariants (topological order, NodeRef ranges) as a gate.
       EXCLUDES: the trust gate / provenance handling and ledger admission-event
       emission (no provenance or ledger model exists yet — deferred); execution
       of any kind; the rust-bridge swap; Phase 5 pipeline/CLI finalisation."))

  (goal
    (primary "A Verifier function that takes a CoreBundle + active core05 scope (+ an
              effect policy) and returns PASS with the derived facts, or a precise hard
              FAIL naming the failing CCall/argument and the reason.")
    (secondary "Land the per-argument def_hash type-check that Phases 2–3 deferred."
               "Derive effect class from the graph (authoritative), never trust a stored value."
               "Keep the crate compiling; keep all prior-phase tests green; additive only.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (soundness high)               ;; a false PASS is the dangerous direction
    (spec-fidelity high)
    (crate-stays-building high)
    (performance low))

  (constraint
    (hard-limit "The Verifier does NOT execute and has NO side effects. It reads the static
                 artifact and returns a verdict.")
    (hard-limit "Every CCall target_identity MUST resolve in the active scope; an unresolved
                 identity is a hard FAIL (UNKNOWN gate) — never a default, never a guess.")
    (hard-limit "Per-argument type-check: compute each argument value's def_hash (pool entry =>
                 its def_hash; node result => the resolved output type of that node) and require
                 it to EQUAL the callee FnGenesis input def_hash at that position. Arity or
                 def_hash mismatch is a hard FAIL naming the CCall index and argument position.")
    (hard-limit "Effect class is DERIVED from the graph's CCall targets (max over their effect
                 profiles), never read from a stored/declared bundle field. Any cached value is
                 non-authoritative; the graph is the truth.")
    (hard-limit "On any check that cannot be completed soundly (e.g. an unresolvable composite
                 body for effect derivation), FAIL CLOSED with a precise report — never PASS by
                 assuming the benign case.")
    (hard-limit "Composite effect derivation is FAIL-CLOSED in this phase. Leaf effect derivation
                 lands now (LeafFacts.effect_class). For a composite target, the body is only a
                 content hash and composite bodies are currently ZERO stubs with no in-scope
                 body-hash -> CoreBundle resolution, so recursive derivation CANNOT run: reject with
                 'cannot derive effect class: composite body not resolvable', never assume pure.
                 Recursive composite derivation is DEFERRED to the phase that wires real bodies.")
    (hard-limit "Reuse src/fabric, src/registry/core05 (resolution + FnGenesis signatures +
                 LeafFacts), and src/ir/core_ir_05 (CoreBundle, flat table). Introduce NO new
                 hash/identity/serialization regime.")
    (hard-limit "Additive only: do not modify the 0.5 lowering/builder/serializer behaviour or
                 any prior-phase path. The Verifier consumes their output."))

  (risk
    (false-pass-on-type-mismatch critical)        ;; unsound admission
    (effect-class-read-from-cache-not-derived high)
    (composite-effect-silently-assumed-pure critical)  ;; PendingDerivation treated as pure
    (fail-open-on-incomplete-check critical)
    (cif-branch-type-mismatch-unchecked medium)   ;; then/else type divergence
    (touching-05-paths-during-verify medium))

  (boundary
    (add    "the Verifier module (new, additive) + its result/error types" allowed)
    (add    "a per-node result-type computation over the flat table (linear, topological)" allowed)
    (add    "effect-class derivation (max over CCall targets; composites per the decision below)" allowed)
    (add    "an effect-policy input (max-allowed class) the Verifier enforces" allowed)
    (add    "Verifier unit tests (pass + every hard-fail mode) and fixtures" allowed)
    (use    "core05 FnGenesis.inputs/output + LeafFacts.effect_class; core_ir_05 CoreBundle" allowed)
    (modify "the 0.5 lowering/builder/serializer/introspection behaviour" forbidden)
    (implement "the trust gate / provenance handling, ledger emission" forbidden)
    (implement "execution / evaluation of the bundle, the rust-bridge swap" forbidden)
    (change "encoding/identity/NodeRef decisions locked in fabric + core_ir_05 + core05" forbidden)
    (default forbidden))

  ;; Composite effect-class derivation: RESOLVED — fail-closed this phase (see the composite
  ;; effect hard-limit above). Recursive body-walking is deferred to the phase that wires real
  ;; composite bodies, since bodies are currently ZERO stubs with no in-scope resolution.

  (assumption
    (statement "Bootstrap identity exception: per axreg-text-projection-reference.md §5b, compiler
                primitive registries MAY author identity = SHA-256(function-name). This is a
                documented, scope-limited exception to 'identity never derived from content' (it
                trades strict nominality for round-trip determinism on stable-named primitives) and
                must NOT be generalised to application registries. The Verifier treats all identities
                uniformly regardless of how they were authored." tentative)
    (statement "Trust gate (provenance) and ledger admission-event emission are DEFERRED — no
                provenance or ledger model exists. The Verifier returns the derived facts (effect
                class, resolved closure tier if cheap, referenced def_hashes) as a structured PASS
                result a future ledger layer can record; it does not persist anything." tentative)
    (statement "A CIf requires its then/else result types to be compatible; treat a then/else
                def_hash divergence as a hard FAIL unless the spec's join rule says otherwise —
                if unclear, report rather than guess." tentative))

  (outcome
    (success "Verifying a well-typed bundle whose targets all resolve returns PASS with the
              derived effect class and the set of referenced def_hashes/identities."
             "An argument whose def_hash does not match the callee's FnGenesis input signature
              yields a hard FAIL naming the CCall index and argument position."
             "An unresolved CCall target yields the UNKNOWN hard FAIL."
             "Effect class is computed from the graph; a deliberately-wrong cached value (if any)
              is ignored."
             "A bundle whose derived effect class exceeds the supplied policy is rejected."
             "A composite target whose effect class cannot be derived FAILS CLOSED, never PASSes
              as pure."
             "cargo build and cargo test pass for the whole crate; all prior-phase tests still green.")
    (failure "A type mismatch passes, effect class is read from a cache, a PendingDerivation
              composite is assumed pure, any incomplete check fails open, or a 0.5 path was modified."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Specs:    core_ir_spec/axis_core_ir_0_5.md  (§Verifier Behaviour — steps 1–5; §Side Effects;
  ;;                                               §Static Analysis: Blast Radius)
  ;;           core_spec/Registry-WorkingDraft.md  (§Validation Behaviour; §Bridge Facts: effect_class
  ;;                                                 pure<reads<writes<fullIo, composite = max over components)
  ;;           core_spec/axreg-text-projection-reference.md  (NORMATIVE .axreg grammar §§2–4; primitive
  ;;                                                 codes; effect vocabulary; §5b bootstrap identity;
  ;;                                                 §7 impl/contract validation is downstream)
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 4)
  ;; Reuse:    src/fabric/*; src/registry/core05/* (resolve_fn -> FnGenesis.inputs/output, LeafFacts,
  ;;           composite Facts::PendingDerivation); src/ir/core_ir_05.rs (CoreBundle, Node, NodeRef).
  ;; Note:     blast-radius forward/backward scans already exist in the Phase-3c introspection
  ;;           walkers — reuse, do not duplicate; the Verifier's job is admission, not analysis UI.
  ;;
  ;; --- VERIFIER STEPS (spec §Verifier Behaviour, ledger/trust deferred) ---
  ;;   0. Structural gate: topological order holds; every NodeRef in range.
  ;;   1. Resolve: every CCall.target_identity resolves -> else UNKNOWN hard FAIL.
  ;;   2. Type-check: walk nodes in index order computing each node's result def_hash;
  ;;      at each CCall, arg def_hashes (by position) MUST equal callee FnGenesis.inputs -> else FAIL.
  ;;   3. Effect: derive class = max over CCall targets' effect_class (leaf = LeafFacts declared;
  ;;      composite = FAIL CLOSED this phase, body not resolvable); apply consumer policy ->
  ;;      denied class hard FAIL.
  ;;   4. (DEFERRED) trust gate + ledger admission event — return derived facts for a future layer.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_VERIFIER constraints and boundary;
     no-execution, UNKNOWN-is-hard-fail, def_hash-must-match, effect-is-derived-not-stored, and
     fail-closed-never-assume-pure are non-negotiable.")
)
