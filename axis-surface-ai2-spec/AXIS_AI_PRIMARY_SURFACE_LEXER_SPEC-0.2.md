# `AXIS_AI_PRIMARY_SURFACE_LEXER_SPEC-0.1.md`

## Status

**Normative – AI-Primary Surface**

This document defines the lexical layer for the Axis AI-Primary Explicit Prefix surface.

This lexer is **not optional**.
This document explains **why it exists** and **what constraints it enforces**.

---

## 1. Purpose

The AI-Primary lexer exists to provide:

* Deterministic tokenization
* Early, local error detection
* Explicit structural boundaries
* Stable semantics across AI and human generation
* A fixed contract between raw text and grammar

The lexer is part of the **semantic safety boundary**, not a convenience layer.

---

## 2. Design Premise

Axis Lang operates as a **multi-stage pipeline**:

```
Text
 → Lexer
 → Tokens
 → Parser
 → CST
 → Schema AST
 → Lowering
 → Core IR
```

The lexer stage is the **only place** where:

* character-level ambiguity is resolved
* keywords are distinguished from identifiers
* delimiters are validated structurally

Skipping or weakening this stage **pushes ambiguity downstream**, where it becomes harder to detect and reason about.

---

## 3. Non-Goals

The AI-Primary lexer does **not** attempt to:

* Be flexible
* Be forgiving
* Infer intent
* Perform semantic validation
* Provide syntactic sugar
* Support comments or strings
* Optimize token count

Any lexer behavior that improves ergonomics at the cost of determinism is explicitly forbidden.

---

## 4. Why a Lexer Is Required (Even for AI-Generated Code)

### 4.1 LLMs Generate Characters, Not Tokens

LLMs emit raw character streams.
They do **not** guarantee:

* correct keyword boundaries
* consistent whitespace
* valid delimiter pairing
* correct identifier formation

The lexer provides a **hard boundary** that converts probabilistic output into a deterministic token stream.

---

### 4.2 Grammar Assumes Token Categories

The parser grammar for the AI-Primary surface assumes the existence of:

* keywords (`let`, `lam`, `app`, etc.)
* identifiers
* literals
* delimiters (`(`, `)`, `,`, `=`)

Without a lexer, these categories become implicit and context-dependent, which violates Axis invariants.

---

### 4.3 Early Failure Is Mandatory

One of the explicit goals of the AI-Primary surface is **fail-fast behavior**.

The lexer ensures:

* invalid characters fail immediately
* malformed literals fail immediately
* unknown tokens fail immediately

No later stage is allowed to “guess” what the user or AI meant.

---

## 5. Lexical Model

### 5.1 Token Classes

The lexer defines the following token classes:

* **Keywords**
  Fixed, reserved words with semantic meaning.

* **Identifiers**
  Names for variables and bindings.

* **Literals**
  Numeric and boolean literals.

* **Delimiters**
  Structural markers defining tree shape.

* **Whitespace**
  Explicitly ignored.

No other token classes are permitted.

---

### 5.2 Keyword Precedence Rule (Mandatory)

Keywords MUST be recognized **before** identifiers.

This guarantees that:

```
let
lam
app
```

are never treated as variable names.

This rule is **non-negotiable**.

---

## 6. Explicit Rejection of Implicit Lexing

The following are explicitly forbidden:

* Context-sensitive tokenization
* Layout-sensitive rules
* Indentation-based structure
* Implicit separators
* Automatic insertion of tokens
* Heuristic recovery

Any such behavior would make the surface less reliable for AI generation.

---

## 7. Relationship to Parser and AST

The lexer has **no knowledge** of:

* grammar structure
* AST nodes
* semantics
* lowering

Its only responsibility is to produce a **correct, unambiguous token stream** or fail.

This separation is intentional and enforced.

---

## 8. Determinism Guarantees

Given the same input text:

* Tokenization is deterministic
* Token boundaries are deterministic
* Token categories are deterministic
* Error locations are deterministic

If tokenization differs between runs, it is a bug.

---

## 9. Error Model

Lexical errors are:

* Immediate
* Local
* Non-recoverable

Examples:

* Unknown character
* Invalid numeric literal
* Unterminated delimiter
* Reserved keyword used incorrectly (where applicable)

No attempt is made to continue after a lexical error.

---

## 10. Why This Lexer Is “Boring” by Design

This lexer intentionally avoids:

* cleverness
* compression
* convenience features

Because:

> **Clever lexers hide errors.
> Boring lexers expose them.**

For AI-generated code, exposed errors are vastly preferable.

---

## 11. Relationship to Other Surfaces

| Surface    | Lexer Characteristics       |
| ---------- | --------------------------- |
| Surface-0  | Instruction-oriented        |
| Human (H1) | Ergonomic, forgiving        |
| AI-Primary | Explicit, strict, redundant |

The AI-Primary lexer is intentionally **stricter** than human-facing lexers.

---

## 12. Rationale Summary

This lexer exists because:

* LLMs benefit from explicit structure
* Determinism beats flexibility
* Early failure beats late ambiguity
* Token boundaries must be unambiguous
* Grammar and semantics depend on a clean lexical contract

This is an engineering decision driven by observed AI behavior.

---

## 13. Versioning

* This lexer spec is versioned independently
* Changes must not weaken determinism
* Any relaxation requires a new surface version

---

**End of specification**
