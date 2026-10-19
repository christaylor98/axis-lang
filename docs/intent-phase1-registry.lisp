;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_1_REGISTRY
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_REGISTRY_CORE)
    (owner chris)
    (scope
      "Implement the binary-canonical Registry core for Core IR 0.5 in
       axis-lang-lab, per core_spec/Registry-WorkingDraft.md and
       Registry-Format-WorkingDraft.md, building on the existing src/fabric
       module (hash + codec) landed in Phase 0.
       INCLUDES: the six record kinds, content-hash type identity, minted
       function identity, the fold resolver with UNKNOWN gate, and the .axreg
       text-projection loader.
       EXCLUDES: Core IR node/bundle rewrite (Phase 2), lowering rewrite
       (Phase 3), validation/Verifier (Phase 4), and the rust-bridge swap."))

  (goal
    (primary "A self-contained registry module that mints/stores/resolves
              definitions by identity, with binary as the source of truth.")
    (secondary "Keep the crate compiling and all existing tests green."
               "Verify the Phase-0 fabric module compiles and its unit tests pass.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (spec-fidelity high)
    (crate-stays-building high)
    (ergonomics low))

  (constraint
    (hard-limit "Canonical form is BINARY; .axreg text is a lossy, NEVER-hashed projection.")
    (hard-limit "Two hashes never conflated: a TYPE's identity IS its content hash;
                 a FUNCTION's identity is a MINTED token stored in FnGenesis, never derived.")
    (hard-limit "Resolution keys on identity only; an unresolved identity is a HARD HALT
                 (UNKNOWN gate) — never a default, never a guess.")
    (hard-limit "Records are immutable and append-only; the contract grows via ContractAppend chained by prev-hash.")
    (hard-limit "Reuse src/fabric (content_hash, mint_identity, codec primitives);
                 introduce NO second serialization regime.")
    (hard-limit "Do not modify the legacy 0.3 emit/decode path or rust-bridge/.")
    ;; --- settled open questions (folded in from design review) ---
    (hard-limit "In .axreg, a function's minted identity is AUTHORED literally in the
                 fn block; the loader NEVER mints at lowering time. Type identities are
                 computed (content hash) and never written. Re-lowering an unchanged
                 projection MUST produce byte-identical canonical output (Format §8)."))

  (risk
    (silent-fusion-of-distinct-functions unacceptable)   ;; minting derived from content
    (caller-breakage-on-reimplementation high)           ;; identity tied to impl
    (non-deterministic-canonical-bytes critical)          ;; breaks hashing
    (text-treated-as-authoritative high))

  (boundary
    (modify "src/registry/**" allowed)
    (add    "registry unit tests + a real registries/*.axreg fixture" allowed)
    (modify "src/fabric/** (only additive helpers if strictly needed)" allowed)
    (modify "src/ir/**, src/lowering/**, src/validation/**, rust-bridge/**" forbidden)
    (change "the canonical encoding decisions locked in src/fabric/codec.rs" forbidden)
    ;; --- settled open questions (folded in from design review) ---
    (implement ".axreg grammar: registry header, type (prim/product/sum), leaf fn, impl" allowed)
    (parse-but-defer "composite fn body derivation" allowed)
    (emit "placeholder/guessed composite fact values" forbidden)
    (default forbidden))

  ;; (unknown) block resolved during design review — both items are now settled
  ;; as constraints/boundary above and the assumption below.

  (assumption
    (statement "Phase 0 fabric (Hash256, content_hash, mint_identity, codec) is correct
                and stable; verify by running its tests before building on it." tentative)
    (statement "A temporary name->identity table is acceptable so later phases can
                port incrementally." tentative)
    (statement "CompositeBody is stored as a record in Phase 1; fact derivation
                (effect_class=max, deterministic=conjunction, idempotent=conservative-false)
                returns an explicit PendingDerivation state until the Core IR 0.5 graph is
                walkable in a later phase. Conservative-ceiling values may be returned ONLY
                behind an explicit pending flag, never silently." tentative))

  (outcome
    (success "Round-trip: encode a definition to canonical bytes, hash it, decode it,
              re-encode — bytes are identical and the hash is stable."
             "Two structurally identical TypeDefs produce the SAME identity (dedupe)."
             "Two FnGenesis mints of the same signature produce DISTINCT identities."
             "Resolving a missing identity returns a hard UNKNOWN error, not a default."
             "Two .axreg projections differing only in names/comments/layout lower to
              identical canonical bytes."
             "cargo build and cargo test pass for the whole crate.")
    (failure "Any canonical encoding is non-deterministic, or text is hashed, or an
              unresolved identity silently defaults, or composite facts are guessed."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Specs:   core_spec/Registry-WorkingDraft.md, core_spec/Registry-Format-WorkingDraft.md
  ;;          core_ir_spec/axis_core_ir_0_5.{md,capnp}
  ;; Plan:    docs/core-0.5-migration-plan.md  (this is Phase 1)
  ;; Existing: src/fabric/{hash.rs,codec.rs}   (Phase 0 — reuse, do not duplicate)
  ;; Records: TypeDef 0x01, FnGenesis 0x02, ContractAppend 0x03,
  ;;          LeafFacts 0x04, CompositeBody 0x05, Implementation 0x06
  ;; Step 0:  run `cargo test fabric::` to confirm Phase 0 is green before starting.
  ;;
  ;; --- .axreg PROJECTION GRAMMAR (Phase 1 target; line-oriented) ---
  ;;   registry <name> <version>          ; projection identity only, no semantic effect
  ;;   type <Name> = prim <unit|bool|int|float|bytes|text>
  ;;   type <Name> = product (<TypeName>, ...)        ; positional; labels are projection-only
  ;;   type <Name> = sum [ <Ctor>(<TypeName>...) | ... ]  ; constructor order significant
  ;;   fn <name>
  ;;     identity <0x… 256-bit>           ; REQUIRED, authored, never minted at load
  ;;     kind <leaf|composite>
  ;;     in   (<TypeName>, ...)           ; names resolve to type identities
  ;;     out  <TypeName>
  ;;     effect <pure|reads|writes|fullIo>      ; leaf only
  ;;     deterministic <true|false>             ; absent => false
  ;;     idempotent    <true|false>             ; leaf only
  ;;     disposition   <interactive|batch>      ; optional, advisory
  ;;     body     <0x…>                          ; composite only (Core IR graph hash)
  ;;     contract gen <n> [ <clauseName>, ... ] ; clause names => predicate-fn identities
  ;;     oracle   <0x…>                          ; optional; absent => zero hash
  ;;   end
  ;;   impl <fn-name> satisfies gen <n> artifact <0x…>
  ;;
  ;;   Lowering determinism: canonical field order is fixed by the encoder regardless
  ;;   of text order; comments/whitespace/unrelated-decl ordering are stripped; names
  ;;   resolve to identities/hashes and never appear in canonical bytes; duplicate names
  ;;   in one active scope are a load error.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_REGISTRY_CORE constraints and boundary;
     binary-is-canonical and the two-hash split are non-negotiable.")
)
