# Core IR 0.6 — Draft for review

**Status:** draft for review (axm-req-r179, amendment of 2026-10-13: bare blocks).
No code implements it yet. Supersedes Core IR 0.5 in one respect only — where a
node runs — and is otherwise 0.5 (`axis-core-ir-0.5.md`): the same pool, NodeRef,
CCall, CIf, CDeterminate, result, wire formats.

## Why

0.5 never says whether a node runs inside a branch. Every backend infers it from
uses: the Rust bridge sinks a node into the arm that alone consumes it, and
EFFECT_ORDER_V1 moves an effect back out when a later effect would otherwise
overtake it. The move dropped the arm's condition (`nested_if_effect_hoist`
printed 1), and the node-order workaround that replaced it cannot tell
`if c { x = eff() }` from `x = eff(); …; if c { x }` — the two lower to the
same 0.5 bundle. 0.6 writes the grouping into the node list as blocks.

## Kinds

`CCall`, `CIf`, `CDeterminate` (all as in 0.5), and two new ones:

- **`CBlock`** has no fields. It opens a block.
- **`CEnd { value }`** closes the innermost open block and yields its value.
  `value` is mandatory. A block that returns nothing ends `CEnd { pool Unit }`.

`CIf { cond, then, else }` is unchanged from 0.5, with the same position and
meaning. Its `then` and `else` are the `CEnd`s of the two blocks directly before
it, then-block first. A one-armed if has `else` = pool Unit and no else block.
The condition lives only in `CIf`; no block repeats it.

## Placement rule

1. **Nodes are in source order, in one flat array, with backward references
   only.** No node carries a number.
2. **Arm blocks and plain blocks.** A block whose `CEnd` is a `CIf`'s `then` or
   `else` is an *arm*, and it runs only when that `CIf` selects it. A `CIf`
   selects then when `cond` is true and else when it is false; when `cond` is
   Unknown or Err it selects neither and yields that Unknown, as in 0.5. Every
   other block is *plain grouping* and always runs.
3. **Every node runs exactly where it was written.** A node runs exactly when
   every block enclosing it runs, in array order. A node in no block is
   top-level and always runs.
4. **Nothing moves a node.** This holds for pure nodes too: a pure node written
   before an `if` runs before it, even if only one branch uses its value. The IR
   carries no effect class. Purity is still the registry's business (sharing,
   below), but placement never depends on it.

This is the order lab already lowers in: cond, then, else, `CIf`. 0.6 adds a
`CBlock` before each arm and a `CEnd` after it.

## Why bare blocks

- **Core IR carries more than code.** Sections, conditional clauses and cases
  are blocks too. A block that knows nothing about `if` serves all of them, and
  a fragment can be spliced or edited without patching any index around it.
  Spans could not.
- **`CIf` stays as in 0.5.** It keeps its fields, position and meaning: the merge
  after its two arms. Analyses that read `CIf` today keep working.
- **The condition is not repeated.** It lives in `CIf` only. A block does not
  carry a guard, so a guard can never disagree with the `CIf` it belongs to.
- **A mandatory value means one encoding per program.** There is no "block with
  no value" next to "block ending in Unit", so one program has one byte form and
  one hash. For the same reason a two-armed `if` always has two blocks, even when
  an arm writes no node (`CBlock`, `CEnd { x }`).

## Sharing (CSE)

The compiler may give two occurrences of an identical pure call one node only
when the existing node is in the current block or one enclosing it. Otherwise it
writes a new node. `CBlock`, `CEnd` and `CIf` are never shared: all `CBlock`s
have equal bytes, and each one is a different place.

## Encoding and hash

```
    kind_tag = 0  CCall         (as 0.5)
    kind_tag = 1  CIf           NodeRef(cond) NodeRef(then) NodeRef(else)   (as 0.5)
    kind_tag = 2  CDeterminate  (as 0.5)
    kind_tag = 3  CBlock        (no fields)
    kind_tag = 4  CEnd          NodeRef(value)
```

The canonical bytes are the identity hash's only input (0.5 §Bundle Identity).
Every bundle with an `if` gains its arm blocks, so it gets a new hash, and every
composite whose body references one moves with it. A bundle with no block keeps
its 0.5 bytes and its hash. The `.coreir` header's version byte becomes 0.6.
JSON adds `{"kind":"CBlock"}` and `{"kind":"CEnd","value":…}`.

## Verifier (at load; a scope stack; each refusal names the node)

The verifier runs 0.5's checks first: every edge points backwards, pool bounds,
types. `CEnd` has its value's type, so 0.5's check that both branches have the
same type is unchanged. Then it walks the nodes once in index order, keeping a
stack of open blocks:

1. **Blocks balance and nest.** `CBlock` pushes a block. `CEnd` pops the
   innermost open block; a `CEnd` with no open block is refused. The stack is
   empty at the end.
2. **Arm blocks sit directly before their `CIf`.** For a `CIf` at index k:
   - `else` is the `CEnd` at k−1, and `then` is the `CEnd` directly before that
     block's `CBlock`; or
   - the if is one-armed: `else` is pool Unit and `then` is the `CEnd` at k−1.
3. **An arm's `CEnd` is referenced by exactly one `CIf`, and by nothing else**,
   `result` included.
4. **No node outside an arm uses a node inside it.**
5. **Scope.** A node uses only nodes in its own block or an enclosing one, or
   the `CEnd` of a closed block that sits directly in its own block. A `CEnd`'s
   `value` is read inside the block it closes. `result` is read at top level. A
   `CBlock` is never referenced.

Rules 3 and 4 put the arm cases of rule 5 into their own refusals. Sharing is
the compiler's rule: a share across sibling blocks shows up as a rule 5
refusal. Source order itself cannot be checked from the bundle. It is an
obligation on the compiler, and the next section shows lab meets it.

## Lab emits nodes in source order

`Builder::push_node` (`src/ir/core_ir_05.rs`) appends, so a node's index is the
order in which lowering created it. `nf_lowering.rs` `lower_nf_node_05` creates
nodes as it walks the source:

- `LetExpr` lowers the value, then the body.
- `IfExpr` lowers the condition, then the then-branch, then the else-branch, and
  pushes `CIf` last. 0.6 pushes a `CBlock` where `then_start` / `else_start` are
  taken today (:517/:520), and a `CEnd` after each arm's `seq_scope_arm_effects`.
- `CallExpr` / `AppExpr` lower their arguments left to right.
- `bool_and(a, b)`: `a`, `CBlock`, `b`, `CEnd{b}`, `CBlock`, `CEnd{false}`,
  `CIf{a, ·, ·}`. `bool_or(a, b)`: `a`, `CBlock`, `CEnd{true}`, `CBlock`, `b`,
  `CEnd{b}`, `CIf`. The rhs start is at :547.
- `seq_scope_arm_effects` appends its `seq` nodes inside the arm, before its
  `CEnd`.

**Where it doesn't:** hash-consing. A repeated pure call returns the earlier
node, so the repeat has no node at its own position. The `nested_if_effect_hoist`
repro is exactly this: `bool_and(f, h)` is created in `_x`'s else-branch and
reused by `_y` at top level. Restricting sharing to the current and enclosing
blocks closes it: a reused node always runs before the reuse site, so "runs where
written" holds.

## Display format

`CBlock` and `CEnd` are shown with their operand. The nodes inside a block are
indented one step per level of nesting, and `CIf` is shown as in 0.5:

```
node[11]  CBlock
node[12]    mem_write_int_raw(node[1], pool[5], pool[2])
node[13]  CEnd value=node[12]
node[14]  CBlock
node[15]    …
node[27]  CEnd value=node[26]
node[28]  if node[10] then node[13] else node[27]
```

## 0.5 bundles

**Refused.** A 0.5 bundle does not say where its nodes run, and that cannot be
recovered from it: that is the defect. The loader refuses version 0.5 with
"recompile from source". There is no upgrade pass. Every repo re-blesses to
fixpoint (axm-req-r179 rollout).

## Worked examples

Indices are 0.6 lowering. `w` is a write, `p` a print, `[ … ]` a block
(`CBlock` … `CEnd`).

**1. nested_if_effect_hoist**
(`_x = if bool_and(f,g) { w } else { if bool_and(f,h) {p} else {p} }; _y = if bool_and(f,h) {p} else {p}`)

```
0 reserve  1 p  2 z  3 f  4 g  5 h
6  CBlock  7  CEnd value=4                 ← bool_and(f,g): then-arm g
8  CBlock  9  CEnd value=false             ←                else-arm false
10 CIf cond=3 then=7 else=9
11 CBlock                                  ← _x then-arm
12   w
13 CEnd value=12
14 CBlock                                  ← _x else-arm
15   CBlock 16 CEnd value=5  17 CBlock 18 CEnd value=false
19   CIf cond=3 then=16 else=18            ← bool_and(f,h), inside 14's block
20   CBlock 21 p 22 CEnd value=21  23 CBlock 24 p 25 CEnd value=24
26   CIf cond=19 then=22 else=25
27 CEnd value=26
28 CIf cond=10 then=13 else=27             ← _x
29 CBlock 30 CEnd value=5  31 CBlock 32 CEnd value=false
33 CIf cond=3 then=30 else=32              ← bool_and(f,h) again: 19 is in a closed block
34 CBlock 35 p 36 CEnd value=35  37 CBlock 38 p 39 CEnd value=38
40 CIf cond=33 then=36 else=39             ← _y
```

The write (12) is in the arm that ends at 13, which runs only when `CIf` 28
selects then, i.e. when `bool_and(f,g)` holds. With z = 0, g is false, so the
write does not run and the program prints 0.

**2. Top-level let used only in a branch**
(`let w = write(..); let _o = p; let _v = if int_lt(z,0) { w } else { p }`)

```
i   w
i+1 p                                    ← _o
i+2 int_lt(z, 0)
i+3 CBlock  i+4 CEnd value=i             ← the then-arm writes no node: `w` is a variable
i+5 CBlock  i+6 p  i+7 CEnd value=i+6
i+8 CIf cond=i+2 then=i+4 else=i+7
```

`w` is in no block, so it runs every call, before `_o`. This is the shape the
r178 workaround could not tell apart from #1 when `cond` was bound before `w`
(p10_gate's `t_n`, gfx3gate's `g3_check`). In 0.6 the blocks settle it.

**3. Two-deep ifs** (`if c1 { if c2 { w } else { p } } else { p }`, with c1 and
c2 bound earlier)

```
a    CBlock
a+1    CBlock  a+2 w  a+3 CEnd value=a+2
a+4    CBlock  a+5 p  a+6 CEnd value=a+5
a+7    CIf cond=c2 then=a+3 else=a+6
a+8  CEnd value=a+7
a+9  CBlock  a+10 p  a+11 CEnd value=a+10
a+12 CIf cond=c1 then=a+8 else=a+11
```

`w` runs iff c1 and c2. The inner `p` runs iff c1 and not c2. The outer `p`
runs iff not c1.

**4. Orphan effect** (`if c { let _ = p; Text("x") } else { Text("y") }`)

```
k   CBlock  k+1 p  k+2 CEnd value="x"
k+3 CBlock  k+4 CEnd value="y"
k+5 CIf cond=c then=k+2 else=k+4
```

`p` is in the then-arm, so it runs iff c, with no consumer edge and no `seq`.
`let _ = p` outside any `if` is in no block and always runs.

**5. av_admit** (`let body = if got { sm_frame_body(h) } else { "" }`, later used
only in `if empty {…} else { if toobig {…} … }`)

`body` is written at top level: `CBlock`, `sm_frame_body(h)`, `CEnd`, `CBlock`,
`CEnd{""}`, `CIf{got}`. `sm_frame_body` runs iff `got`, and the `CIf`, which is
`body`'s value, is in no block. The chain's blocks come after it. `body` runs at
its place whatever `empty` is, as written. No witness is needed.

**6. nv_n_step** (`jdat`/`nv_kind` lets, used only inside a later `if <cond>`)

They are written before that if's blocks, so they are in no block and run every
call, as written. The old bridge ran them unconditionally, and the r178
workaround needed a special rule to keep that.

**7. One-armed if** (`if c { log(x) }`, where the source has one; M1 writes both
arms today)

```
j   CBlock  j+1 log(x)  j+2 CEnd value=Unit
j+3 CIf cond=c then=j+2 else=Unit
```

`log` runs iff c. The if's value is Unit either way.

**8. Plain grouping block** (a section, or temporaries scoped by a producer)

```
m   CBlock
m+1   a = f()
m+2   b = g(m+1)
m+3 CEnd value=m+2
m+4 h(m+3)
```

No `CIf` references `m+3`, so the block always runs. `m+4` may use the block's
`CEnd`, which is the value of a closed child of its own block. It may not use
`m+1` or `m+2`.

## What the bridge deletes (`src/emit/rust_05.rs`)

Emission becomes one pass in index order. At each `CBlock`, the bridge looks
ahead to the block's matching `CEnd` (paired by the verifier's stack) and to the
node after it, which tells whether the block is an arm:

- **The then-arm of `CIf` k** emits
  `let node_k: Value = if <cond is Unknown or Err> { cond } else if truthy(cond) {`.
- **Its `CEnd`** emits its value, followed by `} else {` when k has an else-block,
  or by `} else { Value::Unit };` when it has none.
- **The else-arm's `CBlock`** emits nothing; its `CEnd` emits its value and `};`.
- **`CIf` k** emits nothing, because its value is already `node_k`.
- **A plain block** emits `let node_e: Value = {` at its `CBlock` and
  `<value> };` at its `CEnd`.

Rust's block scoping then matches the IR's.

Deleted:

- **EFFECT_ORDER_V1.** The pop loop, the `Guard` record and `Branch::Guard`
  pseudo-scope, the guarded wrapper and rebind, the contradiction error, and
  the `arm_effect` / `orders` pre-pass (part 2).
- **Both workaround rules (r178).** `guard_this_level` with `outside_witness` (the
  node-order rule and its window witness), and the old trigger kept for levels
  the effect was not written in.
- **The scope inference.** `Use`, `longest_common_prefix`,
  `compute_branch_paths`, `Placement`, `group_by_scope`, GUARDED_SHARED_PURE_V1's
  minimal scopes, and the ORPHAN_IS_TOP_LEVEL_V1 reading. `last_uses` keeps
  working: emission is index order.

## Mutants (each becomes a test when implemented; each must fail)

| Mutant | Expected |
|---|---|
| A `CEnd` with no value | refused at decode: truncated `CEnd` record / missing `value` |
| An arm `CEnd` referenced by two `CIf`s | refused, rule 3 |
| An arm block not directly before its `CIf` | refused, rule 2 |
| A use of a node inside an arm, from outside it | refused, rule 4 |
| Unbalanced blocks (a `CEnd` with no open block; a `CBlock` never closed) | refused, rule 1 |
| A share across sibling blocks (the compiler's sharing not limited to open blocks) | refused, rule 5 |
| A bridge that emits a node outside its block | `nested_if_effect_hoist` prints 1 |
| A 0.5 bundle | refused: "recompile from source" |

## Choices for review

- **Pure nodes stop sinking.** Placement rules 3–4 remove the guarding the
  inference gave by accident. `let x = str_char_code(s, i); if inbounds { x }`
  now evaluates `x` first, and a Defect is possible where 0.5 never ran it.
  Programs that relied on sinking must move the call into the branch.
- **Both arms are always blocks.** A two-armed `if` whose arm writes no node
  still gets `CBlock`, `CEnd { x }` (one encoding per program). Only a one-armed
  if has a pool `else`, and it must be Unit.
- **Kind tags.** `CBlock` = 3 and `CEnd` = 4.
- **Lab side.** `seq_scope_arm_effects` and BOOL_SHORT_CIRCUIT_V1's arm
  threading become unnecessary once blocks exist. Deleting them is lab work
  outside this list.
