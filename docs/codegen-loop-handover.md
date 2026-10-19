# Code-Gen Loop — Handover

**Purpose:** what the Core IR 0.5 migration changed, and what the code-generation
loop must now assume, consume, and never violate. Read this before generating
code from any bundle.

**One-line summary:** the lab now produces **content-addressed, two-node, flat,
identity-dispatched** Core IR 0.5 bundles, validated by a static Verifier, and
hands them to the standalone bridge as the codegen input. Names, literals,
lambdas, and the bundle header are gone; everything a callable *is* lives in the
registry, keyed by a 256-bit identity.

---

## 1. What you receive

The codegen input is a **Core IR 0.5 `CoreBundle`** (`.coreir` on disk is the
capnp 0.5 *transport* encoding). Its shape:

```
CoreBundle { version, constant_pool: [ConstantPoolEntry], nodes: [Node] }
ConstantPoolEntry { def_hash: Hash256, payload: bytes }
Node = CCall { target_identity: Hash256, args: [NodeRef] }
     | CIf   { cond: NodeRef, then_: NodeRef, else_: NodeRef }
NodeRef = Node(u32) | Pool(u32)
```

There are **exactly two node kinds** — `CCall` and `CIf`. There are no literal
nodes, no lambda/apply, no let/var, no annotations, and no semantic header.
Whatever the program means is expressed entirely as these two nodes plus the
constant pool.

The node table is **flat and in topological order**: for every `NodeRef::Node(i)`
inside the node at index `j`, `i < j`. So you can emit in index order and every
dependency is already emitted before its use. Cycles are unrepresentable.

**Structural sharing is real.** A value used in two places is *one* node (or one
pool entry) referenced by the same index twice. Emit each node/pool entry once
(e.g. `let node_N = …`) and reference it; do not duplicate.

`version` is a wire marker only — read it to pick a decoder, then ignore it. It
is **not** part of the bundle's meaning.

---

## 2. The biggest change: dispatch by identity, not name

A `CCall` names its callee by a **256-bit minted identity** (`Hash256`), never by
a string. Names do not exist anywhere in the bundle. To know what a `CCall`
*does*, you must resolve its `target_identity` against the **registry**.

- The registry is supplied to the bridge via `--reg <file.axreg>`.
- Resolution yields the callee's frozen signature (`FnGenesis.inputs`/`output`),
  its kind (leaf | composite), and — for a leaf — its declared facts
  (`effect_class`, `deterministic`, `idempotent`).
- An identity that does not resolve is a **hard halt (UNKNOWN gate)** — never a
  default, never a guess. Do not emit code for an unresolved target.

### Compiler-primitive identities use SHA-256(name) (§5b)

`registries/axis.axreg` (the compiler primitives) authors each function's
identity as `SHA-256(function-name)` — a deliberate, documented exception to
"identity is minted, not content-derived," used only for stable-named primitives
so builds are reproducible. The bridge resolves `CCall` identities this way for
those primitives. **If you add a new primitive**, register it in `axis.axreg`
with the matching `SHA-256(name)` identity *and* implement its runtime behavior;
the two must agree or resolution breaks.

(General application functions are NOT named-derived — their identities are minted
random tokens. Only the primitive registry uses §5b.)

---

## 3. Constants live in the pool

Literals are `ConstantPoolEntry { def_hash, payload }`, referenced by
`NodeRef::Pool(i)`. The `payload` is the canonical value encoding (Registry
Format §10): varint zig-zag for signed ints, fixed 8-byte big-endian IEEE-754 for
floats, length-prefixed UTF-8 for text, single byte for bool, zero bytes for
unit; products are positional concatenation, sums are a constructor-index varint
then the constructor's fields. The `def_hash` is the value's type identity — use
it to know how to decode the payload.

The bridge currently decodes `Unit`/`Bool`/`Int`/`Text` pool entries. **Widening
pool/type coverage (more primitives, products, sums) is codegen work** and a
natural first area to extend.

---

## 4. Identity vs. transport (do not conflate)

The bundle's **identity is `content_hash` of its canonical serialization** (the
`fabric::codec` encoding), **not** of the capnp bytes. capnp 0.5 is transport
only. Your emitter must **not** let the wire format or any re-encoding redefine
identity — if you need the bundle's identity, hash the canonical bytes, never the
`.coreir` framing.

---

## 5. The Verifier is the gate

Before a bundle is fit for codegen it passes the static Verifier
(`src/validation/core_ir.rs` in the lab). The Verifier:

1. checks the structural invariants (topological order, `NodeRef` ranges);
2. resolves every `CCall` target (UNKNOWN → hard FAIL);
3. **type-checks every `CCall` site** — each argument's `def_hash` must equal the
   callee's `FnGenesis` input signature by position (arity or type mismatch →
   hard FAIL);
4. **derives** the effect class (max over `CCall` targets) and applies the effect
   policy.

It returns `VerifyPass { effect_class, referenced_def_hashes }` or a precise
`VerifyError`. **Only generate code for bundles that PASS.** Do not re-derive or
second-guess the verdict; consume it.

`effect_class` (`pure < reads < writes < fullIo`) tells you the maximum privilege
the emitted code may exercise — useful if codegen sandboxes or annotates effects.

---

## 6. What is deferred — and how it affects you

**Composite effect derivation is fail-closed today.** A composite's facts are
derived from its body graph, but bodies are not yet resolvable to a `CoreBundle`
(Phase-1 registered `CompositeBody { body: ZERO }` stubs; no body store exists).
So the Verifier **rejects any bundle that calls a composite** with
`CompositeEffectUnderivable`. Practically: **expect leaf-only bundles for now.**
If the codegen loop needs composites, the unblock is documented in
`docs/phase6b-gate-result.md` — it requires a `HashMap<Hash256, CoreBundle>` body
store populated at lowering time, a `verify_bundle` signature change to pass it,
and at least one real composite registration. The derivation algorithm itself
(MAX/AND/conservative-false with cycle detection) is already designed.

**Tier 3 closures are detect-and-halt.** The lambda-lift pass closes higher-order
constructs into named registry functions (Tier 1/2). A genuine runtime closure
through a dynamic-apply mechanism (Tier 3) is rejected, not lowered. So every
`CCall` target you see is a statically-known registry function — you will never
receive a "call whatever closure flows here at runtime" node.

**The Ledger / trust gate is not built.** There is no admission record, provenance
capture, or human sign-off. This is intentional — no consumer yet. It will be
retrofitted as an observer on the `verify_bundle` choke point. **Implication for
you:** keep all admission flowing through the single `verify_bundle` entry point;
do not invent a side path that emits code without going through it, or the future
ledger will have a blind spot.

---

## 7. Reproducible-build caveat (lambda-lifted identities)

Lambda-lifted (synthesised) function identities are **minted**, and currently
**stable within a build but not guaranteed across builds**. Because a `CCall`
carries `target_identity`, a bundle that contains lifted functions can therefore
hash differently across compiles. If the codegen loop relies on **reproducible
bundle identity across builds** (caching keyed on bundle hash, content-addressed
artifact reuse, etc.), this is an open issue — the recommended fix is a sidecar
map that persists `(construct key → minted identity)` so rebuilds reuse the same
token. Flag it before depending on cross-build identity stability.

First-order programs (no lambdas) are unaffected — their identities come straight
from the registry.

---

## 8. Integration points (as wired today)

- The bridge is the **standalone** crate `axis-codegen-bridge-rs` (the in-tree
  `rust-bridge/` has been deleted). Lib `axis_codegen_bridge`, bin
  `axis-codegen-bridge`.
- The lab's build scripts (`scripts/build_{ai1,ai2,axh,surface0}.sh`) invoke it
  via the `BRIDGE_BINARY` env var (default: the external release binary) and pass
  the registry with `--reg`.
- The bridge auto-detects 0.5 bundles (tries the 0.5 loader first, falls back to
  0.4). Its `rust_05.rs` emitter resolves `CCall` identities (§5b for primitives),
  decodes pool entries, and emits flat `let pool_N` / `let node_N` Rust.
- Sample 0.5 bundles to test against: `build/{ai1,ai2,surface0}/program.coreir`.

---

## 9. Invariants the codegen loop must NOT violate

- **Never reintroduce name dispatch.** Resolve identity → registry; names are not
  in the bundle.
- **Never redefine bundle identity** via the wire/emitter — identity is the
  canonical-bytes hash.
- **Never assume `pure`** for an unresolved target or an unresolvable/cyclic
  composite — that is exactly what the fail-closed posture protects.
- **Never bypass the Verifier** — emit only from PASS bundles, and keep admission
  through `verify_bundle`.
- **Emit shared nodes once** — honor structural sharing; the same index is the
  same value.
- **Emit in index order** — topological order means deps precede uses; don't
  reorder.

---

## 10. Authoritative references

- `~/dev/axis-lang/core_ir_spec/axis_core_ir_0_5.md` + `~/dev/axis-lang/core_ir_spec/axis_core_ir_0_5.capnp` — the model + wire schema.
- `~/dev/axis-lang/core_spec/Registry-WorkingDraft.md` + `~/dev/axis-lang/core_spec/Registry-Format-WorkingDraft.md` — the registry model + binary encoding.
- `~/dev/axis-lang/core_spec/axreg-text-projection-reference.md` — the normative `.axreg` grammar (incl. §5b identities).
- `~/dev/axis-lang/docs/core-0.5-migration-plan.md` — the full phased migration record.
- `~/dev/axis-lang/docs/phase6b-gate-result.md` — why composite derivation is deferred and what unblocks it.
- `~/dev/axis-lang/src/validation/core_ir.rs` — the Verifier (the gate and the admission choke point).
- `~/dev/axis-lang/src/ir/core_ir_05.rs` — the bundle model, builder, canonical serializer, capnp codec.
- `~/dev/axis-lang/src/fabric/hash.rs` + `~/dev/axis-lang/src/fabric/codec.rs` — `Hash256`, `content_hash`, `mint_identity`, the canonical primitives + payload encoders.
