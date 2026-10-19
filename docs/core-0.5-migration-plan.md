# Core 0.5 Migration Plan — getting `axis-lang` back up to speed

**Status:** Planning. No code changes yet.
**Scope decisions (locked with Chris, 2026-06-05):**

1. Produce this written plan before touching code.
2. Commit to the **full content-addressed model** — two-hash registry + minted
   256-bit function identities + `CCall.targetIdentity`. No interim name-shim.
3. Do the **0.5 core/registry rewrite first**; keep the legacy in-tree
   `rust-bridge/` for now and swap to `axis-codegen-bridge` *after* the format
   settles. The existing `docs/rust-bridge-migration-plan.md` targets 0.4 and is
   therefore **superseded for sequencing** — revisit it at the end.

---

## 1. What the rewrite actually changes

The June 5 drafts (`core_ir_spec/axis_core_ir_0.5.{md,capnp}`,
`core_spec/Registry-WorkingDraft.md`, `core_spec/Registry-Format-WorkingDraft.md`)
go materially further than the 0.4 work the bridge migration plan assumed.

### Core IR: 8 nodes → 2 nodes, tree → flat table

| Concern | Old (0.2/0.3, current code) | New (0.5) |
|---|---|---|
| Node kinds | `CIntLit`, `CBoolLit`, `CUnitLit`, `CLam`, `CLet`, `CIf`, `CVar`, `CApp`, `CCall` | **`CCall`, `CIf` only** |
| Literals | literal nodes | **constant pool** entries `(def_hash, bytes)`, referenced by index |
| Lambda / app | `CLam` / `CApp` nodes | **lambda-lifted into the registry** before lowering; no inline abstraction |
| Binding | `CLet` / `CVar` | **structural sharing** — two edges to the same node index; no naming |
| Shape | recursive `CoreTerm` tree | **flat node table in topological order**; edges are integer indices |
| Dispatch | `CCall.target_name: Text` | **`CCall.targetIdentity: Hash256`** (minted token) |
| Annotations | `Vec<Annotation>` + `node_id` on every node | **removed from canonical form** (side channel only) |
| Bundle header | `version`, `entrypoint`, exports, `effectClass`, etc. | **no header** — bundle identity = content hash; `version` is a wire marker only |
| Constants overhead | out-of-bundle interning | local constant pool, no external lookup |

Invariant that buys the simplicity: **for every edge `node(i)` inside the node at
index `j`, `i < j`.** Cycles become unrepresentable; blast-radius, origin-trace,
and effect-class derivation are all linear array passes.

### Registry: text-canonical → binary-canonical, with the two-hash split

| Concern | Old (`src/registry/mod.rs`, spec 0.1) | New (working drafts) |
|---|---|---|
| Source of truth | `.axreg` **text is authoritative** | **canonical binary**; `.axreg` is a lossy, never-hashed projection |
| Entry model | `name + arity + deterministic + profiles + numeric u64 id` | six record kinds (below) |
| Type identity | none | **content hash of the `TypeDef`** (structural; identical shapes dedupe) |
| Function identity | numeric counter `id` | **minted 256-bit token stored in `FnGenesis`**, never derived from content |
| Signature check | arity only | **per-argument `def_hash` match** against the frozen genesis signature |
| Contracts | none | **`ContractAppend`** chain, append-only, clauses are predicate-function identities |
| Resolution | name lookup | **fold by identity**; unresolved → hard **UNKNOWN gate** |

Record kinds: `TypeDef (0x01)`, `FnGenesis (0x02)`, `ContractAppend (0x03)`,
`LeafFacts (0x04)`, `CompositeBody (0x05)`, `Implementation (0x06)`.

**The spine of the whole migration is `name → minted 256-bit identity`.** Lowering,
emit, registry, validation, and (later) the bridge all hang off it.

---

## 2. Current code inventory & blast radius

What exists today, all on the old model:

- `build.rs` — compiles `core_ir_spec/axis_core_ir_0_3.capnp`. Must point at the
  0.5 schema (regenerate the `_capnp.rs`).
- `src/ir/core_ir.rs` (601 lines) — the 8-node recursive `CoreTerm` with
  `Annotation`/`node_id`, constructors (`int_lit`, `lam`, `clet`, `cvar`, `capp`,
  `ccall(target_name, …)`), `bundle_v0_2/v0_3`, and `encode_capnp` /
  `write_core_term` against the 0.3 schema. **Full rewrite.**
- `src/registry/mod.rs` (325 lines) — `RegistryEntry { name, arity, deterministic,
  profiles, id }`, `.axreg` text parser treated as authoritative. **Full rewrite**
  to the six-record binary model + `.axreg` projection loader.
- `src/validation/core_ir.rs` (337 lines) — `validate_minimal_core_bundle` /
  `validate_registry_core_bundle` check `target_name` non-empty and name-in-registry.
  **Rewrite** to: topological-order check, `NodeRef` bounds, `targetIdentity`
  resolves, per-arg `def_hash` match, UNKNOWN gate.
- **Lowering — 7 files, ~4.8k lines** (`h1_lowering`, `registry_lowering`,
  `minimal_lowering`, `schema_lowering`, `spec_driven`, `nf_lowering`,
  `interpreter`) all construct `CoreTerm`. Each must emit the flat 2-node graph +
  constant pool, and (new) run **closure-conversion / lambda-lifting** so every
  callable is registry-resident before Core IR sees it.
- **Introspection — 4 files** (`inspection`, `code_trace`, `trust`,
  `core_ir_json`) walk `CoreTerm`. Re-target to the flat table (these get
  *simpler* — linear scans).
- `src/execution/interpreter.rs` + `src/lowering/interpreter.rs` — tree-walking
  evaluators over the old nodes. Re-point at the flat table or retire (per §5 Q3).
- `rust-bridge/` — legacy bridge on 0.3 + `shim` + name dispatch. **Deferred.**

Cross-cutting: `Registry`/`RegistryEntry` is referenced in ~12 files incl.
`main.rs`, `pipeline/mod.rs`, all lowerings, validation, hooks.

---

## 3. Sequenced phases (0.5 core/registry rewrite)

Ordered so the project compiles and a thin pipeline stays runnable as early as
possible. Each phase ends green.

### Phase 0 — Foundations & freeze the contract
- Pick the canonical **hash function** and **256-bit identity-minting** scheme
  (entropy-at-width, no coordination — see Registry Format §4). One `hash`
  helper, one `mint_identity` helper, used by both core and registry crates.
- Decide the **`NodeRef` wire tagging** the prose left open (capnp uses a union;
  confirm the in-memory + any text form agree). 
- Add `core_ir_spec/axis_core_ir_0.5.capnp` to `build.rs` **alongside** 0.3 (do
  not delete 0.3 yet) so generated bindings exist to build against.
- Write the canonical **payload encoder** (varint / IEEE-754 / length-prefixed
  UTF-8 / positional product / sum constructor-index) per Registry Format §1 +
  §10 — shared by registry records *and* constant-pool entries.

### Phase 1 — Registry core (the identity authority)
- New `src/registry/` model: the six record structs, canonical binary encode +
  content-hash, minted `FnGenesis` identity, the **fold resolver** keyed on
  identity with the UNKNOWN gate.
- `.axreg` **projection loader**: parse text → mint/lower to canonical records;
  bind names → identities in a *projection-only* side table. Round-trip test:
  two projections differing only in names/comments/layout must produce identical
  canonical bytes (Format §8 conformance).
- Keep a temporary `name → identity` resolution table so existing lowerings can
  be ported incrementally in Phase 3.

### Phase 2 — Core IR data model & codec
- Rewrite `src/ir/core_ir.rs`: `Node { CCall { target_identity, args:
  Vec<NodeRef> }, CIf { cond, then, else } }`, `ConstantPoolEntry`,
  `CoreBundle { version, constant_pool, nodes }`. `NodeRef = Node(u32) |
  Pool(u32)`.
- Replace `encode_capnp`/`write_core_term` with a **single forward-pass**
  encoder against the 0.5 schema, and a decoder that builds a random-access
  array.
- A **builder** that maintains topological order + interns constants into the
  pool + dedupes shared subgraphs to a single index (free sharing).

### Phase 3 — Lowering rewrite (the big one)
- Add the mandatory **closure-conversion / lambda-lift** pass: every anonymous
  function → synthesised `FnGenesis` identity; every capture → explicit arg.
  After this pass the graph is closed (Tier 1/2 per spec).
- Port each of the 7 lowering paths to the builder: literals → pool, `let`/`var`
  → shared indices, `app` of a known callable → `CCall` by identity. Do the
  simplest first (`minimal_lowering`) to validate the builder end-to-end, then
  the rest.
- Retarget the 4 introspection walkers + chosen interpreter to the flat table.

### Phase 4 — Validation / Verifier
- Rewrite `src/validation/core_ir.rs` to the spec's Verifier steps: topo-order
  invariant, `NodeRef` in range, every `targetIdentity` resolves, every arg
  `def_hash` matches the genesis signature, derive effect class from `CCall`
  targets. (Ledger admission-event emission is out of scope for the lab unless
  we want a stub.)

### Phase 5 — Pipeline, CLI, fixtures, cleanup
- Wire `pipeline/mod.rs`, `main.rs`, and the `generate_wave*_golds` bins to the
  new types. Regenerate golden `.coreir` fixtures (incl. `build/ai2/program.coreir`).
- Author a real `registries/*.axreg` (replace the placeholder `minimal.axreg`)
  exercising `TypeDef` + `FnGenesis` + a `ContractAppend`.
- Remove 0.3 from `build.rs`; delete dead 0.2/0.3 code paths and stale node
  constructors. Grep for `target_name`, `CLam`, `node_id`, annotations.

### Phase 6 — Verify
- Round-trip tests (encode→decode→encode is byte-identical; bundle hash stable).
- Topological-invariant fuzz: reject any bundle with a forward edge.
- Conformance: identity dedupe for `TypeDef`, identity distinctness for two
  same-signature `FnGenesis`, UNKNOWN gate on missing identity.
- Diff regenerated golds vs the old pipeline's intent (semantics, not bytes —
  bytes will differ).

### Deferred — bridge swap
Once `axis` emits stable 0.5 bundles, return to the bridge: confirm whether
`~/dev/axis-codegen-bridge-rs` speaks 0.5 or only 0.4 (it was 0.4 at
last note), then run the swap from `docs/rust-bridge-migration-plan.md` updated
to 0.5. Until then the legacy in-tree bridge stays.

---

## 4. Decisions to lock during implementation
1. **Hash + mint primitives** (Phase 0) — exact hash function and the
   minter-entropy/prefix layout for 256-bit identities.
2. **`NodeRef` encoding** — the capnp union is clear; confirm the canonical
   *binary* (non-capnp) tagging scheme if we hash bundles outside capnp framing.
3. **Constant-pool boundary** — size threshold above which a value goes to
   axStorage by content hash instead of being inlined.
4. **Closure-conversion home** — confirm it lives in the lowering layer (surface
   → core), not in Core IR, per spec §"Abstraction and Lowering".
5. **Interpreter fate** — keep a flat-table evaluator in the lab for testing, or
   declare execution the bridge's job and retire the in-tree interpreters?
6. **Ledger** — do we stub admission events in the lab, or leave the Verifier's
   step 5 out entirely for now?

## 5. Risks
- **Lowering is the cost center** (~4.8k lines across 7 files) and gains a brand
  new lambda-lift pass. Sequence simplest-first; expect this to dominate effort.
- **Identity model is all-or-nothing** (we chose full): until the registry mint
  + fold is in, nothing downstream resolves. Phase 1 is on the critical path for
  everything after it.
- **Capnp framing vs. canonical hashing.** The bundle identity is the content
  hash of the *canonical encoding*; if we hash capnp-framed bytes we must pin the
  framing as canonical, or hash a separate canonical serialization. Decide in
  Phase 0 to avoid a late rework.
- **Golden churn.** Every `.coreir` fixture and the wave-gold generators change;
  budget for regeneration + review, and diff on meaning not bytes.
- **Two-migration trap avoided** by doing 0.5 before the bridge — but the
  deferred bridge must be re-validated against 0.5, not 0.4.

## 6. Out of scope (for this pass)
- `rust-bridge` → `axis-codegen-bridge` swap (deferred; plan doc to be updated to
  0.5 first).
- Full Ledger implementation (admission events) beyond an optional stub.
- IFC / classification enforcement (consumer concern; Core IR only enables it).
