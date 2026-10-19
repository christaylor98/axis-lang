# axspec — Specification Language for Mechanical Governance

**Version 0.1 — SUPERSEDED by v0.2** (`AXSPEC_SPECIFICATION-v0.2.md`).
v0.2 folds in the `axis-workflow-demo-2` governance machinery: envelope
mechanism, layered governance (L1/L2/L3), blast radius + exposure oracle,
and governance-of-humans. Retained here as the version record.

---

## 1. Scope and Authority

axspec is a language for writing requirements, invariants, and NFRs such that a
**mechanical oracle** can determine their status. It is the normative core of the
governance layer for AI coding agents.

A *spec file* is a user-supplied file that:

* Defines one or more `requirement` constructs and the `rule`/`axiom`/`contract`
  constructs they depend on
* Is consumed by the axspec compiler to produce graph nodes and edges in axSemantica
* Is not produced by any tool itself

Spec files are authored **once** and then read by mechanical oracles forever.

### Relation to other layers

| Layer | Artifact | Role |
| --- | --- | --- |
| Intent spec (prose) | `INTENT_SYSTEM_SPEC.v1.0` `.is.md` files | Human-intent capture, rationale, rehydration anchors |
| **axspec (this doc)** | `.axspec` spec files | **Normative core: machine-determinable requirements** |
| Graph store | axSemantica nodes/edges | Where compiled rules live and are traversed |
| Oracle runner | deterministic binary | Total function from graph state to verdict |

**Authority:** axspec is normative. Intent-spec prose is descriptive. When prose and
axspec disagree, the axspec rule governs, and the intent-spec prose is re-opened for
revision — never the reverse.

### Determinism requirements

The oracle MUST produce an identical verdict given identical graph state and
identical turn input. Spec evaluation MUST NOT:

* Depend on LLM inference in the call path
* Depend on system locale, time, or randomness
* Depend on external services at evaluation time

---

## 2. Design Law: Decidability by Construction

**Every requirement is classified into exactly one tier at authoring time. The tier
determines what statuses the requirement may legally hold, and the tier is enforced
by the compiler.**

| Tier | Name | Determined by | Legal statuses | Authority |
| --- | --- | --- | --- | --- |
| T1 | Structural | Graph queries, static analysis, types, reachability | `satisfied`, `violated` | Sound — proof, not evidence |
| T2 | Behavioral | Execution: tests, contracts, regression suites, benchmarks | `satisfied` (on green evidence), `violated`, `undetermined` | Evidence-based, not proof |
| T3 | Analog | Nothing (aesthetic/qualitative) | `undetermined` **only** | Forced undetermined |

### Tier law

* A T1 requirement MAY never be `undetermined`: the oracle MUST decide.
* A T2 requirement defaults to `undetermined`; it becomes `satisfied` only while
  green evidence is current, and reverts to `undetermined` on stale, missing, or
  red evidence.
* A T3 requirement MAY never claim `satisfied` or `violated`. It is a
  *watchpoint*, surfaced as an open question, never a gate.
* The compiler MUST reject any requirement whose declared tier cannot support its
  determination method.
* **An uncheckable requirement is a specification smell.** If a requirement cannot
  be assigned T1 or T2, it is surfaced as an open-question node and the author is
  asked to (a) sharpen it into a checkable form, or (b) accept T3 explicitly.

---

## 3. Constructs

### 3.1 Grammar sketch

```
spec        := (spec (domain <id>) clause*)

clause      := axiom | rule | contract | property | nfr | requirement

axiom       := (axiom <id> <tier> (statement <expr>))

rule        := (rule <id> (tier <T1|T2>) (check <expr>)
                (rationale <prose>)?)

contract    := (contract <id> (tier <T1|T2>)
                (pre  <expr>) (post <expr>)
                (violation <status>)?)

property    := (property <id> (tier <T1|T2>) (holds <expr>))

nfr         := (nfr <id> (tier <T1|T2|T3>) (measure <expr>)
                (threshold <expr>)?)

requirement := (requirement <id> (tier <T1|T2|T3>) (assert <expr>)
                (blocking <bool>)? (rationale <prose>)?)
```

### 3.2 Semantics per construct

* **`domain`** — the namespace under which all clauses are scoped. Compiles to a
  graph namespace node.
* **`axiom`** — a statement assumed true without discharge. For the oracle it is a
  constant. Axioms MUST be T1 and MUST carry provenance.
* **`rule`** — the primary governance unit. `check` is a graph query or static
  predicate. T1 rules are the backbone of the gate.
* **`contract`** — a pre/post pair on a function or state transition. T1 when the
  pair is expressible statically; T2 when it requires execution.
* **`property`** — an invariant that must hold over all states (e.g. reachability,
  totality, uniqueness). T1 when decidable, else T2.
* **`nfr`** — a measurable attribute with a threshold. T1/T2 when the measure is
  mechanically computable (coverage %, latency bound via benchmark); T3 when
  qualitative.
* **`requirement`** — the top-level obligation. Declares tier, asserts an
  expression, and carries `blocking` (default: `false`).

### 3.3 The gate default

* A `requirement` with `blocking: true` MUST be `satisfied` for the gate to pass.
  Any `violated` blocks the gate.
* A `requirement` with `blocking: false` (default) annotates and warns. `violated`
  warns; `undetermined` warns `unproven`.
* **Blocking is reserved for hard limits** — safety, data integrity, architecture
  laws. Everything else warns by default.

---

## 4. Verdict Lattice

The oracle returns exactly one of:

| Verdict | Meaning | Gate effect |
| --- | --- | --- |
| `satisfied` | The requirement is discharged, current | Pass |
| `violated` | The requirement is not met | Block if `blocking`; else warn |
| `undetermined` | Neither provable nor disproven (T2 no-evidence, or T3) | Warn `unproven`; never blocks |

### Composition rule

The gate filters; only residual semantic ambiguity escalates. Mechanical verdicts
are applied first, in deterministic order. If and only if the mechanical layer is
`undetermined`, the turn MAY be escalated to the AI layer with the oracle's output
as context. A `violated` or `satisfied` verdict is never escalated.

---

## 5. Discharge Spine

Discharge is recorded as edges on the graph, not as text.

```
(satisfies REQ BY artifact STATUS <verdict> (when <timestamp>) (evidence <addr>)?)
```

* `REQ` is the requirement node; `artifact` is the code/intent node discharging it.
* `STATUS` is the mechanical verdict. It is recomputed on every evaluation — never
  trusted from a stored value.
* `evidence` is a content-addressed reference to the evidence (test run output,
  static-analysis report, query trace). Optional for T1 (`satisfied` by query is
  self-evident), required for T2 `satisfied`.
* A `satisfied` T2 edge carries a lifetime: when the referenced evidence goes stale
  (inputs hashed at evaluation time differ), the verdict MUST revert to
  `undetermined`.

### Before / after example

Intent-spec prose (never machine-checkable):

```
(hard-limit "All state mutations must pass through a single authoritative write path")
```

axspec compilation:

```
(rule SINGLE_WRITER
  (tier T1)
  (check (no-reachable-path ?x (module ledger)
          except (module write_path)))
  (rationale "Single authoritative write path; enforced at the graph, not by convention."))
```

The prose becomes `(rationale ...)`. The rule is a reachability query the oracle
runs as a total function.

---

## 6. Mechanical Oracle Contract

The oracle is a **total function**:

```
verdict : (graph-state, turn) -> verdict
```

Requirements on the oracle:

* **No LLM in the call path.** Determinism is not a preference; it is the property
  that buys explainability, unit-testability, and authority separation.
* **Explainable.** Every `violated` verdict MUST carry the failing query path or
  evidence reference. Every `undetermined` MUST carry the reason.
* **Unit-testable.** The oracle ships with a matrix of (state, turn) → expected
  verdict cases, runnable offline.
* **Ordered.** The evaluation order of rules is deterministic and documented.

### Oracle inventory (initial)

| Oracle | Determines | Tier |
| --- | --- | --- |
| Reachability | Can a path exist between X and Y (except via Z)? | T1 |
| Staleness/drift | Do the hashes behind current nodes match the filesystem/git? | T1 |
| Known-failed approach | Is the proposed approach a node marked `known_failed`? | T1 |
| Currency | Is every referenced node `current` per D011? | T1 |
| Contract check | Do pre/post hold for the transition? | T1/T2 |
| Benchmark gate | Does the measure meet the threshold on current evidence? | T2 |

---

## 7. Translation and Authority

Verification is mechanical; **specification is not**. The non-mechanical step is the
translation of intent into a checkable form.

* The AI layer proposes translations (intent → axspec clauses) and MAY propose
  refinements of `check` expressions.
* A human approves a translation **once**. After approval, the clause is frozen and
  the oracle determines it forever.
* Translation can capture the letter but not the intent. The human approval step is
  the guard against that gap; the requirement author is accountable for the
  translation, not the AI.
* If a requirement cannot be translated into a T1/T2 check, it MUST become either an
  open-question node (preferred) or an explicit T3 watchpoint. It MUST NOT silently
  remain uncheckable prose inside the normative layer.

---

## 8. Integrity Chain

Oracles are mechanical, but the graph they read is AI-written. Therefore:

* **Reconciliation is the foundation oracle.** At session start, the hashes behind
  current code/intent nodes are recomputed against the filesystem and git; mismatch
  marks affected nodes stale and drives T1 staleness verdicts.
* Every spec file and every compiled clause is **content-addressed** (sha256). The
  content address is the tamper-evidence: an edited spec or clause produces a new
  address and a detectable inconsistency, never a silent rewrite.
* The gate runs reconciliation first. If reconciliation is `violated`, no other
  verdict is trusted until it is resolved.

---

## 9. Graph Schema

Compilation of a spec file produces:

| Node | Type | Edges |
| --- | --- | --- |
| `spec:domain:<id>` | namespace | `contains` → all clauses |
| `spec:rule:<id>` | rule | `requires` → referenced requirements |
| `spec:requirement:<id>` | requirement | `asserts` → rule/property/nfr |
| `spec:open-question:<id>` | open question | `asks` → requirements it blocks |
| `satisfies` edge | discharge | `REQ --satisfies--> artifact` with `status` attribute |

Requirement/rule nodes carry a `tier` attribute and a content-addressable record
addressing the source `.axspec` file. Orphaned or unaddressed rule nodes are a
reconciliation violation.
