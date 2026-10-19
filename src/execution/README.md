# Wave 6 Execution Substrate

Core IR interpreter and execution layer for Axis Lang.

---

## Overview

Wave 6 implements the **execution substrate** that interprets Core IR and produces runtime values.

**Key Principle:** Core IR is the sole source of semantic authority. The interpreter performs no inference, no optimization, and no semantic interpretation beyond what is explicitly encoded in Core IR.

---

## Quick Start

### Basic Usage

```rust
use axis_lang::ir::core_ir;
use axis_lang::execution::wave6::{eval_core_ir, EvalContext};
use axis_lang::registry::Registry;

// Create execution context with registry
let registry = Registry::load_active().unwrap();
let mut ctx = EvalContext::new(registry);

// Build Core IR
let ir = core_ir::unit_lit();

// Execute
let value = eval_core_ir(&ir, &mut ctx).unwrap();
assert_eq!(format!("{}", value), "()");
```

### Complete Example

See [`examples/wave6_execution_example.rs`](../examples/wave6_execution_example.rs) for comprehensive usage examples.

---

## Architecture

```
Input: CoreTerm + Registry
       ↓
   eval_core_ir()
       ↓
Match CoreTerm variant:
  - CUnitLit  → Value::Unit
  - CLam      → Value::Closure
  - CIf       → Conditional evaluation
  - CCall     → Registry dispatch
       ↓
Output: Value or EvalError
```

---

## Supported Core IR Nodes

| Node | Description | Output |
|------|-------------|--------|
| `CUnitLit` | Unit literal | `Value::Unit` |
| `CLam` | Lambda abstraction | `Value::Closure` |
| `CIf` | Conditional expression | Value from branch |
| `CCall` | Registry function call | Value from registry |

---

## Registry Integration

All external behavior goes through the registry:

```rust
// Registry lookup by ID
let entry = registry.entries.find(|e| e.id == target)?;

// Arity validation
if args.len() != entry.arity {
    return Err(EvalError { ... });
}

// Dispatch
execute_registry_call(entry.id, &entry.name, &arg_values, node_id)
```

Built-in registry functions:
- **`print(x)`** — Display value and return unit
- **`add(x, y)`** — Placeholder addition (returns unit)

---

## Error Handling

All errors are explicit and fatal:

```rust
pub struct EvalError {
    pub message: String,
    pub span: Span,
}
```

Error conditions:
- Missing registry entry
- Arity mismatch
- Invalid operation

No silent failures. No recovery. No guessing.

---

## Determinism

The interpreter is deterministic by design:

```rust
// Same IR + same registry = same result
eval_core_ir(&ir, &mut ctx1) == eval_core_ir(&ir, &mut ctx2)
```

Guarantees:
- No hidden state
- No ambient context
- No non-deterministic operations
- Reproducible results

---

## Testing

### Unit Tests

```bash
cargo test --lib execution::wave6
```

11 tests covering:
- Happy path execution
- Registry calls
- Error conditions
- Determinism

### Integration Tests

```bash
cargo test --test wave6_integration
```

7 tests covering:
- End-to-end pipeline (Schema AST → Core IR → Execution)
- Complex expressions
- Error propagation
- Determinism verification

### Example

```bash
cargo run --example wave6_execution_example
```

10 examples demonstrating all features.

---

## API Reference

### Main Functions

```rust
/// Evaluate a Core IR term
pub fn eval_core_ir(
    ir: &CoreTerm,
    ctx: &mut EvalContext
) -> Result<Value, EvalError>

/// Evaluate a Core IR bundle
pub fn eval_bundle(
    bundle: &CoreBundle,
    registry: Registry
) -> Result<Value, EvalError>
```

### Data Types

```rust
/// Runtime value
pub enum Value {
    Unit,
    Closure {
        param: IdentOrName,
        body: Box<CoreTerm>,
        env: Environment,
    },
}

/// Evaluation context
pub struct EvalContext {
    pub registry: Registry,
    env: Environment,
}

/// Evaluation error
pub struct EvalError {
    pub message: String,
    pub span: Span,
}
```

---

## Design Principles

### 1. IR Authority

Core IR is the sole source of semantic authority:
- No token parsing
- No schema AST inspection
- No syntax analysis
- Pure IR-driven dispatch

### 2. Registry Boundary

Registry is the only external interface:
- All foreign operations via registry
- Missing entries fail immediately
- Arity validation enforced
- No implicit behavior

### 3. Explicit Everything

All behavior is explicit:
- Explicit errors
- Explicit environment
- Explicit dispatch
- No hidden state

### 4. Fail Fast

Invalid operations fail immediately:
- Missing registry entries → Error
- Arity mismatches → Error
- Unsupported operations → Error
- No recovery, no compensation

---

## Forbidden

The following are **explicitly forbidden**:

❌ Modifying Core IR
❌ Inferring behavior not in IR
❌ Adding optimizations
❌ Partial evaluation
❌ Hiding errors
❌ Side channels
❌ Parsing tokens
❌ Inspecting schema AST
❌ Inventing semantics

**If Core IR is invalid: FAIL FAST**

---

## Future Extensions

Wave 6 provides foundation for:

### Near-term
- **CVar/CLet support** — Variable bindings
- **Type values** — Integers, booleans, strings
- **Native registry** — FFI bindings

### Medium-term
- **WASM backend** — Compile to WASM
- **Bytecode** — Alternative execution strategy
- **JIT compilation** — Runtime optimization

### Long-term
- **Concurrency** — Parallel evaluation
- **Profiling** — Performance instrumentation
- **Debugger** — Interactive debugging

All extensions must preserve Wave 6 invariants.

---

## Documentation

- **Implementation:** [`src/execution/wave6.rs`](wave6.rs)
- **Tests:** [`tests/wave6_integration.rs`](../../tests/wave6_integration.rs)
- **Examples:** [`examples/wave6_execution_example.rs`](../../examples/wave6_execution_example.rs)
- **Completion:** [`docs/WAVE6_EXECUTION_COMPLETE.md`](../../docs/WAVE6_EXECUTION_COMPLETE.md)
- **Summary:** [`docs/WAVE6_SUMMARY.md`](../../docs/WAVE6_SUMMARY.md)

---

## Contributing

When extending the execution substrate:

1. **Preserve invariants** — Core IR authority, registry boundary, determinism
2. **Add tests** — Unit tests + integration tests
3. **Document** — Inline docs + examples
4. **Verify** — No forbidden behaviors

---

## License

See [`LICENSE`](../../LICENSE) in repository root.
