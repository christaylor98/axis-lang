;; ============================================================
;; INTENT DECLARATION — AUTHORITATIVE BLOCK
;; ============================================================

You are operating under INTENT_SYSTEM_SPEC.v1.0.

(intent-id AXIS_RESTORE_SPEC_DRIVEN_COMPILATION_V1)

;; ------------------------------------------------------------
;; LONG-CONTEXT REHYDRATION ANCHOR
;; ------------------------------------------------------------
;; All constraints remain binding.
;; Absence implies forbidden.
;; Constraint > Priority > Goal.
;; Authority separation must be preserved.
;; AXIS_RESTORE_SPEC_DRIVEN_COMPILATION_V1 governs interpretation.


(intent-mode
  (state governance)
  (authority human)
  (downgrade-allowed false))


(intent

  ;; ------------------------------------------------------------
  ;; IDENTITY
  ;; ------------------------------------------------------------
  (identity
    (name "AXIS_RESTORE_SPEC_DRIVEN_COMPILATION_V1")
    (owner "Chris")
    (scope
      "Restore the spec-file-driven compilation contract: every input
       consumed by the pipeline must have an explicit CLI flag, including
       registry files, and lowering must be driven by a user-supplied YAML
       spec via spec_driven.rs.
       Excludes: changes to Core IR 0.5 data model, verifier, capnp codec,
       registry record format, introspection, test suites, or the
       nf_lowering path (which may remain for introspection and gold
       generators)."))


  ;; ------------------------------------------------------------
  ;; GOAL
  ;; ------------------------------------------------------------
  (goal
    (primary
      "The axis CLI accepts explicit flags for every input the pipeline
       consumes: --lexer, --parser, --schema, --lowering, --normalize,
       --file (source, repeatable, file or directory), and --reg
       (registry, repeatable, optional, file or directory).
       Compilation is driven by those 5 YAML spec files plus one or more
       source inputs and zero or more registry files.")
    (secondary
      "spec_driven::lower_to_bundle_05 handles all 7 expressible constructs
       in Core IR 0.5: CUnitLit, CIntLit, CBoolLit, CVar, CLet, CIf, CApp.
       CLam emits UnsupportedNode (no 0.5 representation)."
      "The pipeline uses spec_driven when --lowering is provided."
      "--reg accepts one or more inputs (merged last-wins); when no --reg
       is given the scope is empty."
      "--file and --reg each accept files or directories; directories
       expand to all contained files; no extension filtering.")
    (type outcome-oriented))


  ;; ------------------------------------------------------------
  ;; ACTOR
  ;; ------------------------------------------------------------
  (actor
    (Chris
      (type human)
      (role "Final authority on all decisions")
      (authority full)
      (may-decide true))

    (SystemAI
      (type ai)
      (role "Implementation assistant — proposes and executes under constraint")
      (authority none)
      (may-decide false)))


  ;; ------------------------------------------------------------
  ;; PRIORITY
  ;; ------------------------------------------------------------
  (priority
    (correctness high)
    (backward-compat-of-existing-tests high)
    (spec-completeness high)
    (performance low))


  ;; ------------------------------------------------------------
  ;; CONSTRAINT — HARD LIMITS
  ;; ------------------------------------------------------------
  (constraint
    (hard-limit
      "Every input consumed by the pipeline must have an explicit CLI flag.
       The complete required set is:
         --lexer      lexer spec YAML            (one file, required)
         --parser     parser spec YAML           (one file, required)
         --schema     AST schema YAML            (one file, required)
         --lowering   lowering spec YAML         (one file, required)
         --normalize  normalization rules YAML   (one file, required)
         --file       source input               (one or more, required;
                                                  file or directory;
                                                  repeatable;
                                                  no extension filter)
         --reg        registry input             (zero or more, optional;
                                                  file or directory;
                                                  repeatable;
                                                  merged last-wins;
                                                  no extension filter)
       No silent auto-discovery. No implicit defaults for any spec file.
       File extensions are non-mandatory and carry no semantic meaning.
       Directory expansion includes ALL files — no filtering by extension.
       cli_util::normalize_path_inputs is the shared expansion mechanism
       for both --file and --reg; it must not filter by extension."))

  (constraint
    (hard-limit
      "spec_driven::lower_to_bundle_05 must handle all 7 expressible
       Core IR 0.5 constructs: CUnitLit, CIntLit, CBoolLit, CVar, CLet,
       CIf, CApp (→ CCall). CLam returns UnsupportedNode — there is no
       CLam variant in the Core IR 0.5 Node enum. Any other spec target
       also returns UnsupportedNode (STOP-and-report)."))

  (constraint
    (hard-limit
      "nf_lowering.rs must not be removed or modified.
       It is used by introspection (code_trace, trust, core_ir_json)
       and gold generators (wave2, wave3). Those callers must continue
       to work unchanged."))

  (constraint
    (hard-limit
      "PipelineConfig struct fields must not change. Tests in
       tests/phase5_pipeline.rs and tests/wave6_failure_modes.rs
       construct PipelineConfig with named fields and must compile
       without modification. The spec-driven entry point is implemented
       as a new function run_pipeline_spec_driven in pipeline/mod.rs
       that accepts the lowering spec path as a separate parameter."))

  (constraint
    (hard-limit
      "No changes to: Core IR 0.5 data model, verifier, capnp codec,
       registry record format, axreg text format, or the .coreir output
       format. The serialized artifact must remain bit-identical for
       identical source+specs+registries."))

  (constraint
    (hard-limit
      "The source file flag must be --file (not --src)."))

  (constraint
    (hard-limit
      "All existing passing tests must continue to pass.
       No test may be deleted or modified to accommodate this change."))


  ;; ------------------------------------------------------------
  ;; RISK
  ;; ------------------------------------------------------------
  (risk
    ("spec_driven env threading for CLet/CVar is wrong,
      producing silently incorrect Core IR" critical))

  (risk
    ("collect_call_spec walks AppExpr fn/arg fields using hardcoded
      names that diverge from schema — verified against ai2 ast schema
      which defines fn and arg fields" medium))

  (risk
    ("nf_lowering callers in introspection break due to any
      pipeline/mod.rs signature change" high))

  (risk
    ("--reg optional but scope.lookup_name panics on CApp with
      empty scope when no registry provided" medium))

  (risk
    ("auto-discovery of normalize silently retained alongside
      new explicit --normalize flag causing ambiguity" medium))


  ;; ------------------------------------------------------------
  ;; BOUNDARY
  ;; ------------------------------------------------------------
  (boundary
    ("Add --lowering flag to CLI (required)" allowed)
    ("Rename --src to --file in CLI" allowed)
    ("Make --reg optional and repeatable (zero or more)" allowed)
    ("Add --normalize as explicit required flag, remove auto-discovery" allowed)
    ("Add run_pipeline_spec_driven to pipeline/mod.rs" allowed)
    ("Expand spec_driven.rs 0.5 match arms for 6 new cases" allowed)
    ("Add collect_call_spec helper inside spec_driven.rs" allowed)
    ("Add env HashMap parameter to lower_spec_node_05 (private fn)" allowed)
    ("Retain --reg merge logic (last-wins, load_and_merge_registries)" allowed)
    ("Modify nf_lowering.rs" forbidden)
    ("Modify introspection callers of nf_lowering" forbidden)
    ("Modify gold generators" forbidden)
    ("Modify Core IR 0.5 data model" forbidden)
    ("Modify verifier" forbidden)
    ("Modify axreg or registry format" forbidden)
    ("Delete or modify existing tests" forbidden)
    ("Modify PipelineConfig struct fields" forbidden)
    ("Add a run subcommand" forbidden)
    ("Add extension filtering to normalize_path_inputs" forbidden)
    (default forbidden))


  ;; ------------------------------------------------------------
  ;; RESOLVED UNKNOWNS
  ;; ------------------------------------------------------------
  ;; CLam: Core IR 0.5 Node enum has only CCall and CIf variants.
  ;;       CLam → UnsupportedNode. 7 of 8 spec constructs are expressible.
  ;; --normalize: explicit required flag, no auto-discovery.
  ;; CLI shape: flat (no run subcommand); trace subcommand retained.
  ;; PipelineConfig: unchanged; spec-driven path via new function.


  ;; ------------------------------------------------------------
  ;; ASSUMPTION
  ;; ------------------------------------------------------------
  (assumption
    ("AppExpr in the AI2 AST schema has fn and arg fields.
      collect_call_spec uses these hardcoded names for CApp lowering." tentative))

  (assumption
    ("An empty Scope is a valid input to lower_to_bundle_05
      for programs that do not use CApp (no registry lookups)." tentative))


  ;; ------------------------------------------------------------
  ;; OUTCOME
  ;; ------------------------------------------------------------
  (outcome
    (success
      "axis --lexer L --parser P --schema S --lowering W --normalize N
       --file F [--file F2...] [--reg R...] compiles ai2-explicit source
       correctly to Core IR 0.5."
      "All existing tests pass without modification."
      "nf_lowering callers compile and behave identically to before.")
    (failure
      "Any existing test breaks or fails to compile."
      "--file is not repeatable or does not accept directories."
      "nf_lowering callers fail to compile."
      "spec_driven produces wrong Core IR for VarRef/LetExpr."
      "Any pipeline input has no explicit CLI flag."))


  ;; ------------------------------------------------------------
  ;; MODE LOCK — UNLOCKED FOR EXECUTION
  ;; ------------------------------------------------------------
  (mode
    (phase execution)
    (design prohibited)
    (execution allowed))


  ;; ------------------------------------------------------------
  ;; STATUS
  ;; ------------------------------------------------------------
  (status
    (state approved)
    (authority human)
    (execution-allowed true))
)


;; ============================================================
;; INVARIANT COMPRESSION LAYER
;; ============================================================

(intent-invariants
  (hard-limit ALL_INPUTS_HAVE_EXPLICIT_FLAGS)   ;; incl. --reg, no ext filter
  (hard-limit SPEC_DRIVEN_HANDLES_7_CONSTRUCTS) ;; CLam → UnsupportedNode
  (hard-limit NF_LOWERING_UNTOUCHED)
  (hard-limit PIPELINE_CONFIG_STRUCT_UNCHANGED)
  (hard-limit NO_CORE_IR_FORMAT_CHANGES)
  (hard-limit ALL_TESTS_PASS)
  (authority AI_PROPOSE_ONLY))

;; Semantic gravity anchors:
;; ALL_INPUTS_HAVE_EXPLICIT_FLAGS
;; SPEC_DRIVEN_HANDLES_7_CONSTRUCTS
;; NF_LOWERING_UNTOUCHED
;; PIPELINE_CONFIG_STRUCT_UNCHANGED
;; NO_CORE_IR_FORMAT_CHANGES
;; ALL_TESTS_PASS
;; AI_PROPOSE_ONLY


;; ============================================================
;; ARCHITECTURAL SPINE
;; ============================================================

(spine
  "src/main.rs – CLI contract; owns all flag definitions including --reg"
  "src/pipeline/mod.rs – add run_pipeline_spec_driven; PipelineConfig unchanged"
  "src/lowering/spec_driven.rs – expand 0.5 match arms; add env threading"
  "src/lowering/nf_lowering.rs – READ ONLY; owned by introspection"
  "src/lowering/emit_05.rs – shared emit helpers; read-only"
  "src/cli_util.rs – normalize_path_inputs; no extension filtering")

(spine-rules
  (spec_driven.rs receives new CIntLit CBoolLit CVar CLet CApp CLam arms)
  (nf_lowering.rs is read-only for this change)
  (pipeline/mod.rs gains run_pipeline_spec_driven; run_pipeline unchanged)
  (main.rs calls run_pipeline_spec_driven; tests call run_pipeline unchanged)
  (cli_util::normalize_path_inputs passes None for extension_filter always))


;; ============================================================
;; REINTEGRATION CHECK
;; ============================================================

;; Before merging, verify:
;; - ALL_INPUTS_HAVE_EXPLICIT_FLAGS: --lexer --parser --schema --lowering
;;   --normalize --file --reg all present in Cli struct
;; - SPEC_DRIVEN_HANDLES_7_CONSTRUCTS: CUnitLit CIntLit CBoolLit CVar
;;   CLet CIf CApp in match arms; CLam → UnsupportedNode
;; - NF_LOWERING_UNTOUCHED: no edits to src/lowering/nf_lowering.rs
;; - PIPELINE_CONFIG_STRUCT_UNCHANGED: PipelineConfig struct diff = empty
;; - ALL_TESTS_PASS: cargo test exits 0
