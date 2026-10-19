# PyAx Primer — Python syntax, Axis rules

**Scope:** everything an author (human or model) needs to write PyAx that
compiles and runs. If you know Python, you already know most of PyAx: this
primer is mostly about what you **cannot** write, and what to write instead.

**Status:** written 2026-10-03 against axis-lang `da1ca18` plus the
changes that ship with it: the `axis run` argument fix, `.pop()` and
`zip` / `enumerate` in expressions, `dict(...)`, registry typing, and clearer
errors for loop- and branch-local names and mixed-type ternaries. Every example and every
error message below came from running `axis run` on that exact source, and
every runnable example printed the same output under CPython 3.

---

## 0. Quick start

Install once (AI3 primer §2.1, or §2.3 inside a sandbox):

```bash
git clone https://github.com/christaylor98/axis-lang ~/dev/axis-lang
~/dev/axis-lang/scripts/install.sh
```

Then:

```python
# hello.pyax
def greet(name: str) -> str:
    return f"hello, {name}"

def main(args: list[str]) -> None:
    for a in args:
        print(greet(a))
```

```
$ axis run hello.pyax world -v
hello, world
hello, -v
$ axis build hello.pyax -o hello && ./hello there
hello.pyax → hello (pyax, entry `pyax_main`)
hello, there
```

`axis run` builds in a temp dir, runs the program and exits with its exit code.
`axis build` writes the executable. The `.pyax` extension selects PyAx. Options
for `axis` (`--keep <dir>`, `--surface pyax`) go **before** the source file;
everything after it goes to the program. **In a sandbox**, vendor the toolchain
as AI3 primer §2.3 describes and also copy `axis-surface-pyax-config/` into
`axis-toolchain/`. Then `scripts/axis run hello.pyax` works the same way.

---

## 1. What PyAx is

PyAx is Python-*like*: a statically typed subset of Python 3 that compiles to
Axis Core IR and then to a native executable. A PyAx program that compiles is
valid Python and prints the same thing when CPython runs it. That claim was
tested against 133 task programs and 9 whole programs, with **0 differences**
(`PYAX_SURFACE_EXPAND_V1.report.md`, phase 3).

What PyAx leaves out is whatever would stop a program from having one static
type per value, or what models rarely get right: `break`, `continue`,
`lambda`, nested `def`, classes, exceptions you can catch. Each one is
rejected with a message that says what to write instead (§9).

PyAx is a normal Axis surface: a grammar and an expand spec in
`axis-surface-pyax-config/`, plus a prelude written in PyAx
(`pyax-prelude.pyax`). Loops become `fold` / `loop_while` calls, and every
operator becomes a typed registry call. The verifier checks the result like
any other Axis program.

---

## 2. Program shape

A file holds top-level `def`s, plus optionally constants, docstrings and imports.

```python
"""Count words, most frequent first."""

def count_words(text: str) -> dict[str, int]:
    counts = {}
    for w in text.lower().split():
        counts[w] = counts.get(w, 0) + 1
    return counts

def main(args: list[str]) -> None:
    counts = count_words(" ".join(args))
    ranked = sorted([(-n, w) for w, n in counts.items()])
    for n, w in ranked:
        print(f"{w}: {-n}")
```

```
$ axis run words.pyax the cat The dog the
the: 3
cat: 1
dog: 1
```

- **The entry is `main`.** Write it as `def main(args: list[str]) -> None` (the
  arguments, without the program name) or `def main() -> None`. If `main`
  returns a value, that value is printed when it ends: `def main() -> int:
  return 42` prints `42`.
- **Every parameter and every return type is annotated.** `def f(x):` is
  a parse error at the `)`.
- Functions may call each other in any order and may recurse.
- **Default values and keyword arguments work** on your own functions:
  `def pad(s: str, width: int = 6, fill: str = ".")`, called as
  `pad("ab", fill="*")`.
- **Module-level constants** (`LIMIT = 3`) are allowed and are inlined where
  they are read. They can't be rebound: `global` is a parse error.
- `import math`, `from decimal import Decimal` and `from typing import ...` are
  accepted. No other import exists.
- A function declared to return a value must return one on every path:

  ```
  def f(x: int) -> int:
      if x > 0:
          return 1
  ```
  ```
  noret.pyax:1:5: expand error at 4..54: this function can reach its end without returning a value (it is declared to return int); add a `return` or an `else`
  ```

---

## 3. Types

| Type | Values |
|---|---|
| `int` | 64-bit integers |
| `float` | `1.5`, `2e-3`, `float("inf")` |
| `Decimal` | `Decimal("19.99")` |
| `str` | `"text"` or `'text'`; `"""docstrings"""` |
| `bool` | `True`, `False` |
| `None` | the return type of a function with no result |
| `list[T]` | lists of one element type, nested ok: `list[list[int]]` |
| `tuple[A, B, ...]` | fixed-size products: `(1, "one")` |
| `dict[K, V]` | insertion-ordered maps |

`List`, `Dict` and `Tuple` from `typing` are accepted as the same types.

**A variable keeps one type for its whole life:**

```
tchg.pyax:3:5: expand error at 34..35: `x` holds int here, but this assigns str; a variable keeps one type — use a new name
```

**A list holds one element type:** `[1, "a"]` is rejected with
`list elements must all be int; this one is str`.

**Local variables need no annotation**; the type follows from what you assign.
An empty `[]` or `{}` takes its type from the first thing stored in it, even
when that store comes later in the function. Annotate (`out: list[int] = []`)
only when the compiler asks, and it names the line when it does.

**There is no `set`:** use a dict as a set (`seen[x] = True`, `x in seen`).

---

## 4. Updating lists and dicts

The usual updates work: `xs.append(v)`, `xs.extend(ys)`, `xs[i] = v`,
`xs += ys`, `xs.insert(i, v)`, `xs.pop()`, `xs.pop(i)`, `xs.sort()`,
`xs.reverse()`, `d[k] = v`, `d[k] += v`, `del d[k]`, `d.update(e)`.

```python
def evens_squared(n: int) -> list[int]:
    out = []
    for i in range(n):
        if i % 2 == 0:
            out.append(i * i)
    return out
```

Underneath, every value is immutable: an update builds a new value and
rebinds the name. In Python, though, an update changes the value everywhere it
is shared. So PyAx refuses any update whose Python meaning would reach another
name. There are three rules, each with its fix:

```
xs.append(1)          # xs is a parameter
r1.pyax:2:5: expand error at 45..47: `xs` is a parameter: updating it would change the caller's value in Python; update a copy under a new name instead, e.g. `mine = xs.copy()`

ys = xs; ys.append(3)
r2.pyax:4:5: expand error at 52..54: `ys` may share its value with `xs`, so in Python this update would change both; make it a copy: `... = xs.copy()`

for x in xs: xs.append(x)
r3.pyax:4:9: expand error at 61..63: `xs` is updated inside a loop over `xs`; loop over a copy: `for x in xs.copy():`
```

**`.pop()` works inside an expression** (`print(xs.pop())`,
`if stack.pop() > 2:`, `total = 100 - ys.pop()`) whenever the pop runs exactly
once, before the rest of the statement. Two cases are refused, because moving
the pop first would change the answer:

- the same list is also used elsewhere in the statement
  (`print(len(xs), xs.pop())`: Python reads `len(xs)` before the pop);
- the pop only runs on some paths: inside `… if … else …`, after `and` / `or`,
  in a `while` condition, or in a comprehension's body.

```
this `.pop()` can't be part of a larger expression here: the list is also used elsewhere in the statement, or the pop only runs on some paths (…); write `v = xs.pop()` on its own line first
```

---

## 5. Control flow

`if` / `elif` / `else`, `for`, `while`, comprehensions and `return` work as in
Python, including `return` from inside a loop:

```python
def first_negative(xs: list[int]) -> int:
    for i, x in enumerate(xs):
        if x < 0:
            return i
    return -1
```

- `for` iterates a list, a string (by character), `range(...)`,
  `enumerate(xs)` / `enumerate(xs, start)`, `zip(a, b)`, `d`, `d.keys()`,
  `d.values()` and `d.items()`. Arguments of `zip` and `enumerate` can be any
  expression: `for a, b in zip(xs, xs[1:]):`,
  `[(i, k) for i, k in enumerate(sorted(d))]`. One exception: in a
  comprehension inside a larger expression, an argument that calls one of
  your own functions (or `print`, or the registry) must be named first. Binding
  it for the loop would run that call earlier than Python does.
- `while` takes any condition, truthiness included (`while queue:`). A `while`
  loop that passes 100,000,000 iterations stops the program (exit 3) rather
  than running forever.
- **No `break` and no `continue`:**

  ```
  brk.pyax:4:13: expand error at 75..80: `break` is not in PyAx: put the loop in its own function and `return` from it
  ```
  For `continue`, the message says: put the rest of the loop body under an
  `if`. A loop `else:` is a parse error.

- **A variable used after a loop must be assigned before the loop.** One
  first assigned inside the body does not exist after it, and neither does
  the loop variable. The error says which case you hit:

  ```python
  for x in [1, 2, 3]:
      last = x
  print(last + 1)
  ```
  ```
  loop.pyax:4:11: expand error at 71..75: `last` is first assigned inside a loop, and in PyAx it does not exist after the loop; assign it before the loop (e.g. `last = 0`) so the loop updates it
  ```
  For `print(x)` after `for x in …`: `` `x` is a loop variable, and in PyAx it does not exist after its loop ``.

- **After an `if`**, a variable is visible if every branch assigns it
  (`if … s = "neg" elif … s = "zero" else: s = "pos"`, then `return s`), or
  every branch that doesn't assign it returns. Otherwise:
  `` `s` is assigned in only some branches of an `if` above; assign it before the `if`, or in every branch ``.

---

## 6. Expressions

- **Arithmetic:** `+ - * / // % **` and unary `-`. `/` is true division
  (`7 / 2` is `3.5`). `//` and `%` floor as Python does (`-7 // 2` is `-4`, and
  `-7 % 3` is `2`). An int combined with a float is promoted to float.
- **Comparison:** `== != < <= > >=`, chained (`0 <= i < n`), `in` and
  `not in` on lists, strings and dicts. `==` works on lists and tuples too.
- **Truthiness** follows Python: `if xs:`, `while queue:`, `if not name:`.
  `a or b` as a value works when both sides have one type (`"" or "anon"` is
  `"anon"`). With mixed types it is refused, since it has no single type:
  `` `or` between int and str has no single type as a value; use it in a condition, or compare explicitly ``.
- `x if cond else y` needs **one type on both sides**, int beside float
  included (in Python the result has the type of whichever side ran):
  `` both sides of `... if ... else ...` must have one type; this one is int on the left and str on the right ``.
- f-strings `f"{name} has {n}"` with plain `{expr}` fields,
  and `str * int`. **No format specs:** `f"{x:.2f}"` is rejected with
  ``format specs (`{x:...}`) are not in this language; format the value first, e.g. `str(x)` ``.
  Use `round(x, 2)`.
- **Indexing and slicing:** `xs[i]` with negative indices, and any slice, step
  included: `xs[1:4]`, `xs[-2:]`, `xs[::2]`, `xs[::-1]`, `s[:5]`.
- **Comprehensions:** list and dict comprehensions with one `for` and an
  optional `if`. A generator works as the argument of `sum`, `any`, `all`,
  `min` and `max`: `sum(x for x in xs)`.
- **`print`** takes any number of values, plus `sep=` and `end=`. Containers
  print as Python's repr: `[(1, 'one'), (2, 'two')]`, `{1: 'one'}`,
  `Decimal('1.10')`.

```python
from decimal import Decimal
import math

def stats(xs: list[float]) -> tuple[float, float]:
    mean = sum(xs) / len(xs)
    var = sum([(x - mean) ** 2 for x in xs]) / len(xs)
    return mean, math.sqrt(var)

def main() -> None:
    m, sd = stats([2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0])
    print(m, sd)                                    # 5.0 2.0
    print(7 / 2, 7 // 2, -7 // 2, -7 % 3)           # 3.5 3 -4 2
    price = Decimal("19.99")
    print(price * 3, round(2.5), round(3.14159, 2)) # 59.97 2 3.14
    print(str(1e20), 0.1 + 0.2)                     # 1e+20 0.30000000000000004
```

---

## 7. Built-ins and methods

These are the builtins. You can also call your own functions and any registry
function (§10):

| Group | Names |
|---|---|
| numbers | `abs`, `min`, `max`, `sum`, `round`, `divmod`, `int`, `float`, `Decimal`, `bool`, `**` |
| text | `str`, `len`, `ord`, `chr` |
| sequences | `len`, `sorted` (also `reverse=True`), `reversed`, `range`, `enumerate`, `zip`, `any`, `all`, `list` |
| dicts | `dict()`, `dict(d)` (a copy), `dict(pairs)` from `(key, value)` tuples, `dict(zip(keys, values))` |
| output | `print` (`sep=`, `end=`) |
| `math` | `math.sqrt`, `math.floor`, `math.ceil`, `math.isclose`, `math.pi`, `math.e`, `math.inf` |

**`str` methods:** `upper lower capitalize strip lstrip rstrip split
splitlines join replace find rfind index count startswith endswith isdigit
isalpha isalnum islower isupper isspace ljust rjust zfill`.

**`list` methods:** `append extend insert pop sort reverse copy count index`
(updates follow §4).

**`dict` methods:** `get` (with a default), `keys`, `values`, `items`,
`update`, plus `|` and `|=` and `{**d, k: v}`.

`sorted` works on lists of tuples (lexicographic and stable), but not on
tuples that contain floats. `int(" 42 ")` strips whitespace as Python does.

---

## 8. Errors and exit codes

`raise SomeError("message")` and a failed `assert cond, "message"` stop the
program with that message. Nothing can catch them: `try` is a parse error.

```python
def parse_age(s: str) -> int:
    n = int(s)
    if n < 0:
        raise ValueError(f"age must be >= 0, got {n}")
    return n
```

```
$ axis run age.pyax -3
axis: fail: ValueError: age must be >= 0, got -3 (at …)
$ echo $?
3
```

| Exit | When |
|---|---|
| 0 | ok |
| 3 | `raise`, a failed `assert`, a missing dict key (`KeyError: key not in dict`), division or modulo by zero (`ZeroDivisionError`, int or float), a `while` loop past 100,000,000 iterations |
| 4 | a **defect**, i.e. a bug in the program: an index out of range, an int overflow |
| 1 | the program failed to compile, verify or link (the reason is on stderr), or anything else |

Compile errors give `file:line:col` and say what to change. Errors from the
verifier name the function but not the line (Core IR carries no positions). An
error inside a loop body says `in a loop body in \`name\``.

---

## 9. Not in PyAx

| You wrote | What happens / what to write |
|---|---|
| `break` | "put the loop in its own function and `return` from it" |
| `continue` | "put the rest of the loop body under an `if`" |
| `for … else:` | parse error; track a flag instead |
| `lambda`, `key=lambda …` | parse error at `lambda`; sort a list of tuples whose first field is the key: `sorted([(-n, w) for w, n in d.items()])` |
| a nested `def` | "move it to the top level and pass what it uses as arguments" |
| `class` | parse error |
| `try` / `except` / `with` | parse error |
| `global` / `nonlocal` | parse error; pass the value in and return the new one |
| `map(...)` / `filter(...)` | "write a for loop or a list comprehension" |
| `set()` | "use a dict as a set" |
| `isinstance(...)` | "every value has one static type" |
| `input()` / `open()` | not in PyAx; take values as arguments of `main` |
| `dict(a=1)` | keyword form: write `{"a": 1}` |
| `f"{x:.2f}"` | "format the value first, e.g. `str(x)`" |
| `print(len(xs), xs.pop())` | the pop can't move first (§4): `v = xs.pop()` on its own line |
| a name defined nowhere | "is not defined: it is not a function in this file, a PyAx builtin, or a registry function" |
| an unannotated parameter | parse error at the `)` |

---

## 10. Calling registry functions

Any function in the Axis registry can be called by name, like one of your own,
and is typed from its registry signature, as on every other surface. (The
program is then no longer something CPython can run.)

```python
def add(a: int, b: int) -> int:
    return a + b

def main() -> None:
    xs = [1, 2, 3]
    print(int_list_fold_int(xs, 0, add))          # 6
    words = str_split("a b c", " ")
    print(words, len(words), list_len(xs) + 1)    # ['a', 'b', 'c'] 3 4
```

- **Results have their declared type**, so they need no annotation. Results
  declared `Value` or `ValueList` are the exception: say what they hold,
  `x: int = ctor_field(v, 0)`.
- **Lists convert by themselves.** A `list[int]` / `list[str]` / `list[bool]`
  passed where the registry wants `IntList` / `TextList` / `BoolList` is
  converted, and such a result comes back as a `list[...]`.
- **Arity and visible argument types are checked at your line:**
  `` argument 1 of `int_list_fold_int()` must be list[int]; this is list[str] ``,
  `` `int_list_fold_int()` takes 3 argument(s), (list[int], int, Fn) -> int; this call passes 2 ``.
  The verifier checks the rest.
- A builtin wins over a registry function of the same name. Write
  `axis.range(...)` to call the registry one. List the registry with
  `axis symbols --reg …`.

### 10.1 Making your own function a registry function

A PyAx function can itself be published as a registry function, so another
program (or another file) calls it by name and the verifier checks it there.
It becomes a `kind composite` entry whose `body` is the compiled function.
**`axis build` / `axis run` can't do this**: they take no `--reg` or
`--body-dir`, so you compile and link with the long-form commands.

Setup, once per shell (`H` is this repo, `AXR` the registry checkout; see the
AI3 primer §2.3 for a sandbox):

```bash
H=~/dev/axis-lang; C=$H/axis-surface-pyax-config; AXR=~/dev/axRegistry
BRIDGE=~/dev/axis-codegen-bridge-rs/target/release/axis-codegen-bridge
REGS="--reg $AXR/axis-types.axreg --reg $AXR/axis.axreg --reg $AXR/axis-bridge.axreg \
      --reg $AXR/m1-builtins.axreg --reg $AXR/gen-working.axreg --reg $C/pyax.axreg"
SPECS="--lexer $C/pyax-lexer.yaml --parser $C/pyax-parse.yaml --schema $C/pyax-ast.yaml \
       --normalize $C/pyax-normalize.yaml --expand $C/pyax-expand.yaml"
```

**1. Compile the function on its own.** Put it in a file with no `main`:

```python
# applib.pyax
def apply(state: tuple[str, list[tuple[str, str]]], issuer: str, ev: str) -> tuple[tuple[str, list[tuple[str, str]]], str]:
    name, rows = state
    out = rows.copy()
    out.append((issuer, ev))
    return ((name, out), "ok")
```

```bash
mkdir -p lib && axis --file applib.pyax $SPECS $REGS --out lib/ | grep /apply.coreir
# applib.pyax → lib/apply.coreir (identity: 98a04643…d3a57)
```

That `identity` is the bundle's: it goes in `body`. `lib/` also holds the
function's loop helpers (`__apply_loop2.coreir`) and the prelude; keep them.

**2. Declare it** in a project-local registry:

```
registry myproject-local 0.5

fn apply
  identity 0x<a fresh random 256-bit value: openssl rand -hex 32>
  kind composite
  in (Value, Text, Text)
  out Value
  body 0x<the bundle identity from step 1>
end
```

- **`identity` is minted once, at random** (`openssl rand -hex 32`), and
  never changes, even when the code does. Don't hash the name: two functions
  called `apply` in different projects would then share an identity. (Bridges
  built before 2026-10-04 link only a name-hash identity.)
- **Types:** `int` → `Int`, `str` → `Text`, `bool` → `Bool`, `float` →
  `Float`; any `tuple[...]` → `Value`; `list[int]` / `list[str]` /
  `list[bool]` → `IntList` / `TextList` / `BoolList` (converted for you, §10);
  any other list → `ValueList`; `None` → `Unit`.
- **Get `out` right yourself.** `in` is checked against each call (a `Text`
  where the caller passes a tuple is rejected at the call), but nothing checks
  `out` against the body: declaring `out Int` for the function above still
  compiles.
- **Recompiling the function changes its bundle identity.** Update `body`
  every time, or callers fail:
  `` CompositeEffectUnderivable { … reason: "body bundle 98a04643… not in store" } ``.
- A recursive function can't be `composite`
  (`cycle detected in composite body graph`); declare it `leaf` with its effect
  stated (AI3 primer §2.5). A name already in the registry can't be reused:
  check with `axis symbols apply $REGS`.

**3. Call it.** A `Value` result needs its type stated (§10):

```python
# caller.pyax
def main() -> None:
    st: tuple[str, list[tuple[str, str]]] = ("w", [])
    r: tuple[tuple[str, list[tuple[str, str]]], str] = apply(st, "host-A", "node+|x")
    st2, msg = r
    print(st2, msg)        # ('w', [('host-A', 'node+|x')]) ok
```

```bash
mkdir -p main exe
axis --file caller.pyax $SPECS $REGS --reg local.axreg --body-dir lib --out main/
$BRIDGE build main/pyax_main.coreir --out exe/caller --exe \
    --lib-dir main --lib-dir lib $REGS --reg local.axreg --dispatch $C/pyax-dispatch.toml
./exe/caller
```

The compile needs `--reg local.axreg --body-dir lib`; without `--body-dir` it
fails `CompositeEffectUnderivable`. The link needs both `--lib-dir lib` and
`--reg local.axreg`: the bridge follows `identity → body` in the registry to
the bundle in `lib/` with that content hash. Without `--reg local.axreg`:
`` UNKNOWN_GATE: identity 84bd3f00… — not a bridge built-in, not a §5b identity ``.
Without `--lib-dir lib`:
`` registry name 'apply' … has no bridge implementation and no xbundle provider ``.

Tested 2026-10-04 at `1164b35` with a random identity, including a body with
a `for` loop and an early `return`.

---

## 11. Known limits

- **Building a list in a loop is quadratic.** Every update copies the list, so
  appending 10,000 items takes about 3 s and 20,000 about 12 s. Summing over
  `range(10**6)` takes 0.2 s. Loops themselves are linear and don't grow the
  stack.
- **Dict lookup is linear** in the number of keys.
- **`Decimal` division** gives 16 significant digits, where Python gives 28
  (`Decimal(1) / Decimal(3)`).
- **A list passed into your own function** is not element-type-checked at the
  call. A mismatch faults (exit 4) at the first element read.
- Every program also compiles the PyAx prelude, so `--keep` shows a couple of
  hundred bundles for a small program. The build still takes about a second.

---

## 12. Checklist

1. `def main(args: list[str]) -> None` (or `def main() -> None`) is the entry.
2. Every parameter and return type is annotated; nothing is nested; there are
   no classes.
3. Each variable keeps one type, and each list holds one element type.
4. Variables used after a loop are assigned before it.
5. No `break` / `continue` / `lambda` / `try` / `global`.
6. Never update a parameter, a list that shares its value with another name
   (`ys = xs`), or a list you are looping over. Copy it first.
7. f-strings have no format specs; use `round` / `str`.
8. Run it: `axis run prog.pyax args…`. Exit 0 is success; any other code is a
   failure (§8).

---

## 13. Further reading

| Document | For |
|---|---|
| `docs/AI3_AUTHORING_PRIMER.md` §2 | Install, sandbox vendoring, `axis build` options |
| `PYAX_SURFACE_EXPAND_V1.report.md` | Every design decision, and the CPython differential |
| `axis-surface-pyax-config/pyax-expand.yaml` | The full operator, builtin and method tables |
| `axis-surface-pyax-config/pyax-prelude.pyax` | Python semantics written in PyAx (floor division, repr, sorting, dicts) |
| `tests/fixtures/pyax/*.pyax` | Runnable examples with expected output, and rejected ones with expected errors |
