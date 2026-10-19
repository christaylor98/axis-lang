# Axis Surface Language v0.7 — Errata Note

**Errata ID:** S07-E01
**Status:** Accepted
**Date:** 2026-01-17

---

## Erratum: Function Return Arrow Token (`->` vs `=>`)

### Summary

An inconsistency exists between the **Surface Language Specification (v0.7)** and the **Lexical Specification (v0.7)** regarding the token used to denote function return types.

The surface specification documents function declarations using the **thin arrow** `->`, while the lexical specification admits only the **fat arrow** `=>`.

This erratum resolves the inconsistency.

---

## Normative Resolution

### 1. Function Return Types

The **canonical and correct syntax** for function return types in Surface-0.7 is:

```axis
fn name(param: T) -> R { body }
```

The token `->` (THIN_ARROW) is **admitted and required** for function return type annotation.

---

### 2. Match Arms and Structural Mapping

The token `=>` (FAT_ARROW) is **retained exclusively** for:

* `match` arms
* structural mappings introduced by pattern matching

Example:

```axis
match x {
    Option_None    => e1,
    Option_Some(v) => e2,
}
```

---

## Lexical Specification Update

The lexical specification (§10 Punctuation and Symbols) is amended to include:

| Symbol | Token      |
| -----: | ---------- |
|   `->` | THIN_ARROW |
|   `=>` | FAT_ARROW  |

Rules:

* Multi-character tokens MUST be recognized greedily
* `->` and `=>` are distinct tokens
* They MUST NOT be treated as interchangeable

---

## Rationale

* `->` semantically represents **type transition** (input → output)
* `=>` semantically represents **mapping / arm dispatch**
* Conflating these increases ambiguity and reduces semantic clarity
* The bootstrap compiler and existing Axis surface code already and correctly use `->`

This erratum aligns:

* the lexer
* the surface specification
* the bootstrap implementation

without introducing new surface constructs or semantic changes.

---

## Compatibility

* Existing `.ax` code using `->` is **correct and forward-compatible**
* No rewrite of surface code is required
* This erratum does **not** change lowering semantics

---

## Versioning

This correction may be referred to as:

* **Surface-0.7 (with errata)**
  **or**
* **Surface-0.7.1** (editorial / corrective bump)

No semantic version bump is required for Core IR or runtime components.

---

## Axis Principle Affirmed

> The lexer serves the language.
> The language does not serve the lexer.

This erratum restores internal consistency without weakening determinism, simplicity, or AI-first design.
