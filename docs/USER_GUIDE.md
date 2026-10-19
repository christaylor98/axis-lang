# Axis Lang User Guide

This guide explains how to use Axis Lang as a **language author**. You will learn how to design your own language, understand the role of each specification, and work within the architectural constraints of the system.

---

## Table of Contents

1. [Core Concepts](#core-concepts)
2. [Writing Your Own Language](#writing-your-own-language)
3. [Annotations](#annotations)
4. [Introspection & Trust](#introspection--trust)
5. [Sealed Builds](#sealed-builds)
6. [What to Do Next](#what-to-do-next)

---

## Core Concepts

Axis Lang operates on a strict separation of concerns. Understanding these boundaries is essential.

### Lexer Specification

**Purpose:** Define how source text becomes tokens.

**What it defines:**
- Token types (keywords, identifiers, literals, punctuation, operators)
- Whitespace handling (skip or preserve)
- Comment syntax
- String literal delimiters and escape sequences
- Numeric literal formats (decimal, hex, etc.)
- Case sensitivity

**What it does NOT define:**
- Grammar or syntax rules
- Semantics or meaning
- AST structure
- Type system

**Example:**

```yaml
lexer:
  keywords:
    - fn
    - let
    - if
  
  identifiers:
    pattern: "[A-Za-z_][A-Za-z0-9_]*"
  
  literals:
    int:
      pattern: "[0-9]+"
      base: 10
  
  punctuation:
    - "{"
    - "}"
    - "("
    - ")"
```

The lexer is **dumb**: it recognizes patterns but assigns no meaning.

---

### Parser Specification

**Purpose:** Define how tokens combine into structures.

**What it defines:**
- Grammar rules (productions)
- Start symbol
- Token sequences that form valid constructs
- Implicit precedence through rule ordering

**What it does NOT define:**
- Token recognition (that's the lexer's job)
- AST node types (that's the schema's job)
- Semantics or evaluation (that's lowering's job)

**Example:**

```yaml
parser:
  start: Program
  
  grammar:
    Program:
      - 'Decl*'
    
    Decl:
      - 'Function'
    
    Function:
      - '"fn" IDENT "(" ParamList? ")" "->" Type Block'
    
    Block:
      - '"{" Stmt* Expr "}"'
```

The parser builds a **generic parse tree**. It knows structure but not types or meaning.

---

### Schema AST

**Purpose:** Project the untyped parse tree into a typed abstract syntax tree.

**What it defines:**
- AST node types and their fields
- Which fields are single values vs. lists
- Union types (e.g., `Expr` can be `IfExpr | CallExpr | Literal`)
- Annotation extraction rules

**What it does NOT define:**
- Grammar (that's the parser's job)
- Semantics (that's lowering's job)

**Why it exists:**

The parse tree is **generic**: every node is just "I matched this grammar rule." The schema projects this into a **typed tree** where nodes have well-defined structure.

**What it gives you:**

- Type safety: You can't accidentally treat a `FunctionDecl` as an `Expr`
- Field access: You can refer to `function.name` instead of `node.children[1]`
- Validation: The schema enforces that the parse tree matches expected structure

**Example:**

```yaml
nodes:
  Function:
    fields:
      name: Ident
      params: [Param]
      return_type: Type
      body: Block
  
  Param:
    fields:
      name: Ident
      type: Type
  
  Block:
    fields:
      statements: [LetStmt]
      result: Expr
```

This defines that a `Function` has exactly these fields with these types.

---

### Normalisation

**Purpose:** Transform surface AST into Normal Form (NF).

**What it is:**

Normalisation is a **mandatory structural phase** that:
- Removes surface-level sugar
- Makes control flow explicit  
- Ensures lowering receives a single canonical structural shape

Normalisation is **not** a semantic authority — it only makes structure explicit.

**What it does:**

- Desugar high-level constructs
- Expand implicit operations
- Eliminate surface variants
- Produce only NF-admissible nodes

**What it does NOT do:**

- Assign meaning (that's lowering's job)
- Optimize
- Type check
- Make semantic decisions

**Example normalisation rule:**

```yaml
normalisation:
  rules:
    - match:
        node: ForLoop
        iterator: iter
        body: b
      rewrite:
        node: While
        condition: { call: has_next, args: [iter] }
        body:
          node: Block
          statements:
            - { call: advance, args: [iter] }
            - { node: b }
```

This rewrites `for` loops into explicit `while` loops that lowering can understand.

---

### Core IR

**Purpose:** Canonical, immutable intermediate representation.

**What it is:**

Core IR is the **semantic boundary**. Before lowering, programs have structure but no meaning. After lowering, semantics are frozen and explicit.

Core IR is:
- **Name-free:** Uses numeric handles instead of identifiers
- **Self-contained:** Contains all information needed for semantic interpretation
- **Deterministic:** Same input always produces identical Core IR
- **Immutable:** Once produced, Core IR never changes
- **The output boundary:** Axis Lang stops here

**What it is NOT:**

- An optimization target
- A multi-backend IR
- A serialization format (though it can be serialized)

**Core IR constructs (Core IR 0.2):**

- `CUnitLit` — Unit literal
- `CBoolLit` — Boolean literal
- `CIntLit` — Integer literal
- `CLam` — Lambda abstraction (closure)
- `CLet` — Let binding
- `CIf` — Conditional expression
- `CCall` — Registry function call
- `CVar` — Variable reference (by numeric handle)
- `CApp` — Function application

**Example Core IR:**

```
CLam(
  param: Handle(0),
  body: CIf(
    cond: CVar(Handle(0)),
    then: CUnitLit,
    else: CBoolLit(false)
  )
)
```

This represents: `λx. if x then () else false`

---

### Lowering

**Purpose:** Transform NF AST nodes to Core IR.

**The critical rule:**

**Lowering is the sole semantic authority.**

Lowering is **fixed, mechanical, and embedded** in the compiler. It is no longer user-configurable.

Everything that gives meaning to your language happens during lowering. The lexer, parser, schema, and normalisation are purely structural. Lowering is where programs acquire **meaning**.

**What lowering does:**

- Consumes **Normal Form (NF) AST only**
- Maps each NF node to Core IR
- Resolves variable scoping and binding
- Assigns semantics to constructs
- Produces canonical Core IR

**What lowering does NOT do:**

- Handle surface sugar (that's normalisation's job)
- Optimize code
- Type check
- Accept non-NF input

**Authority:** Lowering is versioned with the compiler release to ensure semantic stability and reproducibility.

---

## Writing Your Own Language

### Start Minimal

Design the smallest possible language that produces valid Core IR.

**Minimal language:**

1. One literal type (e.g., unit)
2. One expression type
3. No control flow
4. No variables

**Example:**

**`lexer.yaml`:**

```yaml
lexer:
  charset: ascii
  
  literals:
    unit:
      literal: "()"
      single_token: true
```

**`parsing.yaml`:**

```yaml
parser:
  start: Program
  
  grammar:
    Program:
      - 'Expr'
    
    Expr:
      - 'UNIT'
```

**`ast_schema.yaml`:**

```yaml
nodes:
  Program:
    fields:
      expr: Expr
  
  Expr:
    union:
      - UnitLit
  
  UnitLit: {}
```

**`lowering.yaml`:**

```yaml
lowering:
  rules:
    - match:
        node: UnitLit
      emit:
        node: CUnitLit
```

This is a complete, runnable language. Test it:

```bash
echo "()" > test.ax

./target/release/axis run \
  --lexer lexer.yaml \
  --parser parsing.yaml \
  --schema ast_schema.yaml \
  --file test.ax
```

Output: `()`

---

### Add Complexity Incrementally

Once your minimal language works, add **one feature at a time**.

**Evolution path:**

1. ✅ Unit literal
2. Add integer literals
3. Add boolean literals
4. Add identifiers
5. Add variable binding (`let`)
6. Add conditionals (`if`)
7. Add functions
8. Add function calls

**Rule:** Test after every addition.

---

### Example: Add Integer Literals

**Step 1: Update lexer**

```yaml
literals:
  int:
    pattern: "[0-9]+"
    base: 10
```

**Step 2: Update parser**

```yaml
Expr:
  - 'UNIT'
  - 'INT'
```

**Step 3: Update schema**

```yaml
Expr:
  union:
    - UnitLit
    - IntLit

IntLit:
  fields:
    value: int
```

**Step 4: Update lowering**

```yaml
- match:
    node: IntLit
    value: n
  emit:
    node: CIntLit
    value: n
```

**Step 5: Test**

```bash
echo "42" > test.ax
./target/release/axis run --lexer lexer.yaml --parser parsing.yaml --schema ast_schema.yaml --file test.ax
```

Output: `42`

---

### How Schema Changes Affect Lowering

When you modify `ast_schema.yaml`, you must update `lowering.yaml` to match.

**Schema change:**

```yaml
# Before
Function:
  fields:
    name: Ident
    body: Block

# After
Function:
  fields:
    name: Ident
    params: [Param]
    body: Block
```

**Required lowering change:**

You must now handle the `params` field:

```yaml
- match:
    node: Function
    name: n
    params: ps
    body: b
  emit:
    node: CLam
    param: { handle: ... }
    body: { lower: b }
```

**If mechanical lowering doesn't handle your NF node:** You'll get a compile-time error during the lowering phase.

**Rule:** NF nodes and mechanical lowering must stay synchronized (both are versioned together in the compiler).

---

## Annotations

Annotations are **metadata** attached to AST nodes and propagated to Core IR.

**Critical constraint:**

**Annotations NEVER affect semantics.**

They are pure data. Semantics are defined entirely by Core IR structure. Annotations are metadata-only.

---

### How to Define Annotations in the Schema

Use the `annotations` field in `ast_schema.yaml`:

```yaml
Function:
  fields:
    name: Ident
    params: [Param]
    body: Block
  annotations:
    - type: FromTokens
      token: PUNCT_AT  # Extract '@' tokens before the function
```

This tells the schema projector: "Look for `@` tokens preceding `Function` nodes and extract them as annotations."

---

### Annotation Extraction

**Syntax example:**

```
@inline
fn foo() -> Unit {
  ()
}
```

When the schema sees `@inline`, it extracts an annotation:

```yaml
Annotation {
  key: "inline",
  value: Symbol("inline")
}
```

This annotation is attached to the `Function` AST node.

---

### How Annotations Propagate

During lowering, annotations are **copied** from AST nodes to Core IR nodes.

**Lowering rule:**

```yaml
- match:
    node: Function
    name: n
    body: b
  emit:
    node: CLam
    param: ...
    body: { lower: b }
    annotations: { propagate: true }
```

The `annotations: { propagate: true }` directive copies annotations from the `Function` AST node to the `CLam` Core IR node.

**Result:**

The `CLam` in Core IR carries the `@inline` annotation.

---

### How Bridges Consume Annotations

**Bridges** are external tools that consume Core IR.

Bridges can read annotations to make decisions:

```rust
// Example: Rust bridge inspecting annotations
match core_term {
    CoreTerm::CLam { annotations, .. } => {
        if annotations.iter().any(|a| a.key == "inline") {
            // Perform inlining optimization
        }
    }
}
```

**Important:**

- Bridges MAY read annotations
- Bridges MAY use annotations to guide code generation, optimization, or other behavior
- Axis Lang itself does NOT interpret annotations semantically

Annotations are a **contract between the language author and bridge authors**, not part of Core IR semantics.

---

### Explicit Statement

**Annotations do NOT affect Core IR semantics.**

Test: Run the same program with and without annotations. The output is identical.

```bash
# With annotation
echo "@inline fn main() -> Unit { () }" > test.ax
./target/release/axis run ... --file test.ax
# Output: ()

# Without annotation
echo "fn main() -> Unit { () }" > test.ax
./target/release/axis run ... --file test.ax
# Output: ()
```

Execution is annotation-invariant. This is a verified property (see `tests/annotations_invariance.rs`).

---

## Introspection & Trust

Axis Lang includes **provenance tracking**: you can inspect how Core IR was produced.

---

### How to Inspect Provenance

Use the `axis trace` commands to answer: "Why does this Core IR node exist?"

**Example workflow:**

1. Run the pipeline with trace output:

```bash
./target/release/axis run \
  --lexer lexer.yaml \
  --parser parsing.yaml \
  --schema ast_schema.yaml \
  --file test.ax \
  --trace output.trace
```

2. Inspect the trace:

```bash
# View trace statistics
./target/release/axis trace stats --trace output.trace

# Inspect a specific Core IR node's origin
./target/release/axis trace node --trace output.trace --id 42
```

**Output:**

```
Node ID: 42
Type: CLam
Origin:
  - Source span: line 3, col 1-15
  - AST node: Function "main"
  - Lowering rule: Function -> CLam
  - Timestamp: 2026-01-24T10:23:45Z
```

This shows the complete lineage of the Core IR node.

---

### What Provenance Tells You

- **Where the node came from** (source location)
- **How it was created** (which lowering rule)
- **When it was created** (build timestamp)
- **What AST node produced it** (schema node type)

This enables:
- Debugging lowering rules
- Understanding Core IR structure
- Verifying that lowering behaves as expected
- Auditing build reproducibility

---

### CLI Examples Using `axis trace`

**List all nodes in a trace:**

```bash
./target/release/axis trace stats --trace output.trace --format json
```

**Find nodes by type:**

```bash
./target/release/axis trace stats --trace output.trace | grep CLam
```

**Export full provenance graph:**

```bash
./target/release/axis trace export --trace output.trace --out provenance.json
```

---

## Sealed Builds

Sealed builds embed all specifications into the binary at compile time, eliminating runtime configuration.

---

### What Sealing Means

**Unsealed build:**

```bash
./target/release/axis run \
  --lexer lexer.yaml \
  --parser parsing.yaml \
  --schema ast_schema.yaml \
  --file test.ax
```

You must provide spec paths at runtime.

**Sealed build:**

```bash
# Build with sealed feature
cargo build --release --features sealed

# Run without spec paths
./target/release/axis run test.ax
```

The specs are **embedded** in the binary. No external files are read.

---

### What Artifacts Are Embedded

A sealed binary contains:

1. **Lexer specification** (generated Rust code)
2. **Parser specification** (generated Rust code)
3. **Schema specification** (generated Rust code)
4. **Lowering specification** (embedded data)
5. **Build manifest** (timestamp, git hash, spec checksums)

These are compiled into the binary at build time.

---

### How Determinism Is Enforced

Sealed builds guarantee:

- **No external configuration files** consulted at runtime
- **No environment variables** affect behavior
- **No randomness** in Core IR generation
- **Bit-identical output** for the same source

**Verification:**

Run the same source file twice:

```bash
./target/release/axis run test.ax --out output1.axir
./target/release/axis run test.ax --out output2.axir

sha256sum output1.axir output2.axir
```

The checksums are identical.

---

### When You Should Seal vs. Not Seal

**Use sealed builds when:**

- Distributing the compiler to users
- Requiring reproducible builds
- Eliminating configuration drift
- Ensuring determinism across environments

**Use unsealed builds when:**

- Developing and iterating on language design
- Testing different spec combinations
- Debugging lowering rules
- Rapid prototyping

**Workflow:**

1. Develop with unsealed builds (fast iteration)
2. Test thoroughly
3. Seal for distribution (determinism)

---

### How to Seal

**Step 1: Run the sealing tool**

```bash
cargo run --bin seal
```

This generates:
- `src/generated/embedded_lexer.rs`
- `src/generated/embedded_parser.rs`
- `src/generated/embedded_schema.rs`
- `src/generated/embedded_lowering.rs`
- `src/generated/manifest.rs`

**Step 2: Build with sealed feature**

```bash
cargo build --release --features sealed
```

**Step 3: Verify**

```bash
./target/release/axis inspect manifest
```

Output shows embedded spec checksums and build metadata.

---

## What to Do Next

You now understand:
- ✅ How each specification defines part of your language
- ✅ How to evolve your language incrementally
- ✅ How annotations work and what they're for
- ✅ How to inspect provenance and verify determinism
- ✅ When and how to use sealed builds

---

### Writing Real Axis Code

Axis Lang uses the Axis language to implement parts of itself (self-hosting).

Example: The registry system can be extended by writing Axis code that defines new registry functions.

See `examples/` for sample Axis programs demonstrating:
- Parser examples (`wave2_parser_example.rs`)
- Schema projection (`wave4_schema_example.rs`)
- Lowering (`wave5_lowering_example.rs`)
- Execution (`wave6_execution_example.rs`)

---

### Using Axis to Build Parts of Itself

The long-term goal is **self-hosting**: the Axis compiler is written in Axis.

Currently, the compiler is written in Rust, but you can:
- Write Axis programs that manipulate Core IR
- Define new registry functions in Axis
- Implement domain-specific tools in Axis

This is experimental but demonstrates the language's expressiveness.

---

### Known Limitations

**Current limitations:**

1. **No multi-file projects**: Each source file is independent
2. **No module system**: No imports or exports
3. **No type system**: Type checking is not implemented
4. **Minimal error messages**: Errors are functional but terse
5. **No optimization**: Execution is unoptimized interpretation
6. **Limited registry functions**: Only `print` and `add` are predefined

**These are deliberate:**

Axis Lang focuses on semantic authority and lowering correctness, not production features.

**Workarounds:**

- For multi-file projects: Concatenate files or use a preprocessor
- For type checking: Implement it as a separate Axis program that analyzes Core IR
- For optimization: Write a bridge that consumes Core IR and optimizes it

---

### Getting Help

**If something doesn't work:**

1. Check the error message for span information
2. Verify that your specs are synchronized (parser ↔ schema ↔ lowering)
3. Run with `--trace` to inspect provenance
4. Consult the freeze notes (`WAVE*_COMPLETE.md`) for implementation details

**If behavior seems non-deterministic:**

This is a bug. Report it with:
- Exact commands run
- Spec files used
- Source file that triggered the issue
- Steps to reproduce

Determinism is a non-negotiable invariant.

---

### Community and Contribution

Axis Lang is a working laboratory. Code is structured for clarity, not performance.

**Contribution guidelines:**

- Preserve determinism
- Respect architectural invariants
- Add tests for new features
- Document semantic decisions in freeze notes

See the repository's issue tracker for current work.

---

## Summary

You are now equipped to:
- Design and implement your own language using Axis Lang
- Understand the role and boundaries of each pipeline stage
- Use annotations responsibly as metadata, not semantics
- Inspect provenance to verify correctness
- Build sealed, deterministic artifacts for distribution

**The core principle:**

**Lowering is the sole semantic authority.** Everything before it is structure. Everything after it is projection.

Now go build languages.
