# Axis Registry Specification

## Version 0.1 — AI-First Function Registry

**Status: Normative**

---

## 1. Purpose

The Axis Registry defines the **complete and explicit set of callable functions**
available to an Axis program.

It exists to:

* eliminate implicit built-ins
* eliminate guessing by AI systems
* make all callable semantics enumerable
* provide a stable contract between surface syntax, core semantics, and execution

If a function is not present in the active registry, **it does not exist**.

---

## 2. Design Principles (AI-First)

The registry format is optimized for:

* low token overhead
* linear scanning
* regular structure
* append-only growth
* fast ingestion by LLMs and compilers

Human ergonomics are explicitly secondary.

---

## 3. Registry Format (`.axreg`)

The registry is a flat, line-oriented declarative file.

### 3.1 Record Structure

```text
fn <name>
  arity <n>
  deterministic <true|false>
  profile <ProfileId>
end
```

Each `fn` block declares **exactly one callable function**.

No nesting is permitted.

---

### 3.2 Comments**

The Axis Registry format supports **line comments only**.

```text
// this is a comment
```

Rules:

1. A comment begins with the two-character sequence `//`.
2. A comment extends from `//` to the end of the line.
3. Comments are treated as whitespace.
4. Comments may appear:

   * on their own line
   * after any valid registry directive
5. Comments have no semantic meaning.
6. Comments MUST NOT appear inside identifiers or keywords.

The following are explicitly **not supported**:

* Block comments (`/* ... */`)
* Nested comments
* Documentation comments
* Alternate comment markers

Any file containing unsupported comment forms MUST be rejected.

---

## 4. Field Semantics (Normative)

### 4.1 `fn <name>`

The canonical callable function name.

* MUST be unique within the active registry
* MUST be a flat identifier (no dots, no hierarchy)
* Function identity is defined **solely** by this name

No other field may restate or duplicate identity.

---

### 4.2 `arity <n>`

The required number of arguments.

* Fixed arity only
* No varargs
* Enforced during lowering

---

### 4.3 `deterministic <true|false>`

Declares whether the function is semantically deterministic.

Used by:

* profiles
* analysis
* execution gating

---

### 4.4 `profile <ProfileId>`

Declares that the function is admitted under the named profile.

* Multiple `profile` lines are permitted
* If a function is not admitted by the active profile, lowering MUST fail

Profiles constrain **visibility**, not execution or semantics.

---

## 5. Normative Rules

1. **No implicit functions**
   Every callable name MUST appear in the active registry.

2. **Lowering requires registry resolution**
   If a function name cannot be resolved, lowering MUST fail.

3. **Execution uses resolved name only**
   Runtime dispatch MUST occur by canonical function name.

4. **Profiles gate availability, not syntax**
   Syntax admission and function admission are independent.

---

## 6. Registry Composition (Normative)

The `.axreg` format defines individual function declarations only.

It does NOT define:

* dependency resolution
* inclusion
* versioning
* workspace layout
* federation behavior

### 6.1 Active Registry Set

For any compilation or execution, the **active registry** is an explicit set of
registry files supplied by the host environment.

Rules:

1. The active registry is the union of all supplied `.axreg` files.
2. Function names MUST be unique across the active registry.
3. Duplicate function names are a hard error.
4. Registry files MUST NOT include or reference other registry files.
5. Registry composition is external to this specification.

The compiler and runtime MUST NOT infer or auto-load registry files.

---

## 7. Function Promotion and Stability (Normative)

Functions may be promoted between experimental, candidate, and standard registries.

Promotion MUST preserve:

* function name
* arity
* semantic behavior

Promotion MAY change:

* admitted profiles
* governance level

Changing a function’s **name** defines a new function.

---

## 8. Explicit Non-Goals

The registry does NOT:

* define syntax
* define evaluation order
* perform overload resolution
* infer types or arity
* provide documentation or help text
* encode workspace or federation structure

---

## 9. Relationship to AI Code Generation

AI systems generating Axis code MUST:

* consult the active registry
* emit only declared function names
* respect arity and profile constraints

This prevents hallucinated semantics.

---

## 10. Guiding Principle

Axis code defines meaning.
The registry defines permission.
Neither guesses about the other.

