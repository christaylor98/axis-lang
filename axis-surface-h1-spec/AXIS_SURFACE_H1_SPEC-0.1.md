# `AXIS_SURFACE_H1_SPEC-0.1.md`

## Status

**Normative – Human Surface (H1)**
This document defines the human-readable language surface for Axis.

This surface is designed **for human authorship**, not AI generation. It is ergonomic, expressive, and familiar to programmers with backgrounds in ML-family or systems languages.

---

## 1. Purpose

The Human (H1) surface exists to provide:

* A readable, writable syntax for human programmers
* Explicit effect and concurrency intent without execution semantics
* Familiar constructs: `let`, `fn`, `if`, `for`, `match`, `loop`
* A clean mapping to Core IR via normalization
* Formal separation between description (H1) and execution (bridge)

H1 is a **projection onto Core IR semantics**, not a semantic authority.

---

## 2. Non-Goals

H1 explicitly does **not** attempt to:

* Control execution strategy
* Specify threading, scheduling, or memory models
* Provide `async`/`await` or futures
* Be optimal for AI generation
* Replace or supersede Core IR
* Introduce new semantic primitives beyond what Core IR supports

H1 describes **what a program means**.
The bridge decides **how it runs**.

---

## 3. Design Constraints (Hard)

The following constraints are mandatory:

1. **All bindings are immutable.** There is no mutation syntax.
2. **Effects are explicit values.** No implicit side effects.
3. **Concurrency is intent-only.** `spawn`, `join`, `channel`, `send`, `recv` describe structure, not execution.
4. **`async` and `await` are forbidden identifiers.** Hard lexical error.
5. **Execution boundary is singular.** Only `run <name>;` crosses from description to execution.
6. **Annotations are metadata only.** They do not affect normalization or lowering.
7. **Lowering is fixed.** H1 does not control the lowering implementation.

Any construct that violates these constraints is invalid H1.

---

## 4. Pipeline

H1 source passes through the following stages:

```
H1 Source Text
 → Lexer          (tokens)
 → Parser         (CST)
 → Schema AST     (typed AST nodes)
 → Validator      (structural checks)
 → Normalizer     (NF nodes)
 → Lowering       (Core IR 0.5)
```

Each stage has a distinct responsibility. No stage may skip ahead or substitute for another.

---

## 5. Lexical Model

### 5.1 Character Set

ASCII only. Case-sensitive.

### 5.2 Whitespace and Comments

| Class           | Pattern                     | Action   |
| --------------- | --------------------------- | -------- |
| Whitespace      | `[ \t\n\r]+`                | Skip     |
| Line comment    | `//[^\n]*`                  | Skip     |
| Block comment   | `/* ... */`                 | Skip     |

### 5.3 Keywords

The following are reserved and may not be used as identifiers:

```
let  fn  if  else  loop  while  for  in  match
spawn  join  channel  send  recv
controller  on  run  intent
```

### 5.4 Identifiers

Pattern: `[A-Za-z_][A-Za-z0-9_]*`

**Forbidden identifiers (hard error):** `async`, `await`

### 5.5 Literals

| Kind   | Pattern / Rule                              |
| ------ | ------------------------------------------- |
| Int    | `[0-9]+` (decimal only)                     |
| String | `"..."` with escapes `\"  \\  \n  \t  \r`   |

### 5.6 Punctuation

```
( )  { }  [ ]  ,  ;  :  .  @  _
<  >  =  =>  ==  !=  <=  >=
+  -  *  /  !
```

---

## 6. Grammar

### 6.1 Top-Level Structure

A program is a sequence of top-level items:

```
Program → Item*

Item →
  | FnDecl
  | ControllerDecl
  | RunStmt
  | Stmt
```

### 6.2 Function Declaration

```h1
fn name(param, param, ...) {
  body
}
```

Grammar: `fn IDENT ( ParamList ) Block`

Parameters are positional, comma-separated, bare identifiers. No type annotations.

### 6.3 Blocks

```h1
{
  stmt;
  stmt;
  expr
}
```

Grammar: `{ BlockContent }`

`BlockContent` is zero or more statements followed by an optional final expression. Blocks are expression-valued — they evaluate to their final expression, or `unit` if no final expression is present.

### 6.4 Statements

| Form                    | Grammar                        |
| ----------------------- | ------------------------------ |
| Let binding             | `let IDENT = Expr ;`           |
| Expression statement    | `Expr ;`                       |
| Intent directive        | `@intent.IDENT ;`              |

### 6.5 Expressions

H1 expressions are listed in order of **decreasing precedence** (highest first):

| Level         | Form                                              |
| ------------- | ------------------------------------------------- |
| Primary       | Literal, IDENT, Block, `( Expr )`                 |
| Call          | `Expr ( ArgList )`                                |
| Unary         | `-Expr`, `!Expr`                                  |
| Multiplicative| `Expr * Expr`, `Expr / Expr`                      |
| Additive      | `Expr + Expr`, `Expr - Expr`                      |
| Relational    | `Expr < Expr`, `Expr > Expr`, `Expr <= Expr`, `Expr >= Expr` |
| Equality      | `Expr == Expr`, `Expr != Expr`                    |

The following are also expressions (non-operator, syntactic):

```
IfExpr      if Expr Block else Block
MatchExpr   match Expr { MatchArm+ }
ForExpr     for IDENT in Expr Block
WhileExpr   while Expr Block
LoopExpr    loop Block
SpawnExpr   spawn Block
JoinExpr    join Expr
ChannelExpr channel<TypeName>() | channel()
SendExpr    send(Expr, Expr)
RecvExpr    recv(Expr)
```

### 6.6 Patterns (used in `match`)

```
Pattern →
  | _               (wildcard)
  | Literal         (literal pattern)
  | IDENT           (identifier pattern)
  | IDENT( PatList )  (constructor pattern)
```

### 6.7 Controllers

```h1
controller Name {
  on Event(param) => handler_expr,
}
```

Grammar: `controller IDENT { ControllerMember* }`

Each member: `on EventSig => Expr ,`

### 6.8 Run Statement

```h1
run Name;
```

Grammar: `run IDENT ;`

This is the sole execution boundary. It delegates all effect execution to the bridge.

---

## 7. Construct Reference

### `let`

```h1
let x = expr;
```

Immutable binding. No rebinding. Introduces `x` into scope for the remainder of the enclosing block.

### `fn`

```h1
fn double(x) {
  x + x
}
```

Pure by default. May return effect values. Does not execute effects.

### `if`

```h1
if cond {
  then_expr
} else {
  else_expr
}
```

Condition must be boolean. No implicit truthiness. Always expression-valued.

### `match`

```h1
match value {
  A(x) => expr,
  B    => expr,
  _    => expr,
}
```

Deterministic. No inference. No implicit exhaustiveness checks. Each arm ends with `,`.

### `loop`

```h1
loop {
  body
}
```

Infinite loop. Normalizes to explicit recursion or canonical loop primitive. Execution strategy is unspecified.

### `while`

```h1
while cond {
  body
}
```

Conditional repetition. Equivalent semantics to `loop { if cond { body } else { break } }` at the NF level.

### `for`

```h1
for x in iterable {
  body
}
```

Iteration over a collection or source. No ordering or parallelism guarantee unless annotated.

### `spawn` / `join`

```h1
let h = spawn {
  expr
};
let r = join h;
```

`spawn` describes concurrent work and returns an opaque handle. `join` synchronizes. No threading model implied.

### `channel` / `send` / `recv`

```h1
let ch = channel<T>();
send(ch, value);
let v = recv(ch);
```

Describes communication intent only. Transport, buffering, and blocking behavior are bridge concerns.

### `controller` / `run`

```h1
controller Srv {
  on Request(x) => handle(x),
}
run Srv;
```

Describes event reaction structure. `run` delegates lifecycle, event binding, and effect execution to the bridge.

---

## 8. Annotations

Annotations express **intent metadata only**. They do not affect normalization or lowering.

### `@intent.parallel`

```h1
@intent.parallel
for x in items {
  body
}
```

Advisory: iterations are independent; parallel execution is permitted but not required. Sequential execution is always valid.

### `@intent.serial`

```h1
@intent.serial
for x in items {
  body
}
```

Mandatory for compliant bridges: execution order must be preserved. Parallel execution is forbidden.

**Rules:**
- Annotations attach to loops and blocks only.
- `@intent.parallel` is advisory — bridges may ignore it.
- `@intent.serial` is mandatory — all compliant bridges must honor it.
- Lowering ignores all annotations.

---

## 9. Validation

The following checks run after parsing, before normalization:

| Check                       | Rule                                                            |
| --------------------------- | --------------------------------------------------------------- |
| `intent_directive_validity` | `@intent.X` directives use only known, permitted keys           |
| `no_async_await`            | The identifiers `async` and `await` are hard errors (also lexical) |

**Structural roles:**

| Node              | Role               | Executable? |
| ----------------- | ------------------ | ----------- |
| `Program`         | Structural only    | No          |
| `FnDecl`          | Declaration        | Yes (via bridge) |
| `ControllerDecl`  | Declaration        | Yes (via bridge) |

---

## 10. Lowering

After normalization, H1 programs reduce to NF-level nodes that lower to **Core IR 0.5** via the following rules:

| NF Node     | Core IR Target | Field Mapping                                          |
| ----------- | -------------- | ------------------------------------------------------ |
| `IntLit`    | `CIntLit`      | `value ← token:value`                                  |
| `BoolLit`   | `CBoolLit`     | `value ← token:value`                                  |
| `UnitLit`   | `CUnitLit`     | (none)                                                 |
| `VarRef`    | `CVar`         | `name ← token:name`                                    |
| `LetExpr`   | `CLet`         | `name ← token:name`, `value ← node:value`, `body ← node:body` |
| `LamExpr`   | `CLam`         | `param ← token:param`, `body ← node:body`              |
| `IfExpr`    | `CIf`          | `cond ← node:condition`, `then_branch ← node:then_branch`, `else_branch ← node:else_branch` |
| `AppExpr`   | `CApp`         | `fn ← node:fn`, `arg ← node:arg`                       |
| `CallExpr`  | `CCall`        | `name ← token:name`, `args ← nodes:args`               |

**Implementation status (v0.1):**
- Loops, `match`, controllers, concurrency constructs: **not yet lowered**
- Programs using these constructs will fail lowering at this version

---

## 11. Built-in Operator Registry

The following operators are registered in the H1 registry and resolved during compilation:

| Operator                   | Core IR Op  | Pops | Pushes |
| -------------------------- | ----------- | ---- | ------ |
| `core.math.add`            | `add`       | 2    | 1      |
| `core.math.sub`            | `sub`       | 2    | 1      |
| `core.math.mul`            | `mul`       | 2    | 1      |
| `core.math.div`            | `div`       | 2    | 1      |
| `core.cmp.eq`              | `eq`        | 2    | 1      |
| `core.cmp.ne`              | `ne`        | 2    | 1      |
| `core.cmp.lt`              | `lt`        | 2    | 1      |
| `core.cmp.le`              | `le`        | 2    | 1      |
| `core.cmp.gt`              | `gt`        | 2    | 1      |
| `core.cmp.ge`              | `ge`        | 2    | 1      |
| `core.control.if`          | `if`        | 3    | 1      |
| `core.control.loop`        | `loop`      | 1    | 1      |
| `core.control.match`       | `match`     | 2    | 1      |
| `core.concurrency.spawn`   | `spawn`     | 1    | 1      |
| `core.concurrency.join`    | `join`      | 1    | 1      |
| `core.concurrency.channel` | `channel`   | 0    | 1      |
| `core.concurrency.send`    | `send`      | 2    | 0      |
| `core.concurrency.recv`    | `recv`      | 1    | 1      |

**Effectful operations** (bridge executes, compiler describes): `print`, `send`, `recv`

---

## 12. Forbidden Constructs

The following are **hard errors** in H1:

* `async` — forbidden identifier (lexer)
* `await` — forbidden identifier (lexer)
* Futures, executors, threads, locks, schedulers
* Time-based semantics
* Implicit mutation
* Execution strategy annotations beyond `@intent`

These are bridge concerns only. H1 has no syntax for them.

---

## 13. Resurfacing Guarantee

Any valid H1 program:

* lowers to Core IR
* may be resurfaced into another surface
* preserves semantics exactly

Formatting, naming, and syntactic layout are **not** preserved across resurfacing.

---

## 14. Relationship to Other Surfaces

| Surface        | Role                                     | Audience         |
| -------------- | ---------------------------------------- | ---------------- |
| Surface-0      | Canonical instruction encoding           | Tooling, IR      |
| **Human (H1)** | **Human-friendly expression syntax**     | **Programmers**  |
| AI-Primary     | AI-friendly explicit prefix syntax       | LLMs             |
| RPN (AI-1)     | Compact stack encoding / projection      | Compression, tools |

H1 and AI-Primary map to the **same Core IR semantics**. They are semantic siblings — any program expressible in one is expressible in the other.

---

## 15. Determinism Guarantees

Given identical source text:

* Tokenization is deterministic
* Parsing is deterministic
* AST construction is deterministic
* Normalization is deterministic
* Lowering to Core IR is deterministic

Any divergence between runs is a bug.

---

## 16. Error Model

Errors are detected at the earliest possible stage:

| Stage       | Examples of hard errors                                             |
| ----------- | ------------------------------------------------------------------- |
| Lexer       | Unknown character, `async`/`await` used, invalid literal            |
| Parser      | Missing `else` branch, unclosed block, malformed pattern            |
| Validator   | Unknown `@intent` key, structurally invalid directive               |
| Normalizer  | H1 construct with no NF equivalent                                  |
| Lowering    | NF node with no Core IR rule (unsupported construct in v0.1)        |

No recovery is attempted. Errors fail the pipeline at the stage where they are detected.

---

## 17. Versioning

* This specification is versioned independently (currently v0.1)
* Lexical and grammar changes increment the minor version
* Any change that alters Core IR semantics requires a new surface version
* The lowering target (Core IR 0.5) is versioned independently

---

## 18. Open Questions (Explicit, v0.1)

* Lowering rules for `loop`, `while`, `for`, `match`, `spawn`, `join`, `channel` are not yet specified
* Controller lowering to Core IR is not yet defined
* Whether string literals are lowered to a Core IR primitive or passed through as metadata
* Whether patterns in `match` arms require exhaustiveness checking at the H1 level or at Core IR

These are deferred to a future version.

---

**End of specification**
