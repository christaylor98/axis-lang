# AI3 Authoring Primer — writing AI3 that compiles and verifies

**Scope:** everything an author (human or model) needs to write AI3 source that
compiles *and verifies*. It stands alone and does not assume you have read the M1
primer. Pipeline internals appear only where they explain a rule you will hit.

**Status:** current as of 2026-10-13: axis-lang `4ece818`, axis-codegen-bridge-rs `8add938` (Core IR 0.6).
Originally written against axis-lang commit `c6103ce` (2026-09-29) and
<https://github.com/christaylor98/axRegistry> at
`94f13a68c55b184e9a7cc60e599629e908c4fb1b`, which is now on `main`. §2 was
rewritten on 2026-10-03 for `axis build` / `axis run` and `scripts/install.sh`
(axis-lang `da1ca18`, axRegistry `main` at `a11e51a`).
§12–§14 (where code runs, building text, loop state cost) were added on 2026-10-13 against Core IR 0.6:
axis-lang `4ece818`, axis-codegen-bridge-rs `8add938`, axRegistry `3bced0c`; every program in them was
run on that toolchain.
**Install the toolchain before anything else (§2).**

Every code sample, error message and bundle id below came from
running `axis` on that exact source. Nothing is carried over from
prose unchecked.

**Supersedes, where they disagree:**

- `axis-lang-expr-poc/docs/poc/prelang/a3_shipped_primer.md`. It predates
  M1_HOF_CALLBACK_CONTRACT_V1, and its §8 ("a callback in an `Fn` slot is NOT
  type-checked") is **no longer true** (see §7).
- `examples/surface-ai3/BASELINE.md` Finding 1 (the parse error message) and
  Finding 2 (the registry split). The parse error now names the offending token
  (§9), and the canonical registry set in §2.4 covers every fixture at once. The
  rest of BASELINE.md still holds and is the design record.

---

## 1. What AI3 is

AI3 is M1 with two additions:

1. **`return` is mandatory at the tail of every function body.** There must be
   exactly one, and it must come last. It is forbidden inside `if` branches.
2. **`;` is an optional statement separator.** A newline works just as well.

The rest is M1: the same types, expressions, literals and registry. The two
additions are removed during normalization. An AI3 program and the same program
written in M1 compile to **byte-identical** bundles (17/17 fixtures, see
BASELINE.md). `return` changes how you write code, not what it compiles to.

Config lives in `axis-surface-ai3-config/`. Fixtures live in
`examples/surface-ai3/`. AI3 is config only: zero Rust was written for it.

---

## 2. Install, then build and run

One command takes AI3 source to a native executable:

```bash
axis run   src/prog.ai3 hello world     # build in a temp dir, run, exit with the program's code
axis build src/prog.ai3 -o build/prog   # write the executable
```

There are no grammar or registry flags. `axis` picks the surface from the
extension (`.ai3`; `.m1` and `.pyax` work the same way), finds the AI3 grammar,
the five registry files and the codegen bridge on its own, compiles and
verifies, and links. §2.1 sets this up on a machine you control. §2.3 sets it
up inside a project for an agent whose sandbox cannot see the rest of the
machine.

### 2.1 Install on your own machine

```bash
git clone https://github.com/christaylor98/axis-lang ~/dev/axis-lang
~/dev/axis-lang/scripts/install.sh
```

The repositories are **private**: cloning needs GitHub access to
`christaylor98` (for example, `gh auth login`).

The script checks its prerequisites (`git`, `cargo`/`rustc`, `capnp`, a C
compiler, `ar`; x86_64 only) and says how to install any that are missing.
It clones `axRegistry` and `axis-codegen-bridge-rs` next to the lab
repo if they are not there yet, builds both tools, and symlinks `axis` into
`~/.local/bin`. It finishes by building and running one small program per
surface, so "Installed" means it works. Re-run it with `--update` to pull and
rebuild all three repositories. `--help` lists the options, including
`--offline` and `--bin-dir` for restricted machines.

`rustc` stays a run-time dependency: the bridge generates Rust and links it
with `rustc` on every build.

### 2.2 Build and run

```bash
axis run   src/prog.ai3 hello world
axis build src/prog.ai3 -o build/prog   # default -o: the file stem, in the current directory
```

**The entry is the last function in the file, and it takes exactly one
parameter: the command line.**

```
fn gcd(a: Int, b: Int) -> Int {
  return if int_eq(b, Int(0)) { a } else { gcd(b, int_mod(a, b)) }
}

fn show(n: Int) -> Text {
  return int_to_str(n)
}

fn prog(args: ValueList(Text)) -> Text {
  let xs = value_list_to_text_list(args)
  let a = str_to_int(text_list_head(xs))
  let b = str_to_int(text_list_head(text_list_tail(xs)))
  return show(gcd(a, b))
}
```

```
$ axis run src/gcd.ai3 48 18
6
$ axis build src/gcd.ai3 -o build/gcd
src/gcd.ai3 → build/gcd (ai3, entry `prog`)
$ build/gcd 21 14
7
```

- **Put the whole program in one file.** Every function becomes its own
  bundle and they are linked together, so helpers, call order and recursion
  need nothing extra. (Before `axis build`, only the last function of a file
  could be linked. If you have seen the per-file, local-registry recipe, it is
  now needed only to share code between files; see §2.5.)
- `args` is argv **without** the program path: `prog hello world` gives
  `["hello", "world"]`. `value_list_to_text_list` turns it into a `TextList`
  you can compute with (§6.4). `argv_get(Int(i))`, `argv_count(Unit())` and
  `argv_or(Int(i), Text(…))` read the process argv directly. There, `Int(0)`
  **is** the program path and `Int(1)` the first argument.
- **An entry with several parameters compiles, links, and then panics at run
  time:** `expected Value::Tuple of 2 args, got List([…])`. Check that the
  *last* function in the file is the entry.
- A non-`Unit` result is printed to stdout. A `Unit` result prints nothing.

**Exit codes.** `axis build` exits 0 when it wrote the executable, and 1 on any
compile, verify or link failure, with the compiler's own error on stderr (§9).
`axis run` exits 1 on those same failures, and otherwise with the **program's**
exit code:

| Code | Meaning |
|---|---|
| 0 | ok |
| 2 | a declared Err |
| 3 | an Unknown result (a failure of the world, or `fail(Text)`), printed with its cause |
| 4 | a Defect (overflow, divide by zero, a missing field: a bug in the program) |
| 70 | a bridge fault under `AX_FAULT_MODE=abort` |
| 1 | anything else — including a failed build, so read stderr |

```
$ axis run src/div.ai3          # int_div(a, Int(0))
axis: DEFECT in int_div: int_div: division by zero (at …) -- a bug in the program
$ echo $?
4
```

**Options.** `--keep <dir>` keeps the `.coreir` bundles (one per function)
instead of deleting them. `--surface ai3` overrides the extension. In `axis run`,
these options go **before** the source file: everything after it goes to the
program unchanged, `-x` included.

**Where it looks**, each overridable by an environment variable:

| Variable | Default | Holds |
|---|---|---|
| `AXIS_HOME` | the lab checkout `axis` was built in | `axis-surface-ai3-config/` |
| `AXIS_REGISTRY_DIR` | `$AXIS_HOME/../axRegistry` | `axis-types`, `axis`, `axis-bridge`, `m1-builtins`, `gen-working` `.axreg` |
| `AXIS_BRIDGE` | `$AXIS_HOME/../axis-codegen-bridge-rs/target/release/axis-codegen-bridge` | the bridge binary |
| `TMPDIR` | `/tmp` | where `axis build` links |

A missing piece is named with its variable, for example
`axis build: registry not found at … (set AXIS_REGISTRY_DIR)`.

### 2.3 Inside a sandbox

**An agent's sandbox usually cannot read outside its own project folder.**
`~/dev/axis-lang` and `~/dev/axRegistry` are hidden from
it (`[sandbox: hidden path …]`), so the defaults in §2.2 point at nothing.
Vendor the toolchain into the project instead:

```
<your-project>/
  axis-toolchain/bin/axis                   the compiler (a copy)
  axis-toolchain/bin/axis-codegen-bridge    the bridge (a symlink: see below)
  axis-toolchain/axis-surface-ai3-config/   ai3-lexer / -parse / -ast / -normalize .yaml
  axRegistry/                       the five .axreg files
  scripts/axis                              the wrapper below
  build/                                    executables, and build/tmp
```

Whoever sets up the machine runs, once, after §2.1:

```bash
LAB=~/dev/axis-lang; P=<your-project>
mkdir -p $P/axis-toolchain/bin $P/axRegistry $P/scripts
cp    $LAB/target/release/axis          $P/axis-toolchain/bin/
cp -r $LAB/axis-surface-ai3-config      $P/axis-toolchain/
cp    ~/dev/axRegistry/{axis-types,axis,axis-bridge,m1-builtins,gen-working}.axreg $P/axRegistry/
ln -s ~/dev/axis-codegen-bridge-rs/target/release/axis-codegen-bridge $P/axis-toolchain/bin/
```

**Copy real files, except the bridge.** A symlink into a hidden directory does
not resolve inside the sandbox, so the compiler, grammar and registries are
copies. The bridge is the exception: it finds its runtime library beside its
own binary and calls `rustc`, so it has to run from where it was built. The
sandbox must be given **read-only** access to
`~/dev/axis-codegen-bridge-rs/target/release` and to the Rust toolchain
(`~/.cargo`, `~/.rustup`).

Save this as `<your-project>/scripts/axis` and `chmod +x` it:

```bash
#!/usr/bin/env bash
# scripts/axis — the axis vendored in this project (AI3_AUTHORING_PRIMER §2.3).
# Points axis at the project's own copies, so nothing outside the project is
# read except the bridge's build dir and the Rust toolchain.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export AXIS_HOME="$ROOT/axis-toolchain"
export AXIS_REGISTRY_DIR="$ROOT/axRegistry"
export AXIS_BRIDGE="$ROOT/axis-toolchain/bin/axis-codegen-bridge"
# axis build links in a temp dir; /tmp is often read-only in a sandbox.
export TMPDIR="${TMPDIR:-$ROOT/build/tmp}"
mkdir -p "$TMPDIR"
exec "$ROOT/axis-toolchain/bin/axis" "$@"
```

Then use `scripts/axis` exactly as `axis` in §2.2:

```bash
scripts/axis run   src/gcd.ai3 48 18      # 6
scripts/axis build src/gcd.ai3 -o build/gcd
```

Tested on 2026-10-03 under `bwrap` with `~/dev` hidden except the bridge's
`target/release` (read-only), no network, and a read-only root: the `gcd`
program, a compile error (exit 1) and a division by zero (exit 4) all behaved
as shown in §2.2.

Add `/axis-toolchain/bin/`, `/axRegistry/` and `/build/` to your
`.gitignore`. **After an update** (`install.sh --update`), copy the binary,
grammar and registry files again: the copies do not follow the originals.

### 2.4 Check without linking

`axis build` always links, which needs the bridge and `rustc` and takes a few
seconds. To check that code compiles and verifies, without building anything,
call `axis` with the full flag set:

```bash
CFG=axis-toolchain/axis-surface-ai3-config; AXR=axRegistry   # or the paths in ~/dev
axis --file src/prog.ai3 \
  --lexer     $CFG/ai3-lexer.yaml \
  --parser    $CFG/ai3-parse.yaml \
  --schema    $CFG/ai3-ast.yaml \
  --normalize $CFG/ai3-normalize.yaml \
  --reg $AXR/axis-types.axreg --reg $AXR/axis.axreg --reg $AXR/axis-bridge.axreg \
  --reg $AXR/m1-builtins.axreg --reg $AXR/gen-working.axreg \
  --out build/ai3/
```

Success is exactly one `→` line, with exit 0:

```
src/prog.ai3 → build/ai3/prog.coreir (identity: 9efc4a3d43fa…)
```

Anything else is a failure. Treat any non-zero exit as failure; do not match
on the text. `--no-verify` skips the type gate, so never use it to make a
diagnostic go away.

**Use the full five-file registry set.** It is what `axis build` loads, and the
only set that has lists, text, filesystem, higher-order functions *and*
variadic `value_make`/`list_make`. All 17 positive AI3 fixtures compile under
it, and all 8 negative fixtures are rejected where they should be.
`registry-pin/axis.axreg` on its own is a small core set: `int_add`/`sub`/`mul`/
`eq`/`lt`, `bool_and`/`or`/`not`, `seq_unit`, `unit_id`. Under it, `Value(…)(…)`
and `ValueList(…)(…)` fail with `ArityMismatch { expected: 0, got: N }`, and
anything else is `UNKNOWN gate`.

**Nothing ships without a registry.** Every callable (`int_add`, `str_concat`,
`int_list_fold_int`, `fail`, …) comes from the `.axreg` files. A missing
`--reg` path fails before parsing:
`Failed to load registry files: Path does not exist`.

### 2.5 Sharing code between files

`axis build` takes one source file. To call a function that lives in another
file, compile it first and declare it in a project-local registry so callers
can resolve it:

```
registry myproject-local 0.5

fn helper
  identity 0x<minted once at random: openssl rand -hex 32>
  kind composite
  in (Text)
  out Text
  body 0x<helper's bundle identity, from the → line that compiled it>
end
```

The identity is random, not a hash of the name, so two projects' `helper`s
never share one; the bridge links it through `body`. Compile callees before
callers with the §2.4 command, adding
`--reg local.axreg --body-dir build/ai3` to every compile after the first (the
directory holding the callees' bundles). Then link with the bridge directly,
passing each callee's bundle:

```bash
REGS="--reg $AXR/axis-types.axreg --reg $AXR/axis.axreg --reg $AXR/axis-bridge.axreg \
      --reg $AXR/m1-builtins.axreg --reg $AXR/gen-working.axreg"
axis-codegen-bridge build build/ai3/prog.coreir --out build/exe/prog --exe \
    --lib-dir build/ai3 $REGS --reg local.axreg
```

- Without `--body-dir` the caller fails verification:
  `CompositeEffectUnderivable { … reason: "body bundle 4ebe7f79… …" }`.
- Pass the bridge the same five `--reg` files, or every registry call fails:
  `UNKNOWN_GATE: identity daf80560… — not a bridge built-in, not a §5b identity`.
- **A recursive function can't be a `composite`.** Its body calls itself, and
  the caller fails with `CompositeEffectUnderivable { … reason: "cycle detected
  in composite body graph" }`. Declare it a `leaf` instead: no `body`, and its
  effect stated by you (`effect pure`, `deterministic true`, `idempotent true`;
  one that writes files is `fullIo`, not `pure`).
- A name that is already in a registry can't be reused for your function:
  `function 'count' collides with a registry entry of the same name`. Check with
  `axis symbols` first.

This recipe was tested on 2026-09-29 at `c6103ce`, and with a random identity on 2026-10-04 at `1164b35`. Within one file, prefer
§2.2: it needs none of this.

---

## 3. Program shape

A file is a sequence of top-level declarations. There is no module, no import,
and no program wrapper.

```
fn <name>(<param>: <Type>, ...) -> <Type> {
  <let bindings>
  return <expr>
}

let <name> = <expr>
```

### 3.1 Function body: lets, then exactly one `return`

```
fn double(x: Int) -> Int {
  let y = int_add(x, x)
  return y
}
```

- Put zero or more `let` bindings first, then **exactly one** `return <expr>`
  as the last thing in the body.
- `let` has no type annotation. A binding is in scope for every later binding
  and for the `return`.
- `return` always takes an expression. For a `Unit` result, write
  `return Unit()`. A bare `return` does not exist.
- `return` is a reserved word and cannot name a parameter or a variable.
- `return` is **not an expression**. You cannot write it inside a call, an
  argument or a branch.

Every one of these is a parse error (fixture in brackets):

| You wrote | Why it fails |
|---|---|
| a body ending in a bare expression | a function tail needs `return` [`ai3_neg_missing_return`] |
| `return a` then `let …` | nothing may follow the `return` [`ai3_neg_return_not_at_tail`] |
| two `return`s | the body admits exactly one [`ai3_neg_two_returns`] |
| `{ }` | the body must contain a `return` [`ai3_neg_empty_body`] |
| `return;` | `return` needs an expression [`ai3_neg_bare_return`] |
| `fn f(return: Int)` | `return` is a keyword [`ai3_neg_return_as_ident`] |

### 3.2 Separators

A newline and `;` are interchangeable. These three bodies compile to the same
bundle (`1504cca4…`):

```
let a = f(x)
let b = g(a)
return b
```
```
let a = f(x); let b = g(a); return b;
```
```
let a = f(x); let b = g(a)
return b
```

`;` is allowed in exactly three places: after a body `let`, after a `return`,
and after a top-level `let`. It is illegal anywhere else:

```
let b = a;;          // parse error: unexpected token `;`   (one `;` at most)
fn f(…) -> Int { … };  // parse error: unexpected token `;`   (not after `}`)
```

### 3.3 Several functions per file

A function may call another from the same file in either order, and may call
itself. Recursion compiles, verifies, and links with `axis build` (§2.2):

```
fn gcd(a: Int, b: Int) -> Int {
  return if int_eq(b, Int(0)) { a } else { gcd(b, int_mod(a, b)) }
}
```

Recursion is the only loop. There is no `for` and no `while`.

Declaring two functions with the same name is a hard error, even when their
signatures match:

```
lowering error at 0..0: duplicate function 'g' — declared twice in this file
```

### 3.4 Top-level `let`

```
fn g(x: Bool) -> Bool {
  return x
}
let z = g(Bool(true));
```

A top-level `let` has no body, so it takes no `return`. The trailing `;` is
optional.

### 3.5 Comments

Comments start with `//` and run to the end of the line. `#` is a lex error:
`unexpected character '#'`.

---

## 4. `if` — branches take no `return`

```
if <cond> { <branch> } else { <branch> }
```

**A branch is not a function body.** A branch holds zero or more `let`s followed
by a **bare expression**, with no `return`. The `return` goes **outside** the
`if`, at the function tail:

```
fn absval(n: Int) -> Int {
  return if int_lt(n, Int(0)) { int_sub(Int(0), n) } else { n }
}
```

```
fn pick(c: Bool, a: Int, b: Int) -> Int {
  return if c {
    let d = int_add(a, a)
    d
  } else {
    b
  }
}
```

A `return` inside a branch is a parse error (``unexpected token `return` ``,
[`ai3_neg_return_in_branch`]). This split between function tails and branch
tails is deliberate. In the pre-language A/B, models wrote `return` at function
tails 72/72 times but in branches only 8/55, so the grammar asks for `return`
only where models already put it.

Other `if` rules:

- **`else` is required.** `if a { Int(1) }` alone is a parse error at the `}`.
- **There is no `else if`.** Nest instead:
  `if a { … } else { if b { … } else { … } }`.
- **The condition must be `Bool`.** Anything else fails with
  `CIfCondNotBool`.
- **Both branches must have the same type.** A parameter in one branch facing
  a literal of the same type is fine. A real mismatch fails, and the commonest
  case is an effect (`Unit`) in one branch facing a value in the other:

  ```
  return if c { fs_write_text(p, s) } else { Text("") }
  → CIfBranchDivergence { node_index: 1, then_type: fb50dc07… (Unit), else_type: ffff3528… (Text) }
  ```

  Fix it by giving both branches the same shape, e.g. the same effect with
  different data. Do not turn one branch into a no-op.
- **`fail(Text)` agrees with any sibling type.** A branch that only aborts
  never produces a value, so it does not diverge:

  ```
  return if c { x } else { fail(Text("negative")) }      // verifies
  ```

---

## 5. Expressions

### 5.1 No bare literals

Every literal is a typed constructor call:

| Type | Literal |
|---|---|
| `Int` | `Int(42)` |
| `Text` | `Text("hello")` |
| `Bool` | `Bool(true)` / `Bool(false)` |
| `Unit` | `Unit()` |

- A bare `42` is a parse error: ``unexpected token `42` ``.
- Integers match `[0-9]+`, so there are no negative and no decimal literals.
  Write `int_sub(Int(0), Int(5))` for −5.
- Text escapes are `\"`, `\\`, `\n` and `\t`. A raw newline inside a string is
  rejected.
- `Bytes` is a valid type but has no literal. `Bytes` values arrive only as
  parameters.

### 5.2 No operators

`+ - * / == < &&` do not exist. `a + Int(1)` fails at the lexer with
`unexpected character '+'`. Arithmetic, comparison and text operations are all
registry calls: `int_add(a, b)`, `int_lt(a, b)`, `str_concat(s, t)`.

### 5.3 No grouping parentheses

`return (a)` is a parse error. Parentheses appear only in calls, literals and
type tags.

### 5.4 Calls and names

Calls are positional for everything: registry functions, your own functions,
higher-order functions. There are no labels, no defaults and no partial
application.

A bare identifier is a parameter or a `let` binding. An unknown name in call
position fails at lowering:

```
lowering error at 0..0: UNKNOWN gate: callable 'int_double' not found in scope
```

To check a name, run `axis symbols <substring> --reg …`.

### 5.5 `bool_and` / `bool_or` short-circuit

`bool_and(a, b)` evaluates `b` only when `a` is true, and `bool_or(a, b)` only when `a`
is false, like `&&` and `||`. So a guard and the operation it protects can share one
condition:

```
if bool_and(int_lt(i, str_len(s)), text_eq(str_char(s, i), Text(" "))) { … } else { … }
```

Here `str_char` never runs with `i` out of range (BOOL_SHORT_CIRCUIT_V1, 2026-10-02; since
Core IR 0.6 the second argument's nodes are a block that runs only when it is needed, §12).
Every other call evaluates all of its arguments first.

### 5.6 `determine`

`determine` is a bare keyword expression that lowers to a `CDeterminate` gate.
It discharges the gate-domination obligation on `effect irreversible` callables.
Write it when you are modelling a gate, never as ceremony.

---

## 6. Types and construction

### 6.1 Type expressions

| Syntax | Meaning |
|---|---|
| `Int` `Text` `Bool` `Unit` `Bytes` | Primitives |
| `IntList` `TextList` `BoolList`, other registry names | Registry types, written as bare identifiers |
| `Value(T1, T2, …)` | Product with positional slots |
| `ValueList(T)` | List |
| `Fn` | A function reference |

**`Value` and `ValueList` are keywords and always need their tag.** A bare
`-> Value` fails to parse, and the error points at the `{` after it, which looks
like a body problem but is not one. Write the tag in full, e.g.
`-> Value(Text, Int)`. `ValueList(Value)` with a bare inner `Value` is also
rejected.

### 6.2 Construction

The tag is mandatory at the construction site, and the tag's arity must match
the number of arguments:

```
fn make(a: Text, b: Int) -> Value(Text, Int) {
  return Value(Text, Int)(a, b)
}
```

`ValueList(Int)[Int(1), Int(2)]` and `ValueList(Int)(Int(1), Int(2))` are
equivalent.

### 6.3 Tags erase

In parameter and return position, `Value(Bool, Bool)` and `Value(Int, Int)` are
the **same** type to the verifier. The element tags are documentation only.

### 6.4 The `Value` dead end, and the typed lists that avoid it

`list_head(ValueList) -> Value`, and nothing converts a `Value` back to `Int`,
`Text` or `Bool`:

```
return int_add(list_head(xs), Int(1))
→ TypeMismatch { node_index: 1, arg_index: 0, expected: 16305a6b… (Int), got: 974cb8d9… (Value) }
```

When the elements all have one primitive type and you need to compute with
them, use the monomorphic families `IntList`, `TextList` and `BoolList`. Their
operations return the element type. You build them with `nil` and `cons`, not
with a literal:

```
fn build() -> IntList {
  return int_list_cons(Int(10), int_list_cons(Int(20), int_list_nil(Unit())))
}
```

`ValueList` and `IntList` are distinct types and do not convert into each
other. `ValueList(Int)[…]` always builds a `ValueList`.

---

## 7. Higher-order functions and callbacks

Pass a callback as a **bare function name** without parentheses. It must be a
function declared in the same file:

```
fn is_pos(x: Int) -> Bool {
  return int_gt(x, Int(0))
}

fn keep(xs: IntList) -> IntList {
  return int_list_filter(xs, is_pos)
}
```

`fold`-style functions take a two-argument step, `(acc, elem)`:

```
fn m_add(acc: Int, elem: Int) -> Int { return int_add(acc, elem) }

fn total(xs: IntList) -> Int {
  return int_list_fold_int(xs, Int(0), m_add)
}
```

**Callbacks are type-checked.** Every `Fn` parameter in the registry carries a
`callback` contract. The verifier resolves the function you passed and checks
its arity, argument types and return type against that contract:

```
fn is_even(x: Int) -> Bool { return Bool(true) }
… text_list_fold_int(xs, Int(0), is_even)
→ CallbackContractViolated { node_index: 0, arg_index: 2,
    reason: "callback arity: contract requires a 2-argument fn, target is_even (26637bc8…) takes 1" }
```

Older notes that say "an `Fn` slot accepts any function" are dead. The one
remaining relaxation is for sum-typed contract inputs (erased polymorphism, as
in `loop_while`). A callback's effect is folded into the call site, so a
higher-order function cannot hide what its callback does.

### 7.1 A callback sees only its arguments: carry context in the accumulator

There are **no closures**. A callback is a bare function name, and the only things it can read
are its own arguments (`acc`, `elem`); it cannot see a `let` of the function that passed it.
The typed folds (`int_list_fold_int`, `text_list_fold_text`, …) have a plain-value accumulator —
one `Int`, one `Text` — so a writer who needs a set, a lookup table or some context inside a
fold has been left with two bad options: **grow a `Text` blob** as the accumulator and test it
with `str_contains` (quadratic, §14, and a substring can match inside a longer key), or **stash
the context in a global named table** (`map_put` / `set_add`: mutable and shared, which is
exactly what the kind 2 rule forbids — see below). There is a third option, and it is the one to use:

- **Typed fold** (`*_list_fold_*`) when the result is one plain value and the callback needs nothing
  but `acc` and `elem`.
- **Generic fold** — `fold(ValueList, Value, Fn) -> Value` — when the callback needs context or a set.
  The accumulator is any `Value`, so build it with `value_make(context…, accumulator)`; the step
  receives it whole and returns the next one. Context that never changes is passed along
  unchanged; a map handle (`hmap_new`, below) is passed along and written in place.
- **`loop_while`** when you need an early exit (`fold` always visits every item), or the loop is
  not over a list. Its state is a `Value` in the same way (§14 gives the cost).

A step has the contract `(Value, Value) -> Value`. Tag the parameters and the return type
(`Value(Text, Text, Int)`); the tag is documentation and is **erased**, so the position that
holds a map handle is written with any one registry type (`Text` here). Convert a `TextList` with
`text_list_to_value_list` first, and read the elements back with `value_to_text`:

```
// accumulator = (context text, map handle, count of distinct items)
fn step(acc: Value(Text, Text, Int), x: Value(Text)) -> Value(Text, Text, Int) {
  let ctx = value_to_text(ctor_field(acc, Int(0)))
  let seen = ctor_field(acc, Int(1))
  let k = value_to_text(x)
  let fresh = bool_not(hmap_has(seen, k))
  let put = if fresh { hmap_put(seen, k, ctx) } else { Bool(false) }
  let n = value_to_int(ctor_field(acc, Int(2)))
  return value_make(ctx, seen, if fresh { int_add(n, Int(1)) } else { n })
}

fn distinct(items: TextList) -> Value(Text, Text, Int) {
  return fold(text_list_to_value_list(items), value_make(Text("ctx-from-the-caller"), hmap_new(Unit()), Int(0)), step)
}

// the result leaves the fold as a value: freeze the map, then read it from anywhere
fn read(r: Value(Text, Text, Int)) -> Text {
  return hmap_get(hmap_freeze(ctor_field(r, Int(1))), Text("0"))
}
→ Ok:ctx-from-the-caller
```

(Compiled and run: `src/r185/fold_bench_main.ai3` in axMachina. The context text and the handle both
survive the round trip through `fold`'s `Value` accumulator.) The `put` line is a discarded effect
inside an arm: since 0.6 it runs only when the arm is taken (§12).

**The map handle (`hmap_*`, kind 2 memory).** `hmap_new(Unit) -> Value` makes a local mutable map of
`Text` keys and `Text` values that belongs to the loop that made it. `hmap_put(m, k, v)` writes it
in place and returns whether `k` was new, `hmap_has(m, k)` is exact-key membership (never a
prefix), `hmap_get(m, k)` is `Ok:<value>` or `Unknown`. The storage is thread-local and goes when the
last copy of the handle drops: there is no free call. `hmap_freeze(m)` moves the contents out
without a copy and kills the handle; the frozen map is immutable, readable from any thread
(`hmap_has`, `hmap_get`), and `hmap_hash` is the only thing that hashes it (for a key). Hard
boundary: **a handle belongs to its loop** — never put it in a spawned task, a channel, a key or a
file (`spawn`, `channel_send` and `bchan_send` refuse it by name, as does every use from
another thread and every use after a freeze); freeze it only if the result leaves the loop as a
value.

Measured, one thread, 0.86 load average at every run (`src/r185/fold_bench_main.ai3`, items built
outside the timed part, the same set of `n` distinct keys each time):

| set held as | 10k | 20k | 40k | 80k | exponent 10k→20k, 20k→40k, 40k→80k | larger |
|---|---|---|---|---|---|---|
| `fold`, handle in the accumulator | 5 ms | 11 ms | 23 ms | 45 ms | 1.1, 1.1, 1.0 | 320k: 189 ms (linear) |
| `loop_while`, handle in the state | 9 ms | 19 ms | 38 ms | 78 ms | 1.1, 1.0, 1.0 | 320k: 313 ms (linear) |
| `text_list_fold_text`, `Text` blob + `str_contains` | 84 ms | 181 ms | 1,050 ms | 3,400 ms | 1.1, 2.5, 1.7 | quadratic; 100k keys of 64 bytes took 129 s |

**What not to do.** A `Text` blob as a set: quadratic, and `str_contains` matches a key inside a
longer key unless you delimit both sides. A global table as context (`map_put`, `set_add`, a
name chosen by the caller): it is mutable state shared by everything in the process, it survives
the run, and two callers who pick the same name interfere. If the data must outlive the loop, it
is not kind 2: freeze and publish it, or key it by content hash — do not widen a handle.

**Reading fields back.** `ctor_field(acc, Int(k))` reads field `k` of a `value_make` accumulator at
any index. `value_0` / `value_1` / `value_2` read all three product families but stop at index 2.
**An out-of-range index returns `Unit` silently**, and the process dies later at whatever narrows
it (`value_to_text: expected Text, got Unit`), pointing at the reader, not at the bad index; see the
working-repo `CLAUDE.md` note on `value_N`. Count the fields you build.

---

## 8. What the verifier checks — and what it does not

**Checked:** arity, argument types (parameters resolved to their declared
types), `if` condition is `Bool`, `if` branch agreement, callback contracts, and
gate domination for `irreversible` effects.

**Not checked: the declared return type, and so not the `return` expression
either.** `return` is mandatory but not type-checked:

```
fn f(a: Bool) -> Int {
  return a
}
→ compiles and verifies, identity 9efc4a3d…
```

That id is exactly the id of `ai3_tail_var.ai3`, which correctly declares
`-> Bool`. The declared return type does not even reach the bundle. Write the
correct type anyway, and do not count on the compiler to catch a wrong one.

**Not checked either:**

- Compound tag shapes (§6.3).
- Whether a registry function is actually implemented in the bridge. Declared
  is not implemented. See `M1_REGISTRY_CATALOGUE.md`.
- Runtime failure. Bridge functions panic rather than return an error:
  `fs_read_text` on a missing file aborts the process. Guard it:
  `if fs_file_exists(p) { fs_read_text(p) } else { Text("") }`.

**Effects in a `let` whose value is never used.** Written inside an `if` branch, the
effect runs only when that branch is taken. Written above the `if`, it runs on every call,
whatever the `if` decides. (Before Core IR 0.6, 2026-10-13, this was "hoisted": a discarded
effect was lifted to the top level wherever it was written, and `seq` was the way to pin it
into a branch. Nodes now run where they are written; see §12.)

---

## 9. Reading a failure

Failures are labelled by stage:

| Message | Stage | Usual cause in AI3 |
|---|---|---|
| `lex error at N..M: unexpected character 'X'` | lex | An operator (`+`, `<`) or a `#` comment |
| ``parse error at N..M: unexpected token `X`, parsed up to here then could not continue`` | parse | See below |
| `NF validation error …` | normalize | `Value` construction arity or a literal in the wrong slot |
| `lowering error: UNKNOWN gate: callable 'x' not found in scope` | lower | Typo, a function in no registry file, or the small registry set (§2.4) |
| `Error: Failed to load registry files: Path does not exist: …` | registry load | A `--reg` path is wrong, or you never copied the registry into your project (§2.3) |
| `axis build: … not found at … (set AXIS_…)` | setup | `axis build` can't find the grammar, registries or bridge: run `install.sh` (§2.1), or in a sandbox use `scripts/axis` (§2.3) |
| `lowering error: duplicate function 'g'` | lower | Two `fn g` in one file |
| `TypeMismatch { … expected, got }` | verify | Argument type wrong. `got: f9535076…` means "a parameter": read its declared type |
| `ArityMismatch` | verify | Wrong argument count, or construction under `registry-pin` alone |
| `CIfCondNotBool` / `CIfBranchDivergence` | verify | §4 |
| `CallbackContractViolated` | verify | §7 |

**The parse error names the offending token, and in AI3 that token tells you
which rule you broke:**

| Token named | You probably |
|---|---|
| `Int`, `Text`, an identifier or a call at the start of the last line of a body | forgot the `return` |
| `return` inside `{ … } else { … }` | put a `return` in a branch |
| `return` after another `return` | wrote two returns |
| `let` after a `return` | put a statement after the tail |
| `}` right after `{` | wrote an empty body |
| `;` | wrote `;;`, a bare `return;`, or `;` after a `}` |
| `{` after a return type | used a bare `Value` / `ValueList` as the type |
| `if` after `else` | wrote `else if` |
| `(` after `return` | used grouping parentheses |

Type hashes you will see (canonical set): `Int 16305a6b`, `Bool 85f90dfe`,
`Text ffff3528`, `Unit fb50dc07`, `Value 974cb8d9`, `Param f9535076`
(the sentinel for a parameter, not a type). Resolve any other hash with
`axis types <64-hex> --reg …`.

---

## 10. Author's checklist

Before you submit AI3 source, check each of these:

0. The toolchain is installed (§2.1), or vendored into your project with
   `scripts/axis` if you work in a sandbox (§2.3).
1. Every `fn` body ends in exactly one `return <expr>`, and nothing follows it.
2. No `return` appears inside an `if` branch. The `return` wraps the whole `if`.
3. Every `if` has an `else`, and there is no `else if`.
4. Every literal is wrapped: `Int(…)`, `Text(…)`, `Bool(…)`, `Unit()`.
5. There are no operators and no grouping parentheses.
6. There is at most one `;` per statement, and never after `}`.
7. Every called name exists (`axis symbols`) or is a function in this file.
8. Callbacks are bare names whose signature matches the contract.
9. Every `Value` / `ValueList` type carries its tag.
10. When you need arithmetic on elements, use `IntList`/`TextList`/`BoolList`,
    not `ValueList`.
11. Building an executable (§2.2): the whole program is in one file, the
    entry is the **last** function, and it takes one `args: ValueList(Text)`.
    Run it with `axis run`; a non-zero exit is a failure (§2.2 lists the codes).

---

## 11. Further reading

| Document | For |
|---|---|
| `axis-surface-ai3-config/ai3-parse.yaml` | The grammar. Every AI3 difference from M1 is commented inline |
| `examples/surface-ai3/BASELINE.md` | Per-fixture baseline, byte-identity proof, design decisions |
| `examples/surface-ai3/*.ai3` | Worked positive and negative examples |
| `docs/M1_AUTHORING_PRIMER.md` | Registry grammar (`.axreg`), adding functions, the dedup gate, the product-family accessor trap |
| `CLAUDE.md` | Callback contracts and the duplicate-fn rule, in detail |

---

## 12. Where code runs (Core IR 0.6)

Since Core IR 0.6 (axm-req-r179, 2026-10-13) **a node runs where it is written, in the order
it is written.** Every program below was run with `axis run` on the 0.6 toolchain
(`prog` returns the text shown; `f/` is a scratch directory).

**A `let` above an `if` runs every time**, even when the `if` never uses it:

```
fn prog(args: ValueList(Text)) -> Text {
  let w = fs_write_text(Text("f/above"), Text("x"))
  return if bool_not(Bool(true)) { Text("taken") } else { Text("not taken") }
}
→ not taken          (f/above exists)
```

**Anything written inside a branch runs only when that branch is taken**, so a branch is a
real guard for an effect:

```
  return if bool_not(Bool(true)) { let w = fs_write_text(Text("f/inside"), Text("x"))
    Text("taken") } else { Text("not taken") }
→ not taken          (f/inside does not exist)
```

and for a call that can fail. The old trap and its 0.6 behaviour, side by side:

```
// the call above the if                          // the call inside the branch
let n = str_to_int(Text("x"))                     return if bool_not(Bool(true)) { int_to_str(str_to_int(Text("x"))) }
return if bool_not(Bool(true)) { int_to_str(n) }         else { Text("not taken") }
       else { Text("not taken") }
→ axis: DEFECT in str_to_int: str_to_int: not an   → not taken
  integer: "x" (at src/runtime/arith.rs:204)
```

Before 0.6 both forms behaved like the left one: a call in an untaken arm could run
(`list_head` on an empty list panicked in an `if`-guarded arm, 2026-09-25), and a discarded
effect was hoisted to the top level and ran whichever arm was taken (2026-09-22). Those
observations are from the 0.5 toolchain and were not re-run. **Put a guarded effect or a
fallible call inside the branch; a `let` above the `if` is not guarded.**

**`bool_and` / `bool_or` lower so that the second argument runs only when it is needed**
(§5.5). The second argument's nodes sit in their own block:

```
let a = bool_and(Bool(false), int_lt(Int(0), str_to_int(Text("x"))))
let b = bool_or(Bool(true),  int_lt(Int(0), str_to_int(Text("x"))))
return if bool_and(a, b) { Text("both") } else { Text("short-circuited: no panic") }
→ short-circuited: no panic

let a = bool_and(Bool(true), int_lt(Int(0), str_to_int(Text("x"))))      // needed: runs
→ axis: DEFECT in str_to_int: str_to_int: not an integer: "x"
```

An effect in the second argument follows the same rule
(`bool_and(Bool(false), seq(fs_write_text(…), Bool(true)))` writes nothing;
with `Bool(true)` first it writes). An effect bound by a `let` and used **only** in a condition
now runs like any other `let`: `let c = seq(fs_write_text(…), Bool(true))` above
`if bool_and(Bool(false), c)` wrote its file. `seq` still takes exactly two arguments
(`seq(a, b, c)` → `ArityMismatch { expected: 2, got: 3 }`); nest it.

---

## 13. Building text

A long `str_concat` nest is the commonest way to fail to compile: in one measured session
(208 compile errors) **125 were an unbalanced bracket in a nested `str_concat`.** Rules:

- Build with **staged `let`s**; nest `str_concat` at most two levels.
- Join a list with the existing join, `str_join(text_list_to_value_list(xs), sep)`; never
  grow a `Text` in a loop (§14).

One nest and its staged rewrite, both compiled and run:

```
return str_concat(Text("["), str_concat(Text("id="), str_concat(int_to_str(Int(7)), str_concat(Text(" name="), str_concat(Text("drag"), str_concat(Text(" ok="), str_concat(Text("yes"), Text("]"))))))))
→ [id=7 name=drag ok=yes]

let id = str_concat(Text("[id="), int_to_str(Int(7)))
let name = str_concat(Text(" name="), Text("drag"))
let ok = str_concat(Text(" ok="), Text("yes]"))
let head = str_concat(id, name)
return str_concat(head, ok)
→ [id=7 name=drag ok=yes]
```

Both print the same line. With one bracket dropped from the nest the compiler says

```
Compilation failed for b3.ai3: parse error at 244..245: unexpected token `}`, parsed up to here then could not continue — `(` at 60..61 is closed by `}` at 244..245
```

which names the opening bracket, 184 characters away from where it was noticed. In the staged
form every line is balanced on its own. Join:

```
let xs = text_list_cons(Text("a"), text_list_cons(Text("b"), text_list_cons(Text("c"), text_list_nil(Unit()))))
return str_join(text_list_to_value_list(xs), Text(", "))
→ a, b, c
```

---

## 14. Loop state cost (`loop_while`)

`loop_while(init, more, step, fuel)` hands the state `Value` to both callbacks every
iteration. Measured with a microbenchmark (one `x` added per step; `axis build`, wall time
of the binary, 0.004 s start-up subtracted; one thread; load average read from `/proc/loadavg`
before the first run and after each run: 0.71 before; 0.73 for all `Text` runs; 0.73–0.83 for
`TextList`; 0.83 for `Bytes`; then 0.70–0.73 for the 40k–160k `Text` runs and 0.62 for the
640k `Bytes` runs; never above 0.83, so none of the timings is load noise):

| state | 10k | 20k | 40k | exponent 10k→20k, 20k→40k | larger |
|---|---|---|---|---|---|
| `Text`, `str_concat(acc, "x")`, condition reads the counter | 0.010 s | 0.019 s | 0.055 s | 1.3, 1.8 | 160k: 1.316 s → 2.3 (40k→160k) |
| `Text`, condition reads `str_len(acc)` | 0.012 s | 0.029 s | 0.096 s | 1.6, 1.9 | 160k: 1.959 s → 2.2 |
| `TextList`, `text_list_cons("x", acc)` | 0.325 s | 1.532 s | 6.648 s | 2.2, 2.1 | |
| `Bytes`, `bytes_concat(acc, "x")`, small state | 0.008 s | 0.011 s | 0.019 s | 0.8, 1.1 | 640k: 0.245 s (linear) |
| `Bytes`, condition reads `bytes_len(acc)` | 0.008 s | 0.012 s | 0.020 s | 1.0, 1.0 | 640k: 0.258 s (linear) |

A growing `Text` or `TextList` in the state is **quadratic** (the `Text` copy is a cheap
`memcpy`, so it looks linear below ~40k steps and costs 1.3 s at 160k; the `TextList` is 100×
worse already at 10k). `Bytes` appended through `value_make` / `ctor_field` stays **linear**,
and reading its length in the condition does not make the step copy it; reading a growing `Text`
in the condition adds a constant factor (about 1.7× at 40k). Rules that follow:

1. Keep the loop state small: counters, cursors, and at most one accumulator.
2. Let the condition read only a small cursor (an `Int`), not the accumulator.
3. Accumulate into `Bytes` (`bytes_concat`, `text_to_bytes`, `bytes_to_text` at the end) or
   write each piece to a file with `fs_append_text`; never into a `Text` or `TextList`.
4. Do not rely on a library join written as a loop with a `Text` accumulator for big lists
   (`am_lines_join` in axMachina is one); it is quadratic in the joined length.
