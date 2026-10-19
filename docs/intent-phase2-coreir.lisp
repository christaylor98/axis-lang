;; ============================================================
;; INTENT_SPEC.v0.2  —  AXIS_CORE_05_MIGRATION / PHASE_2_COREIR
;;
;; INTERPRETATION RULES:
;; - All blocks are normative and must be satisfied simultaneously.
;; - Absence of permission implies forbidden (default forbidden).
;; - If ambiguity exists, request clarification before executing.
;; - Re-evaluate every change against (constraint) and (boundary).
;; ============================================================

(intent

  (identity
    (name AXIS_CORE_05_COREIR_MODEL)
    (owner chris)
    (scope
      "Implement the Core IR 0.5 in-memory data model, a topological-order
       graph builder, the canonical bundle serializer, and the capnp 0.5
       encode/decode path in axis-lang-lab, building on src/fabric (Phase 0)
       and src/registry/core05 (Phase 1).
       INCLUDES: the two node kinds (CCall, CIf), NodeRef, constant pool,
       CoreBundle, the builder that enforces topological order + interns
       constants + dedupes shared subgraphs, the fabric-based canonical
       serializer that bundle identity hashes over, and capnp 0.5 read/write.
       EXCLUDES: lowering rewrite (Phase 3), the Verifier (Phase 4), the
       rust-bridge swap, and any surface-language or normalisation changes."))

  (goal
    (primary "A self-contained Core IR 0.5 model + codec: build a 2-node flat
              graph, serialize it canonically, hash it for identity, and round-trip
              it through capnp 0.5 with byte-stable canonical output.")
    (secondary "CCall.target_identity resolves against the Phase-1 registry (core05)."
               "Keep the crate compiling; keep fabric + core05 tests green."
               "Repair the CoreBundle-drift breakage so the lib-test target and the
                affected integration tests compile.")
    (type outcome-oriented))

  (actor
    (claude-code (type ai) (role "implementer") (authority bounded) (may-decide false))
    (chris       (type human) (role "architect/approver") (authority full) (may-decide true)))

  (priority
    (spec-fidelity high)
    (crate-stays-building high)
    (ergonomics low))

  (constraint
    (hard-limit "Exactly TWO node kinds exist: CCall and CIf. No literal nodes, no
                 CLam/CApp, no CLet/CVar, no annotations, no bundle header.")
    (hard-limit "The node table is FLAT and in TOPOLOGICAL ORDER: for every NodeRef::Node(i)
                 appearing in the node at index j, i < j. Cycles are unrepresentable.")
    (hard-limit "Constants live in the constant pool as (def_hash, payload-bytes) entries,
                 referenced by NodeRef::Pool(index). No literal nodes, no out-of-bundle interning.")
    (hard-limit "Sharing is structural: two edges to the same value carry the SAME index.
                 The builder must dedupe identical subgraphs to one node, not duplicate them.")
    (hard-limit "CCall dispatches on target_identity: Hash256 (a minted registry identity),
                 NEVER on a name. Core IR carries no names.")
    (hard-limit "Bundle identity = content_hash of the CANONICAL serialization produced by
                 src/fabric/codec — NOT of capnp-framed bytes. capnp is transport only.
                 (Locks the Phase-0 canonical-vs-capnp decision.)")
    (hard-limit "NodeRef wire tag is the one already locked in fabric::codec: node(i) => i<<1,
                 pool(i) => (i<<1)|1. Do not invent a second scheme.")
    (hard-limit "version is a wire-format transport marker only; it is read first then discarded
                 and is NOT part of the meaning hash.")
    (hard-limit "Reuse src/fabric and src/registry/core05; introduce NO new hash or
                 serialization regime.")
    (hard-limit "Do not modify the legacy 0.3 emit/decode path or rust-bridge/; legacy
                 src/ir CoreTerm stays available until Phase 3 ports its consumers."))

  (risk
    (cycle-or-forward-edge-representable critical)        ;; topological invariant not enforced
    (non-deterministic-canonical-bytes critical)          ;; breaks bundle identity
    (hashing-capnp-framing-instead-of-canonical high)     ;; identity drifts across capnp versions
    (duplicated-shared-subgraphs high)                    ;; sharing not structural
    (name-leaking-into-core-ir high)                      ;; dispatch by name instead of identity
    (arg-defhash-unchecked medium))                       ;; mismatch deferred — see boundary

  (boundary
    (modify "src/ir/** (new 0.5 model; legacy CoreTerm may coexist)" allowed)
    (add    "core_ir 0.5 builder + canonical serializer + capnp 0.5 codec" allowed)
    (add    "core_ir 0.5 unit tests + a sample .coreir fixture" allowed)
    (use    "src/registry/core05 to resolve CCall target_identity" allowed)
    (modify "src/introspection/core_ir_json.rs and other lib-target callers ONLY as needed
             to repair CoreBundle-drift compile errors" allowed)
    (repair "tests/phase1_validation.rs and tests/end_to_end_all_surfaces.rs IF the fix is a
             mechanical consequence of the new model; otherwise leave and report" allowed)
    (modify "src/lowering/**, src/validation/** (semantic rewrite)" forbidden)
    (modify "rust-bridge/**, the legacy 0.3 capnp path" forbidden)
    (change "the canonical encoding + NodeRef tag decisions locked in fabric::codec" forbidden)
    (implement "the Verifier / blast-radius analysis (Phase 4)" forbidden)
    (default forbidden))

  (assumption
    (statement "Per-argument def_hash type-checking at a CCall site is the Verifier's job
                (Phase 4); Phase 2 stores target_identity + arg NodeRefs and need only ensure
                structural validity (indices in range, topological order), not signature match." tentative)
    (statement "The builder may resolve CCall target_identity against an active core05 Scope
                to fail fast on UNKNOWN, but full admission/type-check is deferred to Phase 4." tentative)
    (statement "Legacy src/ir CoreTerm and the 0.3 capnp path remain in the tree until Phase 3
                ports their consumers; Phase 2 adds the 0.5 model alongside them." tentative)
    (statement "Large values (arrays/blobs) are NOT inlined into the pool; a pool entry whose
                payload is an axStorage content-hash reference is acceptable, full large-value
                handling may be deferred." tentative))

  (outcome
    (success "Build a bundle with the builder, serialize canonically, hash it, decode it,
              re-serialize — canonical bytes are identical and the bundle hash is stable."
             "Round-trip through capnp 0.5 (encode -> bytes -> decode) reproduces the same
              in-memory bundle and the same canonical hash."
             "A constant referenced twice appears once in the pool and both uses carry the
              same NodeRef::Pool index."
             "A value produced once and consumed by two nodes is one node with two referrers
              at the same index (structural sharing, no duplication)."
             "The builder/serializer reject (or make unrepresentable) any forward edge:
              every Node edge points to a strictly smaller index."
             "A CCall whose target_identity is absent from the active core05 scope fails fast
              with the UNKNOWN error, not a default."
             "cargo build and cargo test pass for the whole crate; fabric + core05 tests still green.")
    (failure "Canonical bytes are non-deterministic, or bundle identity is computed over capnp
              framing, or a forward edge/cycle is representable, or a shared value is duplicated,
              or a name appears in Core IR, or an unknown target silently defaults."))

  (mode
    (phase execution)        ;; human-approved; design + execution permitted for THIS phase only
    (design allowed)
    (execution allowed))

  (status
    (state approved)
    (authority human)
    (execution-allowed true))

  ;; --- WORKING CONTEXT (reference, do not re-derive) ---
  ;; Specs:    core_ir_spec/axis_core_ir_0_5.md  (model)
  ;;           core_ir_spec/axis_core_ir_0_5.capnp  (wire schema; generated as
  ;;             generated::axis_core_ir_0_5_capnp, already wired into build.rs)
  ;;           core_spec/Registry-Format-WorkingDraft.md §10 (constant-pool payload encoding)
  ;; Plan:     docs/core-0.5-migration-plan.md  (this is Phase 2)
  ;; Existing: src/fabric/{hash.rs,codec.rs}        (Phase 0 — reuse; NodeRefBin tag lives here)
  ;;           src/registry/core05/{record,codec,fold,text}.rs  (Phase 1 — reuse for resolution)
  ;; Legacy:   src/ir/core_ir.rs (0.3 CoreTerm, recursive) — DO NOT remove; coexists until Phase 3.
  ;;
  ;; --- CoreBundle SHAPE (0.5) ---
  ;;   CoreBundle { version: String, constant_pool: Vec<ConstantPoolEntry>, nodes: Vec<Node> }
  ;;   ConstantPoolEntry { def_hash: Hash256, payload: Vec<u8> }   ; payload per Registry Format §10
  ;;   enum Node { CCall { target_identity: Hash256, args: Vec<NodeRef> },
  ;;               CIf   { cond: NodeRef, then_: NodeRef, else_: NodeRef } }
  ;;   enum NodeRef { Node(u32), Pool(u32) }   ; encode via fabric::codec::{encode,decode}_noderef
  ;;   note: capnp field `else` is a Rust keyword — generated as `else_`/escaped; map accordingly.
  ;;
  ;; --- BUILDER CONTRACT ---
  ;;   * push_node returns the new index; edges may only reference already-pushed indices.
  ;;   * intern_constant(def_hash, payload) returns a stable Pool index, deduping identical entries.
  ;;   * identical subgraphs collapse to one node index (hash-cons on canonical node bytes).
  ;;   * finish() validates the topological invariant and emits CoreBundle.

  (rehydration-rule
    "All reasoning must comply with AXIS_CORE_05_COREIR_MODEL constraints and boundary;
     two-nodes-only, flat-topological-order, structural-sharing, identity-not-name, and
     canonical-hash-not-capnp-framing are non-negotiable.")
)
