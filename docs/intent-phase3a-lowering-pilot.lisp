;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_3A_LOWERING_PILOT
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_LOWERING_PILOT)
    (owner chris)
    (scope
      "Port ONE lowering path — minimal_lowering — to emit a Core IR 0.5
       CoreBundle via the Phase-2 builder, proving the surface -> builder ->
       registry -> canonical-bundle seam end to end on the simplest case.
       INCLUDES: a 0.5 lowering entrypoint for the minimal surface that produces
       a CoreBundle, with literals -> constant pool, shared values -> shared
       node indices, and calls to known callables -> CCall by registry identity.
       EXCLUDES: the closure-conversion / lambda-lift pass (Phase 3b), the other
       six lowering paths and introspection retarget (Phase 3c), retiring legacy
       CoreTerm (Phase 3d), the Verifier (Phase 4), and the rust-bridge swap."))

  (goal
    (primary "minimal_lowering emits a valid, canonical-hashable Core IR 0.5 bundle
              whose CCall targets resolve in an active core05 registry scope.")
    (secondary "Prove the seam: surface input -> 0.5 builder -> core05 resolution ->
                CoreBundle -> stable canonical hash, with a round-trip test."
               "Keep the crate compiling; keep fabric + core05 + core_ir_05 tests green."
               "Leave the legacy minimal_lowering path intact and working (coexistence).")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (prove-the-seam high)
    (spec-fidelity high)
    (crate-stays-building high)
    (breadth low))   ;; one path only; resist generalising early

  (constraint
    (hard-limit "Lower to EXACTLY the two node kinds (CCall, CIf) via the Phase-2 builder.
                 Do not construct CoreBundle/Node by hand; go through Builder so the
                 topological + dedupe + UNKNOWN-gate invariants are enforced.")
    (hard-limit "Literals become constant-pool entries (intern_constant), referenced by
                 NodeRef::Pool. No literal nodes. Payloads use the Registry Format §10 encoding
                 already provided by fabric/core05 — do not invent a payload format.")
    (hard-limit "A value used more than once is ONE node/pool entry referenced by the same
                 index (structural sharing). Let/var-style reuse must collapse, not duplicate.")
    (hard-limit "A call lowers to CCall with target_identity: Hash256 resolved from the active
                 core05 scope by the callable's name. Names are resolved at lowering time and
                 NEVER appear in the emitted bundle.")
    (hard-limit "An unresolved callable is a HARD HALT at lowering (surface error / UNKNOWN gate),
                 never a default, never a synthesised identity.")
    (hard-limit "Reuse src/fabric, src/registry/core05, and src/ir/core_ir_05 (builder +
                 serializer). Introduce NO new hash, payload, or serialization regime.")
    (hard-limit "Do not modify the legacy minimal_lowering output path, the legacy CoreTerm
                 model, the 0.3 capnp path, or rust-bridge/. The 0.5 path is ADDITIVE."))

  (risk
    (handwritten-bundle-bypassing-builder critical)      ;; invariants not enforced
    (name-leaking-into-bundle high)                      ;; dispatch by name
    (duplicated-shared-values high)                      ;; sharing not structural
    (unknown-callable-silently-defaulted critical)        ;; guessed identity
    (premature-generalisation medium)                    ;; building for 7 paths instead of 1
    (no-registry-fixture-to-resolve-against medium))     ;; pilot needs known identities

  (boundary
    (add    "a 0.5 lowering entrypoint for the minimal surface (new fn/module, additive)" allowed)
    (add    "pilot unit tests + a minimal .axreg scope the calls resolve against" allowed)
    (use    "src/registry/core05 for name->identity resolution and the UNKNOWN gate" allowed)
    (use    "src/ir/core_ir_05 Builder + canonical serializer" allowed)
    (modify "src/lowering/minimal_lowering.rs ONLY to add the additive 0.5 entrypoint
             alongside the existing one" allowed)
    (modify "the legacy minimal_lowering output, legacy CoreTerm, 0.3 capnp path" forbidden)
    (modify "the other six lowering paths, src/introspection/**" forbidden)
    (implement "closure-conversion / lambda-lift (Phase 3b)" forbidden)
    (implement "the Verifier / per-arg def_hash type-check (Phase 4)" forbidden)
    (change "encoding/NodeRef/hash decisions locked in fabric + core_ir_05" forbidden)
    (default forbidden))

  (assumption
    (statement "The minimal surface is first-order: every callable is already a registry
                entry, so no closure-conversion is needed to lower it. If a higher-order
                construct is encountered, STOP and report — it belongs to Phase 3b, not here." tentative)
    (statement "A small hand-authored .axreg (reusing the core05 text loader) supplies the
                identities the pilot's CCalls resolve against; minting happens in that fixture,
                never at lowering time." tentative)
    (statement "Per-argument def_hash type-checking is the Verifier's job (Phase 4); the pilot
                only needs target_identity to resolve and the bundle to be structurally valid." tentative))

  (outcome
    (success "Lowering a representative minimal-surface input yields a CoreBundle that
              serializes canonically and produces a stable bundle identity hash."
             "Every CCall in the output carries a target_identity present in the active core05
              scope; no names appear anywhere in the bundle."
             "A literal used twice appears once in the constant pool, referenced by the same
              NodeRef::Pool index; a reused computed value is one node with shared referrers."
             "Lowering a call to an unknown callable fails fast with the UNKNOWN error."
             "The new 0.5 entrypoint and the legacy entrypoint coexist; legacy output is unchanged."
             "cargo build and cargo test pass for the whole crate; fabric + core05 + core_ir_05
              tests still green.")
    (failure "A bundle is constructed without the builder, or a name leaks into the bundle, or a
              shared value is duplicated, or an unknown callable is lowered to a default/synthesised
              identity, or the pilot generalises beyond the minimal path."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Specs:    core_ir_spec/axis_core_ir_0_5.md  (model; §Abstraction and Lowering)
  ;;           core_spec/Registry-WorkingDraft.md, Registry-Format-WorkingDraft.md §10
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 3, step 3a)
  ;; Existing: src/fabric/{hash,codec}.rs                  (Phase 0)
  ;;           src/registry/core05/{record,codec,fold,text}.rs  (Phase 1 — resolution + .axreg loader)
  ;;           src/ir/core_ir_05.rs                         (Phase 2 — Builder + serializer + capnp 0.5)
  ;;           registries/core05_example.axreg              (Phase 1 fixture; pattern for the pilot scope)
  ;; Legacy:   src/lowering/minimal_lowering.rs (legacy CoreTerm path) — keep working, add alongside.
  ;;
  ;; --- SEAM TO PROVE (the whole point of 3a) ---
  ;;   minimal surface AST
  ;;     -> 0.5 lowering (this phase)
  ;;        * literal      -> Builder::intern_constant(def_hash, payload §10) -> NodeRef::Pool
  ;;        * shared value -> single index, referenced multiple times
  ;;        * call f(args)  -> resolve "f" in core05 Scope -> Hash256 -> Builder CCall
  ;;        * conditional   -> Builder CIf { cond, then_, else_ }
  ;;     -> Builder::finish() -> CoreBundle
  ;;     -> serialize_canonical -> content_hash -> stable bundle identity
  ;;   Next (do NOT do here): Phase 3b lambda-lift, 3c remaining paths, 3d retire CoreTerm.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_LOWERING_PILOT constraints and boundary;
     go-through-the-builder, identity-not-name, structural-sharing, UNKNOWN-is-a-hard-halt,
     and one-path-only are non-negotiable.")
)
