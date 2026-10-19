# axspec — Specification Language for Mechanical Governance

**Version 0.2**

**Reference implementation:** `axis-workflow-demo-2` — a running AML/CTF workflow engine that implements the governance primitives this spec defines. Where a mechanism is marked **proven in demo**, it is implemented and running there; the spec's job is to generalize the primitive into the language. Cross-references: `app/policy/gate.py`, `app/policy/meta_gate.py`, `app/policy/graph.py`, `app/policy/rules/envelope.py`, `app/policy/outcome_rules.py`, `app/policy/regulatory_rules.py`, `app/policy/penalty_schedules.py`, `app/ledger.py`.

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
| Reference impl | `axis-workflow-demo-2/app/policy/` | Proven implementation of the rule/gate/ledger primitives |

**Authority:** axspec is normative. Intent-spec prose is descriptive. When prose and
axspec disagree, the axspec rule governs, and the intent-spec prose is re-opened for
revision — never the reverse.

### Determinism requirements

The oracle MUST produce an identical verdict given identical graph state and
identical turn input. Spec evaluation MUST NOT:

* Depend on LLM inference in the call path
* Depend on system locale, time, or randomness
* Depend on external services at evaluation time

The demo's determinism is a *shipping* property, not a demo trick: `SEED=42`,
pure-function rules, and an append-only ledger make replay byte-identical across
container restarts (`app/seed.py`, `app/ledger.py`).

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

### Proven in demo — three-outcome lattice

The demo runs 17 rules with exactly the `pass`/`fail`/`unknown` lattice (this spec's
`satisfied`/`violated`/`undetermined`). Key proven properties:

* **Unknown is first-class, never a fallback.** The `typology-classification`
  envelope rule returns `unknown` when the model has no answer, and the gate routes
  to a human. There is *no* default-to-pass, *no* invented answer
  (`app/policy/rules/envelope.py`).
* **No fourth "not-applicable" outcome.** A rule that doesn't apply returns `pass`;
  applicability is built into the pass condition. Three outcomes only.
* **Risk scores never enter the decision path.** The gate evaluates rule *outcomes*,
  never scores. AI confidence levels are captured as ledger observations but do not
  influence routing. The demo enforces this at the data-model level: screening
  stages emit categorical classifications (`no-match`/`confirmed-match`/
  `partial-match`/`screening-unknown`), never numbers.

---

## 3. Constructs

### 3.1 Grammar sketch

```
spec        := (spec (domain <id>) clause*)

clause      := axiom | rule | contract | property | nfr | requirement | envelope

axiom       := (axiom <id> <tier> (statement <expr>))

rule        := (rule <id> (tier <T1|T2>) (check <expr>)
                (rationale <prose>)?)

contract    := (contract <id> (tier <T1|T2>)
                (pre  <expr>) (post <expr>)
                (violation <status>)?)

property    := (property <id> (tier <T1|T2>) (holds <expr>))

nfr         := (nfr <id> (tier <T1|T2|T3>) (measure <expr>)
                (threshold <expr>)?)

envelope    := (envelope <id> (schema <predicates>) (honesty <constraint>)
                (kernel <rule-id>))

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
* **`envelope`** — the boundary between AI judgment and mechanical determination.
  Declares a predicate `schema`, an `honesty` constraint, and the `kernel` rule that
  consumes the filled envelope. See §7.
* **`requirement`** — the top-level obligation. Declares tier, asserts an
  expression, and carries `blocking` (default: `false`).

### 3.3 The gate default

* A `requirement` with `blocking: true` MUST be `satisfied` for the gate to pass.
  Any `violated` blocks the gate.
* A `requirement` with `blocking: false` (default) annotates and warns. `violated`
  warns; `undetermined` warns `unproven`.
* **Blocking is reserved for hard limits** — safety, data integrity, architecture
  laws. Everything else warns by default.

### Proven in demo — the gate is a dumb conjunction

`app/policy/gate.py` is a pure conjunction: `any fail → senior-review`, `any unknown
→ human-review`, `all pass → auto-clear`. The intelligence lives in the rules, not
the gate. The meta-gate (`app/policy/meta_gate.py`) reuses the *same* conjunction
primitive with different inputs and a different scope — `any fail → block-change`,
`any unknown → requires-compliance-review`, `all pass → accept-change`. One primitive,
two scopes, one decomposable architecture.

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

### Proven in demo — escalation is a routing decision, not a mode

In the demo, `unknown` routes to a named human wait state (`aml-officer-review`,
`senior-officer-review`), and the *human's own decision* is then validated against
policy rules (`officer-dismissal-requires-reasoning`,
`senior-dismissal-requires-reasoning`). Escalation goes to an entity governed by the
same rule machinery — **governance of humans is mechanical too.** There is no
"here be judgement" hole in the path.

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
| **Blast radius** | What verdicts *would* change under a proposed parameter change? | T1/T2 |
| **Exposure estimate** | What is the quantified cost of a `violated` outcome? | T2 |

### Blast radius — proven in demo (`app/policy/graph.py`)

The demo implements a two-pass graph traversal blast radius: the same graph is
re-evaluated under (baseline) and (proposed) policy, then diffed. Two proven
optimisations worth carrying into the language:

* **`params_consumed` diffing.** Each rule declares which policy parameters it
  consumes. A rule is only re-evaluated if one of its consumed parameters changed.
  This is honest because rule outputs are deterministic in their declared
  parameters, and it collapses a full re-simulation to ~50 ms for single-parameter
  changes.
* **Deterministic replay under proposal.** The proposed change is simulated on the
  current batch *before* it is applied; nothing is applied optimistically.

### Exposure estimate — proven in demo (`app/policy/penalty_schedules.py`)

When a change is blocked, the demo reports `estimated_exposure_aud = penalty_per_
breach × affected_historical_count`, citing statutory maximums and settlement-tier
figures. The schedule is data with a legislative basis per rule — every number in
the decision path is defendable. axspec carries this as the `exposure` attribute on
a `blocking` requirement: the gate reports cost, not just `violated`.

---

## 7. Envelope Mechanism and Translation Authority

The demo's envelope mechanism is the concrete answer to "verification is mechanical;
specification is not."

* An `envelope` declares a **predicate schema** (what the AI may claim), an
  **honesty constraint** (when the AI must say `unknown` instead of inventing an
  answer), and a **kernel rule** that evaluates the filled envelope.
* The AI fills the envelope **under the honesty constraint**. The kernel evaluates
  the filled envelope **deterministically**. The AI is never in the decision path;
  its confidence scores never reach the gate.
* **The schema is the policy artifact.** The envelope schema is frozen policy — the
  approved translation. No per-use human approval is needed: the schema was approved
  once, and the kernel applies it mechanically forever.

### Proven in demo — two envelope rules

* `typology-classification` (`app/policy/rules/envelope.py`) — the headline
  architectural property. When the model returns `classification=unknown`, the rule
  returns `unknown` and the gate routes to a human. No fallback, no default-to-pass,
  no score-as-routing-input. The envelope's structured reasoning is shown verbatim
  to the operator.
* `adverse-media-classification` (in `app/policy/rules/screening.py`) — the same
  primitive applied to a second domain, demonstrating envelope as a general
  mechanism.

### Translation authority

* The AI layer proposes translations (intent → axspec clauses) and MAY propose
  refinements of `check` expressions.
* A human approves a translation **once**, at the schema level. After approval, the
  clause is frozen and the oracle determines it forever.
* Translation can capture the letter but not the intent. The human approval step is
  the guard against that gap; the requirement author is accountable for the
  translation, not the AI.
* If a requirement cannot be translated into a T1/T2 check, it MUST become either an
  open-question node (preferred) or an explicit T3 watchpoint. It MUST NOT silently
  remain uncheckable prose inside the normative layer.

---

## 8. Layered Governance

The demo runs the *same* rule engine at three governance layers. axspec v0.2
adopts this as a first-class structure:

| Layer | Input | Evaluates | Example rule |
| --- | --- | --- | --- |
| L1 — Operational | `(transaction, policy)` | Transactions against operational policy | `cumulative-structuring-pattern` |
| L2 — Observation | list of completed outcomes | Batch outcomes against regulatory obligations | `smr-filed-within-statutory-window` |
| L3 — Governance | `(proposed PolicyParams, context)` | Proposed policy changes against regulatory constraints | `cross-pipeline-policy-coherence` |

**Cascade law:** a proposed policy change MUST be evaluated by L3, then simulated
through affected L1 pipelines, then passed to L2, before it may be accepted or
blocked. The same conjunction primitive composes at every layer.

### Proven in demo

* `regulatory_rules.py` — Layer 3. Five parameter-facing rules plus two
  architecture rules: `cross-pipeline-policy-coherence` (pipeline-shared params must
  stay coherent) and `system-wide-mandatory-screening` (mandatory rules active in
  every pipeline). Note: two rules migrated *from* L3 *to* L2 because they evaluate
  completed outcomes, not parameters — **the layer a rule belongs in is determined
  by what it evaluates, not by regulatory seniority.**
* `outcome_rules.py` — Layer 2. Same `RuleEvaluation` model, input is a simulated
  batch.
* `meta_gate.py` — the L3 conjunction: `block-change` / `requires-compliance-review`
  / `accept-change`.

---

## 9. Integrity Chain

Oracles are mechanical, but the graph they read is AI-written. Therefore:

* **Reconciliation is the foundation oracle.** At session start, the hashes behind
  current code/intent nodes are recomputed against the filesystem and git; mismatch
  marks affected nodes stale and drives T1 staleness verdicts.
* Every spec file and every compiled clause is **content-addressed** (sha256). The
  content address is the tamper-evidence: an edited spec or clause produces a new
  address and a detectable inconsistency, never a silent rewrite.
* The gate runs reconciliation first. If reconciliation is `violated`, no other
  verdict is trusted until it is resolved.

### Proven in demo

The demo ships the ledger half of this chain (`app/ledger.py`): append-only, every
entry captures full input state, replay forward/backward is deterministic, policy
versions are derived from sorted-JSON hashes. The remaining half — graph nodes
content-addressed against the *working tree* — is the axSemantica responsibility
this spec assigns to the reconciliation oracle.

---

## 10. Graph Schema

Compilation of a spec file produces:

| Node | Type | Edges |
| --- | --- | --- |
| `spec:domain:<id>` | namespace | `contains` → all clauses |
| `spec:rule:<id>` | rule | `requires` → referenced requirements |
| `spec:envelope:<id>` | envelope | `consumes` → kernel rule; `filled_by` → AI layer (one-way) |
| `spec:requirement:<id>` | requirement | `asserts` → rule/property/nfr |
| `spec:open-question:<id>` | open question | `asks` → requirements it blocks |
| `satisfies` edge | discharge | `REQ --satisfies--> artifact` with `status` attribute |

Requirement/rule nodes carry a `tier` attribute, a `layer` attribute (L1/L2/L3,
default L1), an optional `exposure` schedule reference, and a content-addressable
record addressing the source `.axspec` file. Orphaned or unaddressed rule nodes are
a reconciliation violation.
