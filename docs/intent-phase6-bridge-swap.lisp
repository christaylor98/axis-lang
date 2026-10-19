;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_6_BRIDGE_INTEGRATION
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; - GATED PHASE: the schema probe (mode.gate) must PASS before any code change.
;; - TWO-REPO PHASE: work spans axis-lang-lab-working AND axis-codegen-bridge-rs.
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_BRIDGE_INTEGRATION)
    (owner chris)
    (scope
      "Bring the now-STANDALONE bridge (~/dev/axis-codegen-bridge-rs)
       onto Core IR 0.5 and integrate axis-lang-lab against it. The legacy
       in-tree rust-bridge has ALREADY been deleted from axis-lang-lab — this
       phase does NOT remove it again; it repoints the consumers that referenced
       it and applies the 0.5 alignment to the external crate.
       INCLUDES (two repos): (A) in axis-codegen-bridge-rs — a GATING probe of
       which Core IR version it reads, then the 0.5 alignment so it loads/runs
       0.5 bundles, plus real composite-body wiring; (B) in axis-lang-lab —
       repoint the dangling consumers (build_*.sh, audit/example/discover
       scripts, generated.rs, verify scripts) to the external bridge, wire the
       registry via --reg, and add the recursive composite effect derivation the
       Verifier currently fail-closes on; an end-to-end build+run diff vs a
       baseline; and updating docs/rust-bridge-migration-plan.md to 0.5.
       EXCLUDES: changing the 0.5 core/registry/lowering/Verifier SEMANTICS in
       axis-lang-lab (the bridge consumes them); inventing a new IR version;
       re-deleting the already-removed in-tree bridge."))

  (goal
    (primary "axis-lang-lab builds and RUNS a 0.5 bundle through the standalone
              axis-codegen-bridge; the consumers that referenced the deleted in-tree
              bridge are repointed and working.")
    (secondary "The external bridge reads/executes Core IR 0.5 (alignment applied in its repo)."
               "Recursive composite effect derivation lands once real bodies are wired; the
                Verifier derives composite effect instead of fail-closing where bodies resolve."
               "Keep axis-lang-lab compiling; keep all prior-phase tests green."
               "docs/rust-bridge-migration-plan.md reflects 0.5 and the standalone-repo reality.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (resolve-schema-gate-first high)
    (spec-fidelity high)
    (both-repos-build high)
    (speed low))

  (constraint
    (hard-limit "GATE FIRST: before ANY code change, probe ~/dev/axis-codegen-bridge-rs
                 and determine which Core IR schema it reads. Confirm by feeding it a .coreir this
                 project now emits (e.g. build/surface0/program.coreir, a real 0.5 capnp bundle).
                 If it does NOT already read 0.5, STOP and report the gap with the two options below
                 — do NOT start integrating.")
    (hard-limit "If the bridge is not on 0.5: present (a) align the external crate to 0.5 in its own
                 repo vs (b) a 0.5->bridge transport shim, and WAIT for Chris's decision. Neither is
                 chosen unilaterally. (Recommended: (a) — the bridge should speak the one live model.)")
    (hard-limit "The in-tree rust-bridge is ALREADY deleted. Do NOT recreate it. The consumers that
                 still name it (scripts, generated.rs, audit/shim artifacts) are REPOINTED to the
                 external bridge or retired — not restored.")
    (hard-limit "Consume, do not change, the axis-lang-lab 0.5 stack: fabric, core05, core_ir_05,
                 the 0.5 lowering entrypoints, lambda_lift, and the Verifier keep their behaviour.
                 The bridge is a downstream consumer of emitted bundles.")
    (hard-limit "Bundle identity remains content_hash of the CANONICAL serialization; the .coreir
                 handed to the bridge is the capnp 0.5 transport. The bridge's wire expectations must
                 NOT redefine identity.")
    (hard-limit "Recursive composite effect derivation, once real bodies are wired, computes
                 effect_class = MAX over the body graph's CCall components (deterministic = AND,
                 idempotent = conservative false). It replaces the Phase-4 fail-closed path ONLY where
                 the body is genuinely resolvable; an unresolvable body still fails closed, never
                 assumes pure.")
    (hard-limit "Each step in each repo ends with that repo compiling and its tests green; no
                 non-building handoff state in either repo.")
    (hard-limit "The 'shim coverage' concept is superseded by --reg registry validation; do not
                 resurrect shim.rs tooling. Retire registry_shim_coverage*.json / discover scripts or
                 replace them with a registry-coverage report."))

  (risk
    (integrating-before-the-gate-passes critical)        ;; building on an unverified schema assumption
    (schema-version-gap-unhandled critical)              ;; bridge not on 0.5, fed 0.5 bundles
    (identity-redefined-by-bridge-wire-format high)
    (composite-derivation-assumes-pure critical)         ;; the very thing Phase 4 guarded
    (semantic-change-smuggled-into-the-05-stack high)
    (dangling-consumer-left-pointing-at-deleted-bridge high)
    (cross-repo-build-skew medium))                      ;; one repo green, the other not

  (boundary
    ;; --- in axis-codegen-bridge-rs (the standalone bridge) ---
    (probe  "axis-codegen-bridge-rs — read its schema/version, CLI surface, load path" allowed)
    (modify "axis-codegen-bridge-rs to read/run Core IR 0.5 bundles (the 0.5 alignment)" allowed)
    ;; --- in axis-lang-lab-working (this repo) ---
    (modify "scripts/build_{ai1,ai2,axh,surface0}.sh, scripts/audit_registry_coverage.py,
             examples/build_example_lib.sh, oper/discover_reg_script.sh — repoint to the external
             bridge (BRIDGE_BINARY) + --reg; drop the in-tree cargo-build step" allowed)
    (regenerate "build/ai2/generated.rs and any generated bridge glue via the external bridge" allowed)
    (modify "test_cli_semantics.sh, verify_cli_implementation.sh to target the external bridge" allowed)
    (retire "registry_shim_coverage*.json, discovery_out/bridge_stubs.txt, shim-coverage tooling
             (superseded by --reg validation)" allowed)
    (add    "real composite-body wiring (CompositeBody body -> resolvable CoreBundle)" allowed)
    (modify "src/validation/core_ir.rs ONLY to enable recursive composite derivation where bodies
             are now resolvable, preserving fail-closed where they are not" allowed)
    (modify "docs/rust-bridge-migration-plan.md to reflect 0.5 + standalone-repo reality" allowed)
    (add    "end-to-end build+run tests and a baseline diff" allowed)
    ;; --- forbidden ---
    (recreate "the in-tree rust-bridge" forbidden)
    (change "the SEMANTICS of fabric/core05/core_ir_05/lowering_05/lambda_lift or the Verifier
             verdict rules (beyond enabling composite derivation per the constraint)" forbidden)
    (invent "a new Core IR version" forbidden)
    (start-integration "before the schema gate passes" forbidden)
    (default forbidden))

  (assumption
    (statement "axis-codegen-bridge-rs is the live bridge and is reachable by the implementer
                (Claude Code) even though it is NOT mounted for the spec author. Its 0.4-era notes
                (lib axis_codegen_bridge, bin axis-codegen-bridge, structured core_ir, --reg/--lib-dir
                flags) are HINTS to re-verify against the actual crate, not a contract." tentative)
    (statement "Wiring real composite bodies makes a composite's body hash resolve to an actual
                CoreBundle in scope; until that resolution exists the Verifier stays fail-closed." tentative)
    (statement "Execution/runtime behaviour is owned by the bridge; axis-lang-lab emits and hands
                off bundles, it does not re-implement execution." tentative)
    (statement "The recommended gate outcome is to align the bridge to 0.5 in its own repo (option a);
                a transport shim (b) is the fallback only if a 0.5 bump is infeasible." tentative))

  (outcome
    (success "Schema gate resolved: a documented finding of which IR version the bridge reads,
              confirmed against a real emitted 0.5 .coreir."
             "axis-codegen-bridge-rs reads/runs Core IR 0.5 (alignment applied in its repo)."
             "axis-lang-lab builds and RUNS a 0.5 bundle through the external bridge; the build_*.sh,
              audit, example, discover, and verify consumers are repointed and working — zero references
              to the deleted in-tree bridge remain in live scripts/artifacts."
             "Recursive composite effect derivation works where bodies are resolvable; unresolvable
              bodies still fail closed (never assume pure)."
             "docs/rust-bridge-migration-plan.md reflects 0.5 and the standalone repo."
             "Both repos build; axis-lang-lab tests all green; an end-to-end run diff vs baseline is
              captured and explained.")
    (failure "Integration started before the gate passed, the bridge (not on 0.5) was fed 0.5 bundles,
              the bridge wire format redefined identity, composite derivation assumed pure, the 0.5
              stack semantics changed, the in-tree bridge was recreated, or a consumer was left pointing
              at the deleted bridge."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (gate schema-probe-must-pass-before-code-change)
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Plan:     docs/core-0.5-migration-plan.md (Phase 6); docs/rust-bridge-migration-plan.md
  ;;           (0.4-era reference integration — UPDATE to 0.5 + standalone reality)
  ;; Repo A (bridge, apply 0.5 here): ~/dev/axis-codegen-bridge-rs
  ;;           NOT mounted for the spec author; Claude Code has filesystem access. Re-verify the
  ;;           0.4-era notes: lib axis_codegen_bridge; bin axis-codegen-bridge; structured core_ir +
  ;;           load_core_bundle; CLI build <coreir> --out <bin> [--lib/--lib-dir/--reg/...].
  ;; Repo B (this repo, integrate here): ~/dev/axis-lang-lab-working
  ;;           DONE already: in-tree rust-bridge/ deleted; build.rs is 0.5-only.
  ;;           Dangling consumers to repoint/retire (they named the deleted bridge):
  ;;             scripts/build_ai1.sh, build_ai2.sh, build_axh.sh, build_surface0.sh
  ;;             scripts/audit_registry_coverage.py, examples/build_example_lib.sh
  ;;             oper/discover_reg_script.sh, test_cli_semantics.sh, verify_cli_implementation.sh
  ;;             build/ai2/generated.rs (regenerate via external bridge)
  ;;             registry_shim_coverage*.json, discovery_out/bridge_stubs.txt (shim concept retired)
  ;;   Reuse:  src/fabric/*, src/registry/core05/*, src/ir/core_ir_05.rs (encode_capnp transport),
  ;;           src/validation/core_ir.rs (Verifier — composite fail-closed today).
  ;;   Emit:   build/{ai1,ai2,surface0}/program.coreir are real 0.5 capnp bundles (Phase-5 golds) —
  ;;           use one as the probe input.
  ;;
  ;; --- GATE DECISION TREE ---
  ;;   probe axis-codegen-bridge-rs schema/version
  ;;     -> reads 0.5      => integrate-and-test (BRIDGE_BINARY, regen glue, --reg, repoint scripts, diff)
  ;;     -> not 0.5        => STOP, report gap, present (a) align bridge to 0.5 in its repo |
  ;;                          (b) transport shim, WAIT for decision
  ;;   only after a green path is chosen: wire composite bodies -> enable recursive derivation ->
  ;;   prove end-to-end across both repos -> repoint/retire consumers -> update the plan doc to 0.5.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_BRIDGE_INTEGRATION constraints and boundary;
     gate-before-code, two-repos-both-green, don't-recreate-the-in-tree-bridge,
     don't-redefine-identity, consume-don't-change the 0.5 stack, and
     composite-derivation-never-assumes-pure are non-negotiable.")
)
