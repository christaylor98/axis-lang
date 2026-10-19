# **Axis Surface Language Specification**

## **Version 0.7 — AI-Only Surface (Final, Simplified)**

*A deterministic, minimal, AI-first surface syntax that lowers unambiguously into Axis Core under an explicit Surface Profile.*

---

## **0. Design Principles (Normative)**

Surface-0.7 exists **solely** to make AI code generation, repair, and transformation:

1. **Precise**
2. **Unambiguous**
3. **Low-entropy**
4. **Deterministic to lower**

Accordingly:

* There is **exactly one surface representation** for each semantic idea.
* All semantics live in **Axis Core** or **explicit foreign contracts**.
* Surface constructs that introduce choice, positional ambiguity, or cleverness are forbidden.
* Anything not proven to reduce AI reasoning steps is excluded.

All surface function calls MUST resolve to either:

* a function declared in the **Axis Registry**, or
* a foreign function declared in the Registry.

The compiler MUST:

* lower and validate each function independently
* assume **no privileged entry point**
* reject unresolved identifiers at compile time

Axis does **not** compile “programs”.
Compilation correctness is defined **per function**, not per file or collection of files.

---

## **1. Surface Profiles (Normative)**

All surface expressions are interpreted under an explicit **Surface Profile** `p`.

A profile defines:

* which surface constructs are admitted
* which foreign primitives are allowed
* any static limits (e.g. recursion depth, fuel caps)

**Rule:**
If a construct is not admitted by the active profile, the program **MUST be rejected before lowering**.

Notation:

```
⟦ e ⟧ₚ
```

means *lower surface expression `e` under profile `p`*.

---

## **2. AI-Centric Foreign Function Interface (FFI v1)**

The FFI exists **solely** to collapse multi-step reasoning for AI.

All foreign functions are:

* surface-only
* deterministic
* side-effect free
* late-bound by the host

### **2.1 Foreign Function Set (Final, Minimal)**

```text
@deterministic
foreign fn axis_parse(source: String, profile: ProfileId) -> ParseResult

@deterministic
foreign fn axis_lower(surface: SurfaceAst, profile: ProfileId) -> LowerResult

@deterministic
foreign fn axis_check(core: CoreTerm, profile: ProfileId) -> SemanticResult

@deterministic
foreign fn axis_exec(core: CoreTerm, fuel: Int) -> ExecResult
```

No other foreign functions are admitted in Surface-0.7.

---

### **2.2 Type Sketches (Structural, Minimal)**

```text
type ProfileId = opaque

type SurfaceAst = opaque
type CoreTerm   = opaque
type CoreValue  = opaque

type Span = { start: Int, end: Int }

enum ParseResult    { Ok(ast), Err(ParseError) }
enum LowerResult    { Ok(core), Err(LowerError) }
enum SemanticResult { Ok, Err([SemanticError]) }
enum ExecResult     { Ok(CoreValue), Err(ExecError) }

enum ParseErrorKind    { UnexpectedToken, UnexpectedEOF, InvalidLiteral, ProfileDenied }
enum LowerErrorKind    { ProfileDenied, UnboundName, MalformedSurface }
enum SemanticErrorKind { TypeMismatch, UnboundName, NonTotal, ForeignNotAdmitted }
enum ExecErrorKind     { OutOfFuel, Stuck }
```

All failures are **structural only**.
No messages. No heuristics. No recovery.

---

## **3. Lexical Structure**

### **3.1 Identifiers (Normative)**

```
IDENT ::= [A-Za-z_][A-Za-z0-9_]*
```

* Case-sensitive
* **MUST NOT contain `.`**
* Flat, opaque, atomic

Axis **does not encode hierarchy in syntax**.
Any notion of grouping, ownership, or structure is **semantic** and handled by the Axis Registry.

---

### **3.2 Literals**

```
Int  ::= 0 | [1-9][0-9]*
Bool ::= true | false
Unit ::= ()
```

Only literals admitted by default profiles.

---

## **4. Name Resolution (Normative)**

Axis defines:

* **no modules**
* **no imports**
* **no aliases**
* **no qualified names**
* **no file-based identity**

All names are:

* flat identifiers
* resolved by **exact string match**
* validated against the **Axis Registry**

Rules:

* Any unresolved name is a **compile-time error**
* Names are lexically scoped
* Bindings are immutable
* No rebinding
* No mutation

---

## **5. Functions**

### **5.1 Function Declaration**

```
fn name(param: T) -> R { body }
```

* Single surface form only
* Multi-parameter functions lower via tuple packing
* No implicit overloading

---

### **5.2 Lambda**

```
|x| expr
```

* Single-argument only
* Multi-argument lambdas lower via tuple packing
* No closures unless admitted by profile

---

## **6. Tuples and Explicit Projection**

### **6.1 Tuple Construction**

```
(e1, e2, ..., en)
```

* Tuples are ordered, positional values
* Arity is fixed at construction
* Tuples carry **no surface-visible structure**

---

### **6.2 Explicit Projection (`proj`)**

#### **Syntax (Normative)**

```
proj(expr, index)
```

Where:

* `expr` is any surface expression
* `index` is a **non-negative integer literal**
* indexing is **zero-based**

Examples:

```axis
proj(x, 0)
proj(pair, 1)
proj(proj(t, 0), 2)
```

#### **Semantics**

For a tuple value:

```
(x₀, x₁, ..., xₙ₋₁)
```

```
proj((x₀, x₁, ..., xₙ₋₁), i)  ↦  xᵢ
```

iff:

```
0 ≤ i < n
```

Otherwise:

* compile-time error if statically known
* semantic error if detected during checking

---

### **6.3 Parameter Unpacking (Lowering Rule)**

All multi-parameter functions lower via tuple packing and **explicit `proj`**.

Example:

```axis
fn f(a: A, b: B) -> R { body }
```

Lowers to core as:

```
fn f(arg: (A, B)) -> R {
    let a = proj(arg, 0);
    let b = proj(arg, 1);
    body
}
```

`proj` is the **only** surface-level projection mechanism.

---

## **7. Records**

### **7.1 Construction**

```
User { id: 1, name: 2 }
```

Records are **pure data construction only**.

---

### **7.2 Field Access (Explicitly Forbidden)**

Surface-0.7 **does NOT include field-access syntax**.

* Expressions of the form `u.name` are **invalid**
* There is **no `.` operator**
* `proj` **does not apply to records**

Any record access MUST occur via:

* registry-declared accessor functions
* core-level primitives
* or contract-defined operations

Field access is **not surface syntax**.

---

## **8. Enums**

### **8.1 Declaration**

```
enum Option {
    None,
    Some(value: Int),
}
```

---

### **8.2 Construction**

```
Option_None
Option_Some(5)
```

(Enum constructors are flattened identifiers.)

---

## **9. Pattern Matching**

```
match expr {
    Option_None    => e1,
    Option_Some(x) => e2,
}
```

* Exhaustive only
* Guards only if admitted by profile
* Lowers deterministically to nested `if`

Any destructuring introduced by patterns is lowered using **explicit `proj`**.

---

## **10. Control Flow**

### **10.1 If-Expression**

```
if cond { e1 } else { e2 }
```

Always total.

---

## **11. Block Expressions**

```
{
    let x = e1;
    let y = e2;
    e3
}
```

Blocks are expressions.
The final expression is the result.

---

## **12. Formal Lowering Requirements (Augmented)**

For a fixed profile `p`:

1. `⟦ · ⟧ₚ` is deterministic
2. Lowering introduces no new semantics
3. All non-core behavior occurs **only** through foreign primitives admitted by `p`
4. **All access to structured values MUST lower through explicit `proj` nodes**
5. Lowering MUST introduce **no implicit projection or destructuring**

---

## **13. Execution and Compilation Model (Normative)**

* Axis has **no static entry point**
* There is **no privileged `main`**
* All functions are equal

Execution roots MUST be selected externally (CLI, runtime, host, tests).

The compiler MUST:

* parse declarations in any order
* lower functions independently
* validate registry resolution, arity, and determinism
* emit callable artifacts
* **MUST NOT** synthesize an entry function

Files, filenames, directories, and load order:

* have **zero semantic meaning**
* MUST NOT affect identity or behavior
* concatenation is semantics-preserving

Function identity is **registry-centric**.
Surface identifiers are flat and purely syntactic.

---

## **14. Explicit Non-Goals (Locked)**

Surface-0.7 **does NOT include**:

* modules
* imports or `use`
* namespaces
* qualified names
* file-based identity
* visibility modifiers
* implicit hierarchy
* field-access syntax
* implicit destructuring
* tuple indexing syntax (`x.0`)
* reflective access
* dynamic loading
* syntactic sugar

---

## **15. Summary — Why This Works for AI**

Surface-0.7 gives AI:

* one identifier form
* one binding form (`let`)
* one abstraction form (`fn`)
* one projection form (`proj`)
* one control flow primitive (`if` / `match`)
* one deterministic pipeline:

```
generate → parse → lower → check → exec
```

Everything else is **deliberately excluded**.

Hierarchy is semantic.
Meaning is declared.
**Syntax does not lie.**