# M1 Authoring Primer — writing M1 and using the registry

**Scope:** how to write M1 source that compiles *and verifies*, and how to read,
extend, and reason about `.axreg` registries. Nothing about pipeline internals,
lowering, or the IR beyond what an author must know.

**Status:** written against the tree at commit `e71e5b5` (2026-08-23). Every code
sample and every error message below was produced by running the `axis` binary on
the file shown, against the M1 baseline registry set.

**Read this instead of `axis-surface-m1-spec/AXIS_SURFACE_M1_SPEC-0.1.md` where the
two disagree.** That spec is normative for intent but has drifted in three places:
it still documents lambda syntax (removed from M1), it still shows a `--lowering`
flag (does not exist), and its built-in function table lists callables that are not
in the M1 registry set. `CLAUDE.md` is authoritative on the baseline.

---

## 1. The compile command

```bash
axis --file <source.m1> \
     --lexer     axis-surface-m1-config/m1-lexer.yaml \
     --parser    axis-surface-m1-config/m1-parse.yaml \
     --schema    axis-surface-m1-config/m1-ast.yaml \
     --normalize axis-surface-m1-config/m1-normalize.yaml \
     --reg       axis-surface-m1-config/m1-registry.axreg \
     --reg       registries/pilot_lowering.axreg \
     --reg       registries/fnref_test.axreg
```

Four spec flags, one or more `--file`, one or more `--reg`. There is no
`--lowering` flag — lowering is fixed Rust below the NF boundary.

Success is exactly one `→` line per source file:

```
/path/to/two_fns.m1 → coreir/two_fns.coreir (identity: 9d50fe3f17f2…)
```

Anything else is a failure. Useful extras:

| Flag | Effect |
|---|---|
| `--out <dir\|file>` | Where the bundle lands (default `coreir/<sourcename>.coreir`) |
| `--no-verify` | Skip the verifier gate. Compiles ill-typed bundles. Do not use to "fix" a diagnostic |
| `--inspect <stage>` | **Dead for the 0.5 pipeline** — it loads a stale registry and fails. Use `--out` + `capnp decode` instead |

To inspect an emitted bundle:

```bash
capnp decode core_ir_spec/axis_core_ir_0_5.capnp CoreBundle < coreir/foo.coreir
axis bundle-id coreir/foo.coreir
```

---

## 2. Reading a failure

Failures are stage-labelled. The label tells you which chapter of this primer to
read.

| Message prefix | Stage | Meaning |
|---|---|---|
| `parse error at N..M: unexpected token X, parsed up to here then could not continue` | parse | The grammar stopped early, almost always at a construct M1 does not have (§4). `N..M` is the **furthest** position any alternative reached, so it names the offending token; a trailing hint adds the unclosed delimiter, or flags a literal wrapper (`Int(expr)`) when that is the cause |
| `NF validation error at N..M: …` | normalize | Structurally parsed but violates an M1 rule, e.g. Value construction arity |
| `lowering error: UNKNOWN gate: callable 'x' not found in scope` | lower | `x` is in no `--reg` file and is not a fn declared in this source file (§5) |
| `Verification failed: TypeMismatch { … }` | verify | Argument type does not match the callee's registry signature (§6) |
| `Verification failed: ArityMismatch { … }` | verify | Wrong number of arguments to a non-variadic callee |
| `Verification failed: CIfBranchDivergence { … }` | verify | The two `if` arms have different type identities — usually an effectful `Unit` arm facing a value arm (§4.6 — read this one, it bites) |
| `Verification failed: CIfCondNotBool { … }` | verify | `if` condition is not `Bool` |
| `Error: Registry conflict: duplicate name/type …` | registry load | Two `--reg` files declare the same name (§5.5) |

The parse error is deliberately blunt: it reports the *first* token position, not
the offending construct. When you get `unexpected token after end of parse`,
suspect the newest thing you wrote, not the top of the file.

---

## 3. Program structure

An M1 file is a sequence of top-level declarations. Two kinds exist:

```
fn <name>(<params>) -> <ReturnType> {
  <body>
}

let <name> = <expr>
```

There is no module, import, or `Program` wrapper. Multiple declarations per file
are fine — the pipeline processes them one at a time and emits one bundle for the
file.

```
fn negate(flag: Bool) -> Bool {
  if flag { Bool(false) } else { Bool(true) }
}

fn double_negate(flag: Bool) -> Bool {
  negate(negate(flag))
}
```

A later declaration may call an earlier one *and* vice-versa — cross-function
calls in the same file resolve by name.

Top-level `let` has no type annotation and may call functions declared in the
same file:

```
fn g(x: Bool) -> Bool {
  x
}

let z = g(Bool(true))
```

### Function body

Zero or more `let` bindings, then **exactly one** trailing expression. Not two,
not zero.

```
fn make_pair(a: Bool, b: Bool) -> Bool {
  let x = a
  let y = b
  x
}
```

A body ending in a `let` is a parse error. Two trailing expressions is a parse
error. Body `let` bindings carry no type annotation and are in scope for every
later binding and the final expression.

---

## 4. Expressions and the rules that actually bite

### 4.1 No bare literals

Every literal is a typed constructor call. `42` does not parse; `Int(42)` does.

| Type | Literal syntax |
|---|---|
| `Int` | `Int(42)` |
| `Text` | `Text("hello")` |
| `Bool` | `Bool(true)` / `Bool(false)` |
| `Unit` | `Unit()` |
| `Bytes` | **none — there is no `Bytes` literal** |

`Bytes` is a legal *type* (`fn id_bytes(b: Bytes) -> Bytes { b }` compiles and
verifies) but has no surface constructor. `Bytes("ab")` is a parse error. Bytes
values can only enter M1 as parameters.

### 4.2 No operators

There is no `+`, `-`, `==`, `<`, `&&`. Arithmetic and comparison are registry
calls. **And in the M1 baseline registry set there are none** — `int_add`,
`bool_not` and friends live in `registry-pin/axis.axreg`, which cannot currently
be added to the M1 set (§5.5). See §5.2 for the complete list of what you can
actually call.

### 4.3 Calls

Uniform positional syntax for everything — registry primitives, your own
functions, HOFs:

```
value_0(v)
negate(negate(flag))
const_unit()
```

No labels, no defaults, no partial application.

### 4.4 Variable reference

A bare identifier is a parameter or a `let` binding in scope. There is no other
meaning — an identifier that resolves to nothing becomes a callable lookup and
fails at lowering with `UNKNOWN gate`.

### 4.5 Function references (fn-ref)

A function name written **without** parentheses, in argument position, is a
fn-ref constant. The callee must declare that parameter as type `Fn`:

```
fn fnref_handler(p: Int) -> Unit {
  Unit()
}

fn fnref_caller() -> Unit {
  apply_handler(fnref_handler)
}
```

The emitted bundle carries a pool constant whose type is `Fn` and whose 32-byte
payload is the target's identity hash. The target must be in scope (declared in
this file or in a loaded registry) — `apply_handler(nope)` fails at lowering with
`UNKNOWN gate: callable 'nope' not found in scope`.

Passing a fn-ref to a non-`Fn` parameter is a `TypeMismatch`. So is passing a
non-function to an `Fn` parameter.

**But that is the whole of the check. `Fn` carries no signature.**

`Fn` is declared `prim fn` — ONE opaque type identity
(`39e469ce…`), shared by every function in the set. The verifier can tell that
an argument is *a* function; it cannot tell *which shape* of function. So an
`Fn` slot admits any fn-ref, of any arity, with any argument and result types:

```
// text_list_fold_int is (TextList, Int, Fn) -> Int; its callback should be
// (Int, Text) -> Int. is_even is (Int) -> Bool. This COMPILES AND VERIFIES.
text_list_fold_int(str_split(Text("a,b"), Text(",")), Int(0), is_even)
```

Wrong arity, wrong argument type, wrong result type — none of it is caught.
The failure surfaces at link or at run time, or as silently wrong output.

This caps what any correctness-by-construction claim about the callable surface
can say. A contract like `int_list_fold_int(IntList, Int, Fn) -> Int` guarantees
the **data path**: an `IntList` goes in, an `Int` comes out, and no `Value` is in
hand at either end. It guarantees **nothing about the callback**. Recorded under
M1_MONOMORPHIC_LIST_VOCABULARY_V1 as COST 3, accepted and not fixed there;
closing it needs `Fn` to carry a signature, which is a type-system change.

### 4.6 `if` — and the divergence trap

```
if <cond> { <then> } else { <else> }
```

All three parts required. No `else if` — nest explicitly. The condition must be
`Bool`.

**Both arms must agree on type.** The verifier resolves param slots on both arms
and on the condition, so a bare parameter facing a literal of its declared type
is fine. Both of these compile and verify clean:

```
fn f(a: Bool) -> Bool {
  if a { a } else { Bool(false) }                            // OK
}

fn absval(n: Int) -> Int {
  if int_lt(n, Int(0)) { int_sub(Int(0), n) } else { n }     // OK
}
```

Until 2026-09-07 they did not: the branch check compared raw pool hashes while
the condition check one line above already resolved them, so a parameter in one
arm carried the Param sentinel `f9535076…` and could never equal a real type in
the other. `eeaea19` resolves both arms. **If you are reading an older note that
says to avoid mixing a parameter and a literal across arms, or to wrap a bare
parameter in a no-op call to disguise it, that advice is dead.**

What still bites is a *real* type disagreement, and by far the commonest shape
is an effectful call in one arm and a value in the other:

```
fn log_if(c: Bool, p: Text, s: Text) -> Text {
  if c { fs_write_text(p, s) } else { Text("") }             // FAILS
}
```
```
Verification failed: CIfBranchDivergence {
  node_index: 1, then_type: Hash256(fb50dc07…), else_type: Hash256(ffff3528…) }
```

`fs_write_text` returns `Unit`; `Text("")` is `Text`. There is no subtyping and
no unit coercion — two different type identities is the whole of the check.

The remedy is to make the arms the same *shape*, not to make one arm "do
nothing". For an effect, that means performing the same effect on both sides
with different data:

```
fn log_both(c: Bool, hit: Text, miss: Text, s: Text) -> Unit {
  if c { fs_write_text(hit, s) } else { fs_write_text(miss, s) }
}
```

Worth doing on its own merits, not only to satisfy the verifier: two counters
that are *always* both written hand you a free invariant — present + absent must
equal the total checked — which a single conditionally-written counter does not.

Do not reach for a discarded binding in one arm instead (`if c { let a = eff()
Text("y") } else { … }`): that lowers to a `seq` call and no registry in the M1
set declares one, so it dies at lowering with `UNKNOWN gate: callable 'seq' not
found in scope`. See §9. Hoisting the effect to the top level does not help
either — a top-level discarded binding is unconditional and runs on **every**
call, whichever branch the `if` takes (§9).

Nesting an `if` inside an arm is subject to the same agreement rule at each
level.

The `absval` and `fs_*` examples above need a registry that declares those
callables — the M1 baseline set of §6.2 has neither arithmetic nor filesystem
fns. They are here for the branch rule, which is registry-independent.

### 4.7 `determine`

A bare keyword expression, no arguments:

```
fn f(x: Bool) -> Bool {
  determine
}
```

It lowers to a `CDeterminate` node and exists to discharge the gate-domination
obligation on `effect irreversible` callables. **No registry in the M1 set
declares an irreversible fn**, so today `determine` is inert in M1 — write it
when you are modelling a gate, not as ceremony.

### 4.8 Lambdas: gone

`fn(x) { … }` does not parse in M1. It was removed in PASS 3. The fixtures that
still use it (`matrix_lam*.m1`, `fnref_neg_lambda.m1`,
`matrix_value_construct_lam.m1`, `example_compound.m1`) are kept as
*parse-rejected* baseline entries. Behaviour is passed by fn-ref-by-name (§4.5),
which is the only mechanism.

---

## 5. Types and construction

### 5.1 Type expressions

| Syntax | Notes |
|---|---|
| `Int` `Text` `Bool` `Unit` `Bytes` | Primitives |
| `Value(T1, T2, …)` | Product with positional slots |
| `ValueList(T)` | Homogeneous list |
| `Fn` | A function reference |

`ValueList(Value)` — bare inner `Value` — does not parse. The element type must
be fully resolved: `ValueList(Value(Text, Int))`.

### 5.2 Value construction

Mandatory type tag at the construction site; tag arity must equal argument arity:

```
fn make(a: Bool, b: Bool) -> Value(Bool, Bool) {
  Value(Bool, Bool)(a, b)
}
```

Three slots work too. Arity mismatch is caught before lowering:

```
Value(Bool, Bool)(a)
→ NF validation error: Value construction arity mismatch: 2 type args but 1 value args
```

If a slot's tag is a primitive **and** the argument is a literal, their types must
agree (`Value(Text, Int)(Int(1), …)` is rejected). Parameters and compound tags
are not checked here — there is no full inference at the surface.

Nesting is ordinary expression nesting:

```
fn make(a: Text, b: Int) -> ValueList(Value(Text, Int)) {
  ValueList(Value(Text, Int))(Value(Text, Int)(a, b))
}
```

### 5.3 ValueList construction

Mandatory element-type tag. Parens or brackets, both accepted:

```
ValueList(Bool)(a)
ValueList(Bool)[a, a]
```

### 5.4 Compound parameter tags erase — and are unchecked

A `Value(...)` or `ValueList(...)` tag in **parameter** position erases to the
registry constructor type of the same name — `Value` → `212f4f49…`,
`ValueList` → `47b62fb6…` — which is exactly what `value_0`/`value_1`/`value_2`
and the list primitives declare. So this compiles and verifies clean:

```
fn f(v: Value(Bool, Bool)) -> Bool {
  value_0(v)
}
```

Until `eeaea19` (2026-09-07) it did not: `resolve_type_hash` handled only
`PrimTypeExpr`, so both compound tag kinds fell through to the null hash and
every use of such a parameter failed with
`TypeMismatch { expected: 212f4f49…, got: f9535076… }`. Functions could not
accept a list at all. **An older note telling you to construct compound values
inside the function rather than accept them as parameters is dead.**

What erasure costs you: the element tags are discarded, so `Value(Bool, Bool)`
and `Value(Int, Int)` are the *same* parameter type to the verifier, and nothing
checks that the value you are handed has the shape you wrote. The tag is
documentation at the parameter position.

**`Value` and `ValueList` are KEYWORDS, and a keyword type must carry its tag.**
`TypeExpr` offers `ValueTypeExpr` (`Value(T, ...)`), `ValueListTypeExpr`
(`ValueList(T)`) and `PrimTypeExpr` (an `IDENT`). `Value` lexes as `KW_VALUE`,
never as an `IDENT`, so **bare `Value` does not parse in ANY type position** —
parameter or return:

```
fn f(v: Value(Bool, Bool)) -> Value            { … }   // FAILS at the `{`
fn f(v: Value(Bool, Bool)) -> Value(Bool, Bool) { … }  // OK
fn f(v: Int)               -> Value            { … }   // FAILS at the `{`
```

The failure is at the `{` after the return type, which reads as a body problem
and is not one. **Both surfaces behave identically here** — M1 and AI3 share the
`TypeExpr` productions verbatim (`m1-parse.yaml` / `ai3-parse.yaml`), so a
compound tag that parses in one parses in the other. If you hit this on a fn
returning a constructed value, tag the return type; do not fall back to
declaring the parameter `Int` and leaning on the registry contract. That
workaround is not required.

Note also what a `TypeMismatch` diagnostic shows: `got` is the **raw pool hash**,
so for any argument that is a parameter it is always the Param sentinel
`f9535076…`, never the declared type. `got: f9535076…` means "this argument was
a parameter" — go read its declared type in the source; the verifier did compare
the real one.

---

### 5.5 Three product families, and the accessor that spans them

A "compound value" at runtime is one of three distinct `Value` variants, built
by different constructors and read by different accessors. They look identical
in the registry — `tuple_field` and `ctor_field` are both declared
`in (Value, Int) out Value` — and crossing them **aborts the process**.

| Build with | Runtime variant | Read with |
|---|---|---|
| `value_make(a, b, …)` (variadic) | `Value::Ctor { tag, fields }` | `ctor_field`, `value_0/1/2` |
| `list_of_3(a,b,c)`, `list_cons`, `list_nil` | `Value::List` | `tuple_field`, `list_get`, `value_0/1/2` |
| `tcp_listen` / `tcp_listen_shared` (bridge-side) | `Value::Tuple` | `tuple_field`, `value_0/1/2` |

Crossing them is a panic, not a verify error — the contracts are identical, so
nothing catches it until it runs:

```
tuple_field: expected Tuple or List, got Ctor { tag: 0, fields: [...] }
(use ctor_field for a Value(..)(..)-constructed Ctor)
```

**`value_0` / `value_1` / `value_2` read all three families** and never panic
(`runtime/tuple.rs` `value_field` matches `Ctor`, `Tuple` and `List` alike).
That makes them the right default whenever the family is not obvious from two
lines away. Their two costs: they stop at index 2, and an out-of-range index
returns `Value::Unit` **silently** rather than failing — so they trade a loud
panic for a quiet wrong answer. Use `ctor_field` / `tuple_field` when you know
the constructor and want the index checked by a crash instead.

The names are the trap: `value_make` builds a `Ctor`, not a "Value", and
`tuple_field` reads a `List` as happily as a `Tuple`. Read the table, not the
names.

---

## 6. The registry

### 6.1 What a registry is

A `.axreg` file is a *projection* — a human-authored text view of canonical binary
records. It declares **types** and **function contracts**. It does not contain
implementations. Loading is purely additive: every `--reg` file is merged into one
flat namespace for the invocation. There is no `import`, no `inherits`, no
search path.

Nothing is callable from M1 unless it is (a) declared in a loaded registry, or
(b) a function declared in the same source file.

### 6.2 The M1 baseline set — everything you can call

Three files, per `CLAUDE.md`:

| File | Role |
|---|---|
| `axis-surface-m1-config/m1-registry.axreg` | M1 surface projection — Value/ValueList ops, HOFs |
| `registries/pilot_lowering.axreg` | Test fixture — owns `Unit`, `const_unit`, `seq_units` |
| `registries/fnref_test.axreg` | Test fixture — `apply_handler(Fn)`, `consume_unit(Unit)` |

The complete callable vocabulary (`axis symbols` output, signatures in surface
names):

| Fn | Signature | Notes |
|---|---|---|
| `value_make` | variadic → `Value` | Emitted by `Value(…)(…)`; not usually written by hand |
| `value_0` / `value_1` / `value_2` | `(Value) -> Value` | Slot extraction |
| `list_make` | variadic → `ValueList` | Emitted by `ValueList(…)(…)` |
| `sort` | `(ValueList, Value) -> ValueList` | Comparator arg is `Value`-typed |
| `filter` | `(ValueList, Value) -> ValueList` | |
| `map` | `(ValueList, Value) -> ValueList` | |
| `reduce` | `(ValueList, Value, Value) -> Value` | |
| `const_unit` | `(Unit) -> Unit` | |
| `seq_units` | `(Unit, Unit) -> Unit` | |
| `apply_handler` | `(Fn) -> Unit` | `effect fullIo` |
| `consume_unit` | `(Unit) -> Unit` | |

That is the whole list. There is no arithmetic, no comparison, no text
manipulation in the M1 baseline. The HOFs (`sort`/`filter`/`map`/`reduce`) take
their callback as a `Value`, which no M1 expression currently produces in a form
they can consume — they are declared, not reachable.

Primitive type identities, for reading diagnostics:

```
Bool       85f90dfea1d8027e1463e5ca971a250110a20df0119d204a74220bc63516d15b
Int        16305a6b2292e931665a34089864ee3fcc24171c81551f152cf4d1db0dd6122d
Text       ffff35283fe52f01c3d555e5d8ebdcf90e4b2b5e7783d261e0f094bc6fafa134
Unit       fb50dc0717ff266cf9baf82b1ce7a1c2ef6d9247859680b11a19fb7077f5f222
Bytes      dbecab43cb8facaa2f8537a707d982cbc168a7767f3b0d3b1f3cd4aa5ef272bf
Value      212f4f4932108856c618d74c7d6325961cd03cfad5e6a185b364156b32aaf250
ValueList  47b62fb6268dd93269e72fad8115d411c3c95b37e1db6240953cdf9b2bd29bae
Fn         39e469ce5a5ba3136c4a44381f2e4183dca275257adfcf3c0025094f05c022f5
Dec        cc16a44529d2b5a92d014d63669540ce2f7dd654d97971b0b3b60edc44d457a7
Param      f9535076eb26809f7b425c39485d3fd322142a5c4638b9b81d66b919356ee8ba  (sentinel, not a surface type)
```

### 6.3 Inspecting registries

Never guess a name or a hash — the binary will tell you.

```bash
# every projected name, or a case-insensitive substring search
axis symbols          --reg <reg>...
axis symbols value    --reg <reg>...

# a Hash256 → its structural TypeDef, one level deep
axis types 85f90dfea1d8027e1463e5ca971a250110a20df0119d204a74220bc63516d15b --reg <reg>...

# every fn with exactly this (inputs, output) signature, regardless of name
axis find --out <hash> --in <hash>,<hash> --reg <reg>...
```

`axis types` requires the **full** 64 hex chars. Compile diagnostics print
truncated hashes; `axis symbols` prints full ones, and so does the
`m1_verify_diag` MCP tool (§8).

**`axis find` is the dedup gate.** Before authoring any new fn, run it on the
signature you are about to declare. If a fn with that exact contract already
exists under another name, use it.

**What the gate is for, precisely** (ruled 2026-09-07 under
M1_MONOMORPHIC_LIST_VOCABULARY_V1, binding):

> The gate exists to catch ONE OPERATION declared twice under two names.
> Two names, same contract, **different** semantics is permitted.
> Two names, same contract, **same** semantics is not.

`map`, `filter` and `flat_map` all answer `(ValueList, Fn) -> ValueList` and all
three are legitimate. A match is a question to answer, not a verdict.

**The gate is unsound at nullary and `Unit`-arg signatures.** Matching is on the
exact `(inputs, output)` tuple and nothing else, so every fn returning `T` from
no meaningful input collides with every other. `axis find` reports `proc_args`
— which returns the process argv — as a match for `(Unit) -> TextList`, the
signature of an empty-list constructor. Read every match before acting on it;
at these signatures the gate is a prompt to think, not an answer.

The same coincidence bites whenever an operation's result type equals the
element type it works on: `int_list_head(IntList) -> Int` and
`int_list_len(IntList) -> Int` are contract-indistinguishable, and so are
`bool_list_head` and `bool_list_is_empty`. The type system cannot separate them.
Reaching for one and resolving to the other is silently wrong behaviour, not a
compile error.

### 6.4 `.axreg` grammar

Comments are `//` — **not `#`**. A `#` line is an unrecognized directive and
fails the load.

```
registry <name> <version>

type <Name> = prim <unit|bool|int|float|bytes|text|value|dec|fn|param>
type <Name> = product (T1, T2)
type <Name> = sum [ Ctor1(T1, T2) | Ctor2 ]
type <Name> = list <T>

fn <name>
  identity 0x<64 hex>        // REQUIRED — authored literally, never minted at load
  kind leaf|composite        // REQUIRED
  in (T1, T2)                // REQUIRED — `in ()` means nullary, not variadic
  out T                      // REQUIRED
  effect pure|reads|writes|fullIo|irreversible
  deterministic true|false
  idempotent true|false
  variadic true|false        // skips arity AND per-arg type checks at verify
  disposition interactive|batch
  body 0x<64 hex>            // REQUIRED for kind composite
  contract gen 1 [ c1, c2 ]
  oracle 0x<64 hex>
end

impl <fn-name> satisfies gen <n> artifact 0x<64 hex>
```

**A `type` or `fn` field declaration is ONE line.** Wrapping a long
`sum [ A | B | C ]` across lines for readability does not parse — the loader is
line-oriented and sees an unterminated `[`.

**Forward references are legal, within a file and across files.** The loader
resolves the whole type graph order-independently, so a sum arm may name a type
declared further down the same file, and one file may reference a type another
file owns without redeclaring it. (This section previously said declarations
"must precede use within a file". They need not; verified by loading
`type Value = sum [ AsDec(Dec) ]` above `type Dec = prim dec`.) What the loader
*does* reject is a genuine **cycle**: `AsList(ValueList)` with
`ValueList = list Value` is ill-founded and fails with `cannot resolve type(s)
[Value, ValueList] — unknown referenced type name or dependency cycle`.

Because both failures land on a `type` line, they were easy to confuse: an
unterminated multi-line `sum` used to report `expected [ ... ] after sum`, which
reads as a bracket-syntax problem and was once diagnosed as a declaration-order
problem instead. It now names the one-line rule and says explicitly that order
is not the issue.

The loader fails loud rather than skipping: an unknown top-level directive, an
unknown `fn` field, a missing `end`, a bad hash length, or a file with content
but zero entries all abort the build. This is deliberate — a registry that
silently drops an entry is a closure oracle that silently authorizes deletions.

### 6.5 One name, one owner

Across the merged set, **every type name and every fn name must be declared
exactly once**. No last-wins, no ranking, no preference — a collision is a hard
error naming both files:

```
Error: Registry conflict: duplicate type name 'Unit' —
  declared in registries/pilot_lowering.axreg and registry-pin/axis.axreg
```

That example is not hypothetical: it is exactly what happens if you try to add
`registry-pin/axis.axreg` to the M1 set to get `int_add`. `axis.axreg` redeclares
`Unit`, `Bool`, `Int`, `Text`, and the M1 set already owns them. Getting the core
primitives into M1 requires reconciling type ownership first, not just another
`--reg` flag. (`M1_REGISTRY_CATALOGUE.md` suggests adding the flag; it predates
the duplicate-name law.)

The same rule is why `m1-registry.axreg` deliberately does *not* declare `Unit`
(pilot_lowering owns it) and `fnref_test.axreg` declares neither `Unit` nor `Fn`.
Each file carries a comment saying which sibling owns the type. **Preserve those
comments when you edit** — they are the only record of the ownership split.

### 6.6 Adding a function to a registry

1. `axis symbols <name> --reg …` — is the name taken?
2. `axis find --out … --in … --reg …` — does this contract already exist under
   another name? If yes, stop and use it.
3. Mint an identity. Two rules, and only one of them applies to you:
   - **§5b bootstrap rule** — `SHA-256` of the UTF-8 function name. This is a
     *scope-limited exception* for stable-named compiler primitives, already used
     by `axis.axreg`, `m1-registry.axreg`, and `fnref_test.axreg`:
     `echo -n "apply_handler" | sha256sum`.
     Test fixtures may also use obviously-fake identities (`0x1111…`), as
     `pilot_lowering.axreg` does.
   - **New user-defined functions MUST use entropy-minted identities** — not a
     hash of the name.
4. Write the block. Every required field, terminated by `end`.
5. Declare the type only if no other file in the set owns it (§6.5).
6. Recompile and run the full M1 baseline (§7).

Identities are authored literally and never computed at load time. Type
identities are the opposite — always computed as the content hash of the
canonical `TypeDef`, never written. Two projections that differ only in comments,
whitespace, or field order must lower to identical canonical bytes.

### 6.7 Declaring is not implementing

A registry entry is a *contract*. It makes the name callable and type-checkable;
it does not make it executable. The Rust implementation lives in the bridge, and
the two drift. `M1_REGISTRY_CATALOGUE.md` classifies every callable as wired /
source-only / stub / registry-only / undeclared. A `.m1` file can compile and
verify clean and still fail at emit time with "unknown CCall target" if the
callable is registry-only. Check the catalogue before relying on a call to run.

**And the contract is silent about failure.** Every bridge fn returns a plain
type and **panics** on a runtime failure — there are no `ResultText` /
`ResultUnit` / `ResultBytes` sum types, and M1 has no `catch`, no `assert` and no
error primitive, so a panic is an unrecoverable process abort. The registry line
`fn fs_read_text in (Text) out Text` tells you the happy path and nothing else:
on a missing file it aborts, it does not hand back `""`.

Pre-conditions are the author's job. Guard before you read:

```
if fs_file_exists(p) { fs_read_text(p) } else { Text("") }
```

`fs_read_last_line` is the deliberate exception — it returns `""` for a missing
or empty file, because "this name has no binding history yet" is a correct query
result, not an error. It is safe to call unguarded; `fs_read_text` is not. The
full convention is in `axis-codegen-bridge-rs/CLAUDE.md` under
"Plain-return-type convention".

---

## 7. Verification, and what it does and does not check

The verifier gate runs on every compile unless `--no-verify`. It checks, per
node:

- **Arity** — argument count against the registry `in (…)` list. Skipped entirely
  for `variadic true` callables, along with the per-argument type check.
- **Argument types** — each argument's type identity against the declared input.
  Param slots are resolved to their declared types here.
- **`if` condition is `Bool`** — param slots resolved.
- **`if` branch agreement** — param slots resolved on both arms, exactly as the
  condition is (§4.6). Agreement means the two arms share at least one candidate
  type; for non-param refs that is plain hash equality.
- **Effect derivation and ceiling** — the compile-time ceiling is `ANY`, so
  nothing is rejected on effect during a normal compile. To enforce one, verify
  the bundle separately: `axis verify <bundle.coreir> --reg … --effect-ceiling pure`.
- **Gate domination** — an `effect irreversible` call must be dominated by a
  `CDeterminate`. No M1-set registry declares an irreversible fn, so this never
  fires today.

It does **not** check that a function's body type matches its declared return
type. `fn f(a: Bool) -> Int { a }` compiles and verifies. Declared return types
are documentation at the M1 surface right now.

It does **not** check the shape of a callback passed into an `Fn` slot — not its
arity, not its argument types, not its result type. `Fn` is a single opaque
identity, so every fn-ref satisfies every `Fn` parameter. See §4.5.

### After any change: run the baseline

The only valid test of the full system is the `axis` binary on real files. Run
the loop in `CLAUDE.md` and compare against the recorded baseline: **22 PASS**,
6 parse-rejected, 1 NF-rejected, 3 verify-rejected, 1 known deferred gap. A file
that starts passing is as much a regression as one that stops.

---

## 8. Tooling quick reference

| Task | Command |
|---|---|
| Compile + verify | see §1 |
| Verify an existing bundle | `axis verify <b.coreir> --reg … [--effect-ceiling pure\|reads\|writes\|fullio]` |
| Bundle identity | `axis bundle-id <b.coreir>` |
| List / search names | `axis symbols [substring] --reg …` |
| Resolve a type hash | `axis types <64-hex> --reg …` |
| Find a fn by contract | `axis find --out <hash> [--in <h>,<h>] --reg …` |
| Inspect IR | `capnp decode core_ir_spec/axis_core_ir_0_5.capnp CoreBundle < b.coreir` |

`src/bin/axis_mcp.rs` exposes the same five capabilities over MCP —
`m1_validate`, `m1_verify_diag` (full-length hashes), `m1_type_resolve`,
`m1_symbols`, `m1_find` — pinned to the M1 baseline registry set and read-only.
Register it with:

```bash
claude mcp add axis -- <repo>/target/debug/axis_mcp
```

---

## 9. Known gaps an author will hit

| Gap | Effect on you |
|---|---|
| `if` arms need identical type identities (§4.6) | An effectful (`Unit`) arm cannot face a value arm. Write both arms — same effect, different data |
| Compound parameter tags erase (§5.4) | `Value(Bool, Bool)` and `Value(Int, Int)` are one parameter type; nothing checks the shape you wrote |
| Param sentinel in diagnostics (§5.4) | `got: f9535076…` means "a parameter", not a type |
| No arithmetic in the M1 set (§4.2, §6.5) | Cannot write `1 + 2` in any form; `axis.axreg` cannot be added without resolving type ownership |
| No `Bytes` literal (§4.1) | `Bytes` values only arrive as parameters |
| No return-type check (§7) | A wrong `-> T` will not be caught |
| `seq` must be declared, and only the canonical set declares it | A discarded binding inside an `if` arm lowers to `seq(effect, arm_result)`. The canonical `axRegistry/axis-bridge.axreg` declares it; the lab's own `registries/` do not, and there it still dies with `UNKNOWN gate: callable 'seq' not found in scope`. Its `out Value` is documentation only — the verifier gives a seq-wrapped arm the type of `args[1]`, so the arm keeps its own type and does not diverge from its sibling |
| Top-level discarded effects are **unconditional**, not dropped | `let a = eff()` with no consumer is top-level: it runs once on **every** call, in node-index order, whichever branch a later `if` takes. Its position relative to an `if` says nothing about branch membership. Emitter-side rule: `ORPHAN_IS_TOP_LEVEL_V1` |
| Bridge fns panic; there is no `Result` (§6.7) | `fs_read_text` on a missing file **aborts the process** — it does not return `""`. Guard with `fs_file_exists` first, or call `fs_read_last_line`, which returns `""` for a missing file by design. A fallback path that reads unguarded takes the run down with it |
| `--inspect` is dead | Use `--out` + `capnp decode` |
| HOF callbacks unreachable | `sort`/`filter`/`map`/`reduce` are declared but nothing constructs their `Value` callback since lambdas were removed |
