;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_5_PIPELINE
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_PIPELINE)
    (owner chris)
    (scope
      "Finalise the in-tree rewrite: make the `axis` CLI drive the full Core IR
       0.5 path end to end, rewire the pipeline and gold-generation bins to the
       0.5 types, regenerate the golden .coreir fixtures, and replace the
       placeholder registry with a real one authored against the grammar doc.
       INCLUDES: an end-to-end CLI flow (parse -> lower -> lambda-lift -> verify
       -> serialize -> emit .coreir) over a supplied .axreg scope; rewiring
       src/pipeline/mod.rs, src/main.rs, and the generate_wave*_golds bins;
       regenerating golds; authoring a real registry (e.g. registry/axis.axreg)
       per axreg-text-projection-reference.md; and a final straggler grep.
       EXCLUDES: the rust-bridge swap and any execution of emitted bundles (the
       bridge owns execution; deferred), recursive composite effect derivation
       (still deferred until real bodies are wired), and any change to the
       Verifier's verdict semantics (Phase 4)."))

  (goal
    (primary "`axis` builds a .coreir bundle from source + an .axreg scope through the
              real 0.5 pipeline, running the Verifier as a gate, with a stable canonical
              bundle identity.")
    (secondary "pipeline/main/gold bins use ONLY 0.5 types; no legacy references remain."
               "Golden .coreir fixtures are regenerated as 0.5 and reviewed as MEANING diffs."
               "A real registry replaces registries/minimal.axreg, authored per the grammar doc."
               "Keep the crate compiling; keep all prior-phase tests green.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (working-end-to-end-cli high)
    (spec-fidelity high)
    (crate-stays-building high)
    (cli-ergonomics low))

  (constraint
    (hard-limit "The CLI build flow runs the REAL 0.5 stages in order: parse -> 0.5 lowering
                 (with the Phase-3b lambda-lift) -> Verifier -> serialize -> emit .coreir.
                 The Verifier is a GATE: a hard FAIL aborts the build with a precise error and a
                 non-zero exit, never emits a bundle.")
    (hard-limit "CCall resolution and the Verifier run against a registry scope assembled from
                 the supplied .axreg file(s) via the core05 text loader. An UNKNOWN target is a
                 hard FAIL — never a default.")
    (hard-limit "Bundle identity is content_hash of the CANONICAL serialization (fabric/core05/
                 core_ir_05). The .coreir on disk is the capnp 0.5 TRANSPORT encoding; it is NOT
                 what identity hashes over.")
    (hard-limit "pipeline/mod.rs, main.rs, and the gold bins reference ONLY 0.5 types. No legacy
                 CoreTerm / 0.3 capnp symbol may reappear (it was removed in Phase 3d).")
    (hard-limit "Regenerated golds are reviewed as MEANING diffs, not byte diffs: the 0.5 emitter
                 legitimately produces different bytes than the retired legacy path. Document any
                 intended divergence; investigate anything that looks like a semantic regression.")
    (hard-limit "Reuse fabric, core05, core_ir_05, the 0.5 lowering entrypoints, lambda_lift, and
                 the Verifier. Introduce NO new hash/identity/serialization regime and do NOT alter
                 their behaviour — this phase WIRES them, it does not change them.")
    (hard-limit "Do NOT touch rust-bridge/ and do NOT add execution of emitted bundles. Execution
                 is the bridge's job, deferred."))

  (risk
    (cli-half-wired-non-building critical)        ;; left in a broken intermediate state
    (verifier-not-gating-the-build high)          ;; emits bundles that fail verification
    (legacy-symbol-reintroduced high)             ;; undoing Phase 3d
    (gold-churn-masking-a-regression high)        ;; byte-diff noise hides a real semantic change
    (identity-hashing-capnp-not-canonical high)
    (placeholder-registry-left-in-place medium))

  (boundary
    (modify "src/main.rs and src/pipeline/mod.rs to drive the 0.5 end-to-end flow" allowed)
    (modify "src/bin/generate_wave1_golds.rs, generate_wave2_golds.rs, generate_wave3_golds.rs
             to emit 0.5 bundles" allowed)
    (regenerate "golden .coreir fixtures + build/*/program.coreir from the 0.5 pipeline" allowed)
    (add    "a real registry/axis.axreg (compiler primitives) authored per the grammar doc;
             replace/retire registries/minimal.axreg" allowed)
    (add    "CLI flags as needed to supply the .axreg scope (e.g. --reg), mirroring intent" allowed)
    (add    "end-to-end CLI tests (build succeeds; verifier-fail aborts with non-zero exit)" allowed)
    (modify "the behaviour of fabric/core05/core_ir_05/lowering_05/lambda_lift/Verifier" forbidden)
    (modify "rust-bridge/**" forbidden)
    (implement "execution of emitted bundles, recursive composite effect derivation" forbidden)
    (reintroduce "any legacy CoreTerm / 0.3 capnp symbol" forbidden)
    (default forbidden))

  (assumption
    (statement "Golds will be rebaselined to 0.5 output; their legacy byte baselines are obsolete.
                Where a gold's premise was the retired model, retire/rewrite it rather than force a
                legacy-shaped expectation." tentative)
    (statement "registry/axis.axreg uses §5b bootstrap identities (SHA-256 of the primitive name)
                for stable-named compiler primitives — the documented, scope-limited exception —
                not entropy minting, so the registry is reproducible." tentative)
    (statement "Recursive composite effect derivation stays deferred; if a sample program calls a
                composite, the Verifier fail-closes per Phase 4 — choose sample programs/registries
                that do not depend on composite effect verification yet, or report the limitation." tentative))

  (outcome
    (success "`axis` builds a representative source file + .axreg scope into a .coreir bundle via
              parse -> lower -> lambda-lift -> verify -> serialize, exiting 0 with a stable identity."
             "A program with an unknown callable or a type mismatch is REJECTED by the Verifier:
              no .coreir is written and the process exits non-zero with a precise message."
             "pipeline/main/gold bins contain zero legacy symbols; grep for the Phase-3d targets
              still returns zero in live code."
             "All golden .coreir fixtures are regenerated from the 0.5 pipeline; diffs are explained
              as intended 0.5 output, with no unexplained semantic regression."
             "registries/minimal.axreg placeholder is replaced by a real, grammar-conformant registry."
             "cargo build and cargo test pass for the whole crate; all prior-phase tests still green.")
    (failure "The CLI is left non-building, emits a bundle that fails verification, hashes capnp
              framing instead of canonical bytes, reintroduces a legacy symbol, ships the placeholder
              registry, or a semantic regression hides inside gold byte-churn."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 5; bridge swap is the deferred follow-on)
  ;; Specs:    core_ir_spec/axis_core_ir_0_5.md; core_spec/Registry-WorkingDraft.md;
  ;;           core_spec/axreg-text-projection-reference.md  (NORMATIVE .axreg grammar — author the
  ;;           real registry to this; §5b bootstrap identities for compiler primitives)
  ;; Reuse:    src/fabric/*; src/registry/core05/* (text loader -> Scope);
  ;;           src/ir/core_ir_05.rs (Builder, serialize_canonical, encode_capnp);
  ;;           the 0.5 lowering entrypoints + src/lowering/lambda_lift.rs;
  ;;           src/validation/core_ir.rs (the Phase-4 Verifier — run as the build gate).
  ;; Targets:  src/main.rs (the `axis` bin), src/pipeline/mod.rs,
  ;;           src/bin/generate_wave{1,2,3}_golds.rs,
  ;;           registries/minimal.axreg (replace), build/*/program.coreir (regenerate).
  ;; Fixtures pattern: registries/core05_example.axreg, registries/pilot_lowering.axreg.
  ;;
  ;; --- END-TO-END FLOW (the wiring this phase delivers) ---
  ;;   source + --reg <.axreg>
  ;;     -> parse -> AST
  ;;     -> core05 text loader -> active Scope
  ;;     -> 0.5 lowering (+ lambda-lift)            -> CoreBundle
  ;;     -> Verifier(bundle, scope, effect-policy)  -> PASS | hard FAIL (abort, non-zero)
  ;;     -> serialize_canonical -> content_hash      (bundle identity)
  ;;     -> encode_capnp -> write <out>.coreir       (transport)

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_PIPELINE constraints and boundary;
     verifier-is-a-gate, identity-is-canonical-not-capnp, wire-don't-change the 0.5 modules,
     no-legacy-resurrection, and meaning-diff-the-golds are non-negotiable.")
)
