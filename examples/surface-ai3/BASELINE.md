# AI3 Surface — Baseline

Intent: `AI3_SURFACE_STEP1_RETURN_AND_SEPARATORS_V1`, amended by
`AI3_UNSHARE_BODY_V1`.
Recorded: 2026-09-08. **Re-recorded 2026-09-09 after the `Body` split** — every
number below was re-run, not carried over. Oracle: the `axis` binary. Nothing
here is asserted from prose.

**Renamed A3 → AI3 (2026-09-09).** Config, fixtures and this file were renamed;
no grammar or semantics changed. All 17 positive fixtures were re-run and the
four anchor ids reproduce byte for byte, confirming the rename touched only
comments and identifiers, never parsed content. The seven PARSE-rejection byte
offsets below shifted by exactly the number of bytes added to each fixture's
leading `// AI3 fixture: ...` comment (`A3` → `AI3`, +1 byte) and were
re-measured against the binary, not derived arithmetically.

**Re-checked 2026-09-29 at `c6103ce`.** Two findings below are superseded, and
each is marked in place rather than deleted:
[Finding 1](#finding-1--every-ai3-parse-rejection-produces-the-same-message)
(parse errors are now localised, as of `1e0e59a`) and
[Finding 2](#finding-2--the-two-registries-are-mutually-exclusive) (one canonical
registry set now covers every fixture). The per-file outcomes are unchanged:
17 compile, 7 are PARSE-rejected, 1 is VERIFY-rejected. The author-facing
primer is `docs/AI3_AUTHORING_PRIMER.md`.

AI3 is M1 plus two constructs:

1. **`return` is mandatory at the tail of a FUNCTION body.** Exactly one, at the
   tail. Parsed as a `ReturnStmt`, then eliminated during normalization. It never
   reaches NF admissibility or lowering.
2. **`;` and newline are statement separators**, so a body may hold several
   statements before its tail.

**`Body` is unshared, as of `AI3_UNSHARE_BODY_V1` (2026-09-09).** `FnDecl` uses
`Body: LetBinding* ReturnStmt`; `IfExpr`'s two branches use `BranchBody:
LetBinding* Expr`, which is M1's `Body` verbatim. So `return` is **required** at
a function tail and **forbidden** in an if-branch, where the tail is a bare
expression and stays positional exactly as in M1. The exclusion is structural,
not a check — `return` is not an `Expr` alternative, so no rule position admits
it there.

Why the reversal: the earlier decision to share `Body` reasoned that a branch
tail should stop being positional. Measurement contradicted it. In the
pre-language A/B the models complied with `return` at function tails 72/72 and
inside if-branches 8/55, and 24 of AI3's 53 failures were if-branch-missing-return
against 6 at function tails. See `axis-lang-expr-poc/results/poc/prelang/`.

There is **no return-type check** in this step. Checking `return`'s expression
against the declared return type needs a scope, which no config stage has.
Mandatory-but-unchecked is the intended step-1 state — the comprehension benefit
comes from the syntax, not from a check.

AI3 is **config only**. Zero Rust was written for it. `git diff --stat -- '*.rs'`
is empty and `git status -- src tests` is empty.

---

## How to run the baseline

**Preferred, as of 2026-09-29: one canonical set for every fixture.** The five
`axRegistry` files (at `94f13a6`) compile all 17 positive fixtures,
including both construction ones, and reject all 8 negatives at the same stage
as recorded below:

```
AXR=~/dev/axRegistry
./target/debug/axis --file examples/surface-ai3/<f>.ai3 $AI3C \
  --reg $AXR/axis-types.axreg --reg $AXR/axis.axreg --reg $AXR/axis-bridge.axreg \
  --reg $AXR/m1-builtins.axreg --reg $AXR/gen-working.axreg
```

The original two-set procedure is kept below because the recorded ids were
produced with it. Both sets still behave exactly as described (re-run
2026-09-29).

Two registry sets are needed, and they **cannot be combined** — see
[Finding 2](#finding-2--the-two-registries-are-mutually-exclusive).

```
AI3C="--lexer     axis-surface-ai3-config/ai3-lexer.yaml \
     --parser    axis-surface-ai3-config/ai3-parse.yaml \
     --schema    axis-surface-ai3-config/ai3-ast.yaml \
     --normalize axis-surface-ai3-config/ai3-normalize.yaml"

# Set A — the golden registry. Every fixture except the two construction ones.
./target/debug/axis --file examples/surface-ai3/<f>.ai3 $AI3C --reg registry-pin/axis.axreg

# Set B — needed only by ai3_tail_value_construct and ai3_tail_valuelist_construct,
# which require `variadic true` on value_make / list_make.
./target/debug/axis --file examples/surface-ai3/<f>.ai3 $AI3C --reg axis-surface-m1-config/m1-registry.axreg
```

---

## Per-file baseline — stage and reason

Read this as a record of **where each file stops and why**, not as a score.
"Set" names the registry set above.

### Compiles and verifies (17 files)

| Fixture | Set | Stage reached | Reason / what it pins |
|---|---|---|---|
| `ai3_tail_literal.ai3` | A | verified, bundle written | Literal tail. Anchor **t1_literal** `2919b84a…` |
| `ai3_tail_call.ai3` | A | verified, bundle written | Call tail. Anchor **t2_call** `73f67614…` |
| `ai3_tail_var.ai3` | A | verified, bundle written | Bare-variable tail. Anchor **t3_bare_var** `9efc4a3d…` |
| `ai3_tail_if.ai3` | A | verified, bundle written | `if` tail with `return` in the outer function body and a **bare expression** at each branch tail. Anchor **t4_if** `13bb081f…` |
| `ai3_tail_no_semicolon.ai3` | A | verified, bundle written | Same source as `ai3_tail_call` with every `;` removed. Proves `;` is optional → `73f67614…` |
| `ai3_sep_newline.ai3` | A | verified, bundle written | Multi-let body, newline separators, not one `;` in the file → `1504cca4…` |
| `ai3_sep_semicolon.ai3` | A | verified, bundle written | Same body, explicit `;`, all on one line → `1504cca4…` |
| `ai3_sep_mixed.ai3` | A | verified, bundle written | Same body, `;` and newline mixed → `1504cca4…` |
| `ai3_if_branch_lets.ai3` | A | verified, bundle written | `let` bindings inside both `if` branches, each branch closed by a **bare tail expression** → `15d1603d…` |
| `ai3_multi_fn.ai3` | A | verified, bundle written | Two top-level fns, the second calling the first → `f7a8b9ea…` |
| `ai3_tail_text.ai3` | A | verified, bundle written | `Text` literal tail → `f524c991…` |
| `ai3_tail_bool.ai3` | A | verified, bundle written | `Bool` literal tail → `220b9ed2…` |
| `ai3_tail_unit.ai3` | A | verified, bundle written | `Unit` literal tail → `ce56b030…` |
| `ai3_tail_determine.ai3` | A | verified, bundle written | `determine` gate in tail position → `3fda0e62…` |
| `ai3_letdecl.ai3` | A | verified, bundle written | Top-level `LetDecl` with the optional `;`. No `Body`, so no `return` → `2919b84a…` |
| `ai3_tail_value_construct.ai3` | B | verified, bundle written | `Value(Bool, Bool)(a, b)` in tail position → `e661392c…` |
| `ai3_tail_valuelist_construct.ai3` | B | verified, bundle written | `ValueList(Bool)(a)` in tail position → `46b672df…` |

### Correctly rejected at PARSE (7 files)

All seven stop in `parse_with_spec` (`parser_runtime.rs:66-72`). The verbatim
message and span are recorded because the message quality is itself a finding —
see [Finding 1](#finding-1--every-ai3-parse-rejection-produces-the-same-message).

**The message column below is historical (2026-09-09).** Since `1e0e59a` each
rejection names the offending token at the furthest position reached. The
current output, re-run 2026-09-29 and identical under both registry
procedures:

| Fixture | Current message (all end `, parsed up to here then could not continue`) |
|---|---|
| `ai3_neg_missing_return.ai3` | ``parse error at 180..183: unexpected token `Int` `` |
| `ai3_neg_return_in_branch.ai3` | ``parse error at 553..559: unexpected token `return` `` |
| `ai3_neg_return_not_at_tail.ai3` | ``parse error at 205..208: unexpected token `let` `` |
| `ai3_neg_two_returns.ai3` | ``parse error at 103..109: unexpected token `return` `` |
| `ai3_neg_empty_body.ai3` | ``parse error at 79..80: unexpected token `}` `` |
| `ai3_neg_return_as_ident.ai3` | ``parse error at 99..105: unexpected token `return` `` |
| `ai3_neg_bare_return.ai3` | ``parse error at 172..173: unexpected token `;` `` |

| Fixture | Stage | Verbatim message | What it pins |
|---|---|---|---|
| `ai3_neg_missing_return.ai3` | parse | `parse error at 162..164: unexpected token after end of parse` | A bare tail expression is no longer a **function** body. This is the whole point of the surface. |
| `ai3_neg_return_in_branch.ai3` | parse | `parse error at 514..516: unexpected token after end of parse` | **Inverted by `AI3_UNSHARE_BODY_V1`.** Was `ai3_neg_missing_return_in_branch.ai3`, which asserted that a branch tail *required* its own `return` under the shared `Body`. `Body` is now unshared, so `return` in a branch is the rejection. |
| `ai3_neg_return_not_at_tail.ai3` | parse | `parse error at 169..171: unexpected token after end of parse` | Nothing may follow the `return`. "At the tail" is structural, not a check. |
| `ai3_neg_two_returns.ai3` | parse | `parse error at 67..69: unexpected token after end of parse` | "Exactly one" is structural too — `Body: LetBinding* ReturnStmt` admits one. |
| `ai3_neg_empty_body.ai3` | parse | `parse error at 63..65: unexpected token after end of parse` | A `Body` must contain a `ReturnStmt`. |
| `ai3_neg_return_as_ident.ai3` | parse | `parse error at 94..96: unexpected token after end of parse` | `return` is now a reserved keyword and cannot name a parameter or variable. |
| `ai3_neg_bare_return.ai3` | parse | `parse error at 148..150: unexpected token after end of parse` | `ReturnStmt` requires an `Expr`. There is no unit-returning bare `return;`. Write `return Unit();`. |

### Correctly rejected at VERIFICATION (1 file)

| Fixture | Set | Stage | Verbatim message | What it pins |
|---|---|---|---|---|
| `ai3_neg_type_mismatch.ai3` | A | verification | `TypeMismatch { node_index: 0, arg_index: 1, expected: Hash256(16305a6b…), got: Hash256(85f90dfe…) }` | The verifier gate still bites through the new syntax. Eliminating `return` at normalization weakens nothing downstream. |

---

## Byte identity — proven, not assumed

Every one of the 17 positive fixtures was compiled alongside a hand-written M1
equivalent (same program, no `return`, no `;`) under the same registry set. Both
`axis bundle-id` and `cmp` on the raw `.coreir` bytes were checked.

**17 of 17: ID MATCH and `cmp` IDENTICAL.** Zero differences.

**Re-verified 2026-09-09, after the `Body` split: 17 of 17 again, and every
recorded id below reproduces byte for byte.** The split changed which rule an
`if` branch parses under; it changed no bundle. `ai3_tail_if.ai3` and
`ai3_if_branch_lets.ai3` were rewritten to bare branch tails and still produce
`13bb081f…` and `15d1603d…`, the same ids they produced when their branches
carried `return`. That is the split's central claim, and it is the binary's
answer, not an argument: `BranchBody`'s AST projection is M1's `Body` projection
verbatim, and `fold_body_to_let_chain` reads `bindings`/`result` by name without
looking at the node kind (`normalize_engine.rs:286-350`), so both spellings fold
to the same NF.

The four anchors recorded in the intent reproduce exactly:

| Anchor | Recorded id | AI3 id | `cmp` |
|---|---|---|---|
| t1_literal | `2919b84a172baeb9dc27b05766bfec1112de3c356fc29ae1b7e6e8db23acc865` | same | identical |
| t2_call | `73f6761444683e4161dbfca837ff87c7c56b3b2f4d43dc12d65a6be7d1c40aff` | same | identical |
| t3_bare_var | `9efc4a3d43fa54134c3130dba6d7a9dadae69fca34e32cdeeeadc986e67e615f` | same | identical |
| t4_if | `13bb081ffeea58c848736b44bdde07f258eff469943542615cce7f8f905ba247` | same | identical |

Thirteen further tail and separator forms match their M1 equivalents on top of
those four, including the three-way separator family (`ai3_sep_newline`,
`ai3_sep_semicolon`, `ai3_sep_mixed`) which all collapse to `1504cca4…` together
with the M1 original.

---

## The `return` node does not reach NF — falsified two ways

Byte identity is strong indirect evidence. Two direct falsification tests were
run against scratchpad copies of the config (the repo files were not modified):

**(A) Remove `strip_return_stmt` from `rule_order`.** `rule_order` is an explicit
ordered list and a rule not named there does not run. Result:

```
normalization error at 0..0: Rewrite Error on ReturnStmt:
No normalization rule matches node kind 'ReturnStmt'. Available rules: [...]
```

So `strip_return_stmt` is exactly what erases the node, and without it the node
is caught by the fail-loud default at normalization — it cannot silently reach
lowering.

**(B) Rename `ReturnStmt`'s AST field from `expr` to `retval`.** The same hard
error results. `unwrap_transparent_node` (`normalize_engine.rs:216-235`) hunts a
fixed field-name list — `variant, content, expr, value, inner, body, left,
target` — so the field name `expr` is load-bearing and is commented as such in
`ai3-ast.yaml`.

**No new NF node kind is minted.** Diffing the `nf_node:` values of
`ai3-normalize.yaml` against `m1-normalize.yaml` yields an identical set. `;` is
punctuation and is never projected into a field, so it cannot reach NF at all.

---

## Findings

### Finding 1 — every AI3 parse rejection produces the same message

> **SUPERSEDED 2026-09-29.** Parse localisation (`1e0e59a`) fixed this. The
> error now reports the furthest position any alternative reached and names the
> token there, so the seven negatives are now distinguishable: a missing
> `return` names the bare tail expression's first token, `return` in a branch
> names `return`, and so on (current messages in the PARSE table above). A
> dedicated "missing `return`" message still does not exist, but the token alone
> identifies the rule. The text below is the original record.

All seven parse-stage negatives report the identical text, differing only in the
span:

```
parse error at <n>..<n+2>: unexpected token after end of parse
```

In every case the span is the `fn` keyword of the failing declaration, verified
by byte offset. So the message names *which declaration* failed but not *what*
was wrong with it — a missing `return`, two returns, an empty body and `return`
used as an identifier are indistinguishable to the author.

The cause is structural and pre-existing, not specific to AI3: `Program: Item*`
matches zero items, then `parse_with_spec` fails the `at_eof` check
(`parser_runtime.rs:66-72`). M1 behaves identically — its six lambda rejections
all report the same string. There is a single generic message site,
`parser_runtime.rs:69`, with no rule context.

**A targeted "missing `return` at end of body" diagnostic would be a Rust change
and is out of scope for this intent. It was not written.** It is the single
highest-value follow-up: `return` being mandatory is only as useful as the
message the author gets when they omit it.

### Finding 2 — the two registries are mutually exclusive

> **SUPERSEDED 2026-09-29 as a practical constraint.** The two files are still
> mutually exclusive, but a corpus no longer has to choose between them: the
> five-file `axRegistry` set (see [How to run](#how-to-run-the-baseline))
> has the `int_*` arithmetic *and* variadic `value_make` / `list_make`, and
> compiles every AI3 fixture in one invocation. The text below is the original
> record and is still accurate about the two files it names.

`registry-pin/axis.axreg` and `axis-surface-m1-config/m1-registry.axreg` cannot
be passed together:

```
Error: Registry conflict: duplicate type name 'Bool' — declared in
axis-surface-m1-config/m1-registry.axreg and registry-pin/axis.axreg
```

Duplicate **function** names merge last-wins, but duplicate **type** names are a
hard error. The two files each declare `Bool`, so the corpus must pick one:

- `registry-pin/axis.axreg` has `int_add`, `int_sub`, `int_mul`, `int_lt`,
  `bool_*` — needed by 15 fixtures and by all four anchors — but declares
  `value_make` / `list_make` as `in ()` with no `variadic true`, so any Value or
  ValueList construction fails verification with
  `ArityMismatch { node_index: 0, expected: 0, got: N }`.
- `axis-surface-m1-config/m1-registry.axreg` declares both as `variadic true`,
  so construction verifies, but it has no `int_*` at all.

**This is not an AI3 property.** The M1 fixtures `matrix_value_construct.m1` and
`matrix_valuelist_construct.m1` fail with the byte-identical `ArityMismatch`
under `registry-pin/axis.axreg`, and AI3's two construction fixtures produce
bundle ids identical to those M1 fixtures under `m1-registry.axreg`. The split
is a property of the registry files. No `.axreg` was touched.

### Finding 3 — newline cannot be made a significant token from config

`lexer_engine.rs:169-178` consumes any whitespace match unconditionally and never
emits a token; the `skip:` field on the whitespace rule is read into the spec
(`lexspec.rs:45`) but never consulted. So a NEWLINE token is unreachable without
Rust — **and it was not written.**

This does not block the construct. Newline separates statements the same way it
already does in M1: statements juxtapose, the intervening whitespace is skipped,
and `;` is an *optional* explicit separator layered on top. `ai3_sep_newline.ai3`
(no semicolons at all), `ai3_sep_semicolon.ai3` (all semicolons, one line) and
`ai3_sep_mixed.ai3` compile to the same bundle. Making `;` mandatory instead would
have forbidden the newline form outright, which is why it is optional.

A genuinely significant newline would require the grammar to tolerate a NEWLINE
token at every position, which this parser cannot express. Recorded as a limit,
not as pending work.

---

## Design decisions worth revisiting

**~~`Body` is shared between `FnDecl` and both `IfExpr` branches.~~ SETTLED
2026-09-09 — split, by `AI3_UNSHARE_BODY_V1`.** This entry predicted the change
would be "a one-line grammar change" naming `ai3_tail_if.ai3` and
`ai3_if_branch_lets.ai3` as the fixtures needing rewriting. Both halves were
close but not right, and the record is left standing rather than tidied:

- It took **three** config edits, not one: a new `BranchBody` rule in
  `ai3-parse.yaml`, its projection in `ai3-ast.yaml`, and `desugar_branch_body`
  plus a `rule_order` entry in `ai3-normalize.yaml`. `rule_order` is explicit and
  a node with no matching rule is a hard error, so the normalize entry is not
  optional. Still zero Rust.
- A **third** fixture had to change: `ai3_neg_missing_return_in_branch.ai3`
  asserted the rule that was removed and became `ai3_neg_return_in_branch.ai3`.

**`;` is optional, not mandatory.** See Finding 3.

**A surface's primer is part of the surface.** Not a config finding, but the
one that cost the most: the A/B primer described the shared-`Body` rule, and
recompiling the 144 recorded completions on the split grammar dropped the AI3 arm
from 26.4% to 11.1% purely because 50 of 72 completions wrote the `return` the
primer had asked for. A grammar change that is not mirrored in the primer is not
one variable — it is two.

---

## M1 untouched

| File | SHA-256 at session start | after |
|---|---|---|
| `m1-ast.yaml` | `36c26336176292e51512464bba79160891a3879ab14d34bca296bc9397508a2c` | unchanged |
| `m1-lexer.yaml` | `3237fac3e7f8abe88f5cd0240b5ba8983a66a40294be914d33c9e0dcb8d04e6e` | unchanged |
| `m1-normalize.yaml` | `571fa41577aa2ddc695fbea3044d9d798649b24b57f2b9d7839afac53fdbdb42` | unchanged |
| `m1-parse.yaml` | `2008ca7e5618741eb62a42576764be93211b59d224267c2be1d8bef47dbad4e2` | unchanged |

The M1 end-to-end baseline was captured before any AI3 file existed and re-run
after. `diff` of the two runs is empty: **22 PASS, unchanged**, matching CLAUDE.md.
`git status -- examples/surface-m1 axis-surface-m1-config` is empty.
`cargo test` is green, 0 failed.

**Re-checked 2026-09-09, after the `Body` split.** All four SHA-256s above are
unchanged from the values in the first column. `git diff --stat -- '*.rs'` is
empty, `git status -- src tests axis-surface-m1-config` is empty, and
`cargo test` is green with 0 failed. The split touched
`axis-surface-ai3-config/` and `examples/surface-ai3/` and nothing else.
