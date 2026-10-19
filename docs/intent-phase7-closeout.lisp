;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_7_CLOSEOUT
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; - HOUSEKEEPING PHASE: commit + tidy only. No behaviour changes.
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_CLOSEOUT)
    (owner chris)
    (scope
      "Land and tidy the completed 0.5 migration: commit the outstanding
       working-tree changes, archive the now-obsolete pre-rewrite documentation,
       and run a final whole-crate verification. This is housekeeping — NO code
       behaviour changes.
       INCLUDES: committing the uncommitted Phase-6 work (in-tree rust-bridge
       deletion + repointed consumers + updated migration plan), archiving stale
       root *.md reports that describe the retired 0.3/legacy-bridge era, a final
       cargo build + cargo test, and a grep proving zero legacy symbols in live code.
       EXCLUDES: the Ledger / trust-gate stratum (separate future work), reviving
       Phase 6b composite derivation (correctly dormant until a surface emits a
       composite), and any change to fabric/core05/core_ir_05/lowering/Verifier/
       bridge behaviour."))

  (goal
    (primary "The repo's committed state matches the finished 0.5 migration: nothing
              meaningful left uncommitted, obsolete docs archived, tree clean.")
    (secondary "A final cargo build + cargo test pass is captured as the migration's done-state."
               "A legacy-symbol grep returns zero hits in live code.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (clean-committed-state high)
    (no-behaviour-change high)
    (tidiness low))

  (constraint
    (hard-limit "Housekeeping only: do NOT change the behaviour of any code (fabric, core05,
                 core_ir_05, lowering, Verifier, the consumers' build flow). Commits and file moves
                 only; if a real code fix seems needed, STOP and report rather than smuggling it in.")
    (hard-limit "Commit the outstanding Phase-6 working-tree changes as a coherent commit
                 (message: the in-tree rust-bridge removal + standalone-bridge integration). Do not
                 leave the deletion and the repointed scripts split across confusing partial commits.")
    (hard-limit "Archive — do NOT delete — obsolete docs: MOVE stale pre-rewrite root *.md reports
                 into archive/ (or a dated subfolder). Keep docs/*.lisp, docs/core-0.5-migration-plan.md,
                 docs/rust-bridge-migration-plan.md, core_spec/*, and core_ir_spec/* in place.")
    (hard-limit "A doc is 'obsolete' only if it describes the retired 0.3 / legacy-bridge / shim era
                 and has no current accuracy. If in doubt about a file, leave it and list it for review
                 — never archive something still describing live 0.5 behaviour.")
    (hard-limit "End green: cargo build and cargo test pass for the whole crate after the commit and
                 the archive move; the move must not break any path referenced by build/test.")
    (hard-limit "Do NOT revive Phase 6b, do NOT start the Ledger, do NOT touch the bridge repo."))

  (risk
    (smuggling-a-behaviour-change-into-housekeeping high)
    (archiving-a-still-accurate-doc medium)
    (moving-a-doc-that-a-script-or-test-reads medium)
    (partial-confusing-commit low))

  (boundary
    (commit "the outstanding Phase-6 working-tree changes (deletion + repointed consumers + plan doc)" allowed)
    (move   "obsolete pre-rewrite root *.md reports into archive/ (git mv, history-preserving)" allowed)
    (run    "cargo build + cargo test as the final verification" allowed)
    (run    "the legacy-symbol grep (CoreTerm, CLam/CApp/CLet/CVar, target_name, node_id, Annotation,
             axis_core_ir_0_3/0_2, axis_rust_bridge) to confirm zero live-code hits" allowed)
    (add    "a short MIGRATION_STATUS note summarising the finished 0.5 state, IF useful" allowed)
    (modify "the behaviour of any source module" forbidden)
    (delete "any doc (archive instead)" forbidden)
    (revive "Phase 6b composite derivation; start the Ledger; touch axis-codegen-bridge-rs" forbidden)
    (change "encoding/identity/Verifier decisions" forbidden)
    (default forbidden))

  (assumption
    (statement "The candidate obsolete docs are the legacy-era root reports (e.g. BRIDGE_*, CLI_*,
                CORE_IR_0.3_*, NORMALIZATION_*, REGISTRY_SHIM_*, registry_shim_coverage*.json,
                RUNTIME_REGISTRY_*, VERIFICATION_CHECKLIST). Confirm each describes the retired era
                before moving; when unsure, list rather than move." tentative)
    (statement "Phase 6b stays dormant by design; docs/phase6b-gate-result.md remains as the record
                of why and what would unblock it." tentative)
    (statement "The Ledger / trust-gate stratum is acknowledged future work, out of scope here." tentative))

  (outcome
    (success "git status is clean after a coherent Phase-6 commit; the rust-bridge deletion and
              repointed consumers are committed together with a clear message."
             "Obsolete pre-rewrite docs are MOVED into archive/ (history preserved); no live-0.5 doc
              was archived; anything uncertain was listed for review, not moved."
             "cargo build and cargo test pass for the whole crate."
             "The legacy-symbol grep returns zero hits in live code (docs/historical archive aside)."
             "No source behaviour changed — diffs are commits, file moves, and at most a status note.")
    (failure "A behaviour change was smuggled in, a doc was deleted instead of archived, a still-accurate
              doc was archived, a moved doc broke a build/test path, or the tree was left uncommitted/red."))

  (mode
    (phase execution)        ;; human-approved; housekeeping only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Plan:     docs/core-0.5-migration-plan.md (Phases 0–6 complete); docs/phase6b-gate-result.md
  ;;           (composite derivation deferred by gate).
  ;; Uncommitted now (commit these): rust-bridge/ deletion; scripts/build_{ai1,ai2,axh,surface0}.sh;
  ;;           oper/discover_reg_script.sh; docs/rust-bridge-migration-plan.md; build/ai2/generated.rs;
  ;;           the new docs/intent-*.lisp specs; any new registries/*.axreg.
  ;; Keep in place: docs/*.lisp, docs/*-migration-plan.md, core_spec/*, core_ir_spec/*,
  ;;           registries/*.axreg, src/**, tests/**.
  ;; Archive candidates (verify each describes the retired era): BRIDGE_*.md, CLI_*.md,
  ;;           CORE_IR_0.3_*.md, NORMALIZATION_*.md, REGISTRY_*.md, REGISTRY_SHIM_COVERAGE.md,
  ;;           registry_shim_coverage*.json, RUNTIME_REGISTRY_ALIGNMENT_COMPLETE.md,
  ;;           COMPILER_CLEANUP_COMPLETE.md, FULL_PIPELINE_*.md, NO_USER_LOWERING_COMPLETE.md,
  ;;           CLEANUP_SUMMARY.txt, VERIFICATION_CHECKLIST.md, AXIS_PARSE_INT_DECISION.md.
  ;; Out of scope: src behaviour, the bridge repo, Phase 6b, the Ledger.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_CLOSEOUT constraints and boundary;
     housekeeping-only (no behaviour change), archive-don't-delete, end-green, and
     when-unsure-list-don't-move are non-negotiable.")
)
