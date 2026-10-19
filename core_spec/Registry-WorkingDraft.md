# Registry — Working Draft

**Status:** Working draft — current best thinking, **not** settled ground. This
has not been battle-tested against an implementation; treat it as a proposal
subject to revision. The label "foundation" is earned by surviving contact with
a build, and is withheld until then. It corrects the known errors of Registry
0.1 (name-as-identity, arity-only), but its own model is unproven.

Scope: this draft describes the Registry *model*. The governance practice of
organising definitions into layers is deliberately **not** here — it is an
optional, more-opinionated, less-proven structure kept in a separate
supplementary document (*Registry Layering — Practices*) so it can evolve at
governance speed without disturbing the model.

---

## What the Registry Is

The Registry is the stratum that answers one question:

> **What is this, and what does it guarantee?**

It holds *definitions* — the timeless facts about types and callables that are
true across every use, in every context, forever. It is one of three strata,
and the boundaries between them are normative:

| Stratum | Question | Holds |
|---|---|---|
| **Core IR** | *What does this mean / connect to?* | the semantic graph |
| **Registry** (this document) | *What is this / what does it guarantee?* | identity, contract, implementation, type and bridge facts |
| **Ledger** | *What happened?* | events, in time, append-only |

A fact belongs to the Registry only if it is true of a definition *independent
of any particular run*. Facts about a run (priority, resource grants, what
actually executed) belong to the Ledger or to the runtime. Facts about how an
artifact came to be (trust origin) belong to the Ledger. Facts about the
outside world's relationship to a definition (legacy source, form) belong to
external tools keyed on public identity. The Registry holds only *what a
definition is*.

---

## Two Kinds of Definition

The Registry holds two kinds of definition, and the difference between them
resolves the identity question that governs everything else.

**Type definitions** describe data shapes. A type is **fully determined by its
shape** — a pair of integers *is* a pair of integers; there is nothing more to
it. Therefore types are **structural**: a type's identity is the content hash of
its canonical shape (`def_hash`), and two structurally identical types collide
to the same identity automatically. This is correct and desirable — it
deduplicates shapes across the whole fabric for free.

**Function definitions** describe callables. A function is **not determined by
its signature** — `sort_ascending` and `sort_descending` are both
`List<T> → List<T>`, yet they are different meanings. A signature massively
underdetermines behaviour. Therefore functions are **nominal**: a function's
identity is a minted, globally-unique token, distinct from every other function
even one with an identical signature.

This is the resolution of the structural-versus-nominal question:
**data is structural, behaviour is nominal.** Shape determines the meaning of
data; signature does not determine the meaning of behaviour. The two kinds of
definition are identified accordingly.

---

## Identity (Functions)

A function's identity is a **minted token** with three mandatory properties:

1. **Immutable.** Once minted, it never changes.
2. **Globally unique by construction.** Not unique within a registry, not
   unique within a layer — unique across the entire fabric. Uniqueness is a
   structural guarantee of the minting scheme, never an assumption about
   non-collision.
3. **Independent of both contract and implementation.** The identity is derived
   from neither. This is essential and is argued below.

### Identity is not the name

The name of a function is a **projection** — a human-facing, tool-facing handle
bound to the identity token. It is not the identity. Renaming changes nothing:
the binding `name → identity` is updated, all references (which are to the
*identity*) continue to resolve, and no caller is disturbed. Conversely, two
registries may use the same name for different identities without collision,
because resolution is on the identity token, never the name.

Dispatch, references, and the closed callable space all key on the **identity
token**, never the name. This is what makes a bundle's meaning host-independent:
there is no name for a host to rebind, so a bundle means the same thing on every
machine, forever.

### Identity is not the implementation

The identity MUST NOT be derived from the implementation. If it were, every
reimplementation — a faster sort, a bug fix — would mint a new identity and
break every caller. The whole point is that a radically better implementation,
honouring the same contract, is the *same function*. Implementation lives
*below* the identity line and is freely replaceable.

### Identity is not the contract

The identity MUST NOT be derived from the contract either. The contract grows
over time (below); if identity were a hash of the contract, it would change on
every clause appended, which is fatal. The contract hangs *off* a fixed
identity and accretes independently.

### Identity is declared, not computed

A finite contract cannot determine an infinite behaviour: any signature-plus-
properties admits multiple distinct meanings (the specification problem). So
meaning-identity cannot be *computed* from any description short of the
implementation itself. It must be **declared** — minting an identity is the
act of asserting "this is a distinct meaning." Where two things are genuinely
the same meaning, that too is a declaration (reuse the identity), not a
derivation. The minting scheme MAY use a content hash over a genesis
declaration plus a uniqueness marker to produce the token; the uniqueness
marker is the switch that keeps structurally-identical-but-distinct meanings
apart. The normative requirement is only that the token have the three
properties above.

### Identity is stable under governance

Because the identity is immutable, it is unaffected by any governance action
taken on a definition. Two consequences are core to the model (the governance
*practice* that exercises them is separate — see *Registry Layering*):

* **Promotion preserves identity.** Any transition that changes a definition's
  visibility, trust level, or governance status preserves the identity token,
  the signature, and the behaviour. Existing references continue to resolve
  across such a transition; nothing is re-minted.
* **You cannot mutate an identity you do not own; you shadow by minting your
  own.** A context that wants to override a definition does so by minting a
  **new** identity and binding a name to it locally — never by redefining
  another's identity. Immutability of identity makes one party's meaning
  unchangeable by another, which is what makes any layering or sharing scheme
  safe.

---

## The Contract

A function's contract is the set of guarantees it makes about its results. It
is attached to the identity and governed by a strict lifecycle.

### Genesis clause (frozen at minting)

At minting, the **signature** — the input type `def_hash`es and the output type
`def_hash` — is frozen as the contract's genesis clause. Unlike later clauses it
can never be removed or weakened, because changing it breaks every caller. The
signature is the part of the contract fixed at birth alongside the identity. It
is also what makes call-site type-checking possible (a function definition
carries typed shapes, not merely an arity).

### Append-only ratchet

Beyond genesis, the contract is **append-only**:

1. Clauses may be **added**. Existing clauses may **never** be modified or
   removed.
2. An appended clause MUST already be **true of the current behaviour** at the
   moment of append. You may only write down what the function already does. The
   contract grows *toward* the behaviour from below; it never pulls the
   behaviour toward it.
3. Because clauses only accrete and each is already true, every later contract
   generation is a **superset** of every earlier one. Therefore every contract
   change is backward-compatible **by construction** — a breaking contract
   change is structurally impossible without minting a new identity.
4. Contract changes are strongly discouraged and MUST be load-bearing. A clause
   that does not earn its place does not belong.

### Generations and caller pinning

The contract carries a generation number. A caller MAY pin to a minimum
generation ("I rely on the guarantees through generation 3"). Append-only
monotonicity guarantees any later generation still honours that pin, so callers
never break from contract growth.

---

## Protected Behaviour

The contract underdetermines behaviour — there is always a region of inputs no
clause constrains. To make "preserve behaviour" precise, the Registry pairs the
contract with an **oracle**: a content-addressed characterization corpus of
(input → observed output) entries that pin behaviour the contract does not
formalize.

> **Protected behaviour = contract clauses ∪ oracle entries.**
> Behaviour outside both is, by definition, not yet protected.

This makes "behaviour change" a precise, decidable notion:

* A change that alters outputs **only in the unprotected region** is a **fix** —
  permitted, because nothing was promised there.
* A change that alters a **contract-pinned or oracle-pinned** output is a
  **behaviour change** — forbidden silently. It requires either a **new
  identity** (it is a different meaning now) or a **governed amendment** stamped
  with provenance ("this pinned output was wrong; corrected; here is the
  authority"). A bug fix whose wrong output was pinned falls here: it cannot be
  silent, because the pin says that output is the behaviour.

The oracle is how behaviour is *progressively pinned*: as a dependency on some
observable behaviour is discovered, that behaviour is promoted from the
unprotected region into the protected one by an explicit, governed act. This is
Hyrum's Law converted from a hazard into a process.

---

## Implementation

An implementation is content-addressed, is its own artifact, and lives **below
the identity line**. It is admissible for a function if and only if it satisfies
the function's full current contract. Many implementations of one identity may
coexist; they are interchangeable precisely because they satisfy the same
contract. Swapping a faster implementation in changes only an implementation
hash — identity unchanged, contract unchanged, callers undisturbed.

---

## Record Kinds: Leaf and Composite

A function definition is one of two kinds.

**Leaf (bridge).** Its implementation is opaque native code at the boundary
where meaning becomes mechanism. Its facts (below) cannot be derived from
anything the system can see; they MUST be **declared** and trusted. A leaf is
where a side effect is realized.

**Composite.** Its implementation is a hashed sub-graph of other functions. Its
facts are **derived** from its components, not declared. A composite carries a
reference to its body (a Core IR graph), which the genesis-frozen signature and
the contract describe.

The principle is the same one that emptied the bundle header: **declared facts
are load-bearing only at the leaves, where nothing can reconstruct them;
composite facts are derivable and therefore cacheable, not primitive.**

---

## Bridge Facts (Leaf Declarations)

A leaf declares the facts about its mechanism that nothing else can derive,
because the mechanism is opaque native code across the boundary. These are the
irreducible declarations of the Registry.

| Fact | Meaning | Composite derivation |
|---|---|---|
| `effect_class` | `pure < reads < writes < fullIo` — the class of state change | maximum over components |
| `deterministic` | same inputs always yield the same result | conjunction over components |
| `idempotent` | re-execution under identical inputs is safe to repeat | conservatively `false` unless provably derivable |

`idempotent` is a leaf fact (it was never a property of a bundle or a call). It
gates retry: a runtime MAY retry an idempotent leaf without approval; a
non-idempotent leaf requires explicit authority to retry.

`deterministic` is consumed, together with `effect`: a leaf that is BOTH
`effect pure` and `deterministic true` is hash-cons shareable in the compiler
(`src/ir/core_ir_05.rs` `push_node`), may be sunk into an if-arm by the bridge
(EFFECT_ORDER_V1), and has its panics classified as defects rather than
Unknowns by the bridge fault guard. A higher-order call is pure only when its
fn-ref arguments are too. Absent = non-deterministic, which is the safe
direction: over-declaring costs sharing, never correctness.

`independence` — whether the work can go wide — is **not** in this draft. Its
consumer (a scheduler capable of acting on it) does not exist. The right time to
define what form this fact takes is when that scheduler is being built and its
requirements are known. Adding it now would be specifying a format for an unknown
consumer. It will be added via contract append when it is needed.

---

## Advisory Execution Disposition

A function MAY carry an advisory **disposition** describing its execution nature
— for the current crawl phase, a value from a tiny closed vocabulary
(e.g. `interactive | batch`). It is:

* **A property of the function, not of a run.** If a function genuinely needs
  two dispositions, that is two functions (two identities), not one function
  with a per-run dial.
* **Advisory, not authoritative.** A bridge builds against it as best it can. A
  sophisticated bridge with its own control loops honours it; a simple bridge
  ignores it and just runs. Ignoring it is correct behaviour, not failure.
* **A run concern, invisible to Core IR.** The semantic graph neither carries
  nor reads it. It is consumed only by the bridge at execution time.
* **Crawl-phase scaffolding.** It MAY be classified by an LLM at definition time
  from the fixed vocabulary (a judgment, not an invented specification). It is
  expected to be superseded: a future runtime control loop will *derive*
  execution posture from graph structure and live state, at which point the
  declared disposition demotes from authority to default.

A disposition is **not** a contract clause — it is not a checkable property of
the result. It resides in the Registry because it is a per-function fact, which
is what the Registry holds.

---

## Coercions

The Registry has no subtyping and no implicit subsumption. Type compatibility at
a call site is **exact**: the argument's type `def_hash` must equal the expected
type `def_hash`. Surfaces that offer subtyping or widening MUST elaborate it into
**explicit coercion functions** before lowering; the Registry holds those
coercions as ordinary function definitions, and they arrive in Core IR as
ordinary `CCall` nodes. The Registry never performs an implicit conversion,
because an implicit conversion is an unaudited transition.

---

## Resolution and the Active Registry

For any compilation or execution, the **active registry** is an explicit set of
definitions supplied by the host. It is a **hash-resolution scope**, not a
namespace:

1. Resolution asks one question of an identity: *does this scope hold the
   definition for this identity?* Yes → resolve. No → **UNKNOWN gate** (a hard
   halt, never a default, never a guess).
2. Dispatch is by identity token, never by name. There is no name to rebind, so
   the active scope cannot silently change what a reference means — only whether
   it resolves at all.
3. **Names within the active scope MUST be unique.** Duplicate names are a hard
   error at scope-composition time, not at resolution time. This is not merely a
   deduplication rule — it is the guarantee that an author or code generator
   emitting a name can rely on it resolving to **exactly one identity** with no
   ambiguity. A scope that permits duplicate names cannot be safely presented to
   a code generator: the generator emits a name and cannot know which meaning was
   intended. Uniqueness must be enforced before the compiler runs, by the host
   that assembles the scope.
4. The active scope is the union of the supplied definition sets. The compiler
   and runtime MUST NOT infer or auto-load definitions.
5. A definition MUST NOT include or reference definition *sets*; composition of
   the active scope is external to this specification.

Because identity is global, an identity that escapes its intended scope does not
collide with anything elsewhere — it simply fails to resolve, which is the safe
failure.

---

## Layering (Out of Scope — see Practices)

Definitions may be organised into governance layers (local, organisational,
foundational, or otherwise). **That structure is optional, more opinionated than
the model, and unproven — so it is not part of this draft.** It is documented
separately in *Registry Layering — Practices*, where it can be battle-tested and
revised without touching the identity, contract, and resolution model here.

Only the parts that layering *uses but does not own* are retained in this model,
and they live where they belong, not in a layering section:

* That **promotion preserves identity**, and that **an identity cannot be mutated
  by anyone but its owner (others shadow by minting their own)** — both are
  properties of the immutable identity token (see *Identity is stable under
  governance*).
* That **resolution is by identity against a host-supplied scope** — a property
  of resolution (see *Resolution and the Active Registry*).

Everything else — the layer taxonomy, the retraction model, promotion gates, the
"foundational is append-only-forever" property — is governance opinion and lives
in the practices document.

---

## Validation Behaviour

A consumer validating against the Registry:

1. **Resolves** every function identity referenced by a `CCall` in the active
   scope. Unresolvable identity → UNKNOWN gate.
2. **Type-checks** every call site: each argument's type `def_hash` MUST equal
   the signature's expected type `def_hash`. Mismatch → hard FAIL. (This is the
   capability the arity-only registry lacked.)
3. **Derives** composite facts (`effect_class`, `deterministic`, etc.) from
   components where needed, treating declared leaf facts as authoritative and
   declared-but-derivable composite facts as cache.
4. **Admits** an implementation only if it satisfies the full current contract,
   and treats any change to a protected-behaviour output as a behaviour change
   requiring a new identity or a governed amendment.

---

## Fn References as Arguments

A function may be passed as an argument to another function. At the Core IR
level, a fn reference is a **constant pool entry** in the bundle:

- `defHash` resolves to the `Fn` primitive type (`prim_code = 6`)
- `payload` is the 32-byte minted identity hash of the referenced function

No special node type is required. A fn reference is a typed constant, handled
identically to any other pool entry. The Verifier type-checks it against the
parameter's expected `def_hash` at the call site. The bridge resolves the payload
hash to a concrete implementation at translation time.

**Type-checking fn references at the surface:** surface compilers do a registry
lookup on the passed fn name, retrieve its `inputs` and `output` signature, and
verify compatibility with the receiving parameter's declared contract. This is a
compile-time check — not type inference, just equality of `def_hash` values.
If the fn is not in the registry: compile error. If the types do not match:
compile error. Nothing reaches Core IR if the surface check fails.

---

## Bridge Execution Model

The registry's `LeafFacts` record declares execution-model facts that the bridge
reads on dispatch. These are bridge facts — properties of the execution mechanism,
not of the semantic graph. Surface languages never read or write them; only the
bridge consumes them.

**`entry_point`** — the fn is wired as a named external entry point. The bridge
routes incoming external events (HTTP requests, interrupts, messages) to this fn
by identity hash. Entry points are static — they are declared in the registry and
never registered at runtime. H1 code cannot create entry points dynamically.

**`startup`** — the bridge launches this fn at boot as an independent process loop,
in its own execution context. Multiple startup fns may coexist; each runs
concurrently. The process topology is fully declared in the registry.

**`background`** — when H1 calls this fn, the bridge spawns it in a separate
execution context and returns immediately to the caller. The spawned fn runs
concurrently. `background false` (default) means the bridge runs the fn inline
and the caller waits for completion.

**`callbacks`** — lists the arg indices that the bridge calls synchronously within
the fn's own execution frame. This is the closure rule declaration: the referenced
fn arg may be called, but it must not escape the frame. Any foreign fn that stores
a fn reference for deferred or async invocation violates the closure rule
regardless of what its `callbacks` field declares.

**The closure rule** (normative): a foreign fn (leaf) cannot call an Axis fn
unless that fn is passed in as a parameter (`callbacks` declares which args), and
the call must occur synchronously within the foreign fn's execution frame. The fn
reference must not escape the frame — it must not be stored, passed to another
system, or called from an interrupt/timer/async context.

---

## Explicit Non-Goals

The Registry does **not** hold:

* **Run-time facts** — priority, salience, resource grants, deadlines,
  performance. These belong to the runtime and the Ledger. The Registry's only
  performance-adjacent fact is the mechanism-free `independence` capability.
* **Trust origin (provenance)** — how an artifact was produced is an *event*,
  recorded in the Ledger at admission, not a property of a timeless definition.
* **Legacy-source correspondence** — the map from a legacy function to a
  semantic identity is irrecoverable and load-bearing, but it is the legacy
  tool's concern, kept externally and keyed on the public identity. It is not a
  Registry fact.
* **Form / reconstitution data** — recomputable by regenerating a surface from
  the graph; not stored, and never in the Registry.
* **Classification labels / IFC** — enabled by Core IR's purity and implemented
  by consumers; never a Registry fact.

And the Registry does not provide: subtyping, inheritance, overloading, varargs,
syntax, evaluation order, or documentation as a load-bearing field.

---

## Rationale (Non-Normative)

Registry 0.1 made a single foundational error and several consequent ones: it
declared that a function's identity *is its name*. Name-as-identity makes meaning
host-relative — dispatch keys on a string the host can rebind, so the same
reference means different things in different scopes, breaking the immutable and
distributed axioms at their root. It also forced the registry to be a flat
namespace with no shapes, so call-site type-checking was impossible and the
closed callable space guaranteed only "known name, right arg count," not "right
meanings flowing in."

The correction begins from the observation that **meaning-identity cannot be
computed.** A finite contract underdetermines an infinite behaviour, so no hash
of a description can decide whether two things mean the same — short of the
implementation itself, which would make every reimplementation a new identity.
Identity must therefore be *declared and minted*, independent of both contract
and implementation, with the name demoted to a projection. The contract then
accretes append-only beneath the fixed identity, growing toward the behaviour
and never breaking a caller; the oracle pins the behaviour the contract cannot
formalize; and "behaviour change" becomes a decidable line between the protected
and unprotected regions.

The structural-versus-nominal question, open since the value model was first
sketched, resolves cleanly once data and behaviour are separated: **data is
structural** (a type is its shape, so types collide on shape and dedup),
**behaviour is nominal** (a function is not its signature, so functions get
minted identities and distinct meanings stay distinct).

The layer model — should it prove out — falls from one insight rather than
several rules: the rules are identical everywhere, and the only thing that varies,
retractability, is not assigned but *inherited from who owns the callers*. That
governance story is developed and tested separately (*Registry Layering —
Practices*); the model here keeps only what is intrinsic to the immutable
identity: promotion preserves it, and no one may mutate an identity they do not
own.

What remains, after the corrections, is a Registry whose declared facts are
exactly those nothing else can reconstruct — the leaf facts at the boundary
where meaning becomes mechanism — and whose every other property is derived,
projected, governed, or owned by another stratum. The Registry holds *what a
thing is*. The graph holds *what it means*. The Ledger holds *what happened*.
Each fact lives in exactly one of the three, and the discipline of putting it
there is the whole design.
