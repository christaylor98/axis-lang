# AXBI Bridge Spec — Core IR 0.5 binary format, M1 interface

**Status:** `axbi_parse` implemented and registered (AXBI_PARSE_V1).
M1 layer deferred — see placeholders in `examples/surface-m1/format/`.

---

## Design principle

ONE bridge function. The bridge parses the full `.coreir` stream in Rust and
returns a self-describing `Value` tree. M1 navigates the result using standard
list/tuple bridge functions that are already registered — no opaque handles,
no per-field accessors.

---

## Bridge function

### `axbi_parse`

**Registry:** `axis-codegen-bridge.axreg`, identity `AXBI_PARSE_V1`
**Impl:** `axis-codegen-bridge-rs/src/runtime/axbi.rs`

```
fn axbi_parse
  identity 0x5493ec17ab42802b6ce813f9c9f295580ca60677a555aecbc74ebf55e5d80de7
  kind     leaf
  in       (ValueList)
  out      Value
  effect   reads
  deterministic true
  idempotent    true
end
```

**Input:** `Value::List` where each element is `Value::Int(0..=255)` — the raw
bytes of a `.coreir` file.

**Output:** `Value::Tuple([pool, nodes])`:

```
pool   = List<Tuple<Str(def_hash_hex_64), List<Int(byte)>>>
nodes  = List<Ctor>

Ctor variants:
  "CCall"       fields: [Str(name), Str(identity_hex_64), List<NodeRef>]
  "CIf"         fields: [NodeRef(cond), NodeRef(then), NodeRef(else)]
  "CDeterminate" fields: []

NodeRef = Tuple<Str("node"|"pool"), Int(index)>
```

**Error policy:** UNKNOWN gate — hard-fails (panic) on bad magic, unsupported
version, truncated data, non-minimal varint, forward edge, out-of-range pool
reference.

---

## M1 navigation example (pseudocode — not yet compiled M1)

```
-- bundle = axbi_parse(bytes)
-- pool   = tuple_get(bundle, 0)   -- List of pool entries
-- nodes  = tuple_get(bundle, 1)   -- List of nodes

-- node count
let n = list_length(nodes)

-- first node (a CCall)
let node0  = list_get(nodes, 0)
let name   = ctor_field(node0, 0)   -- Str
let id_hex = ctor_field(node0, 1)   -- Str (64-char hex)
let args   = ctor_field(node0, 2)   -- List of NodeRef

-- first arg
let arg0  = list_get(args, 0)
let space = tuple_get(arg0, 0)   -- "node" or "pool"
let idx   = tuple_get(arg0, 1)   -- Int
```

See `examples/surface-m1/format/` for placeholder M1 files.

---

## What is NOT implemented (deferred)

| Item | Why deferred |
|------|-------------|
| `axbi_write` | Not yet needed; add when write-back from M1 is required |
| M1 programs that call `axbi_parse` | Requires `list_get`, `tuple_get`, `ctor_field`, `ctor_tag` in M1-callable registry; wire those first |
| JSON format bridge | Python/JS/Rust clients handle JSON natively; M1 may never need it |
