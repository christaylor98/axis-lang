;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_3D_RETIRE_LEGACY
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; - THIS IS A DELETION PHASE: prove zero references BEFORE removing anything.
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_RETIRE_LEGACY)
    (owner chris)
    (scope
      "Retire the legacy Core IR 0.3 model now that every consumer has a 0.5
       path: remove the recursive CoreTerm model and its constructors, the 0.3
       (and dead 0.2) capnp encode/decode path, the legacy lowering OUTPUT
       entrypoints, and the legacy introspection read paths. Resolve the two
       remaining pre-existing broken integration tests as a consequence.
       INCLUDES: deleting legacy 0.3/0.2 code + schema wiring, removing legacy
       lowering/introspection branches, and a tree-wide grep proving no
       stragglers remain.
       EXCLUDES: the Verifier (Phase 4), pipeline/CLI/fixture finalisation
       beyond what deletion forces (Phase 5), and the rust-bridge swap — which
       is a SEPARATE crate with its own 0.3 schema copy and is out of scope."))

  (goal
    (primary "The main crate contains exactly ONE Core IR model — 0.5 — with the
              legacy CoreTerm / 0.3 capnp paths fully removed and no dangling references.")
    (secondary "The whole crate builds and ALL tests pass, including the two integration
                tests that were broken on main (resolved or retired as a consequence)."
               "No 0.5 behaviour changes: deletion only — the 0.5 paths are untouched.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (zero-dangling-references high)
    (all-tests-green high)
    (no-05-behaviour-change high)
    (aggressiveness-of-deletion low))   ;; remove only what is provably unused

  (constraint
    (hard-limit "Remove a legacy symbol ONLY after a tree-wide grep proves it has no remaining
                 references in the main crate (excluding rust-bridge/, which is out of scope).
                 Targets to grep to zero: CoreTerm, CIntLit/CBoolLit/CUnitLit/CLam/CApp/CLet/CVar,
                 target_name, node_id, Annotation/annotations, axis_core_ir_0_3_capnp,
                 axis_core_ir_0_2_capnp, bundle_v0_2/bundle_v0_3.")
    (hard-limit "Deletion only — do NOT modify any 0.5 path (fabric, core05, core_ir_05, the 0.5
                 lowering entrypoints, the 0.5 introspection read paths). 0.5 behaviour and tests
                 must be byte-for-byte unaffected.")
    (hard-limit "Do NOT touch rust-bridge/ or its own capnp copy. The main crate's build.rs may
                 drop the 0.3/0.2 schema only after the main crate stops referencing them; the
                 bridge keeps its own schema independently.")
    (hard-limit "If removing a symbol would break a still-needed capability (not merely legacy
                 plumbing), STOP and report rather than deleting the capability.")
    (hard-limit "Every deletion step ends with the crate compiling and tests green; do not leave
                 the tree in a non-building intermediate state across the handoff."))

  (risk
    (deleting-a-still-referenced-symbol critical)        ;; build break / lost capability
    (silently-dropping-needed-functionality critical)     ;; e.g. retiring execution with no replacement
    (touching-05-paths-during-cleanup high)              ;; behaviour change smuggled into a deletion phase
    (breaking-rust-bridge-by-removing-shared-schema high)
    (leaving-dangling-references medium))                ;; partial removal

  (boundary
    (remove "legacy CoreTerm model + constructors in src/ir/core_ir.rs (file or module)" allowed)
    (remove "the 0.3/0.2 capnp encode/decode path and legacy bundle_v0_2/v0_3 helpers" allowed)
    (remove "legacy lowering OUTPUT entrypoints in the five+pilot lowering modules
             (keep the 0.5 entrypoints)" allowed)
    (remove "legacy introspection read paths (keep the 0.5 flat-table read paths)" allowed)
    (modify "build.rs / src/generated/mod.rs / src/lib.rs to drop 0.3/0.2 schema wiring
             AFTER references are gone" allowed)
    (resolve "tests/end_to_end_all_surfaces.rs (syntax error) and tests/wave_c_determinism_tests.rs
              (removed Registry symbol): fix to 0.5 or retire if their premise is the deleted model" allowed)
    (modify "any 0.5 path (fabric, core05, core_ir_05, 0.5 lowering/introspection)" forbidden)
    (modify "rust-bridge/** and its capnp schema" forbidden)
    (implement "the Verifier (Phase 4) or new functionality of any kind" forbidden)
    (default forbidden))

  ;; --- OPEN QUESTION surfaced for decision (does NOT block; assumption below is the working
  ;;     resolution until Chris rules) ---
  (unknown
    "Fate of the tree-walking interpreters (src/execution/interpreter.rs, src/lowering/interpreter.rs).
     They evaluate the legacy CoreTerm tree, so retiring CoreTerm forces a choice:
       (a) RETIRE them — execution is the bridge's job (matches '0.5 rewrite first, bridge later');
           simplest, removes surface area; in-lab execution testing is lost until the bridge lands.
       (b) PORT a minimal flat-table evaluator over CoreBundle for in-lab testing, then retire the
           tree-walkers.
     Recommendation: (a) retire now; add a small flat-table evaluator later as its own task ONLY if
     in-lab execution testing proves necessary before the bridge. Confirm before deleting.")

  (assumption
    (statement "Working resolution: retire the tree-walking interpreters with the legacy model
                (option a). If they provide test coverage that would be lost, report it rather than
                silently dropping it." tentative)
    (statement "The two broken integration tests assert against the legacy model; expect to retire
                or rewrite them to 0.5 rather than preserve their original assertions." tentative)
    (statement "After this phase the main crate has a single Core IR model; Phase 5 finalises
                pipeline/CLI/fixtures and Phase 4 adds the Verifier." tentative))

  (outcome
    (success "grep for every legacy target (CoreTerm, CLam/CApp/CLet/CVar, target_name, node_id,
              Annotation, axis_core_ir_0_3_capnp, axis_core_ir_0_2_capnp, bundle_v0_2/v0_3) returns
              ZERO hits in the main crate (rust-bridge/ excluded)."
             "build.rs no longer compiles the 0.3/0.2 schema; the main crate builds clean."
             "cargo test passes for the whole crate, INCLUDING the two formerly-broken integration
              tests (now fixed to 0.5 or retired with rationale)."
             "All 0.5 tests from Phases 0–3c remain green and unmodified — deletion caused no
              0.5 behaviour change."
             "rust-bridge/ is untouched and still builds against its own schema.")
    (failure "Any legacy reference remains, a 0.5 path was modified, a needed capability was deleted
              without report, rust-bridge was touched, or the tree was left non-building."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 3, step 3d; see also Phase 5 cleanup)
  ;; Keep (0.5, untouched): src/fabric/*, src/registry/core05/*, src/ir/core_ir_05.rs,
  ;;           the 0.5 lowering entrypoints, the 0.5 introspection read paths.
  ;; Remove (legacy):
  ;;           src/ir/core_ir.rs            (legacy CoreTerm: CIntLit/CBoolLit/CUnitLit/CLam/CLet/
  ;;                                          CIf/CVar/CApp/CCall(target_name), Annotation, node_id,
  ;;                                          encode_capnp/write_core_term, bundle_v0_2/v0_3)
  ;;           core_ir_spec/axis_core_ir_0_3.capnp + 0_2.capnp wiring in build.rs / generated/mod.rs / lib.rs
  ;;           legacy lowering OUTPUT fns in minimal/registry/h1/schema/spec_driven/nf_lowering
  ;;           legacy read paths in introspection/{inspection,code_trace,trust,core_ir_json}.rs
  ;; Decide:   src/execution/interpreter.rs, src/lowering/interpreter.rs (see (unknown) above)
  ;; Excluded: rust-bridge/** (separate crate; own schema; deferred bridge swap).
  ;;
  ;; --- DELETION ORDER (suggested; each step ends green) ---
  ;;   1. Remove legacy lowering/introspection output+read branches (leaf consumers first).
  ;;   2. Decide + remove the interpreters per (unknown).
  ;;   3. Remove the legacy CoreTerm model + 0.3/0.2 encode/decode.
  ;;   4. Drop 0.3/0.2 schema from build.rs / generated/mod.rs / lib.rs.
  ;;   5. Fix/retire the two integration tests.
  ;;   6. Final grep to zero + full cargo test.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_RETIRE_LEGACY constraints and boundary;
     prove-zero-references-before-deletion, deletion-only (no 0.5 change), don't-touch-rust-bridge,
     and stop-and-report-before-dropping-a-capability are non-negotiable.")
)
