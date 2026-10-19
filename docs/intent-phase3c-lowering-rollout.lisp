;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_3C_LOWERING_ROLLOUT
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_LOWERING_ROLLOUT)
    (owner chris)
    (scope
      "Roll out the proven 0.5 lowering machinery (Phase-3a builder path +
       Phase-3b lambda-lift) to the REMAINING lowering paths, and retarget the
       introspection walkers to the flat 0.5 node table.
       INCLUDES: 0.5 lowering entrypoints for h1_lowering, registry_lowering,
       schema_lowering, spec_driven, and nf_lowering; and retargeting the four
       introspection consumers (inspection, code_trace, trust, core_ir_json) to
       read CoreBundle (flat table) instead of the recursive CoreTerm tree.
       EXCLUDES: retiring legacy CoreTerm / 0.3 capnp (Phase 3d), the Verifier
       (Phase 4), pipeline/CLI/fixtures cleanup (Phase 5), and the bridge swap.
       The tree-walking interpreters (execution/interpreter, lowering/interpreter)
       are NOT in scope here — their fate is decided in Phase 3d/5."))

  (goal
    (primary "Every remaining lowering path can emit a valid, canonical Core IR 0.5
              bundle through the Builder, reusing the 3a/3b machinery — no path
              constructs nodes by hand or reinvents the lift.")
    (secondary "The four introspection walkers read the flat 0.5 table and produce
                correct output (these get SIMPLER — linear scans, not tree recursion)."
               "Keep the crate compiling; keep ALL prior-phase tests green; legacy
                CoreTerm output paths remain intact and working (coexistence).")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (reuse-not-reinvent high)        ;; share 3a/3b machinery across paths
    (spec-fidelity high)
    (crate-stays-building high)
    (cleverness low))

  (constraint
    (hard-limit "Every 0.5 lowering path emits through the Phase-2 Builder ONLY. No path
                 hand-builds CoreBundle/Node. The topological, dedupe, and UNKNOWN-gate
                 invariants are enforced by the builder, never re-implemented per path.")
    (hard-limit "Higher-order constructs in ANY path are closed via the Phase-3b lambda-lift
                 pass before the builder. No lambda/inline abstraction may reach Core IR 0.5.
                 Tier 3 dynamic-apply remains detect-and-halt.")
    (hard-limit "Shared lowering logic is FACTORED OUT and reused across paths; copy-paste of
                 the 3a/3b code into each path is forbidden. Divergent per-path behaviour for the
                 same construct is a defect.")
    (hard-limit "CCall dispatches on resolved registry identity (Hash256), never a name; names are
                 resolved at lowering and never appear in the bundle. Unresolved target = HARD HALT.")
    (hard-limit "Introspection walkers read the flat node table by index; they must NOT depend on
                 the legacy recursive CoreTerm shape for 0.5 input. Both code paths may coexist.")
    (hard-limit "Reuse src/fabric, src/registry/core05, src/ir/core_ir_05, and the 3a/3b modules.
                 Introduce NO new hash/identity/serialization regime.")
    (hard-limit "Do not retire or rewrite the legacy CoreTerm model or the 0.3 capnp path; the 0.5
                 entrypoints are ADDITIVE alongside the legacy ones. Retirement is Phase 3d."))

  (risk
    (per-path-divergence high)                  ;; same construct lowered differently across paths
    (copy-pasted-lift-or-builder-logic high)    ;; drift instead of shared helper
    (lambda-surviving-in-some-path critical)    ;; one path forgets the lift
    (introspection-misreads-flat-table high)    ;; wrong index/edge interpretation
    (name-leaking-into-bundle high)
    (legacy-path-regressed medium))             ;; coexistence broken

  (boundary
    (add    "0.5 lowering entrypoints for h1_lowering, registry_lowering, schema_lowering,
             spec_driven, nf_lowering (additive, alongside legacy entrypoints)" allowed)
    (add    "a shared lowering helper module factoring the common 3a/3b emit logic" allowed)
    (modify "src/introspection/{inspection,code_trace,trust,core_ir_json}.rs to read the flat
             0.5 CoreBundle (additive 0.5 read path; legacy read path may remain)" allowed)
    (add    "per-path lowering tests + introspection tests over 0.5 bundles" allowed)
    (use    "core05 for resolution/registration; core_ir_05 Builder + serializer; lambda_lift" allowed)
    (modify "legacy CoreTerm model, 0.3 capnp path, legacy lowering OUTPUT, rust-bridge/" forbidden)
    (modify "execution/interpreter.rs, lowering/interpreter.rs (tree-walkers — Phase 3d/5)" forbidden)
    (implement "the Verifier (Phase 4), pipeline/CLI rewiring (Phase 5)" forbidden)
    (change "encoding/identity/NodeRef decisions locked in fabric + core_ir_05 + core05" forbidden)
    (default forbidden))

  (assumption
    (statement "The five remaining paths lower the same core constructs the pilot proved
                (literals, shared values, calls, conditionals) plus path-specific surface forms;
                any construct without a defined 0.5 lowering is STOP-and-report, not improvised." tentative)
    (statement "Bundle-identity cross-compile stability remains per the Phase-3b working
                resolution (stable within a build) unless Chris has since ruled otherwise." tentative)
    (statement "Per-argument def_hash type-checking stays the Verifier's job (Phase 4)." tentative))

  (outcome
    (success "Each of the five remaining paths lowers a representative input to a CoreBundle that
              serializes canonically and yields a stable (within-build) identity hash."
             "All CCall targets across all paths resolve in the active core05 scope; no name or
              lambda appears in any emitted bundle."
             "The shared lowering logic lives in ONE place; the per-path entrypoints are thin."
             "The four introspection walkers produce correct output reading the flat table; their
              0.5 read path is linear (no tree recursion)."
             "Legacy lowering output for every path is unchanged; legacy introspection still works."
             "cargo build and cargo test pass for the whole crate; all prior-phase tests still green.")
    (failure "Any path hand-builds a bundle, diverges on a shared construct, lets a lambda reach the
              builder, leaks a name, regresses a legacy path, or an introspection walker misreads the
              flat table."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Specs:    core_ir_spec/axis_core_ir_0_5.md  (model; §Static Analysis: Blast Radius for introspection)
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 3, step 3c)
  ;; Reuse:    src/fabric/*, src/registry/core05/*, src/ir/core_ir_05.rs,
  ;;           src/lowering/minimal_lowering.rs (3a 0.5 entrypoint pattern),
  ;;           src/lowering/lambda_lift.rs (3b pass), registries/pilot_lowering.axreg (fixture pattern)
  ;; Targets (remaining lowering paths):
  ;;           src/lowering/h1_lowering.rs
  ;;           src/lowering/registry_lowering.rs   (already rejects 3b variants in legacy arm)
  ;;           src/lowering/schema_lowering.rs
  ;;           src/lowering/spec_driven.rs
  ;;           src/lowering/nf_lowering.rs
  ;; Targets (introspection — retarget to flat table):
  ;;           src/introspection/inspection.rs
  ;;           src/introspection/code_trace.rs
  ;;           src/introspection/trust.rs
  ;;           src/introspection/core_ir_json.rs
  ;; Out of scope here: execution/interpreter.rs, lowering/interpreter.rs (Phase 3d/5).
  ;;
  ;; --- FLAT-TABLE INTROSPECTION (why it gets simpler) ---
  ;;   forward (blast radius): scan nodes for those referencing index i (linear)
  ;;   backward (origin trace): read a node's arg indices, recurse to smaller indices
  ;;   CIf: treat both branches as reachable (conservative)
  ;;   No cycle detection needed — topological invariant guarantees acyclicity.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_LOWERING_ROLLOUT constraints and boundary;
     builder-only, factor-don't-copy, lift-before-builder, identity-not-name, and
     additive-coexistence are non-negotiable.")
)
