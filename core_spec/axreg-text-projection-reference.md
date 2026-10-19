# `.axreg` Text Projection Reference

**Status:** Normative for the text form. Implements the Registry model
(`Registry-WorkingDraft.md`) and the binary encoding format
(`Registry-Format-WorkingDraft.md`).

**Relationship to binary:** The text form is a **lossy, non-authoritative
projection** of the canonical binary. It is never hashed, never the source of
truth, and lowers to the binary of `Registry-Format-WorkingDraft.md §§3–6`.
Two projections differing only in names, comments, whitespace, or field ordering
**must** lower to identical canonical bytes.

**Supersedes:** `axis-registry-0.1.md`. The key differences are in §9.

---

## 1. File Structure

An `.axreg` file is a sequence of declarations separated by blank lines and
comments. Declarations appear in this order (enforced at lowering):

```
registry <name> <version>     // optional file header — ignored by the loader
type …                        // type declarations (before any fn referencing them)
fn …                          // function declarations
end
impl …                        // implementation declarations
```

**Comments:** `//` to end of line only. `#` is not a comment character.

**Blank lines:** ignored everywhere.

**Ordering constraint:** types must be declared before any `fn` block that
names them in `in` or `out`. Functions must be declared before any `impl` that
names them.

---

## 2. Type Declarations

Types are structural. The identity of a type is the content hash of its
canonical encoding — no authoring is required.

### Syntax

```
type <Name> = prim <prim-code>
type <Name> = product (<Type1>, <Type2>, …)
type <Name> = sum [ <Ctor1>(<Type1>, …) | <Ctor2>(<Type3>, …) ]
```

### Primitive codes

| Code    | Meaning           |
|---------|-------------------|
| `unit`  | zero-byte unit    |
| `bool`  | one-byte boolean  |
| `int`   | variable-width integer (varint encoding) |
| `float` | IEEE-754 double   |
| `bytes` | length-prefixed byte array |
| `text`  | length-prefixed UTF-8 string |

### Product types

Fields are positional — no field names appear in the canonical form. The
declared names in the projection are for human reading only and are stripped on
lowering.

```
type Point = product (Int, Int)
type StringPair = product (Text, Text)
```

### Sum types

Constructor names are projection-only and stripped on lowering. The canonical
form contains only a constructor index (varint) followed by the fields of that
constructor.

```
type Option = sum [ None() | Some(Int) ]
type Result = sum [ Ok(Int) | Err(Text) ]
```

A constructor with no fields uses an empty parameter list: `None()`.

### Type names in scope

Declared type names are bound to their computed identities for the duration of
the file. They are available to `fn` blocks in `in` / `out` fields. Built-in
primitive types do not need pre-existing entries — declare them explicitly:
`type Text = prim text` etc.

---

## 3. Function Declarations

A function declaration is a multi-line block terminated by `end`.

### Syntax

```
fn <name>
  identity     <hex256>
  kind         <leaf | composite>
  in           (<Type1>, <Type2>, …)
  out          <Type>
  effect       <pure | reads | writes | fullIo>
  deterministic <true | false>
  idempotent   <true | false>
  [disposition  <interactive | batch>]
  [body         <hex256>]
  [contract     gen <n> [<clause1>, <clause2>, …]]
  [oracle       <hex256>]
end
```

Fields in `[…]` are optional. All other fields are **required** for leaf
functions. Composite functions substitute `body` for `effect`, `deterministic`,
and `idempotent`.

### Field reference

#### `identity <hex256>` — **required, minted, permanent**

A 256-bit token, authored literally as 64 lowercase hex digits. May be prefixed
with `0x`.

```
identity 0xf9898f93d79e11e623470fc02330ddc87817715f36fcf6755104d4d6d57f04a0
```

This field is the singular difference from Registry 0.1. It is the function's
globally-unique identity token, authored once and never regenerated. See §5 for
how to generate tokens.

#### `kind <leaf | composite>` — **required**

- `leaf` — opaque native code at the bridge boundary. Bridge facts (`effect`,
  `deterministic`, `idempotent`) are declared here because nothing can derive
  them from the mechanism.
- `composite` — implementation is a Core IR graph. Bridge facts are derived
  from the graph's components and are not declared. Requires a `body` field.

#### `in (<Type1>, <Type2>, …)` — **required**

Ordered input types by name, resolved to their type identities at lowering. A
zero-arity function takes an empty list: `in ()`.

```
in (Text)
in (Int, Int)
in ()
```

#### `out <Type>` — **required**

The output type, by name.

```
out Int
out Unit
```

#### `effect <pure | reads | writes | fullIo>` — **required for leaf**

The maximum privilege the bridge exercises, per the Foundation model.

| Value    | Meaning                                    |
|----------|--------------------------------------------|
| `pure`   | no external state accessed or modified     |
| `reads`  | reads external state but does not write    |
| `writes` | writes external state                      |
| `fullIo` | unrestricted I/O (terminal, network, etc.) |

Effect class is totally ordered: `pure < reads < writes < fullIo`. A composite's
derived effect class is the maximum over its components.

#### `deterministic <true | false>` — **required for leaf**

`true` if the same inputs always produce the same output (i.e. the function has
no hidden random or time-dependent state). The conservative default when absent
is `false`.

#### `idempotent <true | false>` — **required for leaf**

`true` if re-executing the function with identical inputs under identical state
is safe (the runtime may retry without requesting authority). Non-idempotent
bridges require explicit retry authority.

#### `disposition <interactive | batch>` — **optional**

Advisory execution posture. Ignored if absent; never authoritative.

| Value         | Meaning                                             |
|---------------|-----------------------------------------------------|
| `interactive` | expected latency is human-facing (sub-second)       |
| `batch`       | expected latency is background / offline            |

#### `body <hex256>` — **required for composite**

Content hash of the Core IR graph that implements this composite function. Not
applicable to leaf functions.

#### `contract gen <n> [<clause1>, …]` — **optional**

Declares that this file appends generation `n` to the function's contract. The
clauses are predicate-function names, resolved to identities at lowering. Only
`gen 1` is currently supported (see §7 — Limitations).

```
contract gen 1 []
contract gen 1 [is_positive, result_bounded]
```

#### `oracle <hex256>` — **optional**

Content hash of the characterisation corpus for this function. The all-zero hash
(`Hash256::ZERO`) is the canonical absent value.

---

## 4. Implementation Declarations

An implementation record is a single line (no block form). It binds an
admissible implementation artifact to a contract generation.

### Syntax

```
impl <fn-name> satisfies gen <n> artifact <hex256>
```

`<fn-name>` must be declared in a `fn` block in the same file. `<n>` is the
contract generation the implementation satisfies. `<hex256>` is the
content-addressed native bridge artifact (leaf) or the body graph identity
(composite).

```
impl axis_io_print satisfies gen 1 artifact 0x0000000000000000000000000000000000000000000000000000000000000000
```

---

## 5. Authoring Identity Tokens

Function identity tokens are 256-bit values, authored literally. They are
**minted once and never regenerated**. Two options:

### 5a. Entropy minting (canonical, preferred for production)

The `fabric::hash::mint_identity()` function generates 256 random bits. Use
this via a helper script or a one-off invocation:

```
axis-tools mint-identity
// or
cargo run --example mint_identity
```

The result is a 64-char hex string. Write it into the `identity` field and
commit. It never changes again. This is the only scheme that remains correct
when minting on disconnected systems.

### 5b. Bootstrap derivation (acceptable for compiler primitive registries)

For functions that will not be renamed and where round-trip determinism is
more valuable than strict nominality, derive the identity as:

```
identity = SHA-256(function-name as UTF-8 bytes)
```

Write the result as a hex literal. Mark these entries with a comment:

```
// identity: bootstrap — SHA-256("axis_io_print"); stable unless renamed
identity 0x00870f017ede8cc5136c3b35d413e65c425ddb8f7a71ef0c224f9f3fde531092
```

This approach is acceptable for `registry/axis.axreg` (compiler primitives with
stable names) but not for application registries where functions may be renamed
or where meaning-identity independence matters.

**Shell one-liner:** `echo -n "axis_io_print" | sha256sum`

---

## 6. Complete Example

```axreg
registry axis-primitives 0.1

// ── primitive type declarations ───────────────────────────────────────────

type Unit  = prim unit
type Bool  = prim bool
type Int   = prim int
type Text  = prim text
type Bytes = prim bytes

// ── core structural ───────────────────────────────────────────────────────

fn axis_proj
// identity: bootstrap — SHA-256("axis_proj")
identity 0xf9898f93d79e11e623470fc02330ddc87817715f36fcf6755104d4d6d57f04a0
kind     leaf
in       (Unit, Int)
out      Unit
effect   pure
deterministic true
idempotent    true
end

// ── IO primitives ─────────────────────────────────────────────────────────

fn axis_io_print
// identity: bootstrap — SHA-256("axis_io_print")
identity 0x00870f017ede8cc5136c3b35d413e65c425ddb8f7a71ef0c224f9f3fde531092
kind     leaf
in       (Text)
out      Unit
effect   fullIo
deterministic false
idempotent    false
end

fn axis_io_eprint
// identity: bootstrap — SHA-256("axis_io_eprint")
identity 0x3c31ced6d5fb16e428142ea77a9c0ce475eba12f8127496c05fb138ddf897719
kind     leaf
in       (Text)
out      Unit
effect   fullIo
deterministic false
idempotent    false
end

// ── string primitives ─────────────────────────────────────────────────────

fn axis_str_len
// identity: bootstrap — SHA-256("axis_str_len")
identity 0xf0887b5440d69e141b11e68d29e4497dc3fb66614b30075ee1abbcb39a9de1ed
kind     leaf
in       (Text)
out      Int
effect   pure
deterministic true
idempotent    true
end
```

---

## 7. Limitations (Current Implementation)

These constraints apply to the text loader (`src/registry/core05/text.rs`):

- **`contract gen N` supports `N = 1` only.** Multi-generation projections
  (gen 2+) require explicit intermediate `ContractAppend` records, which the
  text form does not yet model. Most registry files will not use `contract` at
  all, or will use `gen 1`.

- **`impl` lines are parsed but not yet validated by the lowering pipeline.**
  The `Implementation` record is emitted into the scope, but the admission gate
  that checks an artifact satisfies the declared contract generation is
  downstream of Phase 1.

- **Forward type references are not permitted.** A type must be declared before
  any `fn` that references it in `in` or `out`. The parser enforces this.

- **Cross-file name resolution is not supported in a single `lower()` call.**
  Each file is lowered independently. The active scope is assembled from the
  union of lowered scopes by the host (the compiler CLI or the pipeline).

---

## 8. What Changes from Registry 0.1

Registry 0.1 (`axis-registry-0.1.md`) is replaced by this format.

| Registry 0.1 field | Status | Replacement |
|--------------------|--------|-------------|
| `arity <n>` | **removed** | `in (T1, T2, …)` encodes arity and types |
| `deterministic true\|false` | **kept** | same semantics |
| `profile <id>` | **removed** | `effect <class>` replaces the named-profile vocabulary |
| `// comment` | **kept** | same |
| `end` | **kept** | same block terminator |
| *(none)* | **added** | `identity <hex256>` — the minted identity token |
| *(none)* | **added** | `kind leaf\|composite` |
| *(none)* | **added** | `in (…)` / `out <Type>` — typed signature |
| *(none)* | **added** | `idempotent true\|false` |
| *(none)* | **added** | `disposition <…>` (optional) |
| *(none)* | **added** | `body <hex256>` (composite) |
| *(none)* | **added** | `contract gen <n> […]` (optional) |
| *(none)* | **added** | `oracle <hex256>` (optional) |
| *(none)* | **added** | `type <Name> = …` declarations |
| *(none)* | **added** | `impl <fn> satisfies gen <n> artifact <hex256>` |

The fundamental shift: in 0.1 the function name was the identity. In this
format the name is a projection binding to an explicit identity token. All
registry resolution keys on the identity token; names are for human and tool use
only.

---

## 9. Canonical Bytes Guarantee

A conforming lowering of an `.axreg` file satisfies (Registry Format §8):

> Two projections that differ only in names, comments, whitespace, or field
> order produce identical canonical bytes.

This means the content hash of the lowered scope is stable under cosmetic
edits. The loader `src/registry/core05/text.rs` satisfies this guarantee by
sorting records by `(tag, content_hash)` before serialising.

---

## 10. Validation Errors

The loader emits hard errors (never silently defaults) for:

| Error | Trigger |
|-------|---------|
| `MissingField` | a required field is absent from an `fn` block |
| `UnknownName` | a type name in `in` / `out` has not been declared |
| `DuplicateName` | the same name appears twice in the same file |
| `BadHash` | an `identity`, `body`, `oracle`, or `artifact` value is not valid hex256 |
| `Parse` | any unrecognised directive or malformed syntax |

All errors include a line number.
