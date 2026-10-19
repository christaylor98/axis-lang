;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_3B_LAMBDA_LIFT
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_LAMBDA_LIFT)
    (owner chris)
    (scope
      "Implement the mandatory closure-conversion / lambda-lift pass that closes
       the callable space BEFORE Core IR 0.5 lowering: every anonymous function
       becomes a named, registry-resident function with a synthesised identity,
       and every captured variable becomes an explicit argument. This resolves
       the higher-order constructs that the Phase-3a pilot rejected
       (IdentNotSupported and friends).
       INCLUDES: free-variable analysis, lambda-lifting to synthesised FnGenesis
       entries registered into the active core05 scope, capture -> explicit-arg
       rewriting, and integration so the 0.5 lowering emits a fully closed
       (Tier 1/2) graph with no inline abstraction.
       EXCLUDES: Tier 3 runtime closures / dynamic-apply (detect-and-halt only),
       the remaining six lowering paths (Phase 3c), retiring legacy CoreTerm
       (Phase 3d), the Verifier (Phase 4), and the rust-bridge swap."))

  (goal
    (primary "After this pass, the surface program contains NO inline abstraction:
              every callable is a registry-resident function with a 256-bit identity,
              so 0.5 lowering produces a closed CCall/CIf graph.")
    (secondary "Lift correctly: captured free variables become leading explicit args;
                the lifted call sites pass exactly those captures."
               "Synthesised functions are registered into the core05 scope so their
                CCall targets resolve through the existing UNKNOWN gate."
               "Keep the crate compiling; keep all prior-phase tests green; legacy
                CoreTerm path untouched.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (correctness-of-capture-analysis high)   ;; a missed capture is a wrong program
    (spec-fidelity high)
    (crate-stays-building high)
    (breadth low))   ;; Tier 1->2 only; do not chase Tier 3

  (constraint
    (hard-limit "Every anonymous function is lifted to a NAMED registry function with a
                 synthesised 256-bit identity. No CLam/lambda/closure construct may survive
                 into the 0.5 builder input. Core IR 0.5 has no inline abstraction.")
    (hard-limit "Free-variable analysis must be COMPLETE: every variable free in a lambda body
                 becomes an explicit parameter, prepended to the lambda's own params in a fixed,
                 deterministic order. The lifted call site supplies exactly those captures.")
    (hard-limit "A synthesised function identity is a MINTED token (entropy at width via
                 fabric::mint_identity), NOT derived from the lambda's content/signature.
                 This is a compile-time mint event — legitimate and distinct from .axreg load,
                 which still never mints.")
    (hard-limit "Synthesised FnGenesis records are registered into the active core05 scope so
                 their CCall targets resolve. An unresolved target after lifting is a HARD HALT,
                 never a default.")
    (hard-limit "Within a single lowering run, two structurally identical lambdas in the SAME
                 capture context must lift to ONE synthesised function (dedupe), so the emitted
                 bundle does not carry redundant identities.")
    (hard-limit "Reuse src/fabric, src/registry/core05, src/ir/core_ir_05, and the Phase-3a
                 0.5 lowering entrypoint. Introduce NO new hash/identity/serialization regime.")
    (hard-limit "Do not modify the legacy CoreTerm model, the 0.3 capnp path, or rust-bridge/.
                 The pass and its wiring are ADDITIVE."))

  (risk
    (incomplete-free-variable-capture critical)          ;; silently wrong program
    (capture-order-nondeterministic high)                ;; param order varies -> unstable lift
    (tier3-closure-silently-mishandled critical)         ;; runtime closure treated as static
    (synthesised-identity-content-derived high)          ;; violates two-hash model
    (redundant-identities-not-deduped medium)
    (lambda-surviving-into-builder critical))            ;; inline abstraction reaches Core IR

  (boundary
    (add    "the lambda-lift / closure-conversion pass (new module, additive)" allowed)
    (add    "synthesised-FnGenesis registration into a working core05 scope" allowed)
    (add    "pass unit tests + lowering tests for higher-order cases" allowed)
    (modify "the Phase-3a 0.5 lowering entrypoint to run the pass and handle lifted constructs" allowed)
    (use    "fabric::mint_identity for synthesised identities; core05 for registration/resolution" allowed)
    (detect-and-halt "Tier 3: a closure invoked through a dynamic-apply mechanism, where which
                      closure flows where is runtime-determined — report a distinct error, do NOT lift" allowed)
    (modify "legacy CoreTerm, 0.3 capnp path, the other six lowering paths, src/introspection/**" forbidden)
    (implement "Tier 3 dynamic-apply support, the Verifier (Phase 4)" forbidden)
    (change "encoding/identity/NodeRef decisions locked in fabric + core_ir_05 + core05" forbidden)
    (default forbidden))

  ;; --- OPEN QUESTION surfaced for confirmation (does NOT block; assumption below is the
  ;;     working resolution until Chris rules) ---
  (unknown
    "Cross-compile stability of synthesised identities: minting fresh each compile makes the
     bundle hash non-reproducible across builds (CCall.target_identity changes), which weakens
     content-addressing. Recommended resolution (option c): mint once and PERSIST a sidecar map
     keyed by a stable construct key (e.g. canonical source location + capture signature), reused
     on later compiles — identity stays MINTED (not content-derived, spec-faithful) yet stable
     across builds. Confirm before relying on reproducible cross-build bundle hashes.")

  (assumption
    (statement "Working resolution for the open question: within a single compile, mint once per
                distinct (lambda, capture-context) and dedupe; cross-compile persistence (sidecar)
                is RECOMMENDED but may be deferred — until confirmed, bundle hashes are stable
                within a build but not guaranteed across builds." tentative)
    (statement "The target surface(s) are Tier 1/2: higher-order with statically-known captures.
                Genuine Tier 3 (runtime closure through dynamic-apply) is detect-and-halt here." tentative)
    (statement "Per-argument def_hash type-checking of the lifted captures is the Verifier's job
                (Phase 4); the pass need only produce structurally valid, resolvable CCalls." tentative))

  (outcome
    (success "A higher-order minimal-surface input lowers end-to-end: lambdas are lifted to named
              registry functions, captures flow as leading args, and the 0.5 builder emits a closed
              CCall/CIf bundle with a stable (within-build) canonical identity hash."
             "Every lifted CCall target_identity resolves in the active core05 scope; no lambda or
              name survives into the bundle."
             "Free-variable capture is complete and order-deterministic: re-running the pass on the
              same input yields the same parameter order and the same lift structure."
             "Two identical lambdas in the same capture context lift to one synthesised identity."
             "A Tier 3 dynamic-apply closure produces a distinct detect-and-halt error, not a lift."
             "Phase-3a first-order lowering is unchanged; legacy CoreTerm path untouched."
             "cargo build and cargo test pass for the whole crate; all prior-phase tests still green.")
    (failure "A capture is missed, a lambda survives into the builder, a synthesised identity is
              derived from content, a Tier 3 closure is silently lifted, or capture/param order is
              non-deterministic."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Specs:    core_ir_spec/axis_core_ir_0_5.md  (§Abstraction and Lowering; §Guarantee Tiers 1/2/3)
  ;;           core_spec/Registry-WorkingDraft.md (function identity is minted, never derived)
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 3, step 3b)
  ;; Existing: src/fabric/{hash,codec}.rs                  (mint_identity, content_hash)
  ;;           src/registry/core05/{record,codec,fold,text}.rs  (FnGenesis, Scope, resolution)
  ;;           src/ir/core_ir_05.rs                         (Builder + serializer)
  ;;           src/lowering/minimal_lowering.rs             (Phase-3a 0.5 entrypoint; rejected Ident)
  ;;           registries/pilot_lowering.axreg              (Phase-3a fixture pattern)
  ;;
  ;; --- LIFT CONTRACT (the transform) ---
  ;;   lambda  \p1..pk. body   with free vars  c1..cm  (deterministic order)
  ;;     -> synthesised fn  F(c1..cm, p1..pk) = body'    ; minted identity, FnGenesis into scope
  ;;     -> use site (direct application)  (\..)(a1..ak)  -> CCall F [c1..cm, a1..ak]
  ;;     -> use site (lambda as value, statically resolvable) -> reference to F's identity + captures
  ;;     -> use site (runtime-determined closure, dynamic-apply) -> DETECT-AND-HALT (Tier 3, deferred)
  ;;   After the pass: no inline abstraction remains; hand the closed form to the Phase-3a builder path.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_LAMBDA_LIFT constraints and boundary;
     complete-capture, deterministic-order, minted-not-derived identity, Tier-3-is-halt, and
     no-lambda-reaches-the-builder are non-negotiable.")
)
