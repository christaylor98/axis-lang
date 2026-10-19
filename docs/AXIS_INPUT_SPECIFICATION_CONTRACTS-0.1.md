# Axis Lang — Input Specification Contracts

## 1. Scope and Authority

This document defines all user-supplied input files required by the Axis Lang compilation pipeline.

A user input is a file that:
- Defines lexical structure, syntactic grammar, AST schema, or semantic lowering rules
- Is consumed by Language Lab to produce Core IR from source code
- Is not produced by Language Lab itself

User inputs are:
- lexer.yaml
- parsing.yaml
- ast_schema.yaml
- lowering.yaml

User inputs are NOT:
- Source files written in the target language
- Registry files
- Configuration files
- Build artifacts

Pipeline stages consume inputs as follows:
- Lexing consumes lexer.yaml
- Parsing consumes parsing.yaml
- Schema projection consumes ast_schema.yaml
- Lowering consumes lowering.yaml

Lowering is the sole semantic authority. All other inputs define structure only.

---

## 2. Pipeline Overview (Input-Centric)

| Stage              | Input File(s)   | Consumes              | Produces          | Semantic Authority |
|--------------------|-----------------|-----------------------|-------------------|--------------------|
| Lexing             | lexer.yaml      | Source text           | Token stream      | No                 |
| Parsing            | parsing.yaml    | Token stream          | Parse tree        | No                 |
| Schema projection  | ast_schema.yaml | Parse tree            | Schema AST        | No                 |
| Lowering           | lowering.yaml   | Schema AST, Registry  | Core IR bundle    | Yes                |
| Execution          | None            | Core IR bundle        | Runtime values    | No                 |

Semantic authority resides exclusively in lowering. All prior stages perform structural transformations without assigning meaning.

---

## 3. Lexer Specification (lexer.yaml)

### Required Top-Level Keys

- `lexer`: Root container for lexer specification

### Required Nested Keys

- `lexer.charset`: Character set (ascii only)
- `lexer.case_sensitive`: Boolean flag for case sensitivity
- `lexer.keywords`: List of reserved keyword strings
- `lexer.identifiers`: Identifier pattern specification
- `lexer.punctuation`: List of punctuation tokens

### Optional Keys

- `lexer.whitespace`: Whitespace handling specification
- `lexer.comments`: Comment pattern specifications
- `lexer.literals`: Literal token specifications

### Determinism Requirements

The lexer specification MUST produce a deterministic token stream. Given identical source text:
- Token sequence MUST be identical
- Token positions MUST be identical
- Token values MUST be identical

Lexer specifications MUST NOT:
- Depend on system locale
- Depend on time or randomness
- Depend on external state

### What the Lexer MUST Define

- How source text maps to tokens
- Which characters are valid
- How whitespace is handled
- Which patterns constitute errors

### What the Lexer MUST NOT Define

- Syntax rules
- Parse structure
- Semantic meaning
- Type information
- Execution behavior

### What is Ignored

- Tokens marked with `skip: true`
- Content within comment patterns

### What is Preserved

- All non-skipped token text
- Source positions (spans)
- Token classification

### What is Forbidden

- Multi-pass lexing
- Stateful lexing requiring parser feedback
- Context-dependent tokenization beyond keyword recognition
- Lexer-level semantic interpretation

### Error Conditions

The lexer MUST error when:
- Encountering characters outside the specified charset
- Encountering unrecognized character sequences
- Detecting patterns explicitly forbidden in the specification

---

## 4. Parser Specification (parsing.yaml)

### Required Structure

- `parser`: Root container
- `parser.start`: Name of the start rule
- `parser.grammar`: Map of rule names to rule definitions

### Start Symbol Rules

- The `start` field MUST name a rule defined in `grammar`
- Parsing begins at the start rule
- The start rule MUST derive the entire program structure

### Grammar Rule Form

Each rule maps a name to a list of alternative productions. Each production is a string containing:
- Literal tokens in double quotes
- Non-terminal rule names without quotes
- Terminal token type names in uppercase
- Repetition operators: `*` (zero or more), `+` (one or more), `?` (zero or one)
- Grouping with parentheses

### Token References

- Terminal tokens reference types defined in lexer.yaml
- Reserved keywords are referenced as string literals
- Punctuation is referenced as string literals

### Ordering and Precedence Rules

- Alternatives are tried in the order specified
- First matching alternative is selected
- No automatic precedence resolution
- No automatic left-recursion elimination

### What Grammar Rules Can Express

- Sequence
- Alternation
- Repetition
- Optional elements
- Grouping

### What Grammar Rules Cannot Express

- Semantic constraints
- Type requirements
- Name resolution
- Context-sensitive parsing beyond what the parser runtime supports

### Ambiguity Handling

Ambiguous grammars are accepted. The parser runtime selects the first successful parse according to its algorithm. Ambiguity is NOT an error unless it causes parse failure.

### Invalid Grammar Conditions

A grammar is invalid if:
- The start symbol is undefined
- A rule references an undefined non-terminal
- A rule references an undefined terminal type
- Syntax is malformed

---

## 5. AST Schema Specification (ast_schema.yaml)

### Required Structure

- `nodes`: Root container mapping node names to node definitions

### Node Definitions

Each node definition contains exactly one of:
- `fields`: Map of field names to field types
- `union`: List of variant node names

### Field Types

Valid field types are:
- Node type name (e.g., `Ident`, `Expr`)
- `[NodeType]` for list fields
- `int` for integer fields
- `string` for string fields
- `bool` for boolean fields

### Union Types

Union nodes specify alternatives. Each variant MUST be a defined node type.

### List Semantics

List fields accept zero or more instances of the specified element type. Lists preserve order.

### Annotation Extraction Rules

Annotations are metadata extracted during schema projection. Annotations:
- Are not part of the semantic node structure
- May be preserved or discarded
- MUST NOT influence lowering semantics

### Relationship Between Parse Tree and AST

The schema defines a projection from parse tree to AST:
- Parse tree nodes map to AST nodes by name
- Parse tree children map to AST fields by position or label
- Unmapped parse tree nodes are ignored
- Missing fields cause projection failure

### Schema Projection Guarantees

Schema projection guarantees:
- Every parse tree matching the grammar CAN be projected if the schema matches the grammar
- Projection is deterministic
- Projection preserves structure

### Fatal Errors

Schema projection MUST fail when:
- A required field is missing
- A field type does not match the parse tree structure
- A union variant is invalid

### Synchronization Requirements

The schema MUST stay synchronized with:
- The parser grammar (structural compatibility)
- The lowering specification (lowering consumes schema AST)

Mismatches cause pipeline failure.

---

## 6. Lowering Specification (lowering.yaml)

### Required Structure

- `lowering`: Root container
- `lowering.rules`: List of lowering rules

### Match Rules

Each rule contains:
- `match`: Pattern matching a schema AST node
  - `node`: Node type name
  - Field bindings as key-value pairs
  - Variable capture with identifiers

### Emit Rules

Each rule contains:
- `emit`: Template for Core IR emission
  - `node`: Core IR node type
  - Field assignments
  - Recursive lowering with `lower:` directives

### Variable Binding Responsibility

Variables bound in `match` patterns:
- Capture schema AST substructure
- Are available in the `emit` template
- Have scope limited to the rule

### Annotation Propagation Rules

Annotations from schema AST:
- MAY be propagated to Core IR
- MUST NOT influence Core IR semantics
- Are metadata only

### Semantic Authority

Lowering is the ONLY place where semantics are defined. Lowering:
- Assigns meaning to AST constructs
- Resolves names against the registry
- Determines what Core IR nodes are emitted

No other pipeline stage assigns meaning.

### What Lowering is Allowed to Invent

Lowering may:
- Desugar surface constructs into primitive Core IR
- Generate fresh variable names for desugaring
- Reorder operations to match Core IR semantics
- Insert explicit operations for implicit surface behavior

### What Lowering is Forbidden to Invent

Lowering MUST NOT:
- Introduce semantics not present in the surface language design
- Infer types or constraints beyond what the schema provides
- Perform optimization or transformation beyond desugaring
- Depend on execution results

### Determinism Requirements

Lowering MUST be deterministic. Given identical schema AST:
- Core IR structure MUST be identical
- Node ordering MUST be canonical
- Variable names MUST be deterministic

### Equivalence Requirements

Two programs with identical Core IR have identical semantics. Core IR equivalence is the acceptance criterion.

---

## 7. Core IR Boundary (Input/Output Contract)

### What Enters Lowering

- Schema AST (from schema projection)
- Registry (function signatures and identities)

### What Exits Lowering

- Core IR bundle containing:
  - A single root Core IR term
  - Optional node identity mappings
  - Optional annotations

### What Core IR Guarantees

- Semantic completeness: meaning is fully explicit
- Configuration-free: no policy or settings embedded
- Name-free: all symbolic references resolved to numeric handles
- Immutable: never modified after emission
- Deterministic: identical inputs produce identical Core IR

### What Core IR Never Contains

- Surface syntax
- Unresolved names
- Type annotations (not yet enforced)
- Configuration or profile information
- Comments or documentation
- Implicit behavior

### Immutability Contract

Once emitted, Core IR is immutable. No pipeline stage, optimization pass, or runtime component may modify Core IR structure or semantics.

### Configuration-Free Contract

Core IR contains no configuration. Execution profiles, optimization flags, and policy toggles:
- Influence which Core IR is produced
- Do NOT influence what Core IR means

### Equivalence Signal

Core IR equivalence is the acceptance signal. If two surface programs produce equivalent Core IR, they have identical semantics by definition.

---

## 8. Execution Boundary (Non-Input Clarification)

### What Execution Consumes

- Core IR bundle (immutable)
- Registry (for external function dispatch)

### What Execution Ignores

- Surface syntax
- Parser grammar
- Schema definitions
- Annotations (unless explicitly used for diagnostics)

### What Execution Must Not Infer

- Semantics not present in Core IR
- Type information not encoded
- Optimization opportunities

### What Execution Must Never Modify

- Core IR structure
- Core IR semantics
- Registry definitions

### Non-Semantic Status

Execution is NOT a semantic input surface. It is a projection of Core IR semantics into observable behavior. Execution:
- Evaluates Core IR terms
- Dispatches to registry functions
- Produces values

Execution does NOT:
- Interpret or infer meaning
- Optimize or transform
- Compensate for missing semantics

---

## 9. Sealed vs Unsealed Builds (Input Implications)

### Build-Time vs Runtime Inputs

Unsealed builds:
- Load lexer.yaml at runtime
- Load parsing.yaml at runtime
- Load ast_schema.yaml at runtime
- Load lowering.yaml at runtime

Sealed builds:
- Embed lexer.yaml at compile time
- Embed parsing.yaml at compile time
- Embed ast_schema.yaml at compile time
- Embed lowering.yaml at compile time

### What Sealing Embeds

- Full content of all input specifications
- SHA-256 hashes of input files
- Build timestamp and git commit
- Compiler version

### What Sealing Forbids

Sealed builds MUST reject:
- External specification paths
- Environment variable overrides
- Runtime configuration files
- Fallback to default specifications

### Invalidation Conditions

A sealed build is invalid if:
- Any embedded specification is modified
- Embedded hashes do not match original files
- Binary is tampered with

Sealed builds provide determinism and security guarantees. Unsealed builds provide flexibility.

---

## 10. Invariants and Failure Conditions

### Non-Negotiable Invariants

- **Determinism**: Identical inputs produce identical outputs at every pipeline stage
- **Semantic Authority**: Only lowering assigns meaning
- **One-Way Information Flow**: Later stages cannot influence earlier stages
- **No Implicit Meaning**: All semantics are explicit in Core IR
- **Immutability**: Core IR is never modified after emission
- **Totality**: All admitted surface constructs MUST have lowering rules
- **Structural Integrity**: Schema AST MUST match both grammar and lowering expectations
- **Registry Binding**: All function calls MUST resolve to registry entries during lowering

### Explicit Failure Conditions

The compiler MUST error when:

- **Lexer Specification Invalid**:
  - Malformed YAML
  - Undefined charset
  - Invalid regular expressions

- **Parser Specification Invalid**:
  - Undefined start symbol
  - Undefined non-terminal reference
  - Malformed grammar rule

- **Schema Specification Invalid**:
  - Undefined node type in union
  - Invalid field type
  - Structural mismatch with grammar

- **Lowering Specification Invalid**:
  - Undefined node type in match pattern
  - Undefined Core IR node type in emit template
  - Missing lowering rule for admitted construct

- **Lexing Failure**:
  - Unrecognized character
  - Invalid token pattern
  - Charset violation

- **Parsing Failure**:
  - No valid parse exists
  - Start symbol does not match input

- **Schema Projection Failure**:
  - Parse tree structure does not match schema
  - Required field missing
  - Type mismatch

- **Lowering Failure**:
  - Unresolved name
  - Registry lookup failure
  - No matching lowering rule
  - Arity mismatch
  - Profile violation

- **Sealed Build Mismatch**:
  - External spec path provided to sealed binary
  - Embedded spec hash mismatch

Failures are fatal. No recovery, no fallback, no implicit defaults.
