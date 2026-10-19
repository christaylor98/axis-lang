# Registry Format — Working Draft

**Status:** Working draft — current best thinking, **not** settled ground. The
encoding rules below are stated precisely so an implementation can follow them
exactly or diverge deliberately, but they are unproven against a build and
subject to revision. Implements the Registry working draft; where the two
conflict, the Registry model governs and this document governs only the
encoding.

---

## Committed Decisions (read first)

This format bakes in three decisions from the design record. They are stated up
front because they are load-bearing and reversing them would reshape the format:

1. **The canonical form is binary; text is a projection.** A definition's source
   of truth is its canonical binary encoding, over which all hashes are computed.
   Any text form (`.axreg`) is a lossy, non-authoritative projection that lowers
   to the canonical form. This mirrors the rest of the fabric (binary IR
   canonical, text projected) and corrects Registry 0.1, which made text
   canonical.

2. **One serialization regime.** Definitions are content-addressed nodes with
   hash-edges to other definitions — the same shape as the IR graph and value
   payloads. They reuse the fabric's existing canonical encoding primitives
   rather than introducing a registry-specific format.

3. **Two hashes, never conflated.** A *type's* identity **is** its content hash
   (structural). A *function's* identity is a **minted token stored in the
   record**, distinct from the record's content hash (nominal). References and
   dispatch use *identity*; storage, integrity, and ledger pinning use the
   *content hash*.

---

## 1. Encoding Primitives

The canonical encoding uses only these primitives. All are deterministic: a
given value has exactly one byte sequence.

| Primitive | Encoding |
|---|---|
| `varint` | LEB128, unsigned, **no redundant high bytes** (minimal form). |
| `hash` | Fixed **32 bytes** (256-bit), matching the fabric's content-address width. |
| `bool` | One byte, `0x00` or `0x01`. No other value is valid. |
| `bytes` | `varint` length prefix, then exactly that many bytes. |
| `seq<T>` | `varint` count, then that many `T` encoded in order. |

Canonicalisation rules (mandatory, because records are hashed):

* Fields appear in the fixed order given below — never reordered.
* No optional-field omission: a field with no value uses its defined zero form
  (e.g. the all-zero `hash` for "absent reference").
* No names anywhere in the canonical form. Names are projection only (§8).
* No comments, no whitespace, no framing beyond what is specified.

A consumer that cannot decode a record under these rules MUST reject it.

---

## 2. Record Kinds

A registry store is a collection of **records**. Each begins with a one-`varint`
kind tag:

| Tag | Record | Purpose |
|---|---|---|
| `0x01` | `TypeDef` | a structural type definition |
| `0x02` | `FnGenesis` | mints a function identity; freezes its signature |
| `0x03` | `ContractAppend` | appends a contract generation (append-only) |
| `0x04` | `LeafFacts` | declared bridge facts for a leaf function |
| `0x05` | `CompositeBody` | the body graph of a composite function |
| `0x06` | `Implementation` | an admissible implementation of a function |
| `0x07` | `ChannelDecl` | declares a named inter-process channel |

Records are immutable once written. The registry never mutates a record; it
**appends** new records (notably `ContractAppend`), and the resolved state of a
definition is the *fold* of its records (§7).

---

## 3. Type Definitions (`0x01`)

A type is structural: its identity is the content hash of its shape.

```
TypeDef:
  tag:        varint = 0x01
  shape_kind: varint            // 0 = primitive, 1 = product, 2 = sum
  body:       per shape_kind below

  primitive (shape_kind = 0):
    prim_code: varint           // canonical primitive code — assigned values:
                                //   0 = int      (LEB128 signed integer)
                                //   1 = text     (varint length + UTF-8 bytes)
                                //   2 = bool     (0x00 = false, 0x01 = true)
                                //   3 = unit     (zero bytes)
                                //   4 = float    (raw UTF-8 decimal string — NEVER binary float)
                                //   5 = dec      (raw UTF-8 decimal string — trailing zeros significant)
                                //   6 = fn       (32-byte minted identity hash of referenced fn)
                                //   7 = bytes    (raw byte sequence for large values / axStorage refs)

  product (shape_kind = 1):
    fields:    seq<hash>         // ordered field TYPE identities; positional, no names

  sum (shape_kind = 2):
    ctors:     seq< seq<hash> >  // ordered constructors; each is an ordered list of field type identities
```

**Identity of a `TypeDef` = `hash(its full canonical encoding)`.** Two
structurally identical types therefore produce the **same** identity and
deduplicate, including when minted on disconnected systems — which is correct,
because identical data shapes *should* converge.

A `TypeDef` is the **schema** that drives payload layout: a product payload is
its fields concatenated in declared order; a sum payload is a constructor index
(`varint`) followed by that constructor's fields; a primitive payload uses the
primitive's canonical encoding. The bridge reads this schema to access data.

---

## 4. Function Genesis (`0x02`)

`FnGenesis` is the singular mint event for a function. It establishes the
immutable identity and freezes the signature as the genesis clause.

```
FnGenesis:
  tag:         varint = 0x02
  identity:    hash              // MINTED, opaque, stored (NOT derived from this record)
  inputs:      seq<hash>         // ordered input TYPE identities (the frozen signature)
  output:      hash              // output TYPE identity
  record_kind: varint            // 0 = leaf, 1 = composite
  disposition: varint            // 0 = unspecified, 1 = interactive, 2 = batch  (advisory; see Foundation §Disposition)
  variadic:    bool              // true = fn accepts variable number of args beyond the declared inputs
                                 // false (default) = exactly the declared inputs, no more
                                 // Used by: value_construct, value_list_construct, event_subscribe
```

**`identity` is a minted token, not a hash of this record.** It is generated
once, at this event, as a fresh unique 256-bit value, optionally prefixed with
opaque minter/context entropy for forensics and partition-safety. It is **never
regenerated** — thereafter it is only referenced. Uniqueness is achieved by
entropy at width, requiring no coordination, which is the only scheme that
survives minting on disparate, disconnected systems. The function's content and
signature contribute **nothing** to the identity; this is what keeps two
disconnected, same-signature functions correctly distinct rather than silently
fused.

The **content hash** of a `FnGenesis` record (used for chaining and pinning) is
`hash(its canonical encoding)`, which is distinct from `identity`.

---

## 5. Contract Append (`0x03`)

The contract grows append-only. Each `ContractAppend` adds one generation,
chained to the previous by content hash.

```
ContractAppend:
  tag:          varint = 0x03
  identity:     hash             // the function whose contract this extends
  prev:         hash             // content hash of the prior generation record (FnGenesis for gen 1)
  generation:   varint           // monotonic, prev.generation + 1
  added:        seq<hash>        // added clauses: each is a PREDICATE FUNCTION identity that must hold over (inputs, output)
  oracle:       hash             // content-addressed characterization corpus, or all-zero if none
```

A contract **clause is a reference to a predicate function** (by identity) that
must hold over the function's `(inputs, output)`. Clauses are thus themselves
ordinary functions in the registry — the contract is built from the same
material it governs. The `oracle` references the characterization corpus that
pins behaviour the clauses do not formalise (Foundation §Protected Behaviour).

Append-only is enforced structurally: a record may only **add** to `added`; it
never references or rewrites a prior clause. The chain via `prev` makes the
generation history tamper-evident.

---

## 6. Facts and Implementation (`0x04`, `0x05`, `0x06`)

### LeafFacts (`0x04`) — declared, irreducible

```
LeafFacts:
  tag:           varint = 0x04
  identity:      hash
  effect_class:  varint          // 0 = pure, 1 = reads, 2 = writes, 3 = fullIo
  deterministic: bool
  idempotent:    bool
  background:    bool            // true = bridge spawns in separate execution context,
                                 //        returns immediately to caller
                                 // false (default) = runs inline; caller waits
  startup:       bool            // true = bridge launches at boot as independent process loop
                                 // false (default) = called on demand
  entry_point:   bytes           // non-empty = bridge wires as named external entry point
                                 //   e.g. "http_request", "interrupt_keyboard"
                                 // bridge routes incoming events to this fn by identity hash
                                 // empty = not an entry point (default)
  callbacks:     seq<varint>     // arg indices the bridge calls synchronously within this fn's frame
                                 // e.g. [0] = arg 0 is a Fn called within the leaf's execution
                                 // closure rule: referenced fn args must not escape the frame
                                 // empty = no synchronous callbacks (default)
```

These are declared because the leaf's mechanism is opaque and nothing can derive
them. `deterministic` is included despite having no current consumer — the
conservative default (false) is safe and retrofitting later costs a contract
append. `independence` is absent: its consumer (a scheduler) does not exist yet
and will be added when it does.

The bridge execution fields (`background`, `startup`, `entry_point`, `callbacks`)
are bridge facts — properties of the execution mechanism, not the semantic graph.
H1 code never reads these fields. The bridge reads them on dispatch to decide how
to run the fn.

### CompositeBody (`0x05`) — facts derived, not declared

```
CompositeBody:
  tag:       varint = 0x05
  identity:  hash
  body:      hash                // a Core IR graph
```

A composite's facts (`effect_class`, `deterministic`, etc.) are **derived** from
its body's components per the Foundation rules, not stored. A consumer MAY cache
the derived facts, but the cache is non-authoritative; the body is the truth.

### Implementation (`0x06`)

```
Implementation:
  tag:        varint = 0x06
  identity:   hash
  satisfies:  varint             // contract generation this implementation satisfies
  artifact:   hash               // content-addressed native bridge (leaf); for a composite this equals its body
```

Multiple `Implementation` records for one `identity` may coexist; each is
admissible iff it satisfies the named contract generation. Identity and contract
are unchanged by adding or replacing implementations.

### ChannelDecl (`0x07`)

```
ChannelDecl:
  tag:           varint = 0x07
  name:          bytes           // channel name (UTF-8) — unique within a registry scope
  type_identity: hash            // TYPE identity of values carried by this channel
                                 // typically the Value type (def_hash of Value TypeDef)
```

Declares a named inter-process channel. Channels are **static** — they must be
declared in the registry at compile time. Dynamic channel creation at runtime is
forbidden.

The bridge uses `ChannelDecl` records to:
- Create channel buffers at startup
- Validate `channel_send(name, value)` call sites — reject unknown names
- Map each declared channel to an event ID for `event_subscribe`

`name` is a UTF-8 byte sequence (no embedded nulls). Channel names must be unique
within the active scope. `channel_send` calls that reference a name not present
in any `ChannelDecl` in the active scope are a hard compile error.

---

## 7. Resolution (the fold)

To resolve an `identity` against an active scope (a set of records):

1. Find the `FnGenesis` (or, for a type, the `TypeDef` whose hash equals the
   identity). Absent → **UNKNOWN gate** (hard halt; never a default).
2. Follow the `ContractAppend` chain via `prev` to the highest generation
   present; the resolved contract is the union of all clauses (monotone).
3. Attach facts: `LeafFacts` (declared) for a leaf, or derive from
   `CompositeBody` for a composite.
4. Collect admissible `Implementation` records.

Resolution keys on **identity** only. There is no name in the canonical form to
resolve against, so the active scope determines only *whether* an identity
resolves, never *what it means*. Composition of the active scope (which records
are present) is external to this format, per the Foundation Specification.

---

## 8. Text Projection (`.axreg`) — Secondary, Non-Canonical

A text form MAY be provided for authoring and inspection. It is **not the source
of truth**, is **never hashed**, and **lowers to** the canonical binary of §§3–6.
It carries information that is *projection only*:

* **Names** — bound to identities for humans and tools. A name is a label on an
  identity, not the identity; renaming changes only this projection.
* **Comments** — stripped on lowering; they have no canonical existence.
* **Layout** — whitespace, ordering of unrelated declarations; cosmetic only.

Illustrative (non-normative) projection of a leaf function:

```
// projection only — lowers to FnGenesis + ContractAppend + LeafFacts
fn score_population            // name: a projection binding to the identity below
  identity   0x7f3a…e91c       // the minted token (shown truncated for humans)
  in         PopulationBatch   // names resolve to type identities
  out        ScoreVector
  effect     reads
  deterministic true
  idempotent  true
  independence true            // "may go wide" — no mechanism named
  disposition batch
  contract   gen 3             // clauses are predicate-function names, resolved to identities
  oracle     0x9c02…           // characterization corpus reference
end
```

Two projections that differ only in names, comments, or layout MUST lower to the
**identical** canonical bytes and therefore the identical content hash. If they
do not, the lowering is non-conforming. A projection is the registry analogue of
a surface program: convenient, lossy, and downstream of the canonical form —
never the form itself.

---

## 9. Conformance

An implementation conforms iff:

1. It computes type identity as the content hash of the canonical `TypeDef`.
2. It treats function identity as the minted token in `FnGenesis`, never derived
   from content.
3. It enforces the canonicalisation rules of §1 (deterministic bytes; no names,
   comments, or framing in canonical form).
4. It resolves by identity, halts to UNKNOWN on an unresolved identity, and
   never defaults.
5. It treats any text form as a non-authoritative projection that lowers to the
   canonical binary, and never hashes the text.

---

## 10. Relationship to Core IR Bundles

Core IR bundles carry a **constant pool** — a local inline table of small
literal values. Each pool entry is a `(def_hash, bytes)` pair where:

* `def_hash` is a type identity per §3 of this format (a `TypeDef` content
  hash, resolved against the active registry scope).
* `bytes` is the canonical payload encoding of the value, using the primitive
  encoding rules of §1 — varints for integers, fixed IEEE-754 for floats,
  length-prefixed UTF-8 for text, positional field concatenation for products,
  constructor index for sums.

This format spec defines both of those things. The constant pool introduces no
new encoding concepts; it uses the type and payload machinery already here.
Large values (arrays, blobs) are not inlined into the pool — they reference
axStorage by content hash and resolve against the type registry independently.

The pool is a Core IR bundle concern, not a Registry concern. It is noted here
only to make clear that the encoding of pool entries is governed by this
document, and that an implementation of the Registry format that also implements
Core IR bundle encoding does not need a separate serialisation regime for
constants.

---

## Rationale (Non-Normative)

The format follows from the model rather than the reverse, which is why it is
short. Because identity, type, contract, oracle, body, and implementation are
all either hashes or hash-referencing records, a definition is just a
content-addressed node in the same DAG the IR and value payloads already live
in — so the format reuses one serialization regime and adds no new ideas, only
record shapes.

The single subtlety the format must encode, rather than assume, is the
identity/content-hash split. A type's identity *is* its content hash, so
disconnected mints of the same shape converge — correct for data. A function's
identity is a minted token *stored in* the record and independent of the
record's content hash, so disconnected mints of the same signature stay
distinct — correct for behaviour, and the only design that survives minting
without a coordination channel. Conflating the two hashes is the one error that
would reintroduce either silent fusion of distinct functions or breakage of
callers on reimplementation, which is why §1's third committed decision is stated
before any bytes.
