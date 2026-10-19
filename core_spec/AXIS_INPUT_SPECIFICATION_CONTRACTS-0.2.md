# Axis Lang — Input Specification Contracts

**Version 0.2**

---

## 1. Scope and Authority

This document defines all **user-supplied input files** required by the Axis Lang compilation pipeline.

A *user input* is a file that:

* Defines lexical structure, syntactic grammar, AST schema, **normalisation rules**, or semantic lowering rules
* Is consumed by Language Lab to produce Core IR from source code
* Is not produced by Language Lab itself

### User inputs are:

* `lexer.yaml`
* `parsing.yaml`
* `ast_schema.yaml`
* `normalisation.yaml`

### User inputs are NOT:

* Source files written in a target language
* Registry files
* Configuration files
* Build artifacts

---

## 2. Pipeline Overview (Input-Centric)

| Stage             | Input File(s)      | Consumes                | Produces          | Semantic Authority |
| ----------------- | ------------------ | ----------------------- | ----------------- | ------------------ |
| Lexing            | lexer.yaml         | Source text             | Token stream      | No                 |
| Parsing           | parsing.yaml       | Token stream            | Parse tree        | No                 |
| Schema projection | ast_schema.yaml    | Parse tree              | Schema AST        | No                 |
| **Normalisation** | normalisation.yaml | Schema AST              | **NF Schema AST** | No                 |
| Lowering          | *(embedded)*       | NF Schema AST, Registry | Core IR bundle    | Yes                |
| Execution         | None               | Core IR bundle          | Runtime values    | No                 |

**Semantic authority resides exclusively in lowering.**
All prior stages perform **structural transformation only**.

---

## 3. Lexer Specification (`lexer.yaml`)

*(unchanged from v0.1 — retained verbatim)*

> No semantic interpretation
> Deterministic token streams only

---

## 4. Parser Specification (`parsing.yaml`)

*(unchanged from v0.1)*

> Parsing defines **shape**, never meaning.

---

## 5. AST Schema Specification (`ast_schema.yaml`)

*(unchanged from v0.1)*

> The Schema AST is **structural truth**, not semantic truth.

---

## 6. Normalisation and Normal Form (NF)

### 6.1 Purpose

**Normalisation** is a mandatory structural phase that converts a surface-projected Schema AST into **Normal Form (NF)**.

Normalisation:

* removes surface-level sugar
* makes all control flow explicit
* ensures lowering receives a **single canonical structural shape**

Normalisation is **not** a semantic authority.

> **Lowering remains the sole semantic authority.**

---

### 6.2 Position in the Pipeline

Normalisation occurs **after schema projection and before lowering**.

Lowering MUST assume its input is already in Normal Form.

Lowering MUST NOT:

* perform surface desugaring
* compensate for missing normalisation
* handle surface-specific constructs

Any surface construct that reaches lowering is a **compiler error**.

---

### 6.3 Definition of Normal Form (NF)

A Schema AST is in **Normal Form (NF)** if and only if:

* All surface-level control constructs have been eliminated
* All control flow is explicit
* All iteration is explicit
* No surface-specific sugar remains
* The AST conforms to the **NF-Admissible Node Set**

**NF is surface-agnostic.**
It is not tied to H1 or any future surface.

---

### 6.4 NF-Admissible Node Set (Normative)

After normalisation, the Schema AST **MUST contain only nodes from the following admissible set**.

This set defines the **maximum structural shape** permitted to reach lowering.

#### 6.4.1 Admissible Node Categories

The following categories are permitted:

**Program Structure**

* `Program`
* `FunctionDecl`

**Binding and Scope**

* `LetExpr`
* `Block`

**Control Flow**

* `IfExpr`

**Computation**

* `LambdaExpr`
* `CallExpr`

**Values**

* `Ident`
* `Literal`
* `Unit`

**Effects**

* Effect-producing expressions as defined by the schema (e.g. calls whose lowering produces effects)

---

#### 6.4.2 Explicitly Forbidden Post-Normalisation Nodes

The following node kinds **MUST NOT appear** in NF:

* `ForExpr`
* `LoopExpr`
* surface `MatchExpr`
* surface pattern nodes
* implicit sequencing constructs
* implicit iteration constructs
* surface-only convenience or helper nodes

If any forbidden node is present after normalisation, compilation **MUST fail**.

---

#### 6.4.3 Authority of the NF-Admissible Set

* This set is **structural**, not semantic
* It does **not** define evaluation or meaning
* It is enforced **before lowering**
* It is **stricter than lowering**
* Lowering MUST NOT expand or reinterpret this set

This set exists solely to answer:

> *“Is this AST allowed to reach lowering?”*

---

### 6.5 Normalisation Guarantees

Normalisation MUST guarantee that:

* Transformation is **deterministic**
* Transformation is **total** for all admitted surface constructs
* Transformation is **semantics-preserving**
* No semantic meaning is introduced or inferred
* Output is valid Schema AST
* Output conforms to the NF-Admissible Node Set

Normalisation MUST NOT:

* access the registry
* resolve names
* infer types
* execute effects
* emit Core IR
* depend on execution behavior

---

### 6.6 Normalisation Specification (`normalisation.yaml`)

Normalisation rules are defined in a dedicated input file:

```
normalisation.yaml
```

This file:

* defines ordered normalisation passes
* defines pattern-based structural rewrite rules
* operates exclusively on Schema AST
* produces Schema AST in Normal Form

Rules are **structural only** and MUST NOT encode semantics.

---

### 6.7 Ordering and Determinism

Normalisation is applied as an **explicitly ordered sequence of passes**.

* Pass order is fixed
* Rule application is deterministic
* Ambiguous rule application is a **hard error**

Identical inputs MUST always produce identical NF output.

---

### 6.8 Validation and Failure Conditions

After normalisation, the compiler MUST validate that the resulting AST:

* conforms to the NF-Admissible Node Set
* contains no forbidden surface constructs
* is structurally compatible with lowering rules

The compiler MUST error if:

* a surface construct survives normalisation
* a forbidden node appears post-normalisation
* no normalisation rule exists for an admitted surface construct
* the resulting AST violates the Schema AST definition
* the resulting AST is not valid NF

Failures are fatal.
No fallback. No partial lowering.

---

### 6.9 Worked Example (Normative)

#### Surface Input (example language)

```text
for x in items {
  emit(process(x));
}
```

#### Schema AST (pre-normalisation, conceptual)

```
ForExpr
 ├─ iter: Ident(items)
 └─ body:
     Call emit(Call process(x))
```

#### After Normalisation (NF)

```
Let loop =
  Lambda(iter) {
    If has_next(iter) {
      let x = next(iter);
      emit(process(x));
      loop(iter);
    } else {
      Unit
    }
  }

Call loop(items)
```

Properties demonstrated:

* No surface nodes remain
* Iteration is explicit
* Control flow is explicit
* Output uses only NF-admissible nodes
* Semantics are unchanged

This NF AST is the **only valid input to lowering**.

---

## 7. Lowering (Fixed, Mechanical, Embedded)

Lowering is **no longer user-configurable**.

Lowering is:

* **fixed and mechanical**
* **embedded in the compiler**
* **versioned with the compiler release**

Lowering consumes **NF Schema AST only**.

Lowering:

* assigns semantics
* resolves registry bindings
* emits Core IR

Lowering MUST NOT:

* handle surface sugar
* infer missing structure
* compensate for invalid NF

**Authority:** Lowering is the sole semantic authority, but is no longer extensible via user-supplied specifications.

---

## 8. Core IR Boundary (Input / Output Contract)

*(unchanged from v0.1)*

> Core IR is immutable, deterministic, and semantically complete.

---

## 9. Execution Boundary (Non-Input Clarification)

*(unchanged from v0.1)*

> Execution projects semantics; it never defines them.

---

## 10. Sealed vs Unsealed Builds

Sealed builds embed specifications at compile time for deterministic, distribution-ready compilation.

* In **sealed builds**: `lexer.yaml`, `parsing.yaml`, `ast_schema.yaml`, and `normalisation.yaml` are embedded in the compiler binary
* In **unsealed builds**: these files are loaded at runtime from user-specified paths

Lowering is **always embedded** regardless of build mode, as it is no longer user-configurable.

---

## 11. Invariants and Failure Conditions

### Additional Invariants (v0.2)

* **Normal Form is mandatory**
* **Lowering only consumes NF**
* **NF-admissible node set is enforced**
* **Normalisation is structural only**
* **Surface semantics never reach lowering**
* **NF is surface-agnostic and permanent**

All prior invariants remain in force.

---

## 12. Versioning Note

Version 0.2 introduces:

* mandatory Normalisation
* explicit Normal Form
* a normative NF-Admissible Node Set
* `normalisation.yaml` as a first-class input
* a hard boundary between surface sugar and semantic lowering
* **lowering as fixed, mechanical, and embedded (no longer user-configurable)**

No existing semantic contracts are weakened.

---

### One-line summary

> Surfaces explain meaning.
> Normalisation makes it explicit.
> Lowering freezes it.
> Core IR defines it.