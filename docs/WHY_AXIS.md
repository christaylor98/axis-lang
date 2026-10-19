# Why Axis — the running list

Reasons to move to Axis, each grounded in a measured problem elsewhere. One entry per reason.
Every entry has the same fields, so nothing reads as a slogan:

- **Problem elsewhere**: what goes wrong in mainstream languages, with the evidence.
- **Why Axis removes it**: which property of AI3 / CoreIR does it.
- **What's left**: residual gaps and their defined treatments. An unknown is never silently a pass.
- **Status**: *argued* (by construction) → *demonstrated* (measured on real Axis code).

---

## 1. Safe, exact test selection: run only the blast radius of a change

*Added 2026-09-30 (axis-chat session: grounding the chronicle's blast-radius idea).*

**Problem elsewhere.** "Only run the tests a change can affect" is regression test selection (RTS), studied since at least 1993. In mainstream languages the static dependency graph can't be trusted as the selector:

- **Java, function-level static selection** was unsafe on 10.6% of revisions and *slower than running every test*. The authors call it "effectively useless" (Legunsen et al., FSE 2016, 985 revisions of 22 projects, doi:10.1145/2950290.2950361). The causes:
  - reflection;
  - library code excluded from the analysis;
  - imprecise guessing of dynamic dispatch targets;
  - rebuilding the call graph on every revision (e.g. 527% overhead on commons-math).
- **Python** is worse (Wang, Pradel & Liu, arXiv 2605.25356, 2026 preprint):
  - static call graphs found only 49% of the call edges observed at run time;
  - eager imports mean one test file imports 83% of source files;
  - the static file-level tool BabelRTS was safe on only 76.6% of commits (14% on matplotlib).
- **Industry falls back to probability.** Google cuts at a dependency distance of 10, a correlation (Memon et al., ICSE-SEIP 2017). Facebook uses a trained model that catches over 95% of individual test failures, openly approximate (Machalica et al., ICSE-SEIP 2019).

**Why Axis removes it.** Every cause above is absent by construction:

- no reflection;
- library code is hashed definitions in the same graph;
- calls are explicit, so there are no dispatch targets to guess;
- the graph is maintained as code changes, never rebuilt.

A test's blast radius is the transitive set of body hashes of the definitions it references, resolved through the registry, since Axis fn identities are name-derived (see the edge check below). A test whose radius is unchanged cannot behave differently, so its cached verdict stands. This is exact, not probabilistic. Unison, a content-addressed language with typed effects, ships the same design: pure tests are cached by hash, and IO tests always rerun (`HandleInput/Tests.hs`: `-- Don't cache IO tests`).

**What's left, each with a treatment:**

| Gap | Treatment |
|---|---|
| Effects / FFI / foreign modules | A pure handler replaces the effect, and the handler's hash joins the cache key. A contract test (handler vs real effect) always runs. An effect with no model gets a real run. |
| A model disagrees with reality | Failure of the model: every verdict under its hash is invalidated and the model is fixed. |
| A bug in the graph builder (a missing edge) | Run the suite once with execution tracing. Every observed dependency must be a graph edge; a missing edge is a failure. |

**Status: argued; edge check passed on the paths that ran (see below).** To demonstrate it:

1. Run the edge check on the AI3 test suite.
2. Measure the fraction of tests whose radius survives a typical commit (the saving).
3. Measure the fraction of tests that touch effects (the part that always reruns).

**Edge check, run 2026-09-30 on axMachina** (all 64 built AI3 programs). Each program ran sandboxed (bwrap, no network, writes to a throwaway overlay). gdb recorded every `ax_fn_<identity>` that executed. Each program's static radius is the walk of its CoreIR: CCall targets, Fn-ref pool constants, and the entry's own `sha256(stem)`, through the registry to `build/driver/bodies/<name>.coreir`.

- **No missing edges.** Across the 64 programs, 574 composites executed, and every one was in its program's static radius.
  - A first run flagged 3 programs, but the fault was in my tool. It resolved edges only through the current registries, and those three 09-11 probes call fns no registry lists any more. The calls are right there in the CoreIR.
  - Coverage is 574 of 1175 static composites (49%). Most store-backed programs stopped early because the sandbox blocks their TCP connection to the store. The check therefore holds on the paths that ran, which is not every path.
- **Every program is effectful.** All 64 reach a `fullIo` builtin: fs, proc_run, tcp, or io_print. Some also inherit effects through callback-taking list ops, because a HOF's effect is unioned from its callback. Under Unison's rule (cache pure tests, always rerun IO tests), axMachina would cache nothing. The saving depends on the effect-handler route in the table above, which isn't built yet.
- **An identity alone is not a valid cache key.** Fn identity is `sha256(name)`, so editing a body keeps its identity. That is how most of these binaries (older than their current bodies) still matched by identity. A radius key must therefore be the transitive set of registry `body` hashes, not identities. This is still exact, but it differs from Unison, where a definition's own hash covers its dependencies.

Not yet measured: the fraction of tests whose radius survives a typical commit.

Tools used for this check (scratch, not committed): `static_radius.py`, `trace.py` (gdb), `compare.py`.

Comparison point: axis-chat (Python) has to use a measured line map (`mutagen --narrow`: 2.3x less compute) that must be re-recorded after every change. Axis needs neither the tracing nor the re-recording.

---

## 2. Effects, error paths and branch arms can be listed, so coverage gaps have names

*Added 2026-09-30 (axis-chat experiment `experiments/2026-09-30-effect-split/`, tui.py).*

**Problem elsewhere.** A passing suite only covers the paths its inputs reach. The error paths it never triggers are invisible. In a language without effect tracking, the effects are hidden inside ordinary calls, so the effects are neither listed nor controllable. Measured on axis-chat's `tui.py` (Python 3.14; 183 tests incl. semantic cases; effects via `sys.addaudithook` plus patched clock/random/uuid/env/pid; branch arms via `sys.monitoring` BRANCH_LEFT/RIGHT and `code.co_branches()`):

- **Line coverage hides the error gap.** The suite runs 88% of tui.py's statement lines, yet **0 of its 26 `except` handlers** are ever entered. Every error path is untested.
- **Most branch arms go untaken.** On the 75 non-case tests, 340 of 1654 arms are taken (21%), and 112 of 183 branching functions have no arm taken at all. This figure excludes the cases, whose child processes the arm recorder didn't follow.
- **Effects are pervasive and not deterministic.** 42 of the 75 tests (56%) perform effects in their own code: open, flock, mkdir, env reads, clock. Across two identical runs, 41 of the 75 tests produced different effect streams, mostly from `time.monotonic`, but also from uuid/urandom and file ops ordered by time.

**Why Axis removes it.**

- **Every effect is a typed `CCall`.** The registry declares `effect` and `deterministic` for each builtin, so a program's effect sites can be listed from its CoreIR without running it.
- **Effects split cleanly.** The effect nodes' results are just inputs to the rest of the graph, so "deterministic core + recorded effects" is the IR's shape, not a transformation Python needs.
- **Every branch arm is known up front.** `CIf` is the only branch node, so the arms to cover can be listed statically.

Together these make the gaps nameable. Each effect site has a finite outcome space (ok, missing, denied, timeout, non-zero exit) that can be injected one outcome at a time. Each `CIf` arm no test takes is a named gap, not a silent pass.

**What's left, each with a treatment:**

| Gap | Treatment |
|---|---|
| An arm no test takes | ⊥ with a treatment: generate an input that reaches it, or inject the effect outcome that does. Mutation-test the arm to prove a test would notice it changing. |
| **AI3 builtins abort instead of returning errors.** E.g. `fs_read_text` panics on a missing file (axMachina CLAUDE.md), so an error outcome is a process abort, not a value code can handle. | The error paths aren't in the language yet. Treatment: builtins that return a result sum type (ok or error), so error arms become `CIf` arms and join the enumeration above. **This is a gap in Axis today, not a strength.** |
| A recorded effect log goes stale | A contract run re-executes the real effects and compares. A mismatch invalidates every verdict under that log. |

**Status: argued for Axis; the problem is measured in Python.** To demonstrate it, list the effect sites and `CIf` arms of an axMachina program from its CoreIR and inject outcomes at those sites.

**The combination beats any single technique**, because each closes a different unknown:

| Technique | The unknown it closes |
|---|---|
| Blast radius (graph or line map) | Which tests a change can affect |
| Effect split + record/replay | Non-deterministic verdicts; lets verdicts be cached |
| Branch-arm / handler coverage | Which paths the inputs never reach |
| Effect-outcome injection | Error conditions the world didn't happen to produce |
| Mutation testing | Whether a test would notice a change on a covered path |
| Contract runs | Whether a model or log still matches reality |

An unknown found by one technique feeds the next. An untaken arm gets an input or an injected outcome, and mutation then checks that a test notices it.

---

## 3. Honest outcomes: Ok | Err | Unknown, and Unknown can't be shaken off

*Added 2026-09-30. The shape is chosen; the surface and mechanism are not yet tested.*

**The shape.** Every outcome is one of three:

- **Ok**: an explicit success condition holds.
- **Err**: an explicit, declared failure condition holds.
- **Unknown**: everything else.

Never "not Ok means Err". Two things follow:

- **Unknown is sticky taint.** Code may ignore it and carry on, but every value derived from it stays Unknown. It is discharged only by a recorded treatment (retry, escalate, degrade-and-record, deadend), and it must be discharged before an irreversible effect or at a boundary.
- **No panic-as-fail, no unwrap, no silent default.**

Whether a failure is Err or Unknown is decided by the contract, not by the event. An obvious failure stays Unknown until something declares it.

**Problem elsewhere, and in Axis today (measured 2026-09-30):**

- Mainstream arithmetic closes the world by fiat. IEEE defines `inf + 1 = inf` and `0/0 = NaN`. Integer overflow wraps.
- AI3 today behaves the same way. `int_add(Int(9223372036854775807), Int(1))` prints `-9223372036854775808` and exits 0: a silent wrong value.
- `int_div` panics on zero, although the registry declares it `effect pure, deterministic true`, so the contract hides the failure.
- `fs_read_text` panics on any IO error (`runtime/io.rs:55`).
- `value_0/1/2` return `Unit` silently when the index is out of range.
- Exceptions let `catch (Exception) { return default; }` pass unknowns off as known results. axis-chat's tui.py has 4 such catch-alls; injection showed they produce quietly wrong state (a silently empty jobs panel).
- **Result as a type lets models swallow.** In the AI3_ATTEMPT_AB_V1 A/B (axis-lang-expr-poc `results/poc/attempt/report.md`), the accessor encoding turned the unknown outcome into a value in 10 of 60 samples. The `attempt` syntax did so in 0 of 60, because the payload exists only in `ok(x)`. Pass rates tied (48-52%), so the surface is not settled.

**Why Axis can do it.**

- The IR is a dataflow graph (CCall args are node refs), so taint is plain reachability.
- The verifier already enforces a domination rule of this exact shape: an irreversible CCall must be dominated by `CDeterminate` on every path. The Unknown rule is the same kind of rule.
- Effects are already unioned into derived signatures, so "may be Unknown" can be derived the same way, with nothing written at ordinary call sites.
- Bridge edges are the only place foreign errors enter, so vendor errors are classified there. A mapping table sends `io::ErrorKind::NotFound` to `Err NotFound`; anything unmapped becomes `Unknown{evidence}`.

**What's left, each with a treatment:**

| Gap | Treatment |
|---|---|
| Which builtins are partial today | A census of the bridge runtime against the registry, classifying each as total / panics / silent default / silent wrong |
| How much real code the taint rule touches | A static count on axMachina's CoreIR: tainted values, and how many reach an irreversible call or a result untreated |
| The surface | A second A/B round (attempt on a bound outcome, brace-less arms, a surface list filtered to real builtins, N=12, "zero swallowed" in the decision rule) |
| Unknown must propagate as a value, not an abort | A runtime representation for Unknown with evidence, plus Kleene rules so no comparison, `if`, fold or filter drops it |

**Panic becomes an explicit Unknown, tested 2026-09-30** (axis-codegen-bridge-rs branch `fault-as-unknown-v1`,
prototype, not merged). Every builtin call is wrapped in a guard, with three rules:

- a panic becomes `Unknown{bridge_fault, builtin, message, location}` and a line in `$AX_FAULT_LOG`, the bridge's
  work list. A panic is the bridge admitting it has no classification for the case;
- an Unknown argument is sticky: the builtin isn't called, and the first cause is kept;
- an `if` on an Unknown yields it and chooses no branch.

An Unknown result exits 3, not 101.

- `list_head([])`, `int_div(7, 0)`, `fs_read_text` on EACCES, a chain (`str_concat(fs_read_text(denied), int_div(1, 0))`)
  and `if text_eq(fs_read_text(denied), …)`: all were 101 panics. All now exit 3 naming the first cause. A normal
  program is unchanged.
- **The test caught a swallow.** The block-form `if` emitter was missed at first. It took the then-branch on an
  Unknown and printed `then`, exit 0. Both emitters had to be guarded: "never swallow" has to be checked at every
  path, not assumed.
- **Cost:** ~8 ns/call on `int_add`, an upper bound (the loop also clones). ~1 ns (2%) on `str_concat`. Only
  builtins that can panic need the `catch_unwind`, and the census names them. The sticky check is a tag compare.
- **Not yet handled:**
  - silent defaults and silent wrong values. They never panic, so they must first be made to fail (`checked_add`,
    no `unwrap_or(Unit)`);
  - an Unknown returned by a callback inside a HOF (it could be stored in a list);
  - location is the generated-Rust line, not the CoreIR node;
  - effect-may-have-partly-happened evidence.

**Surface chosen by experiment, 2026-09-30** (axis-lang-expr-poc `results/poc/attempt/report2.md`). The decision
was sequential and declared in advance, and the north star was "what AI plus mechanical checking gets right".

The winner is **accessors plus a verifier rule, with `else if`.** The rule works on CoreIR:
- R1: the payload is read only where the tag is proven `ok`;
- R2: a branch where the tag could still be `unknown` must `fail`, so "not Ok" is never treated as `Err`;
- R3: no outcome is ignored.

Results:

| model | result |
|---|---|
| Sonnet | both forms perfect (45/45) |
| gemma-3-27b | accessors + rule 5/6 vs `attempt` 2/6 |
| local gemma-4-12b | accessors + rule + `else if` 11/12 vs `attempt` 5/12 (paired, stopped early) |

- No swallow in any run.
- The rule caught the lazy "not Ok = Err" form 6 times. Feedback fixed it every time `else if` was available.
- `else if` (AI3_ELSE_IF_V1: config only, byte-identical on all 25 fixtures) took the small model from 5/12 to 11/12.

Caveat: small n, and the early stop may overstate the lead.

**Status: direction chosen; panic→Unknown prototype passes its test; surface chosen (accessors + verifier rule +
`else if`).** Record each further mechanism only after its
test passes.
