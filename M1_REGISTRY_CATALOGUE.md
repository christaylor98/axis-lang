# M1 Registry Catalogue

Generated from: `axis-surface-m1-config/`, `registries/`, `axis-codegen-bridge-rs/registry/`

---

## Implementation status legend

| Status | Meaning |
|--------|---------|
| ✅ **wired** | In `symbol_map()` in `rust_05.rs` — fully emittable and executable |
| ⚠️ **source-only** | Rust source exists in `src/runtime/` but NOT in dispatch table — calling from M1 fails at emit time with "unknown CCall target" |
| 🔶 **stub** | Same as source-only, but the source implementation is a no-op identity (`fn f(v) -> v`) |
| ❌ **registry-only** | Declared in registry, no Rust source anywhere |
| 🔵 **undeclared** | In dispatch table and callable, but absent from all axreg files |

---

## How registries are loaded

The canonical M1 invocation (per `CLAUDE.md`) passes two `--reg` flags:

```
--reg axis-surface-m1-config/m1-registry.axreg
--reg registries/pilot_lowering.axreg
```

`m1-registry.axreg` carries a comment saying core primitives are "inherited from axis.axreg",
but the `.axreg` format has no `inherits` keyword — loading is purely additive via `--reg` flags.
To make `axis.axreg` primitives available in the binary invocation, add:

```
--reg registries/axis.axreg
```

The codegen bridge registry (`axis-codegen-bridge.axreg`) is the **runtime** library;
it is not passed to the `axis` compiler. Identity hashes are shared (same sha256 derivation),
so a fn declared in `axis.axreg` resolves to the same bridge implementation.

---

## Registry 1 — `axis-surface-m1-config/m1-registry.axreg`

**registry m1 0.1** — M1-specific primitives. Loaded by default in M1 pipeline.

Valid types declared: `Unit Bool Int Text Value Dec ValueList Fn`

### Value compound operations

| fn | status | in | out | impl path |
|----|--------|----|-----|-----------|
| `value_make` | ✅ wired | `()` variadic | `Value` | `runtime::tuple::value_make` |
| `value_0` | ✅ wired | `(Value)` | `Value` | `runtime::tuple::value_0` |
| `value_1` | ✅ wired | `(Value)` | `Value` | `runtime::tuple::value_1` |
| `value_2` | ✅ wired | `(Value)` | `Value` | `runtime::tuple::value_2` |

`value_make` is variadic (arity 1–3); validator enforces the arity constraint.
`value_0/1/2` extract positional slots from a compound Value.

### ValueList construction

| fn | status | in | out | impl path |
|----|--------|----|-----|-----------|
| `list_make` | ✅ wired | `()` variadic | `ValueList` | `runtime::list::list_make` |

### Higher-order functions (HOF)

Lambda/predicate argument is typed `Value` at the registry level; surface validator
enforces fn-ref semantics. Per memory `fn-ref-as-argument.md`, lambdas are to be
replaced by fn-refs.

| fn | status | in | out | notes |
|----|--------|----|-----|-------|
| `sort` | ❌ registry-only | `(ValueList, Value)` | `ValueList` | no bridge impl |
| `filter` | ❌ registry-only | `(ValueList, Value)` | `ValueList` | no bridge impl |
| `map` | ❌ registry-only | `(ValueList, Value)` | `ValueList` | no bridge impl |
| `reduce` | ❌ registry-only | `(ValueList, Value, Value)` | `Value` | no bridge impl |

These HOFs predate the fn-ref design. The successors (`foreach`, `flat_map`, `any`, `all`, etc. in
the bridge registry with `Fn` args) are the intended replacements.

---

## Registry 2 — `registries/pilot_lowering.axreg`

**registry pilot-lowering 0.5** — Pilot test fixtures. Loaded by default in M1 pipeline.

| fn | status | in | out | notes |
|----|--------|----|-----|-------|
| `const_unit` | ✅ wired | `(Unit)` | `Unit` | maps to `runtime::arith::unit_id` |
| `seq_units` | ❌ registry-only | `(Unit, Unit)` | `Unit` | no bridge impl; `seq_unit` (singular) IS wired |

Note: identities are authored literals (all-1s / all-2s), not sha256-derived. For tests only.
`seq_units` (plural) is a pilot fixture with no dispatch entry — `seq_unit` (singular, from `axis.axreg`) IS implemented.

---

## Registry 3 — `registries/axis.axreg`

**registry axis 0.5** — Core compiler primitives. NOT loaded by default in M1 binary
invocation. Must add `--reg registries/axis.axreg` explicitly. Used by matrix tests.

Identity rule: §5b — SHA-256 of UTF-8 function name.

### Unit operations

| fn | status | in | out | impl path |
|----|--------|----|-----|-----------|
| `unit_id` | ✅ wired | `(Unit)` | `Unit` | `runtime::arith::unit_id` |
| `seq_unit` | ✅ wired | `(Unit, Unit)` | `Unit` | `runtime::arith::seq_unit` |

### Boolean operations

| fn | status | in | out | impl path |
|----|--------|----|-----|-----------|
| `bool_not` | ✅ wired | `(Bool)` | `Bool` | `runtime::bool_ops::bool_not` |
| `bool_and` | ✅ wired | `(Bool, Bool)` | `Bool` | `runtime::bool_ops::bool_and` |
| `bool_or` | ✅ wired | `(Bool, Bool)` | `Bool` | `runtime::bool_ops::bool_or` |

### Integer arithmetic

| fn | status | in | out | impl path |
|----|--------|----|-----|-----------|
| `int_add` | ✅ wired | `(Int, Int)` | `Int` | `runtime::arith::int_add` |
| `int_sub` | ✅ wired | `(Int, Int)` | `Int` | `runtime::arith::int_sub` |
| `int_mul` | ✅ wired | `(Int, Int)` | `Int` | `runtime::arith::int_mul` |

### Integer comparisons

| fn | status | in | out | impl path |
|----|--------|----|-----|-----------|
| `int_eq` | ✅ wired | `(Int, Int)` | `Bool` | `runtime::arith::int_eq` |
| `int_lt` | ✅ wired | `(Int, Int)` | `Bool` | `runtime::arith::int_lt` |

### Value construction primitives (compiler-internal)

Under-specified by design — actual arity is enforced at the surface, not the registry.

| fn | status | in | out | notes |
|----|--------|----|----|-------|
| `value_construct` | ❌ registry-only | `()` | `Unit` | surface tag checked before lowering; no bridge impl needed |
| `value_list_construct` | ❌ registry-only | `()` | `Unit` | same — surface-only gate |

---

## Registry 4 — `axis-codegen-bridge-rs/registry/axis-codegen-bridge.axreg`

**registry axis-codegen-bridge 0.1** — Bridge runtime implementation. Not passed to
`axis` compiler; loaded by the codegen bridge at execution time. Identity hashes
match `axis.axreg` where functions are shared (same §5b derivation).

Types: `Unit Int Text Bool Dec Float Value ResultText ResultUnit TextList ValueList Fn`

Note: `Fn` is declared as `prim value` here (parser limitation); verifier semantics unchanged.

### Arithmetic (`arith.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `int_add` | ✅ wired | `(Int, Int)` | `Int` |
| `int_sub` | ✅ wired | `(Int, Int)` | `Int` |
| `int_mul` | ✅ wired | `(Int, Int)` | `Int` |
| `int_div` | ✅ wired | `(Int, Int)` | `Int` |
| `int_div_checked` | ✅ wired | `(Int, Int)` | `Value` |
| `int_mod` | ✅ wired | `(Int, Int)` | `Int` |
| `int_to_str` | ✅ wired | `(Int)` | `Text` |
| `str_to_int` | ✅ wired | `(Text)` | `Int` |
| `int_abs` | ✅ wired | `(Int)` | `Int` |
| `int_min` | 🔵 undeclared | `(Int, Int)` | `Int` | wired but not in any axreg |
| `int_max` | 🔵 undeclared | `(Int, Int)` | `Int` | wired but not in any axreg |
| `int_clamp` | 🔵 undeclared | `(Int, Int, Int)` | `Int` | wired but not in any axreg |
| `celsius_to_fahrenheit` | 🔵 undeclared | `(Int)` | `Int` | wired but not in any axreg |
| `fahrenheit_to_celsius` | 🔵 undeclared | `(Int)` | `Int` | wired but not in any axreg |
| `is_positive` | 🔵 undeclared | `(Int)` | `Bool` | wired but not in any axreg |

### Comparisons (`arith.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `int_lt` | ✅ wired | `(Int, Int)` | `Bool` |
| `int_lte` | ✅ wired | `(Int, Int)` | `Bool` |
| `int_gt` | ✅ wired | `(Int, Int)` | `Bool` |
| `int_gte` | ✅ wired | `(Int, Int)` | `Bool` |
| `int_eq` | ✅ wired | `(Int, Int)` | `Bool` | declared in `axis.axreg`, not bridge registry |
| `value_eq` | ✅ wired | `(Value, Value)` | `Bool` |
| `__eq__` | ✅ wired | `(Value, Value)` | `Bool` | alias for `value_eq` |

### Boolean (`bool_ops.rs`)

| fn | status | in | out | notes |
|----|--------|----|-----|-------|
| `bool_and` | ✅ wired | `(Value, Value)` | `Bool` | bridge takes Value,Value; `axis.axreg` declares (Bool,Bool) — same identity |
| `bool_or` | ✅ wired | `(Value, Value)` | `Bool` | same note |
| `bool_not` | ✅ wired | `(Value)` | `Bool` | |
| `assert` | ✅ wired | `(Bool)` | `Unit` | declared in `axtest.axreg`; maps to `bool_ops::ax_assert` |

### String (`str_ops.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `str_len` | ✅ wired | `(Text)` | `Int` |
| `str_concat` | ✅ wired | `(Text, Text)` | `Text` |
| `str_char` | ✅ wired | `(Text, Int)` | `Text` |
| `str_char_at` | ✅ wired | `(Text, Int)` | `Value` |
| `str_char_code` | ✅ wired | `(Text, Int)` | `Int` |
| `str_slice` | ✅ wired | `(Text, Int, Int)` | `Text` |
| `str_split` | ✅ wired | `(Text, Text)` | `TextList` |
| `str_starts_with` | ✅ wired | `(Text, Text)` | `Bool` |
| `str_ends_with` | ✅ wired | `(Text, Text)` | `Bool` |
| `str_trim` | ✅ wired | `(Text)` | `Text` |
| `str_contains` | ✅ wired | `(Text, Text)` | `Bool` |
| `str_index_of` | ✅ wired | `(Text, Text)` | `Int` |
| `str_eq` | ✅ wired | `(Text, Text)` | `Bool` |
| `chr` | ✅ wired | `(Int)` | `Text` |
| `str_replace` | ✅ wired | `(Text, Text, Text)` | `Text` |
| `str_repeat` | ✅ wired | `(Text, Int)` | `Text` |
| `str_to_upper` | ✅ wired | `(Text)` | `Text` |
| `str_to_lower` | ✅ wired | `(Text)` | `Text` |
| `str_pad_left` | ✅ wired | `(Text, Int, Text)` | `Text` |
| `str_pad_right` | ✅ wired | `(Text, Int, Text)` | `Text` |
| `str_join` | ✅ wired | `(ValueList, Text)` | `Text` |
| `str_before` | 🔵 undeclared | `(Text, Text)` | `Text` | wired but not in any axreg |
| `str_after` | 🔵 undeclared | `(Text, Text)` | `Text` | wired but not in any axreg |
| `str_between` | 🔵 undeclared | `(Text, Text, Text)` | `Text` | wired but not in any axreg |

### List (`list.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `list_nil` | ✅ wired | `(Unit)` | `Value` |
| `list_cons` | ✅ wired | `(Value, Value)` | `Value` |
| `list_len` | ✅ wired | `(TextList)` | `Int` |
| `list_get` | ✅ wired | `(Value, Int)` | `Value` |
| `list_get_at` | ✅ wired | `(Value, Int)` | `Value` |
| `list_head` | ✅ wired | `(Value)` | `Value` |
| `list_tail` | ✅ wired | `(Value)` | `Value` |
| `list_is_empty` | ✅ wired | `(TextList)` | `Bool` |
| `list_append` | ✅ wired | `(Value, Value)` | `Value` |
| `list_concat` | ✅ wired | `(Value, Value)` | `Value` |
| `list_reverse` | ✅ wired | `(Value)` | `Value` |
| `list_of_1` | ✅ wired | `(Value)` | `Value` |
| `list_of_2` | ✅ wired | `(Value, Value)` | `Value` |
| `list_of_3` | ✅ wired | `(Value, Value, Value)` | `Value` |
| `list_str_len_lte_if_some` | ✅ wired | `(TextList, Int, Int)` | `Int` |
| `list_get_println_if_some` | ✅ wired | `(TextList, Int)` | `Unit` |
| `list_make` | ✅ wired | `()` variadic | `ValueList` | declared in `m1-registry.axreg`, not bridge registry |

### HOF / functional combinators (BRIDGE_FOREIGN_FN_FNREF_M1 Phase 1–2)

These use `Fn` (fn-ref) in callee position per Core IR 0.5 spec.

| fn | status | in | out |
|----|--------|----|-----|
| `foreach` | ✅ wired | `(ValueList, Fn)` | `Unit` |
| `range` | ✅ wired | `(Int, Int)` | `ValueList` |
| `loop_count` | ✅ wired | `(Int, Value, Fn)` | `Value` |
| `flat_map` | ✅ wired | `(ValueList, Fn)` | `ValueList` |
| `any` | ✅ wired | `(ValueList, Fn)` | `Bool` |
| `all` | ✅ wired | `(ValueList, Fn)` | `Bool` |
| `find_index` | ✅ wired | `(ValueList, Fn)` | `Int` |
| `count` | ✅ wired | `(ValueList, Fn)` | `Int` |
| `loop_while` | ✅ wired | `(Value, Fn, Fn, Int)` | `Value` |
| `range_step` | ✅ wired | `(Int, Int, Int)` | `ValueList` |
| `repeat` | ✅ wired | `(Value, Int)` | `ValueList` |
| `enumerate` | ✅ wired | `(ValueList)` | `ValueList` |
| `zip` | ✅ wired | `(ValueList, ValueList)` | `ValueList` |
| `take` | ✅ wired | `(ValueList, Int)` | `ValueList` |
| `drop` | ✅ wired | `(ValueList, Int)` | `ValueList` |
| `slice` | ✅ wired | `(ValueList, Int, Int)` | `ValueList` |
| `flatten` | ✅ wired | `(ValueList)` | `ValueList` |

### Numeric coercion (BRIDGE_VALUE_COERCION_V1)

Tag-dispatching HOFs. Take a `Value` + 3 converter `Fn` refs (positional: Int, Dec, Float).

| fn | status | in | out |
|----|--------|----|-----|
| `bridge_to_dec` | ✅ wired | `(Value, Fn, Fn, Fn)` | `Dec` |
| `bridge_to_float` | ✅ wired | `(Value, Fn, Fn, Fn)` | `Float` |
| `int_to_dec` | 🔵 undeclared | `(Int)` | `Dec` | wired, not in any axreg |
| `dec_id` | 🔵 undeclared | `(Dec)` | `Dec` | wired, not in any axreg |
| `float_to_dec` | 🔵 undeclared | `(Float)` | `Dec` | wired, not in any axreg |
| `int_to_float` | 🔵 undeclared | `(Int)` | `Float` | wired, not in any axreg |
| `dec_to_float` | 🔵 undeclared | `(Dec)` | `Float` | wired, not in any axreg |
| `float_id` | 🔵 undeclared | `(Float)` | `Float` | wired, not in any axreg |

### Tuple / constructor (`tuple.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `tuple_field` | ✅ wired | `(Value, Int)` | `Value` |
| `ctor_field` | ✅ wired | `(Value, Int)` | `Value` |
| `ctor_is_ok` | ✅ wired | `(Value)` | `Bool` |
| `result_text_unwrap` | ✅ wired | `(Value)` | `Text` |
| `value_make` | ✅ wired | `()` variadic | `Value` | declared in `m1-registry.axreg` |
| `value_0` | ✅ wired | `(Value)` | `Value` | declared in `m1-registry.axreg` |
| `value_1` | ✅ wired | `(Value)` | `Value` | declared in `m1-registry.axreg` |
| `value_2` | ✅ wired | `(Value)` | `Value` | declared in `m1-registry.axreg` |

### Option (`option.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `option_none` | ✅ wired | `(Unit)` | `Value` |
| `option_some` | ✅ wired | `(Value)` | `Value` |
| `option_is_none` | ✅ wired | `(Value)` | `Bool` |
| `option_is_some` | ✅ wired | `(Value)` | `Bool` |
| `option_unwrap` | ✅ wired | `(Value)` | `Value` |

### IO (`io.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `io_print` | ✅ wired | `(Value)` | `Unit` |
| `io_println` | ✅ wired | `(Value)` | `Unit` |
| `io_eprint` | ✅ wired | `(Value)` | `Unit` |
| `io_read_line` | ✅ wired | `(Unit)` | `Text` |
| `fs_read_text` | ✅ wired | `(Text)` | `Value` |
| `fs_write_text` | ✅ wired | `(Text, Text)` | `Value` |
| `fs_append_text` | ✅ wired | `(Text, Text)` | `Value` |
| `debug_trace` | ✅ wired | `(Value)` | `Unit` |

### Process (`process.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `proc_args` | ✅ wired | `(Unit)` | `TextList` |
| `proc_exit` | ✅ wired | `(Unit)` | `Unit` |
| `proc_sleep` | ✅ wired | `(Int)` | `Unit` |
| `sleep` | ✅ wired | `(Int)` | `Unit` |
| `argv` | ✅ wired | `(Int)` | `Text` |
| `argv_int` | ✅ wired | `(Int)` | `Int` |
| `argv_count` | ✅ wired | `(Unit)` | `Int` |
| `argv_or` | ✅ wired | `(Int, Text)` | `Text` |
| `argv_get` | 🔵 undeclared | `(Int)` | `Text` | wired but not in any axreg |

### IR constructors / eval / accessors (`ir_constructors.rs`, `ir_eval.rs`, `ir_accessors.rs`)

| fn | status | in | out |
|----|--------|----|-----|
| `ir_make_int_lit` | ✅ wired | `(Int)` | `Value` |
| `ir_make_bool_lit` | ✅ wired | `(Bool)` | `Value` |
| `ir_make_unit_lit` | ✅ wired | `(Unit)` | `Value` |
| `ir_make_var` | ✅ wired | `(Text)` | `Value` |
| `ir_make_lam` | ✅ wired | `(Text, Value)` | `Value` |
| `ir_make_let` | ✅ wired | `(Text, Value, Value)` | `Value` |
| `ir_make_if` | ✅ wired | `(Value, Value, Value)` | `Value` |
| `ir_make_app` | ✅ wired | `(Value, Value)` | `Value` |
| `ir_make_call` | ✅ wired | `(Text, Value)` | `Value` |
| `ir_subst` | ✅ wired | `(Text, Value, Value)` | `Value` |
| `ir_rename` | ✅ wired | `(Text, Text, Value)` | `Value` |
| `ir_free_vars` | ✅ wired | `(Value)` | `TextList` |
| `ir_write_bundle` | ✅ wired | `(Unit)` | `Unit` |
| `ir_read_bundle` | ✅ wired | `(Text)` | `Value` |
| `ir_bundle_view` | ✅ wired | `(Text)` | `Text` |
| `ir_eval` | ✅ wired | `(Value, Value)` | `Value` |
| `ir_apply` | ✅ wired | `(Value, Value)` | `Value` |
| `ir_get_kind` | ✅ wired | `(Value)` | `Text` |
| `ir_get_name` | ✅ wired | `(Value)` | `Text` |
| `ir_get_int_val` | ✅ wired | `(Value)` | `Int` |
| `ir_get_fn` | ✅ wired | `(Value)` | `Value` |
| `ir_get_arg` | ✅ wired | `(Value)` | `Value` |
| `ir_get_body` | ✅ wired | `(Value)` | `Value` |
| `ir_get_value` | ✅ wired | `(Value)` | `Value` |
| `ir_get_cond` | ✅ wired | `(Value)` | `Value` |
| `ir_get_then` | ✅ wired | `(Value)` | `Value` |
| `ir_get_else` | ✅ wired | `(Value)` | `Value` |
| `ir_term_kind` | ⚠️ source-only | `(Value)` | `Text` | source in `ir_constructors.rs`, not in dispatch |
| `ir_to_string` | ⚠️ source-only | `(Value)` | `Text` | source in `ir_constructors.rs`, not in dispatch |
| `ir_to_h1_string` | ⚠️ source-only | `(Value)` | `Text` | source in `ir_constructors.rs`, not in dispatch |
| `ir_build_program_from_spec` | ⚠️ source-only | `(Text)` | `Value` | source in `ir_constructors.rs`, not in dispatch |
| `ir_build_fold_from_spec` | ⚠️ source-only | `(Text)` | `Value` | source in `ir_constructors.rs`, not in dispatch |

### Registry introspection (`registry.rs`)

All have Rust source but are **not in the dispatch table** — calling from M1 fails at emit time.

| fn | status | in | out |
|----|--------|----|-----|
| `registry_has_entry` | ⚠️ source-only | `(Text)` | `Bool` |
| `registry_lookup` | ⚠️ source-only | `(Text)` | `Value` |
| `registry_get_provenance` | ⚠️ source-only | `(Text)` | `Value` |
| `registry_get_contract` | ⚠️ source-only | `(Text)` | `Value` |
| `registry_get_effect_sig` | ⚠️ source-only | `(Text)` | `Value` |
| `registry_all_entries` | ⚠️ source-only | `(Unit)` | `Value` |
| `registry_insert` | ⚠️ source-only | `(Text, Text, Text)` | `Value` |
| `registry_verify_chain` | ⚠️ source-only | `(Unit)` | `Bool` |
| `registry_compound_id` | ⚠️ source-only | `(Text, Text)` | `Text` |

To make these callable: add them to `symbol_map()` in `src/emit/rust_05.rs`.

### IR transitions (`transitions.rs`)

All have Rust source (identity stubs — each just returns its input) but are **not in the dispatch table**.

| fn | status | in | out |
|----|--------|----|-----|
| `introduce_let_binding` | 🔶 stub | `(Value)` | `Value` |
| `introduce_lambda` | 🔶 stub | `(Value)` | `Value` |
| `apply_function` | 🔶 stub | `(Value)` | `Value` |
| `extract_subterm_to_function` | 🔶 stub | `(Value)` | `Value` |
| `inline_let_binding` | 🔶 stub | `(Value)` | `Value` |
| `rename_bound_variable` | 🔶 stub | `(Value)` | `Value` |
| `reference_registry_function` | 🔶 stub | `(Value)` | `Value` |
| `verify_foreign_reference` | 🔶 stub | `(Value)` | `Value` |

### Frontend (`frontend.rs`)

Rust source exists but **not in the dispatch table**.

| fn | status | in | out |
|----|--------|----|-----|
| `frontend_lookup_shape` | ⚠️ source-only | `(Text, Text)` | `Text` |
| `frontend_walk` | ⚠️ source-only | `(Text, Text, Text)` | `Text` |

### Misc

| fn | status | in | out | notes |
|----|--------|----|----|-------|
| `composite_io_passthrough` | ❌ registry-only | `(Unit)` | `Unit` | kind composite with body hash; no Rust source |
| `test_lam` | ❌ registry-only | `(Value)` | `Value` | test scaffold, no impl |
| `test_let` | ❌ registry-only | `(Value)` | `Value` | test scaffold, no impl |
| `ping_loop` | ✅ wired | `(Unit)` | `Unit` | `runtime::signals::ping_loop` |
| `pong_loop` | ✅ wired | `(Unit)` | `Unit` | `runtime::signals::pong_loop` |

---

## Registry 5 — `registries/axtest.axreg` (test-only)

**registry axtest 0.1** — Test assertion primitives. Used in axtest scenarios.

| fn | status | in | out | notes |
|----|--------|----|-----|-------|
| `assert` | ✅ wired | `(Bool)` | `Unit` | maps to `bool_ops::ax_assert`; panics on false |

---

## Registry 6 — `registries/fnref_test.axreg` (test-only)

**registry fnref_test 0.1** — Fn-ref verification fixtures (AXIS_FN_REF_ARG_VERIFY_V1).

| fn | status | in | out |
|----|--------|----|-----|
| `apply_handler` | ❌ registry-only | `(Fn)` | `Unit` |
| `consume_unit` | ❌ registry-only | `(Unit)` | `Unit` |

These are verification fixtures only — there is no bridge implementation. They exist to test
that the verifier correctly accepts/rejects Fn-ref arguments at the type level.

---

## Summary

### By implementation status

| Status | Count | What to do |
|--------|-------|-----------|
| ✅ wired | ~90 | Fully usable from M1 programs |
| ⚠️ source-only | 14 | Rust exists; add to `symbol_map()` in `rust_05.rs` to enable |
| 🔶 stub | 8 | Identity no-ops; need real impl + `symbol_map()` entry |
| 🔵 undeclared | 16 | Wired and callable; need axreg declaration to be type-checked |
| ❌ registry-only | 7 | Declared but not callable; need source + wiring |

### By registry

| Registry | Total fns | ✅ wired | ⚠️/🔶 unwired | ❌ no impl | When available |
|----------|-----------|----------|----------------|-----------|----------------|
| `m1-registry.axreg` | 10 | 6 (value_make/0/1/2, list_make, + HOF registry-only below) | 0 | 4 (sort/filter/map/reduce) | Default M1 invocation |
| `pilot_lowering.axreg` | 2 | 1 (const_unit) | 0 | 1 (seq_units) | Default M1 invocation |
| `registries/axis.axreg` | 12 | 9 | 0 | 3 (value_construct, value_list_construct + int_eq in both) | Add `--reg registries/axis.axreg` |
| `axis-codegen-bridge.axreg` | ~100 | ~65 | 22 | 5 | Bridge runtime |
| `axtest.axreg` | 1 | 1 | 0 | 0 | Test scenarios |
| `fnref_test.axreg` | 2 | 0 | 0 | 2 | Verification tests only |

### Source-only / stub — ready to wire

These need only a `m.insert(...)` line added to `symbol_map()` in `src/emit/rust_05.rs`:

```
// registry.rs
"registry_has_entry"      → axis_codegen_bridge::runtime::registry::registry_has_entry
"registry_lookup"         → axis_codegen_bridge::runtime::registry::registry_lookup
"registry_get_provenance" → axis_codegen_bridge::runtime::registry::registry_get_provenance
"registry_get_contract"   → axis_codegen_bridge::runtime::registry::registry_get_contract
"registry_get_effect_sig" → axis_codegen_bridge::runtime::registry::registry_get_effect_sig
"registry_all_entries"    → axis_codegen_bridge::runtime::registry::registry_all_entries
"registry_insert"         → axis_codegen_bridge::runtime::registry::registry_insert
"registry_verify_chain"   → axis_codegen_bridge::runtime::registry::registry_verify_chain
"registry_compound_id"    → axis_codegen_bridge::runtime::registry::registry_compound_id

// ir_constructors.rs
"ir_term_kind"            → axis_codegen_bridge::runtime::ir_constructors::ir_term_kind
"ir_to_string"            → axis_codegen_bridge::runtime::ir_constructors::ir_to_string
"ir_to_h1_string"         → axis_codegen_bridge::runtime::ir_constructors::ir_to_h1_string
"ir_build_program_from_spec" → axis_codegen_bridge::runtime::ir_constructors::ir_build_program_from_spec
"ir_build_fold_from_spec" → axis_codegen_bridge::runtime::ir_constructors::ir_build_fold_from_spec

// frontend.rs
"frontend_lookup_shape"   → axis_codegen_bridge::runtime::frontend::frontend_lookup_shape
"frontend_walk"           → axis_codegen_bridge::runtime::frontend::frontend_walk

// transitions.rs (stubs — real impl needed before these are useful)
"introduce_let_binding"   → axis_codegen_bridge::runtime::transitions::introduce_let_binding
...
```

### Undeclared — need axreg entries

These are callable today but lack registry declarations (no type-checking at compile time):

```
int_min  int_max  int_clamp  celsius_to_fahrenheit  fahrenheit_to_celsius  is_positive
str_before  str_after  str_between
argv_get
int_to_dec  dec_id  float_to_dec  int_to_float  dec_to_float  float_id
```

---

## Gap: axis.axreg not in default M1 invocation

The `CLAUDE.md` baseline command does not include `--reg registries/axis.axreg`.
This means `int_add`, `bool_not`, `int_eq`, etc. are **not in scope** when running
M1 files through the `axis` binary as specified. The matrix tests load it explicitly
via `AXIS_AXREG` in Rust. If M1 programs need these primitives, the baseline command
must be extended or the functions added to `m1-registry.axreg`.
