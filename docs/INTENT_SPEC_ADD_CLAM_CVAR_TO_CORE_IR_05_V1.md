;; ============================================================
;; INTENT DECLARATION — AUTHORITATIVE BLOCK
;; ============================================================

You are operating under INTENT_SYSTEM_SPEC.v1.0.

(intent-id AXIS_ADD_CLAM_CVAR_TO_CORE_IR_05_V1)

;; ------------------------------------------------------------
;; LONG-CONTEXT REHYDRATION ANCHOR
;; ------------------------------------------------------------
;; All constraints remain binding.
;; Absence implies forbidden.
;; Constraint > Priority > Goal.
;; Authority separation must be preserved.
;; AXIS_ADD_CLAM_CVAR_TO_CORE_IR_05_V1 governs interpretation.


(intent-mode
  (state governance)
  (authority human)
  (downgrade-allowed false))


(intent

  ;; ------------------------------------------------------------
  ;; IDENTITY
  ;; ------------------------------------------------------------
  (identity
    (name "AXIS_ADD_CLAM_CVAR_TO_CORE_IR_05_V1")
    (owner "Chris")
    (scope
      "Add CLam and CVar as first-class node variants to Core IR 0.5.
       CLam represents lambda abstraction inline in a bundle.
       CVar represents a variable reference (lambda parameter) that
       cannot be eliminated at compile time.
       This restores first-class function support to the IR,
       enabling AI and mechanical code generators to emit lambdas
       directly without requiring a mandatory lambda-lifting pre-pass.
       Excludes: changes to the registry format, axreg text format,
       normalization engine, AST schema system, or the pipeline
       orchestration layer."))


  ;; ------------------------------------------------------------
  ;; RATIONALE
  ;; ------------------------------------------------------------
  ;; Lambda abstraction is the primary abstraction in Axis surface
  ;; languages (AI2, H1). Without CLam in the IR, any program using
  ;; a lambda requires a full lambda-lifting pre-pass before lowering.
  ;; Lambda lifting is a stateful, order-dependent, global transformation
  ;; that multiplies complexity across every AI and mechanical code
  ;; generator targeting the IR. With CLam in the IR:
  ;;   - AI sees lam x. body -> emits CLam{param=x, body=<ref>} directly
  ;;   - One-to-one structural mapping, no global analysis needed
  ;;   - Lambda lifting becomes an optional optimisation, not a hard gate
  ;; CVar is required alongside CLam: lambda parameters cannot be
  ;; eliminated at compile time (no value to substitute), so a CVar
  ;; node is the correct representation for a formal-parameter reference.
  ;; Let-bound variables remain env-transparent (replaced by NodeRef
  ;; during lowering), preserving the existing CLet semantics.


  ;; ------------------------------------------------------------
  ;; GOAL
  ;; ------------------------------------------------------------
  (goal
    (primary
      "Add CLam { param: String, body: NodeRef } and
       CVar { name: String } to the Core IR 0.5 Node enum.
       Wire both through the full stack: canonical codec,
       capnp transport codec, verifier, nf_lowering, spec_driven.")
    (secondary
      "CLam lowering: lower body with env unchanged (param is formal,
       not a value); emit Node::CLam { param, body }."
      "CVar lowering: if name is in env (let-bound), return env NodeRef
       (env-transparent, existing behaviour); if name is NOT in env
       (lambda parameter), emit Node::CVar { name }."
      "Canonical tag assignment: CLam = tag 2, CVar = tag 3.
       Existing tags 0 (CCall) and 1 (CIf) are unchanged."
      "Verifier: accept CLam and CVar nodes without error.
       Full static-scope checking (every CVar refers to an enclosing
       CLam param) is deferred — out of scope for this intent.")
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
    (codec-determinism high)
    (performance low))


  ;; ------------------------------------------------------------
  ;; CONSTRAINT — HARD LIMITS
  ;; ------------------------------------------------------------
  (constraint
    (hard-limit
      "CLam canonical tag = 2. CVar canonical tag = 3.
       CCall tag = 0 and CIf tag = 1 are UNCHANGED.
       Existing .coreir files that contain only CCall/CIf nodes
       must decode identically before and after this change."))

  (constraint
    (hard-limit
      "CLam { param: String, body: NodeRef }.
       CVar { name: String }.
       No other fields. No type annotations at this stage."))

  (constraint
    (hard-limit
      "CVar lowering is dual-path:
         - name in env  → return env NodeRef (env-transparent, no node emitted)
         - name not in env → emit Node::CVar { name }
       This preserves all existing CLet / let-binding semantics.
       Let-bound variables are ALWAYS env-transparent. Only lambda
       formal parameters produce CVar nodes."))

  (constraint
    (hard-limit
      "CLam lowering: env is NOT modified before lowering the body.
       The lambda parameter is NOT inserted into env.
       Any CVar reference to the parameter inside the body will fall
       through to the 'not in env' path and emit a CVar node."))

  (constraint
    (hard-limit
      "All existing passing tests must continue to pass.
       No test may be deleted or modified to accommodate this change."))

  (constraint
    (hard-limit
      "The capnp schema file (axis_core_ir_0_5.capnp) must be updated
       to include CLam and CVar union variants.
       The generated Rust bindings must be regenerated or manually
       updated to match."))

  (constraint
    (hard-limit
      "nf_lowering.rs MAY be modified. It is in scope for this intent.
       (Previous intent AXIS_RESTORE_SPEC_DRIVEN_COMPILATION_V1
       locked nf_lowering.rs as read-only — that lock expires here.)"))

  (constraint
    (hard-limit
      "No changes to: registry format, axreg text format, normalization
       engine, AST schema system, pipeline orchestration (pipeline/mod.rs),
       PipelineConfig struct, or CLI flags."))


  ;; ------------------------------------------------------------
  ;; RISK
  ;; ------------------------------------------------------------
  (risk
    ("Canonical tag collision: if CLam=2 or CVar=3 conflict with any
      existing usage in the codec, existing bundles silently
      misparse. Verified: tags 0 and 1 only are currently used." low))

  (risk
    ("capnp schema regeneration: if the generated bindings are not
      updated to match the new schema, the codec will panic at
      runtime on any CLam/CVar bundle." high))

  (risk
    ("CVar emitted for a name that is neither in env nor a lambda
      param — e.g. a truly unbound free variable. Currently this
      would silently emit a CVar node with an unresolvable name.
      Full scope checking is deferred; unbound CVar is valid IR
      at this stage." medium))

  (risk
    ("nf_lowering.rs LamExpr handler currently returns UnsupportedNode.
      It must be updated or the existing introspection callers
      (code_trace, trust, core_ir_json) will still see UnsupportedNode
      for any program with a lambda." medium))


  ;; ------------------------------------------------------------
  ;; BOUNDARY
  ;; ------------------------------------------------------------
  (boundary
    ("Add CLam and CVar variants to Node enum in core_ir_05.rs" allowed)
    ("Add CLam/CVar arms to push_node_bytes (canonical serializer)" allowed)
    ("Add CLam/CVar arms to decode_node_canonical" allowed)
    ("Add CLam/CVar arms to encode_capnp / decode_capnp" allowed)
    ("Update axis_core_ir_0_5.capnp schema" allowed)
    ("Add CLam/CVar arms to Builder::push_node" allowed)
    ("Update verifier (validation/core_ir.rs) to accept CLam/CVar" allowed)
    ("Update spec_driven.rs CLam arm: real lowering, not UnsupportedNode" allowed)
    ("Update spec_driven.rs CVar arm: dual-path env/node emission" allowed)
    ("Update nf_lowering.rs LamExpr and VarRef handlers" allowed)
    ("Update emit_05.rs helpers if needed" allowed)
    ("Modify normalization engine" forbidden)
    ("Modify AST schema system" forbidden)
    ("Modify pipeline/mod.rs" forbidden)
    ("Modify PipelineConfig struct" forbidden)
    ("Modify CLI flags" forbidden)
    ("Modify registry or axreg format" forbidden)
    ("Delete or modify existing tests" forbidden)
    ("Change canonical tags 0 (CCall) or 1 (CIf)" forbidden)
    (default forbidden))


  ;; ------------------------------------------------------------
  ;; RESOLVED UNKNOWNS
  ;; ------------------------------------------------------------
  ;; CLam tag: 2. CVar tag: 3. (Next available after CCall=0, CIf=1.)
  ;; CVar dual-path: env-transparent for let-bindings, CVar node for lambda params.
  ;; CLam env: unchanged during body lowering (param is formal, not value).
  ;; Full scope checking: deferred. CVar with unresolvable name is valid IR.
  ;; nf_lowering.rs: in scope (previous lock was per-intent, not permanent).


  ;; ------------------------------------------------------------
  ;; ASSUMPTION
  ;; ------------------------------------------------------------
  (assumption
    ("The capnp schema is defined in
      src/generated/axis_core_ir_0_5.capnp (or equivalent path)
      and the Rust bindings are in src/generated/.
      If the schema cannot be regenerated automatically,
      the Rust bindings will be manually updated." tentative))

  (assumption
    ("Builder::push_node currently only accepts CCall and CIf.
      Adding CLam and CVar requires updating the push_node method
      or its match arms. No other Builder method is affected." tentative))

  (assumption
    ("The verifier walk (validation/core_ir.rs) iterates nodes
      and checks structural invariants. Adding CLam and CVar
      requires new match arms that at minimum check:
        CLam: body NodeRef is backward (i < node index)
        CVar: name is non-empty string." tentative))


  ;; ------------------------------------------------------------
  ;; OUTCOME
  ;; ------------------------------------------------------------
  (outcome
    (success
      "cargo build --release exits 0."
      "cargo test exits 0 (all existing tests pass)."
      "ai2-lam.ai2 compiles to a CoreBundle containing a CLam node."
      "ai2-let.ai2 still compiles correctly (CLet env-transparent path unchanged)."
      "ai2-var.ai2 with a free variable emits a CVar node (not an error).")
    (failure
      "Any existing test breaks or fails to compile."
      "Existing CCall/CIf bundles decode differently than before."
      "CLam lowers with param inserted into env (wrong — param must be formal)."
      "CVar for a let-bound name emits a CVar node instead of being env-transparent."))


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
  (hard-limit CLAM_TAG_2_CVAR_TAG_3)
  (hard-limit EXISTING_TAGS_UNCHANGED)
  (hard-limit CVAR_DUAL_PATH_ENV_TRANSPARENT_FOR_LET)
  (hard-limit CLAM_ENV_UNCHANGED_DURING_BODY_LOWER)
  (hard-limit ALL_TESTS_PASS)
  (authority AI_PROPOSE_ONLY))

;; Semantic gravity anchors:
;; CLAM_TAG_2_CVAR_TAG_3
;; EXISTING_TAGS_UNCHANGED
;; CVAR_DUAL_PATH_ENV_TRANSPARENT_FOR_LET
;; CLAM_ENV_UNCHANGED_DURING_BODY_LOWER
;; ALL_TESTS_PASS
;; AI_PROPOSE_ONLY


;; ============================================================
;; ARCHITECTURAL SPINE
;; ============================================================

(spine
  "src/ir/core_ir_05.rs      — Node enum, serializer, codec, Builder"
  "src/generated/            — capnp schema + Rust bindings"
  "src/validation/core_ir.rs — verifier walk"
  "src/lowering/spec_driven.rs — CLam real lowering; CVar dual-path"
  "src/lowering/nf_lowering.rs — LamExpr and VarRef handlers"
  "src/lowering/emit_05.rs    — helpers (update if needed)")

(spine-rules
  (core_ir_05.rs gains CLam and CVar in Node enum and all codec paths)
  (capnp schema gains clam and cvar union branches)
  (verifier gains CLam and CVar match arms — structural check only)
  (spec_driven.rs CLam arm emits Node::CLam; CVar arm is dual-path)
  (nf_lowering.rs LamExpr emits Node::CLam; VarRef is dual-path)
  (no other files changed))


;; ============================================================
;; REINTEGRATION CHECK
;; ============================================================

;; Before merging, verify:
;; - CLAM_TAG_2_CVAR_TAG_3: push_node_bytes uses write_varint(buf, 2) for CLam,
;;   write_varint(buf, 3) for CVar
;; - EXISTING_TAGS_UNCHANGED: CCall=0, CIf=1 match arms untouched
;; - CVAR_DUAL_PATH: CVar arm in spec_driven and nf_lowering checks env first
;; - CLAM_ENV_UNCHANGED: CLam arm does NOT insert param into env before body lower
;; - ALL_TESTS_PASS: cargo test exits 0
;; - ai2-lam.ai2 compiles: axis ... --file examples/ai2/ai2-lam.ai2 exits 0
