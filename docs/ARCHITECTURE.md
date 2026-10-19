# Axis Lang — Architecture (Authoritative)

## 1. Purpose and Scope

**Axis Lang** is a compiler frontend whose sole responsibility is to transform **surface language source text** into **canonical Core IR**.

Language Lab:

* **does** produce Core IR
* **does not** execute programs
* **does not** generate target code
* **does not** manage runtimes
* **does not** define or invoke bridges
* **does not** execute registry functions

The **only output boundary** of Language Lab is **Core IR**.

Everything beyond Core IR is **explicitly out of scope**.

---

## 2. End-to-End Pipeline (Ground Truth)

The compilation pipeline is linear, deterministic, and stage-isolated:

```
Surface Source Text
        ↓
Lexing
        ↓
Parsing
        ↓
Concrete Syntax Tree (CST)
        ↓
AST Projection (Schema-based)
        ↓
Projected AST
        ↓
Normalization (YAML-driven, mandatory, rewrite + validate)
        ↓
Normal Form (NF) AST (type-enforced boundary)
        ↓
Mechanical Lowering (NF-only, 100% code, fixed, embedded)
        ↓
Core IR 0.2
```

Key properties:

* Each stage consumes the output of the previous stage only
* Information flows **forward only**
* Semantics are assigned **once**, in a single place (Mechanical Lowering)
* Normalization is the **ONLY** rewrite stage
* Mechanical Lowering accepts **ONLY** NF AST (type boundary enforced)
* **NO lowering configuration file exists** — lowering is 100% embedded code

---

## 3. Stage Responsibilities

### 3.1 Lexing

**Inputs**

* Surface source text
* Lexer specification (YAML)

**Outputs**

* Token stream

**Responsibilities**

* Tokenisation only
* Character classification
* Span tracking

**Non-responsibilities**

* Syntax recognition
* Semantic interpretation
* Structural inference

Lexing is **entirely data-driven** and deterministic.

---

### 3.2 Parsing

**Inputs**

* Token stream
* Grammar specification (YAML)

**Outputs**

* Concrete Syntax Tree (CST)

**Responsibilities**

* Grammar recognition
* Structural grouping
* Rule application

**Non-responsibilities**

* Semantic interpretation
* Meaning assignment
* Name resolution

Grammar structure does **not** imply semantic meaning.

---

### 3.3 Schema Projection

**Inputs**

* CST
* AST schema specification (YAML)

**Outputs**

* Projected AST

**Responsibilities**

* Structural validation against schema
* Field extraction from CST
* Union resolution
* Shape normalization (schema-level only)

**Non-responsibilities**

* Semantic meaning
* Evaluation order
* Execution logic
* Surface sugar elimination (handled by Normalization)

**Critical clarification**

* The schema defines **shape only**
* The schema does **not** drive semantics
* The schema does **not** influence lowering
* Projected AST may still contain surface-specific constructs

---

### 3.4 Normalization (YAML-Driven, Rewrite + Validate)

**Status**: **Mandatory, YAML-driven, authoritative, single rewrite stage**

**Implementation**: `src/normalize/` (YAML normalization engine)

**NOTE**: The legacy code in `src/normalisation/` is DEAD and NOT USED. Pipeline uses `src/normalize/` exclusively.

**Inputs**

* Projected AST
* Normalization rules configuration (per-surface: `*/*-normalize.yaml`)
* NF target specification (`core_spec/NORMAL_FORM_SPEC_0.1.yaml`)

**Outputs**

* Normal Form (NF) AST (type-wrapped as `NfAst`)

**Responsibilities**

* **Load** YAML normalization rules from surface config
* **Rewrite** projected AST into Normal Form (semantic-preserving)
* **Eliminate** all surface-level sugar
* **Enforce** NF structural invariants
* **Synthesize** missing constructs (e.g., explicit else branches)
* **Validate** resulting AST against NF spec
* **Wrap** validated AST in `NfAst` type (boundary enforcement)
* **Fail loud** on malformed input or forbidden patterns

**Non-responsibilities**

* Semantic assignment (lowering's responsibility)
* Type inference or checking
* Name resolution beyond structural rewriting
* Registry interaction
* Core IR generation

**Architectural facts**

* Normalization is **mandatory** and **cannot be skipped**
* Normalization is **100% YAML-driven** (rules per surface)
* Normalization is **the ONLY rewrite stage** before lowering
* Normalization applies **deterministic, ordered rewrite rules**
* Normalization **preserves meaning** (semantic equivalence guaranteed)
* Normalization **validates output** and wraps in `NfAst` type
* Lowering **MUST NEVER** handle surface sugar
* **NO CLI flag or config for lowering exists** — lowering is mechanical code only

**Normalization Contract**

```
Valid Projected AST + YAML Rules → NfAst OR loud failure
```

* **Input**: Any valid projected AST from schema projection + YAML rules
* **Output**: Guaranteed NF-compliant AST wrapped in `NfAst` type
* **Failure mode**: Explicit error with span, node kind, and reason

**Auto-Discovery**:
* If `--normalize` not provided, auto-discovered from config dir
* Pattern: `*-normalize.yaml` in same directory as lexer/parser/schema
* Hard error if missing or ambiguous

**Rewrite Rules** (YAML-configured per surface):

* Pattern match on projected AST node kinds and fields
* Replace matched patterns with NF constructs
* Apply rules in explicit, deterministic order
* Support fresh name generation for synthesized bindings
* Operate recursively bottom-up

**Examples of Normalization Rewrites**:

* `if-without-else` → `if-with-explicit-unit-else`
* `block { s1; s2; expr }` → `let _tmp1 = s1 in let _tmp2 = s2 in expr`
* `f(a, b, c)` → `((f a) b) c` (left-associative binary application)
* Implicit sequences → nested `LetExpr`

---

### 3.5 Type Boundary: NfAst

**Status**: **Enforced at compile time**

The `NfAst` type is a **wrapper** around `SchemaAstNode` that can ONLY be constructed by the normalization stage after successful validation.

**Purpose**:
* Enforce that lowering receives ONLY NF-compliant AST
* Prevent accidental surface sugar reaching lowering
* Make NF validation guarantee explicit in the type system

**Construction**:
* `NfAst::from_validated_node()` — normalization stage only (crate-private)
* No public constructor
* Lowering API accepts `&NfAst` or `&SchemaAstNode` from `NfAst::node()`

**Verification**:
* Normalization loads YAML rules
* Normalization rewrites AST
* Normalization validates result against NF spec
* Normalization wraps validated AST in `NfAst`
* Lowering unwraps `NfAst` and proceeds with guarantee

**If validation fails**:
* Normalization returns error
* Pipeline terminates
* Lowering never runs

---

## 4. Normal Form (NF) — Structural Boundary

**Status**: **Normative, versioned, authoritative**

**Definition**

Normal Form (NF) is the **canonical structural representation** that sits between surface languages and semantic lowering. It is the **ONLY** AST shape eligible for mechanical lowering to Core IR.

**Specification**

NF is fully defined in:
* `core_spec/NORMAL_FORM_SPEC_0.1.yaml` (target shape, admissible nodes, invariants)
* `core_spec/NORMALIZATION_RULES_0.1.yaml` (rewrite rules to reach NF)

**An AST in NF**:

* Contains **only** NF-admissible node kinds (closed set)
* Has **explicit** control flow (no implicit branches)
* Has **explicit** sequencing (nested `LetExpr`, no implicit order)
* Contains **no surface-specific constructs**
* Has **explicit else branches** (no implicit unit)
* Uses **binary application** only (no multi-arg sugar)

**NF is**:

* **Enforced** by the compiler (validation is mandatory)
* **Surface-independent** (all surfaces normalize to same NF)
* **Non-extensible** (admissible set is closed and versioned)
* **A hard precondition** for lowering

**NF Admissible Node Set** (NF 0.1):

* Literals: `IntLit`, `BoolLit`, `UnitLit`
* Variables: `VarRef`
* Binding: `LetExpr`
* Functions: `LamExpr`, `AppExpr`
* Control: `IfExpr` (with explicit else)
* Registry calls: `CallExpr`
* Structural wrappers: `Expr`, `Program`

**NF Enforcement**

After normalization, the compiler **validates** that the resulting AST conforms to the NF-Admissible Node Set and contains no forbidden patterns or constructs.

**If validation fails**:
* Compilation **terminates immediately**
* Error message includes: span, node kind, violation reason
* Lowering **MUST NOT run**

**If an AST is not in NF**:
* It **cannot** be lowered
* It **must** pass through normalization first
* Lowering has **zero responsibility** to handle non-NF constructs

**NF Versioning**

* NF spec is versioned independently (current: 0.1)
* Breaking changes require version bump
* Target Core IR version is explicitly declared (0.2)
* Lowering must verify NF version compatibility

---

## 5. Mechanical Lowering (Semantic Boundary)

### Definition

**Mechanical Lowering** is the single component that:

> Consumes **NfAst** (and ONLY NfAst) and emits **Core IR 0.2**.

That is its **only responsibility**.

---

### What Mechanical Lowering Is

* A deterministic NF AST → Core IR transformer
* The **sole semantic authority**
* Canonical and versioned
* Fully embedded in the compiler (100% Rust code, no config file)
* **NF-only by construction** (enforced by `NfAst` type boundary)
* **Located**: `src/lowering/h1_lowering.rs` (current implementation)

---

### What Mechanical Lowering Is Not

* A bridge
* A runtime
* A code generator
* A registry executor
* A user-configurable component
* A rewriting or transformation engine (that's Normalization)
* **NOT** driven by any YAML/config file

---

### Current and Intended State

* Lowering logic is **fixed** and **embedded** (Rust code)
* Lowering rules are **NOT user-configurable**
* **NO lowering config file exists or should exist**
* Lowering is **versioned with the compiler**
* Identical NF AST inputs produce identical Core IR outputs
* Lowering performs **zero rewrites** (all sugar eliminated by Normalization)
* Lowering performs **zero validation** (already done by Normalization)

This is intentional and foundational.

---

### Lowering Contract (NfAst → Core IR)

**Preconditions**:
* Input is `NfAst` type (enforced at compile time)
* NF version matches lowering version
* All surface sugar eliminated

**Guarantees**:
* Every NF node maps to exactly one Core IR construct
* Mapping is deterministic and total
* No ambiguity, no fallbacks, no heuristics
* Output is valid Core IR 0.2

**Mapping** (1:1, mechanical):

| NF Node Kind | Core IR Construct |
|--------------|-------------------|
| `IntLit`     | `CIntLit`         |
| `BoolLit`    | `CBoolLit`        |
| `UnitLit`    | `CUnitLit`        |
| `VarRef`     | `CVar`            |
| `LetExpr`    | `CLet`            |
| `LamExpr`    | `CLam`            |
| `IfExpr`     | `CIf`             |
| `AppExpr`    | `CApp`            |
| `CallExpr`   | `CCall`           |

**Forbidden Operations** (Lowering MUST NOT):
* Rewrite or transform AST structure
* Handle surface sugar
* Infer missing information
* Validate structural correctness
* Resolve names beyond what's explicit
* Consult normalization rules

**If lowering receives non-NF AST**:
* **PANIC** or **FAIL LOUD** immediately
* This indicates a compiler bug (NF validation was bypassed)

---

## 6. Core IR (Output Boundary)

**Produced by**

* Mechanical Lowering

**Consumed by**

* Bridges and tooling (out of scope)

**Version**

* Core IR 0.2

**Properties**

* Canonical
* Deterministic
* Semantics-complete
* Immutable
* Target-agnostic
* Versioned and stable

Language Lab treats Core IR as:

> The final, authoritative representation of program meaning.

**Core IR Node Set** (0.2, closed):

* `CIntLit`, `CBoolLit`, `CUnitLit`
* `CVar`
* `CLet`
* `CLam`
* `CIf`
* `CApp`
* `CCall`

All higher-level constructs are eliminated before Core IR emission.

---

## 7. Compiler Hooks (Structural Guidelines)

**Note**: Compiler hooks are **implementation-level guidelines** not specified in the Input Specification Contracts (AXIS_INPUT_SPECIFICATION_CONTRACTS-0.2.md). Hooks do not alter semantic authority or contractual obligations. See `AXIS_COMPILER_HOOKS_AND_PLUGINS_SPECIFICATION_0.1.md` for details.

Language Lab exposes **compiler hook points** as **guidelines**, not enforcement mechanisms.

Hooks exist to enable:

* structural adaptation
* surface experimentation
* desugaring research
* tooling and diagnostics
* legacy interop

Hooks **do not** exist to:

* alter semantics
* influence lowering
* introduce execution behaviour
* bypass Normal Form

---

### 7.1 Hook Placement in the Pipeline

Hooks may be invoked at specific structural boundaries:

* pre-lex (text-level)
* post-parse (surface structure)
* during normalisation
* post-normalisation (read-only)

Hooks operate on **declared data shapes** and are expected to respect:

* determinism
* stage boundaries
* Normal Form requirements

Lowering is **not hookable**.

---

### 7.2 Relationship to Semantics

Hooks are **structural only**.

* They do not assign meaning
* They do not interpret intent
* They do not affect Core IR semantics

This keeps the semantic boundary explicit and defensible.

---

## 8. Bridges (Explicitly Out of Scope)

Bridges:

* consume Core IR
* project semantics into execution or tooling environments
* may interpret intent metadata

Bridges:

* do not participate in Language Lab
* do not influence lowering
* do not affect surface semantics

This separation is intentional and non-negotiable.

---

## 9. Spec-Driven vs Embedded Components

| Stage                  | Spec-Driven | Embedded | Configuration File(s)                       |
| ---------------------- | ----------- | -------- | ------------------------------------------- |
| Lexer                  | Yes         | No       | `*-lexer.yaml`                              |
| Parser                 | Yes         | No       | `*-parse.yaml`                              |
| AST Projection         | Yes (shape) | No       | `*-ast.yaml`                                |
| Normalization          | Yes         | No       | `NORMALIZATION_RULES_0.1.yaml`              |
| NF Target Spec         | Yes         | No       | `NORMAL_FORM_SPEC_0.1.yaml`                 |
| NF Validation          | No          | Yes      | (uses NF spec)                              |
| Mechanical Lowering    | No          | Yes      | (fixed, versioned)                          |
| Core IR                | Canonical   | Yes      | (output format)                             |

**Spec-Driven** stages are configured via user-supplied or surface-specific YAML specifications. 

**Embedded** stages are fixed within the compiler binary.

**Normalization is spec-driven** (configured via YAML), but **NF Validation and Mechanical Lowering are embedded** (fixed, versioned, non-configurable).

Lowering is intentionally embedded to preserve semantic stability.

---

## 10. Sealed vs Unsealed Builds

Language Lab supports two deployment modes:

**Sealed builds** embed all user specifications (`lexer.yaml`, `parsing.yaml`, `ast_schema.yaml`, `normalisation.yaml`) at compile time, producing a self-contained, deterministic compiler binary.

**Unsealed builds** load specifications at runtime from user-specified file paths, enabling rapid iteration and experimentation.

Lowering is **always embedded** regardless of build mode.

---

## 11. Architectural Invariants

These invariants must not be broken:

* Core IR is the only output
* Semantics are defined **once** (in Mechanical Lowering only)
* Semantics live **only** in lowering
* Lowering is fixed, versioned, and embedded
* **Normalization is the ONLY rewrite stage**
* **Normalization is mandatory** and cannot be skipped
* **NF Validation is mandatory** and blocks lowering on failure
* **Lowering accepts ONLY NF AST** (enforced by construction)
* NF is surface-agnostic
* Hooks are structural only
* Bridges are out of scope
* **Lowering performs zero rewrites** (all sugar eliminated by Normalization)
* **Lowering performs zero validation** (already done by NF Validation)

---

## 12. Naming Clarification

Within Language Lab, the terms:

* **"Backend"** or **"IR-Lowering Backend"** or **"Mechanical Lowering"** refer **only** to:
  > **Mechanical Lowering (NF AST → Core IR 0.2)**

* **"Normalization"** (NOT "desugaring") is the rewrite stage:
  > **Normalization (Projected AST → NF AST)**

* **"NF Validation"** is the boundary enforcement stage:
  > **NF Validation (NF AST → ✓ or ✗)**

Nothing else.

---

## 13. Architectural Status

The current architecture is **intentional, stable, and sufficient**.

Future evolution must preserve:

* determinism
* semantic versioning
* authority boundaries
* Core IR equivalence guarantees

---

### Final Summary

> Axis Lang is a deterministic, data-driven compiler frontend that transforms surface languages into canonical Core IR 0.2 via:
> 
> 1. **AST Projection** (surface-specific, schema-driven)
> 2. **Normalization** (surface-agnostic, rewrite to NF, ONLY rewrite stage)
> 3. **NF Validation** (mandatory boundary enforcement)
> 4. **Mechanical Lowering** (NF-only, fixed, sole semantic authority)
> 
> **Normalization** is the single, explicit, mandatory rewrite stage.  
> **Mechanical Lowering** is total and unambiguous from NF AST to Core IR.  
> **There is no separate desugaring stage.**
```
