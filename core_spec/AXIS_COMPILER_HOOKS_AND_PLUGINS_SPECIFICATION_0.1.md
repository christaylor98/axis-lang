> **Axis Lang — Compiler Hooks & Plugins Specification**
> (Guidelines, v0.1)

---

# Axis Lang — Compiler Hooks & Plugins Specification

**Guidelines (v0.1)**

---

## 1. Purpose and Scope

This document describes the **compiler hook and plugin model** exposed by **Axis Lang**.

Compiler hooks provide **structured extension points** within the compilation pipeline, enabling experimentation, tooling, and adaptation **without changing program semantics**.

This document is **guidance**, not an enforcement contract.

It exists to:

* establish shared expectations
* prevent accidental semantic coupling
* keep the compiler architecture legible as the ecosystem grows

---

## 2. What a Compiler Hook Is (and Is Not)

In Axis, a compiler hook is **not**:

* a language extension mechanism
* a way to customise program meaning
* a facility for altering execution behaviour
* a mechanism to override lowering semantics

A compiler hook **is** best understood as:

> A *structural interception point* at a **named pipeline boundary**, operating on **declared data shapes**, with **explicitly understood read/write intent**.

This framing is intentional. Hooks exist to support **structure**, not **meaning**.

**Important:** Hooks are **guidelines for extension points**, not enforcement mechanisms. They provide a shared vocabulary for discussing structural transformations without creating hard constraints.

---

## 3. Intended Uses

Compiler hooks are well suited to:

* surface adaptation and experimentation
* desugaring and normalisation exploration
* compiler tooling (analysis, tracing, diagnostics)
* legacy syntax or format interop
* research and prototyping

They are **not intended** for:

* extending the language semantics
* bypassing normalisation
* influencing lowering decisions (lowering is fixed and embedded)
* embedding execution or runtime policy

Keeping this boundary clear avoids long-term architectural drift.

**Note:** These are recommended use patterns, not hard restrictions. Hooks provide flexibility while maintaining semantic clarity.

---

## 4. Documentation Expectations for Hook Points

When a hook point is documented, it is recommended that the documentation clearly describe the following aspects:

1. **Pipeline stage**
   Where in the compilation pipeline the hook runs.

2. **Input shape**
   The data structure provided to the hook.

3. **Output shape**
   What the hook may return or modify.

4. **Intended permissions**
   Read-only vs structural rewrite.

5. **Ordering model**
   How this hook relates to others in the same stage.

6. **Determinism expectations**
   Whether identical inputs should yield identical outputs.

7. **Known limitations**
   Behaviours the hook is not designed to support.

These dimensions are not bureaucratic — they are what keep hooks understandable and composable over time.

---

## 5. Placement Within the Documentation Set

Compiler hooks are **compiler-level concerns**, not language-level ones.

Accordingly:

* Surface language specifications should not rely on hooks
* Lowering specifications should not reference hooks
* Language reference documentation should remain hook-agnostic

Hooks are defined **here**, and only **referenced lightly** elsewhere.

In particular, the Input Specification Contracts may mention that:

* normalisation may invoke registered hooks
* hooks operate prior to lowering
* hooks are expected to respect Normal Form

But they should not define hook APIs or behaviour.

---

## 6. Hook Lifecycle Overview

Hooks are invoked at **well-defined pipeline boundaries**, typically:

* after one stage completes
* before the next stage begins

They operate on **explicit data structures** and return results that flow forward in the pipeline.

Hooks are not designed to:

* observe future pipeline stages
* retroactively modify earlier stages
* communicate across stages except through their declared outputs

This keeps the pipeline predictable and debuggable.

---

## 7. Common Hook Stages

The following hook stages are commonly useful and are supported conceptually by Axis Lang.

These are **guidelines**, not guarantees.

---

### 7.1 Pre-Lex Hooks (Text-Level)

**Pipeline position:** before lexing
**Typical input:** raw source text
**Typical output:** transformed source text

**Common uses:**

* file inclusion or stitching
* macro expansion
* generated headers
* legacy text adaptation

**Notes:**

* No AST exists at this point
* Hooks here should avoid making semantic assumptions

---

### 7.2 Post-Parse Hooks (Surface Structure)

**Pipeline position:** after parsing, before schema projection
**Typical input:** parse tree or surface AST
**Typical output:** surface AST

**Common uses:**

* structural rewrites
* surface experimentation
* syntax adaptation

**Notes:**

* Meaning is still undefined
* These hooks should remain purely structural

---

### 7.3 Normalisation Hooks (Primary Extension Area)

**Pipeline position:** during normalisation passes
**Typical input:** Schema AST
**Typical output:** Schema AST

**Common uses:**

* desugaring
* explicit control-flow expansion
* surface feature collapse

**Strong expectation:**

* Output should conform to **Normal Form**
* Only NF-admissible constructs should remain

Normalisation hooks are where most legitimate compiler experimentation lives.

---

### 7.4 Post-Normalisation Hooks (Read-Only)

**Pipeline position:** after NF validation, before lowering
**Typical input:** NF Schema AST
**Typical output:** metadata or diagnostics

**Common uses:**

* analysis
* tracing
* debug visualisation
* reporting

These hooks are observational by design.

---

## 8. Ordering and Composition

When multiple hooks apply at the same stage, ordering is typically resolved using:

1. pipeline stage
2. declared normalisation pass (if applicable)
3. explicit before/after hints
4. stable name ordering

Where ordering is ambiguous, it is recommended that tooling surface this clearly rather than guessing.

---

## 9. Error Handling and Failure Behaviour

If a hook:

* produces structurally invalid output
* violates Normal Form expectations
* behaves nondeterministically
* fails unexpectedly

The compiler may choose to halt compilation and report the issue.

Hooks are not expected to provide recovery semantics.

---

## 10. What Hooks Are Not Intended For

To keep the compiler architecture coherent, hooks are **not a good fit** for:

* feature flags or conditional semantics
* performance tuning
* execution strategy selection
* intent enforcement or interpretation

Those concerns belong elsewhere in the Axis stack.

---

## 11. Compatibility and Evolution

Hook APIs are:

* explicitly versioned
* opt-in
* expected to evolve

Stability is a goal, but **not at the cost of architectural clarity**.

Experimental hooks are encouraged; long-term reliance on unstable hooks should be done with care.

---

## 12. Closing Perspective

Compiler hooks exist to make Axis **adaptable without becoming ambiguous**.

Used well, they enable:

* experimentation
* tooling
* research
* interoperability

Used poorly, they blur the semantic boundary Axis is explicitly designed to protect.

This document exists to help the ecosystem stay on the right side of that line — collaboratively, not coercively.

**Hooks are guidelines and extension points, not enforcement mechanisms.** They document where structural transformations can occur and what constraints should be considered, while leaving implementation decisions to language designers and tool authors.

---

### One-line summary

> Hooks shape structure, not meaning.
> Normalisation explains.
> Lowering defines (and is fixed/embedded).
> The compiler stays honest.
