# `AXIS_SURFACE_M1_SPEC-0.1.md`

## Status

**Normative – Experimental (Human-Mechanical)**
This document defines the M1 (Mechanical 1) language surface for Axis.

M1 is designed for **direct human authorship** of structurally disciplined programs:
typed function declarations, explicit literal constructors, and no implicit semantics.

---

## 1. Purpose

The M1 surface exists to provide a minimal, explicit syntax for writing Axis programs
that compile cleanly through the spec-driven pipeline. It prioritises:

* Structural transparency — every expression announces its type at the construction site
* Predictable lowering — no implicit coercions, no sugar, no overloading
* Safety-by-restriction — lambda expressions are constrained by two hard rules
* Human readability at the cost of brevity

M1 is the reference surface for testing the full Axis pipeline from source to Core IR 0.5.

---

## 2. Non-Goals

M1 explicitly does **not** attempt to:

* Minimise keystrokes
* Infer types from context
* Support operator syntax (`+`, `-`, `==`, etc.) — all arithmetic and comparison is a registry call
* Provide mutable state, loops, or pattern matching
* Support type aliases, type variables, or generics
* Replace higher-level or AI-primary surfaces

---

## 3. Program Structure

An M1 program is a sequence of **top-level declarations**. Two kinds are permitted:

```
<FnDecl>
<LetDecl>
```

Declarations are processed one at a time by the pipeline; there is no wrapping
`Program` node in the IR.

### Function Declaration

```
fn <name>(<params>) -> <return_type> {
  <body>
}
```

### Top-Level Let Declaration

```
let <name> = <expr>
```

Top-level `let` is a simple binding with no type annotation. It is used for
constants and REPL-style programs.

---

## 4. Type Expressions

Types appear in parameter annotations, return-type positions, and at construction sites.

| Syntax                       | Meaning                                  |
| ---------------------------- | ---------------------------------------- |
| `Int`                        | Primitive integer                        |
| `Text`                       | Primitive text / string                  |
| `Bool`                       | Primitive boolean                        |
| `Unit`                       | Unit / void                              |
| `Value(T1, T2, ...)`         | Compound value (product) with named slots |
| `ValueList(T)`               | Homogeneous list of elements of type `T` |
| `Fn`                         | A function reference (fn-ref constant)   |
| `<IDENT>`                    | User-defined or imported named type      |

`ValueList(Value)` (bare, unparameterised inner `Value`) is **forbidden** — the
inner type must be a fully resolved type.

---

## 5. Parameters

Function parameters are typed:

```
fn apply(flag: Bool, count: Int) -> Bool {
  flag
}
```

A function with no parameters uses an empty parameter list:

```
fn always_true() -> Bool {
  Bool(true)
}
```

Multiple parameters are comma-separated:

```
fn make(a: Bool, b: Bool) -> Value(Bool, Bool) {
  Value(Bool, Bool)(a, b)
}
```

---

## 6. Body

A function body is a sequence of zero or more **let-bindings** followed by exactly
one **return expression**:

```
fn make_pair(a: Bool, b: Bool) -> Bool {
  let x = a
  let y = b
  x
}
```

Let-bindings inside a body have no type annotation. The name is bound to the
expression on the right and is in scope for all subsequent bindings and the
final expression.

---

## 7. Expressions

### 7.1 Variable Reference

A bare identifier refers to a parameter or let-binding in scope:

```
fn identity(x: Bool) -> Bool {
  x
}
```

### 7.2 If Expression

```
if <cond> { <then_expr> } else { <else_expr> }
```

All three sub-expressions are required. There is no `else if` — nest `if`
expressions explicitly.

```
fn negate(flag: Bool) -> Bool {
  if flag { Bool(false) } else { Bool(true) }
}
```

### 7.3 Call Expression

All function calls — including registry primitives, user-defined functions, and
HOFs — use the same uniform call syntax:

```
<name>(<arg1>, <arg2>, ...)
```

No argument label syntax. Arguments are positional.

```
fn double_negate(flag: Bool) -> Bool {
  negate(negate(flag))
}
```

If the function takes no arguments, write an empty argument list:

```
fn f() -> Unit {
  const_unit()
}
```

### 7.4 Lambda Expression

A lambda is an inline anonymous function:

```
fn(<param1>, <param2>, ...) { <body_expr> }
```

Lambda parameters are **untyped** — the types are enforced at the call site by
the registry's leaf-guard contract. The body is a single expression (not a
sequence of let-bindings).

```
fn f(x: Bool) -> Unit {
  const_unit(fn(a) { a })
}
```

**Lambda rules — both are hard constraints enforced at normalisation:**

| Rule   | Constraint                                          | Violation result        |
| ------ | --------------------------------------------------- | ----------------------- |
| RULE 1 | A lambda body may only reference its own parameters | Rejected: capture error |
| RULE 2 | A lambda must appear as a direct call argument      | Rejected: escape error  |

RULE 1 means a lambda cannot close over variables from the enclosing function.
RULE 2 means a lambda cannot be the value of a let-binding or the return
expression of a function — it must be passed directly as an argument.

**Legal:**
```
fn f(x: Bool) -> Unit {
  const_unit(fn(a) { a })     // RULE 1: body only uses 'a'; RULE 2: direct arg
}
```

**Illegal — RULE 1 violation (capture):**
```
fn f(x: Bool) -> Unit {
  const_unit(fn(a) { x })     // 'x' is the outer fn's parameter — capture error
}
```

**Illegal — RULE 2 violation (escape via let):**
```
fn f(x: Bool) -> Bool {
  let g = fn(a) { a }         // lambda escapes into a let-binding — error
  g
}
```

**Illegal — RULE 2 violation (escape as return value):**
```
fn f(x: Bool) -> Bool {
  fn(a) { a }                 // lambda is the function body — error
}
```

### 7.5 Determine Expression

`determine` is the explicit determinacy-discharge gate. It lowers to a
`CDeterminate` node in Core IR. It is written as a keyword with no arguments:

```
fn f(x: Bool) -> Bool {
  determine
}
```

`determine` must be written by the author; the compiler never inserts it
automatically. It is used to discharge a reachability obligation in the effect
system.

---

## 8. Literals

All literals use **typed constructor syntax**. The wrapper dissolves at lowering —
the IR sees a typed constant. There is no bare numeric or string syntax.

### Integer

```
Int(<integer>)
```

```
fn f() -> Int {
  Int(42)
}
```

### Text

```
Text("<string>")
```

```
fn f() -> Text {
  Text("hello")
}
```

### Boolean

```
Bool(true)
Bool(false)
```

```
fn negate(flag: Bool) -> Bool {
  if flag { Bool(false) } else { Bool(true) }
}
```

### Unit

```
Unit()
```

```
fn f() -> Unit {
  Unit()
}
```

---

## 9. Value and ValueList Construction

### Value (Product Type)

A compound value is constructed with a mandatory type tag at the construction site:

```
Value(<T1>, <T2>, ...)(<e1>, <e2>, ...)
```

The type-tag list and the argument list must have the same arity.

```
fn make(a: Bool, b: Bool) -> Value(Bool, Bool) {
  Value(Bool, Bool)(a, b)
}
```

Nesting is permitted — inner constructions are full expressions:

```
fn make(a: Text, b: Int) -> ValueList(Value(Text, Int)) {
  ValueList(Value(Text, Int))(Value(Text, Int)(a, b))
}
```

### ValueList (Homogeneous List)

A list is constructed with a mandatory element-type tag:

```
ValueList(<T>)(<e1>, <e2>, ...)
```

Bracket syntax is also accepted:

```
ValueList(<T>)[<e1>, <e2>, ...]
```

```
fn make(a: Bool) -> ValueList(Bool) {
  ValueList(Bool)(a)
}
```

The inner type `T` must not be bare `Value` — it must be `Value(T1, T2, ...)`.

---

## 10. Function References (fn-ref)

A function can be passed **by name** as an argument to a higher-order registry
function. The callee must declare its parameter as type `Fn` in the registry.

Write the function's name without parentheses or arguments:

```
fn fnref_handler(p: Int) -> Unit {
  Unit()
}

fn fnref_caller() -> Unit {
  apply_handler(fnref_handler)
}
```

`fnref_handler` is a **fn-ref constant**, not a call. The emitted Core IR encodes
the target's identity hash as a constant pool entry of type `Fn`. The surface
does not allow constructing a fn-ref to a function that is not in scope.

---

## 11. Built-in Registry Functions

The M1 registry (`m1-registry.axreg` + `registries/pilot_lowering.axreg`)
provides the following callable functions. All are pure leaf functions unless
noted.

### Value operations

| Name          | Signature                           | Description                         |
| ------------- | ----------------------------------- | ----------------------------------- |
| `value_make`  | `(...) -> Value`                    | Construct a compound Value (variadic)|
| `value_0`     | `(Value) -> Value`                  | Extract slot 0                      |
| `value_1`     | `(Value) -> Value`                  | Extract slot 1                      |
| `value_2`     | `(Value) -> Value`                  | Extract slot 2                      |

### ValueList operations

| Name        | Signature                          | Description                         |
| ----------- | ---------------------------------- | ----------------------------------- |
| `list_make` | `(...) -> ValueList`               | Construct a list (variadic)         |
| `sort`      | `(ValueList, Value) -> ValueList`  | Sort list with comparator lambda    |
| `filter`    | `(ValueList, Value) -> ValueList`  | Filter list with predicate lambda   |
| `map`       | `(ValueList, Value) -> ValueList`  | Transform list elements             |
| `reduce`    | `(ValueList, Value, Value) -> Value` | Fold list with accumulator lambda |

### Pilot scope (from `pilot_lowering.axreg`)

`const_unit` and other primitives are available in the pilot scope and can be
called directly by name in any M1 program compiled with `--reg registries/pilot_lowering.axreg`.

---

## 12. Complete Examples

### Identity function

```
fn identity(x: Bool) -> Bool {
  x
}
```

### Boolean negation

```
fn negate(flag: Bool) -> Bool {
  if flag { Bool(false) } else { Bool(true) }
}
```

### Multiple functions

```
fn identity(x: Bool) -> Bool {
  x
}

fn negate(flag: Bool) -> Bool {
  if flag { Bool(false) } else { Bool(true) }
}
```

### Let-bindings in body

```
fn make_pair(a: Bool, b: Bool) -> Bool {
  let x = a
  let y = b
  x
}
```

### Cross-function call

```
fn negate(flag: Bool) -> Bool {
  if flag { Bool(false) } else { Bool(true) }
}

fn double_negate(flag: Bool) -> Bool {
  negate(negate(flag))
}
```

### Compound value construction

```
fn make(a: Bool, b: Bool) -> Value(Bool, Bool) {
  Value(Bool, Bool)(a, b)
}
```

### Nested value + list construction

```
fn make(a: Text, b: Int) -> ValueList(Value(Text, Int)) {
  ValueList(Value(Text, Int))(Value(Text, Int)(a, b))
}
```

### HOF with lambda

```
fn f(xs: ValueList(Bool)) -> ValueList(Bool) {
  filter(xs, fn(a) { a })
}
```

### Function reference

```
fn handler(p: Int) -> Unit {
  Unit()
}

fn caller() -> Unit {
  apply_handler(handler)
}
```

---

## 13. Compilation Command

```
axis \
  --file <source.m1> \
  --lexer  axis-surface-m1-config/m1-lexer.yaml \
  --parser axis-surface-m1-config/m1-parse.yaml \
  --schema axis-surface-m1-config/m1-ast.yaml \
  --normalize axis-surface-m1-config/m1-normalize.yaml \
  --lowering axis-surface-m1-config/m1-lowering.yaml \
  --reg    axis-surface-m1-config/m1-registry.axreg \
  --reg    registries/pilot_lowering.axreg
```

A successful compile prints one or more `→` lines (one per top-level declaration).
Any other output is an error.

---

## 14. Error Reference

| Error                           | Cause                                                    |
| ------------------------------- | -------------------------------------------------------- |
| `no 0.5 lowering for node kind` | An AST node has no matching rule in the lowering spec     |
| `RULE 1 violation`              | Lambda body references a variable it did not declare     |
| `RULE 2 violation`              | Lambda appears outside a direct call-argument position   |
| `TypeMismatch`                  | Argument type does not match the registry parameter type |
| `type mismatch at value construction` | Wrong type in a Value slot                       |
| `not in scope`                  | Called function is not in any loaded registry            |

---

## 15. Relationship to Other Surfaces

| Surface    | Role                                    |
| ---------- | --------------------------------------- |
| Surface-0  | Canonical instruction encoding          |
| M1         | Human-mechanical expression syntax      |
| H1         | Human-friendly syntax (richer, in progress) |
| AI-Primary | AI-friendly prefix-tree syntax          |

No surface is semantically authoritative except via lowering to Core IR 0.5.

---

## 16. Versioning

* This spec is versioned independently of the pipeline and other surfaces.
* Syntax may evolve; semantics must not.
* Any change that alters the meaning of a valid M1 program requires a new version.

---

**End of specification**
