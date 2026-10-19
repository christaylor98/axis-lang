# Axis Lang

**A compiler construction laboratory for the Axis programming language.**

Axis Lang is a deterministic, spec-driven compiler pipeline that transforms Axis source code into canonical Core IR bundles. It serves as a testbed for semantic design decisions and provides a reference implementation for how surface language constructs are given meaning through mechanical lowering.

---

## What This Is

Axis Lang converts Axis source code through a multi-stage pipeline:

1. **Lexer** — tokenizes source text according to a lexer specification
2. **Parser** — builds a generic parse tree from grammar rules
3. **Schema AST** — projects the parse tree into a typed abstract syntax tree
4. **Normalisation** — converts surface AST into Normal Form (NF), removing surface sugar
5. **Mechanical Lowering** — assigns semantics by transforming NF AST into Core IR (fixed, embedded, 100% Rust code)

Language Lab **stops at Core IR**. Execution, code generation, and other backends are handled by optional **bridges** that consume Core IR.

The system is **deterministic**: the same source and specifications always produce identical Core IR. This ensures reproducibility and enables sealed builds where all specifications are embedded at compile time.

---

## What This Is NOT

Axis Lang explicitly does **not** provide:

- Runtime execution or interpreters
- Virtual machines or execution engines
- Code generation or native compilation
- Production-grade performance or optimization
- Multiple compilation targets or backends
- Tooling integration (LSP, debuggers, IDE support)
- Multi-file project management or build systems
- Security hardening or sandboxing
- Concurrency or async execution
- Dynamic language features

These are intentionally out of scope. Language Lab is a **frontend only** — it stops at Core IR. The lab focuses exclusively on semantic authority and canonical lowering behavior.

---

## Current Maturity

**Status: Experimental reference implementation**

What works:
- ✅ Spec-driven lexer, parser, and schema projection
- ✅ YAML-driven normalization (mandatory, total, validates NF)
- ✅ Deterministic mechanical lowering to Core IR (NF-only, embedded)
- ✅ Sealed builds with embedded specifications
- ✅ Annotation support (metadata only)
- ✅ Full pipeline observability and inspection

What is experimental:
- The language design itself is evolving
- Error messages are functional but minimal
- Performance is not optimized
- The API surface may change

This is a laboratory, not a production tool.

---

## Quick Start

### Prerequisites

- **Rust:** 1.70 or later
- **Git:** Any recent version

### Clone and Build

```bash
git clone https://github.com/christaylor98/axis-lang.git
cd axis-lang
cargo build --release
```

### Emit Your First Core IR Bundle

```bash
# Compile source to Core IR
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file samples/pipeline_test.ax \
  --inspect core-ir

# Output: Core IR structure (YAML)
```

This runs the complete Language Lab pipeline from source to Core IR emission, using specification files that define the language syntax and lowering semantics.

---

## Pipeline Overview

### Lexer → Tokens

The lexer specification (`lexer.yaml`) defines:
- Token types (keywords, identifiers, literals, punctuation)
- Whitespace and comment handling
- String escape sequences
- Numeric literal formats

The lexer consumes source text and produces a stream of tokens with span information.

### Parser → Parse Tree

The parser specification (`parsing.yaml`) defines:
- Grammar rules (which tokens combine into which constructs)
- Start symbol
- Precedence and associativity (implicit in rule ordering)

The parser consumes tokens and produces a generic parse tree.

### Schema AST → Typed AST

The schema specification (`ast_schema.yaml`) defines:
- AST node types and their fields
- Which fields are single values vs. lists
- Union types for expressions
- Annotation extraction rules

Schema projection converts the untyped parse tree into a structured, typed AST.

### Normalisation → Normal Form (NF)

The normalisation specification (`normalisation.yaml`) defines:
- Surface sugar removal rules
- Control flow expansion
- Structural transformations to Normal Form

Normalisation is **mandatory** and ensures that lowering receives a single canonical structural shape. Normalisation does not assign semantics — it only makes structure explicit.

### Mechanical Lowering → Core IR

Lowering is **fixed, mechanical, and embedded** in the compiler (100% Rust code in `src/lowering/`).

**There is no lowering.yaml configuration file.**

Lowering:
- Consumes **Normal Form (NF) AST only** (enforced by `NfAst` type boundary)
- Assigns semantics through 1:1 mechanical mapping
- Resolves registry bindings
- Emits canonical Core IR 0.2
- Is deterministic and total
- Performs zero rewrites (all sugar eliminated by normalization)
- Performs zero validation (already done by NF validation)

Lowering is the **sole semantic authority**. Before lowering, programs have structure but no meaning. After lowering, semantics are frozen in Core IR.

**Language Lab stops here.** Core IR is the output boundary.

---

## Pipeline Observability & Inspection

Axis Lang provides **full observability** at every pipeline stage, enabling language authors to see exactly what the compiler sees without guessing or inference.

### Inspection Mode

Use the `--inspect` flag to dump the internal state of any pipeline stage:

```bash
# Inspect lexer rules
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file samples/pipeline_test.ax \
  --inspect lexer

# Inspect loaded grammar
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file samples/pipeline_test.ax \
  --inspect grammar

# Inspect multiple stages at once
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file samples/pipeline_test.ax \
  --inspect lexer --inspect schema --inspect pipeline

# Inspect all stages
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file samples/pipeline_test.ax \
  --inspect all
```

### Available Inspection Stages

| Stage | Description |
|-------|-------------|
| `lexer` | Ordered lexer rules with patterns and skip flags |
| `grammar` | Resolved grammar with all nonterminals and productions |
| `cst` | Concrete syntax tree with token spans and values |
| `schema` | Schema nodes with fields and types |
| `ast` | Abstract syntax tree after schema projection |
| `nf` | Normal Form AST after normalization and validation |
| `registry` | All registered operations with arity and profiles |
| `core-ir` | Core IR structure after lowering |
| `pipeline` | Pipeline execution summary with timings |
| `all` | Enable all inspection stages |

### Output Format

All inspection output is deterministic and machine-readable (YAML). This enables:
- **Debugging:** See exactly what the compiler loaded from YAML specs
- **Validation:** Confirm grammar rules expanded correctly
- **Tracing:** Track how source maps to AST to Core IR
- **Documentation:** Generate reference materials from actual compiler state

### No Guessing Mode

Inspection provides **concrete, inspectable representations** of every loaded spec. If a stage shows unexpected results, it's because the spec was written that way - not because the compiler inferred something silently.

Example inspection output:

```yaml
lexer:
  rules:
    - index: 0
      name: FN
      pattern: "fn"
      skip: false
    - index: 1
      name: IDENT
      pattern: "[A-Za-z_][A-Za-z0-9_]*"
      skip: false
```

---

## Documentation

- **[ARCHITECTURE.md](docs/ARCHITECTURE.md)** — Authoritative architectural specification
- **[Getting Started](docs/GETTING_STARTED.md)** — Step-by-step tutorial with runnable examples
- **[User Guide](docs/USER_GUIDE.md)** — How to use Axis Lang as a language author

**Normative Specifications:**
- **[Core IR 0.2](core_ir_spec/axis-core-ir-0.2.md)** — Core IR canonical specification
- **[Normal Form 0.1](core_spec/NORMAL_FORM_SPEC_0.1.yaml)** — NF target specification
- **[Registry 0.1](core_spec/axis-registry-0.1.md)** — Registry semantic specification

---

## Sealed Builds

Axis Lang supports **sealed builds** where all specifications are embedded into the binary at compile time. This eliminates runtime configuration and ensures complete determinism.

```bash
# Build sealed binary
cargo build --release --features sealed

# Run without spec paths
./target/release/axis run samples/pipeline_test.ax
```

Sealed builds are distribution-ready: they contain everything needed to reproduce identical Core IR from the same source.

See the User Guide for details on when to use sealed vs. unsealed builds.

---

## Contributing

This is a working laboratory. Code is structured for clarity and correctness, not performance.

If you find behavior that violates the stated contracts or produces non-deterministic results, please open an issue with:
- The exact command you ran
- The spec files used
- The source file that triggered the issue
- Expected vs. actual output

---

## License

See [LICENSE](LICENSE) for details.
