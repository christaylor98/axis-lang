# Getting Started with Axis Lang

This guide walks you through compiling your first Axis program end-to-end. By the end, you will have run the complete pipeline from source code to Core IR emission.

---

## Prerequisites

### Required Software

- **Rust**: Version 1.70 or later
  - Check: `rustc --version`
  - Install: https://rustup.rs/
  
- **Git**: Any recent version
  - Check: `git --version`

### Time Required

First-time setup: ~5 minutes (depending on build time)  
Running examples: ~2 minutes

---

## Step 1: Clone and Build

```bash
# Clone the repository
git clone https://github.com/christaylor98/axis-lang.git
cd axis-lang

# Build the compiler
cargo build --release
```

This produces the `axis` binary at `./target/release/axis`.

**Build time:** Typically 1-3 minutes on a modern machine.

---

## Step 2: Understand the Minimal Example

The repository includes a complete working example at `lang-lab-poc-userfiles/minimal/`.

### Directory Structure

```
lang-lab-poc-userfiles/minimal/
├── lexer.yaml           # Defines tokens (keywords, literals, punctuation)
├── parsing.yaml         # Defines grammar rules
├── ast_schema.yaml      # Defines AST node structure
├── normalisation.yaml   # Defines Normal Form transformations
└── (source files)       # Your .ax programs go here
```

### What Each File Does

**`lexer.yaml`** — Token Specification

Defines how source text becomes tokens. Example:

```yaml
lexer:
  keywords:
    - fn
    - let
  
  literals:
    int:
      pattern: "[0-9]+"
  
  punctuation:
    - "("
    - ")"
```

This tells the lexer to recognize `fn` as a keyword, sequences of digits as integers, and `(` `)` as punctuation.

**`parsing.yaml`** — Grammar Specification

Defines how tokens combine into structures. Example:

```yaml
parser:
  start: Program
  
  grammar:
    Program:
      - 'Expr'
    
    Expr:
      - 'UNIT'
      - 'INT'
```

This defines a trivial language where programs are single expressions, and expressions can be unit literals or integers.

**`ast_schema.yaml`** — AST Node Schema

Defines typed AST nodes. Example:

```yaml
nodes:
  Program:
    fields:
      expr: Expr
  
  Expr:
    union:
      - UnitLit
      - IntLit
  
  UnitLit: {}
  
  IntLit:
    fields:
      value: int
```

This creates a typed tree structure with `Program` containing an `Expr`, which is either a `UnitLit` or `IntLit`.

---

## Step 3: Create a Minimal Source File

Create a file `lang-lab-poc-userfiles/minimal/test.ax`:

```
()
```

This is the simplest valid Axis program: a unit literal.

---

## Step 4: Compile to Core IR

Run the complete Axis Lang pipeline:

```bash
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/minimal/lexer.yaml \
  --parser lang-lab-poc-userfiles/minimal/parsing.yaml \
  --schema lang-lab-poc-userfiles/minimal/ast_schema.yaml \
  --file lang-lab-poc-userfiles/minimal/test.ax \
  --inspect core-ir
```

**Expected Output:** Core IR structure (YAML)

### What Just Happened

1. **Lexer** read `test.ax` and produced token: `UNIT("()")`
2. **Parser** matched `UNIT` against grammar, produced parse tree node `Expr`
3. **Schema projection** converted parse tree to typed AST: `UnitLit`
4. **Normalisation** confirmed AST is already in Normal Form (no sugar to remove)
5. **Mechanical Lowering** transformed `UnitLit` NF node to Core IR: `CUnitLit`
6. **Output** — Core IR bundle emitted (Axis Lang stops here)

**Axis Lang has no execution stage.** Bridges (out of scope) may consume Core IR for execution, code generation, or other purposes.

---

## Step 5: Understanding the Pipeline Stages

### Structural Stages (No Semantics)

These stages transform **structure only**:

- **Lexer:** text → tokens
- **Parser:** tokens → parse tree
- **Schema:** parse tree → typed AST
- **Normalisation:** surface AST → Normal Form AST (YAML-driven rewrites)

None of these stages assign meaning. They only recognize and transform structure.

### Semantic Stage (Meaning Assigned)

- **Mechanical Lowering:** NF AST → Core IR 0.2 (this is where semantics are defined)

Mechanical Lowering is **fixed and embedded** in the compiler (100% Rust code). It is the sole semantic authority.

**Axis Lang stops at Core IR.** There is no execution stage in Axis Lang.

---

## Step 6: Modify the Program

### Example: Add an Integer Literal

The minimal example only supports unit literals. Let's extend it to support integers.

Change `test.ax` to:

```
42
```

You'll get an error:

```
lex error at 0..1: unexpected character '4'
```

This is expected! The minimal lexer doesn't recognize integers yet. Let's add support.

**Update `lexer.yaml`:**

```yaml
literals:
  unit:
    literal: "()"
    single_token: true
  
  int:
    pattern: "[0-9]+"
    base: 10
```

**Update `parsing.yaml`:**

```yaml
parser:
  start: Program

  grammar:
    Program:
      - 'Expr'
    
    Expr:
      - 'UNIT_LIT'
      - 'INT'
```

**Update `ast_schema.yaml`:**

```yaml
nodes:
  Program:
    fields:
      expr: Expr
  
  Expr:
    union:
      - UnitLit
      - IntLit
  
  UnitLit: {}
  
  IntLit:
    fields:
      value: int
```

**Note:** Mechanical Lowering is fixed and embedded in the compiler (100% Rust code in `src/lowering/`). **There is no lowering.yaml configuration file.** The compiler provides deterministic semantic lowering from NF AST to Core IR automatically.

Run again:

```bash
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/minimal/lexer.yaml \
  --parser lang-lab-poc-userfiles/minimal/parsing.yaml \
  --schema lang-lab-poc-userfiles/minimal/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/minimal/normalisation.yaml \
  --file lang-lab-poc-userfiles/minimal/test.ax
```

---

## Step 7: Try a More Complex Example

The repository includes a richer language specification at `lang-lab-poc-userfiles/`.

### Create a Function Program

Create `lang-lab-poc-userfiles/function_test.ax`:

```
fn main() -> Unit {
  let x = ();
  x
}
```

### Run It

```bash
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file lang-lab-poc-userfiles/function_test.ax
```

**Expected Output:** Core IR bundle

This compiles a function definition, processes a `let` binding, and emits the corresponding Core IR.

---

## Common Failure Modes

### 1. Invalid Syntax

**Symptom:** Parser error

**Example:** Create `broken.ax`:

```
fn main( -> Unit {}
```

**Output:**

```
Error: Parser failed
Expected: )
Found: ->
```

**Fix:** Correct the syntax to match the grammar rules in `parsing.yaml`.

---

### 2. Schema Mismatch

**Symptom:** Schema projection error

**Example:** If `parsing.yaml` produces a `FunctionDecl` node but `ast_schema.yaml` doesn't define it:

**Output:**

```
Error: No schema definition for node type: FunctionDecl
```

**Fix:** Ensure every grammar rule in `parsing.yaml` has a corresponding definition in `ast_schema.yaml`.

---

### 3. Normalisation Error

**Symptom:** Normal Form validation failure

**Example:** If a construct cannot be normalized to NF:

**Output:**

```
Error: Invalid node after normalisation: SurfaceSpecificConstruct
```

**Fix:** Ensure all surface constructs are transformed to NF-admissible nodes in `normalisation.yaml`.

---

## How Error Messages Look

Errors include:
- **Span information:** Line and column numbers
- **Context:** What the parser or compiler expected
- **Failure point:** Exactly where the error occurred

Example:

```
Error at line 3, column 10:
  Expected: "}"
  Found: ";"
  Context: Parsing Block
```

Errors are designed to be actionable. They tell you what went wrong and where.

---

## Understanding Determinism

Run the same program twice:

```bash
./target/release/axis run --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file lang-lab-poc-userfiles/function_test.ax

./target/release/axis run --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file lang-lab-poc-userfiles/function_test.ax
```

Both runs produce **identical** Core IR and **identical** output. No randomness, no timestamps in Core IR, no environment variable influence.

This is a design requirement.

---

## What to Try Next

1. **Modify the grammar** — Add a new expression type in `parsing.yaml`
2. **Add a schema node** — Define the AST structure in `ast_schema.yaml`
3. **Define normalisation rules** — Transform surface constructs to NF in `normalisation.yaml`
4. **Run your extended language** — Lowering (fixed, embedded) handles the rest

Example: Add a boolean literal

**`lexer.yaml`:**

```yaml
literals:
  bool:
    values: ["true", "false"]
```

**`parsing.yaml`:**

```yaml
Literal:
  - 'BOOL'
```

**`ast_schema.yaml`:**

```yaml
BoolLit:
  fields:
    value: bool
```

**`normalisation.yaml`:**

```yaml
normalisation:
  rules:
    - match:
        node: BoolLit
      rewrite:
        node: BoolLit  # Already in NF
```

Mechanical Lowering (fixed and embedded) automatically transforms NF nodes to Core IR.

Now you can use `true` and `false` in your programs.

---

## Inspection and Debugging

### View Core IR

To see the emitted Core IR:

```bash
./target/release/axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --normalisation lang-lab-poc-userfiles/normalisation.yaml \
  --file lang-lab-poc-userfiles/function_test.ax \
  --inspect core-ir
```

This shows the exact Core IR produced by mechanical lowering.

### Axis Lang Output Boundary

Axis Lang stops at Core IR emission. There is no built-in execution, validation, or code generation in Axis Lang.

Bridges (out of scope) may consume Core IR for various purposes.

---

## Next Steps

You've successfully:
- ✅ Built the compiler
- ✅ Run the complete Axis Lang pipeline
- ✅ Modified a program and observed deterministic Core IR output
- ✅ Understood the role of each specification file

**Continue to [User Guide](USER_GUIDE.md)** to learn how to design your own language from scratch.
