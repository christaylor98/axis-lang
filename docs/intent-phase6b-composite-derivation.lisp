;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_6B_COMPOSITE_DERIVATION
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; - GATED PHASE: the resolvability gate (mode.gate) decides whether this phase
;;   writes code at all. If composite bodies are NOT resolvable, the correct
;;   outcome is a documented "still deferred" report and NO code change.
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_COMPOSITE_DERIVATION)
    (owner chris)
    (scope
      "Close the last deferred Phase-6 deliverable: derive a composite function's
       facts from its body graph instead of fail-closing — BUT ONLY IF composite
       bodies are genuinely resolvable to a CoreBundle now. Today the Verifier
       returns CompositeEffectUnderivable for any PendingDerivation composite
       (src/validation/core_ir.rs); Phase-1 registered CompositeBody { body: ZERO }
       stubs, so bodies may still be unresolvable.
       INCLUDES: a GATING check of whether a CompositeBody.body hash resolves to a
       real CoreBundle in scope; if yes — a recursive derivation
       (effect_class = MAX over body CCall components, deterministic = AND,
       idempotent = conservative false) with cycle-safe memoisation, replacing the
       fail-closed path where the body resolves; if no — a documented confirmation
       that fail-closed is correct and this stays deferred.
       EXCLUDES: changing any other Verifier verdict rule, the lowering/builder/
       registry/bridge behaviour, and inventing a body-storage mechanism that does
       not already exist (if bodies are stubs, that is the gate's answer)."))

  (goal
    (primary "Where a composite's body resolves to a CoreBundle, the Verifier DERIVES its effect
              class from the body (max over components) rather than fail-closing.")
    (secondary "Where a body does NOT resolve, the Verifier still FAILS CLOSED — never assumes pure."
               "Recursion over composites-calling-composites is cycle-safe and memoised."
               "Keep the crate compiling; keep all prior-phase tests green; additive only.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (soundness high)              ;; a false 'pure' is the dangerous direction
    (resolve-the-gate-first high)
    (spec-fidelity high)
    (scope-discipline high))      ;; do NOT build body storage if it doesn't exist

  (constraint
    (hard-limit "GATE FIRST: determine whether a CompositeBody.body hash resolves to a real
                 CoreBundle in the active scope. If composite bodies are still ZERO stubs / not
                 resolvable, STOP: document that fail-closed remains correct, change NO code, and
                 report what would be needed to make bodies resolvable. Do NOT fabricate a body
                 store to satisfy this phase.")
    (hard-limit "If bodies resolve: effect_class of a composite = MAX over the effect classes of its
                 body graph's CCall targets, recursively (leaf => LeafFacts; composite => recurse).
                 deterministic = AND (conjunction) over components; idempotent = conservatively false
                 unless provably derivable.")
    (hard-limit "An UNRESOLVABLE body — at any depth of the recursion — FAILS CLOSED with a precise
                 report. Never assume pure for a body that cannot be walked.")
    (hard-limit "Recursion is cycle-safe: memoise derived facts by function identity and detect a
                 composite-call cycle; a cycle is a hard FAIL (or a defined conservative ceiling),
                 NEVER an assumed-pure shortcut.")
    (hard-limit "Derived facts are NON-AUTHORITATIVE cache: the body graph is the truth. Do not read
                 a stored composite fact as authority; compute it.")
    (hard-limit "Additive and surgical: modify ONLY the composite branch of effect derivation in
                 src/validation/core_ir.rs (and the minimal resolution helper it needs). Do NOT change
                 leaf derivation, the type-check, the UNKNOWN gate, or any other verdict rule.")
    (hard-limit "Reuse fabric, core05 (resolution), core_ir_05 (CoreBundle/flat-table walk). Introduce
                 NO new hash/identity/serialization regime and do NOT touch the bridge or lowering."))

  (risk
    (assuming-pure-on-unresolvable-body critical)        ;; the exact thing Phase 4 guarded
    (cycle-causes-infinite-recursion-or-false-pure critical)
    (fabricating-a-body-store-to-pass-the-gate high)     ;; scope violation; masks reality
    (changing-an-unrelated-verdict-rule high)
    (cache-read-as-authority medium))

  (boundary
    (probe  "whether CompositeBody.body resolves to a CoreBundle in scope (the gate)" allowed)
    (modify "src/validation/core_ir.rs — the composite effect-derivation branch ONLY" allowed)
    (add    "a minimal body-hash -> CoreBundle resolution helper IF such bodies already exist" allowed)
    (add    "derivation unit tests: leaf-only composite, composite-of-composite, unresolvable-body
             fail-closed, cycle detection, max/AND/conservative-idempotent correctness" allowed)
    (modify "leaf derivation, the type-check, the UNKNOWN gate, other Verifier verdict rules" forbidden)
    (modify "lowering, the builder, core05 record semantics, the bridge" forbidden)
    (fabricate "a composite body store / synthetic bodies to make the gate pass" forbidden)
    (change "encoding/identity decisions locked in fabric + core_ir_05 + core05" forbidden)
    (default forbidden))

  (assumption
    (statement "If Phase 6 did not wire real composite bodies (bodies remain ZERO stubs), the gate
                fails and this phase correctly produces a 'still deferred' report with zero code
                change — that is a SUCCESS, not a failure, of this spec." tentative)
    (statement "idempotent for a composite stays conservatively false; no consumer yet needs a
                derived-true, and false is the safe default (Registry §Bridge Facts)." tentative)
    (statement "A composite-call cycle is pathological for the current first-order surfaces; treat
                it as a hard FAIL with a clear message rather than a silent ceiling, unless Chris
                prefers a defined conservative ceiling." tentative))

  (outcome
    (success "GATE result is documented either way: bodies resolve (proceed) or do not (stop, deferred)."
             "IF proceeding: a composite whose body resolves gets effect_class = max over its
              components; a composite-of-composite derives correctly through recursion."
             "An unresolvable body at any depth FAILS CLOSED with a precise report; no assumed pure."
             "A composite-call cycle is detected and handled (hard FAIL or defined ceiling), never
              infinite recursion and never assumed pure."
             "Leaf derivation, the type-check, and the UNKNOWN gate are unchanged."
             "cargo build and cargo test pass for the whole crate; all prior-phase tests still green."
             "IF stopping: a written note states fail-closed remains correct and what body-resolution
              wiring would be required, with NO code change.")
    (failure "An unresolvable or cyclic body is treated as pure, a body store was fabricated to pass
              the gate, an unrelated verdict rule changed, or derivation read a stored fact as
              authority."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (gate composite-body-resolvability-must-be-determined-first)
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Plan:     docs/core-0.5-migration-plan.md (Phase 6 follow-on); docs/intent-phase4-verifier.lisp
  ;;           (the fail-closed constraint this phase conditionally lifts);
  ;;           docs/intent-phase6-bridge-swap.lisp (listed composite wiring as a deliverable).
  ;; Specs:    core_spec/Registry-WorkingDraft.md §Bridge Facts:
  ;;             effect_class: pure<reads<writes<fullIo; composite = MAX over components
  ;;             deterministic: composite = conjunction (AND) over components
  ;;             idempotent: composite = conservatively false unless provably derivable
  ;;           core_ir_spec/axis_core_ir_0_5.md (§Side Effects; flat-table walk for components)
  ;; Code today (the gate's starting point):
  ;;   src/validation/core_ir.rs — Facts::PendingDerivation { body } => CompositeEffectUnderivable
  ;;     (FAIL CLOSED at lines ~178–188). This is the branch this phase conditionally replaces.
  ;;   src/registry/core05/* — CompositeBody { identity, body: Hash256 }; resolve_fn -> Facts.
  ;;     Phase-1 registered body: ZERO stubs — CHECK whether real bodies exist now.
  ;;   src/ir/core_ir_05.rs — CoreBundle flat table; walk CCall targets as components.
  ;;
  ;; --- DERIVATION (only if the gate passes) ---
  ;;   derive(identity, scope, seen):
  ;;     if identity in seen        -> CYCLE: hard FAIL (or defined ceiling), never pure
  ;;     facts = resolve_fn(identity)
  ;;     leaf  -> return LeafFacts.effect_class
  ;;     composite -> bundle = resolve_body(body) or FAIL CLOSED
  ;;                  return MAX over { derive(t, scope, seen+identity) : t in bundle CCall targets }
  ;;   memoise by identity; deterministic = AND over the same components; idempotent = false.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_COMPOSITE_DERIVATION constraints and boundary;
     gate-decides-whether-to-code, never-assume-pure (unresolvable OR cyclic), don't-fabricate-a-body-store,
     surgical-change-to-the-composite-branch-only, and graph-is-truth are non-negotiable.")
)
