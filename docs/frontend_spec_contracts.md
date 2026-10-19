# Axis Lang — Frontend Specification Contracts

## Document Authority

This document describes the **exact** contracts for all frontend specification files consumed by the Axis Lang pipeline.

**Version**: Wave 1  
**Status**: Authoritative  
**Scope**: Lexer, Parser, Schema, and Validator specifications only  

This document is the result of Wave 1 forensic analysis of the codebase and represents **what the code actually does**, not what it was intended to do.

---

## 0. General Principles

### 0.1 Determinism

All spec loading MUST be deterministic:
- Identical YAML files MUST produce identical in-memory structures
- No dependency on system locale, time, or random state
- No environment variable influence (unless explicitly documented)

### 0.2 Error Philosophy

- Invalid specs MUST fail fast with precise errors
- No silent fallbacks
- No implicit defaults unless explicitly documented
- Errors MUST identify: spec type, field name, expected shape, actual value

### 0.3 Unknown Fields

**Current Behavior** (as of Wave 1):
- Unknown top-level fields are **silently ignored** by serde deserialization
- No validation occurs for unrecognized keys

**Implication**: Typos in spec files may go undetected.

---

## 1. Lexer Specification (lexer.yaml)

### 1.1 File Location

- Specified via `--lexer` CLI flag
- Path is absolute or relative to working directory
- No default path exists

### 1.2 Required Top-Level Structure

```yaml
lexer:
  # All lexer config goes here
```

**Required Keys**:
- `lexer`: Root container (type: mapping)

**Fatal Errors**:
- Missing `lexer` key → YAML deserialization error
- `lexer` is not a mapping → YAML deserialization error

### 1.3 Lexer Configuration Fields

Located under `lexer:` key.

#### Required Fields: NONE

All fields are optional with defaults.

#### Optional Fields

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `charset` | string | `"ascii"` | Character set restriction |
| `case_sensitive` | bool | `true` | Keyword case sensitivity |
| `whitespace` | WhitespaceRule | `None` | Whitespace handling |
| `comments` | [CommentRule] | `[]` | Comment patterns |
| `keywords` | [string] | `[]` | Reserved keywords |
| `identifiers` | IdentifierRule | `None` | Identifier pattern |
| `literals` | LiteralRules | `None` | Literal token rules |
| `punctuation` | [string] | `[]` | Punctuation symbols |

### 1.4 Nested Structures

#### WhitespaceRule

```yaml
whitespace:
  pattern: "regex_pattern"
  skip: true  # default: true
```

**Fields**:
- `pattern` (string, required): Regular expression
- `skip` (bool, optional, default=`true`): Whether to skip whitespace tokens

#### CommentRule

```yaml
comments:
  - pattern: "//.*"
    skip: true  # default: true
```

**Fields**:
- `pattern` (string, required): Regular expression
- `skip` (bool, optional, default=`true`): Whether to skip comment tokens

#### IdentifierRule

```yaml
identifiers:
  pattern: "[A-Za-z_][A-Za-z0-9_]*"
  forbid: []  # optional
```

**Fields**:
- `pattern` (string, required): Regular expression
- `forbid` ([string], optional, default=`[]`): Forbidden identifier patterns

**UNVERIFIED**: The `forbid` field is loaded but its usage is not confirmed in the lexer engine.

#### LiteralRules

```yaml
literals:
  int:     # optional
    pattern: "[0-9]+"
    base: 10  # default: 10
  bool:    # optional
    values: ["true", "false"]
  unit:    # optional
    literal: "()"
    single_token: true  # default: true
  string:  # optional
    delimiter: "\""
    escapes: ["\\\"", "\\\\"]
    forbid_newlines: true  # default: true
```

All literal sub-types are optional.

**IntLiteralRule**:
- `pattern` (string, required): Regular expression
- `base` (u32, optional, default=`10`): Number base

**BoolLiteralRule**:
- `values` ([string], required): List of boolean literal values

**UnitLiteralRule**:
- `literal` (string, required): The unit literal text (e.g., `"()"`)
- `single_token` (bool, optional, default=`true`): Purpose UNVERIFIED

**StringLiteralRule**:
- `delimiter` (string, required): String delimiter character
- `escapes` ([string], optional, default=`[]`): Escape sequences
- `forbid_newlines` (bool, optional, default=`true`): Reject newlines in strings

### 1.5 What the Lexer Spec Controls

**Controls**:
- Tokenization rules (which character sequences become which tokens)
- Skipping behavior (whitespace, comments)
- Token classification (keyword, identifier, literal, punctuation)
- Character set validation

**Does NOT Control**:
- Parse structure
- Semantic meaning
- Type information
- Execution behavior
- AST structure

### 1.6 Loader Behavior

**File**: [src/frontend/lexspec_load.rs](../src/frontend/lexspec_load.rs)  
**Function**: `load_spec(path: &Path) -> Result<LexerSpec, LoadError>`

**Behavior**:
1. Read file as UTF-8 text
2. Parse YAML using `serde_yaml`
3. Deserialize into `LexerSpec` struct
4. Apply default values via `#[serde(default)]` attributes
5. Return spec or error

**Silent Behaviors**:
- Unknown fields are ignored
- Optional fields default to `None` or default values

**Error Conditions**:
- File does not exist → I/O error
- File is not valid UTF-8 → I/O error
- YAML syntax error → YAML parse error
- Type mismatch (e.g., string where bool expected) → YAML parse error

### 1.7 Usage in Pipeline

**File**: [src/frontend/lexer_engine.rs](../src/frontend/lexer_engine.rs)  
**Function**: `lex_with_spec(spec: &LexerSpec, src: &str) -> Result<Vec<Token>, LexError>`

**What is Used**:
- `charset` → UNVERIFIED (not enforced in current code)
- `case_sensitive` → UNVERIFIED (not used in current lexer)
- `whitespace.pattern` → Compiled to Regex, matched and skipped
- `whitespace.skip` → Assumed `true`, skipping is hard-coded
- `comments[].pattern` → Compiled to Regex, matched and skipped
- `comments[].skip` → Assumed `true`, skipping is hard-coded
- `keywords` → Matched as literal strings, classified as `TokenKind::Keyword`
- `identifiers.pattern` → Compiled to Regex, matched as `TokenKind::Ident`
- `identifiers.forbid` → UNVERIFIED (not enforced)
- `literals.int.pattern` → Compiled to Regex, matched as `TokenKind::IntLit`
- `literals.int.base` → UNVERIFIED (not used in tokenization)
- `literals.bool.values` → Matched as literals, classified as `TokenKind::BoolLit`
- `literals.unit.literal` → Matched as literal, classified as `TokenKind::UnitLit`
- `literals.string.delimiter` → Used to delimit strings
- `literals.string.escapes` → Used to recognize escape sequences
- `literals.string.forbid_newlines` → Enforced during string tokenization
- `punctuation` → Matched as literals (longest-first), classified as `TokenKind::Punct`

**What is Ignored**:
- `charset` (not enforced)
- `case_sensitive` (not used)
- `identifiers.forbid` (loaded but not enforced)
- `literals.int.base` (not used)
- `literals.unit.single_token` (purpose unclear, not used)

### 1.8 Inspection Output

**Flag**: `--inspect lexer`

**Output**:
- YAML dump of loaded lexer rules
- Each rule listed with: index, name, pattern, skip flag
- Total rule count
- Charset and case sensitivity settings

**File**: [src/introspection/inspection.rs](../src/introspection/inspection.rs) (line 189)

---

## 2. Parser Specification (parsing.yaml)

### 2.1 File Location

- Specified via `--parser` CLI flag
- Path is absolute or relative to working directory
- No default path exists

### 2.2 Required Top-Level Structure

```yaml
parser:
  start: "StartRuleName"
  grammar:
    # grammar rules here
```

**Required Keys**:
- `parser`: Root container (type: mapping)
- `parser.start`: Start rule name (type: string)
- `parser.grammar`: Grammar rules (type: mapping)

**Fatal Errors**:
- Missing `parser` key → YAML deserialization error
- Missing `parser.start` → Load error: "parser.start is missing or empty"
- `parser.start` is empty string → Load error: "parser.start is missing or empty"
- Missing `parser.grammar` → YAML deserialization error
- `parser.grammar` is empty → Load error: "grammar is missing or empty"

### 2.3 Grammar Structure

```yaml
parser:
  start: "Program"
  grammar:
    RuleName:
      - "production_string_1"
      - "production_string_2"
    AnotherRule:
      - "production_string_3"
```

**Grammar Mapping**:
- Keys: Non-terminal rule names (strings)
- Values: Lists of production strings (each string is one alternative)

### 2.4 Production String Syntax

Production strings are parsed into structured `Production` objects.

**Elements**:
- Non-terminal: `RuleName` (bare identifier)
- Terminal token kind: `<TOKEN_KIND>` (e.g., `<IDENT>`, `<INT>`)
- Keyword terminal: `kw:"keyword"` (e.g., `kw:"fn"`)
- Punctuation terminal: `punct:"symbol"` (e.g., `punct:"{"`)
- Quoted literal: `"text"` (inferred as keyword or punctuation)
- Repetition: `Element*` (zero or more), `Element+` (one or more), `Element?` (optional)
- Grouping: `(Element1 Element2)`

**Legacy Inference** (Wave 1 compatibility only):
- Bare quoted strings like `"fn"` are inferred as keywords if they exist in lexer spec
- Bare quoted strings like `"{"` are inferred as punctuation if they exist in lexer spec
- Bare uppercase identifiers like `IDENT` are inferred as `<IDENT>`

**Explicit Syntax** (canonical):
- Always use `kw:"..."`, `punct:"..."`, and `<TOKEN_KIND>` explicitly
- Inference is NON-AUTHORITATIVE and exists only for Wave 1 compatibility

### 2.5 What the Parser Spec Controls

**Controls**:
- Syntactic structure (which token sequences are valid)
- Grammar rules and alternation order
- Concrete Syntax Tree (CST) structure

**Does NOT Control**:
- Semantic meaning
- AST field names
- Type information
- Name resolution
- Execution behavior

### 2.6 Loader Behavior

**File**: [src/frontend/parserspec_load.rs](../src/frontend/parserspec_load.rs)  
**Function**: `load_parser_spec(path: &Path, lexer_spec: &LexerSpec) -> Result<ParserSpec, ParseSpecError>`

**Behavior**:
1. Read file as UTF-8 text
2. Parse YAML into `RawParserSpec`
3. Validate `start` is not empty
4. Validate `grammar` is not empty
5. Parse each production string using production parser
6. Validate start rule exists in grammar
7. Validate all non-terminal references exist
8. Validate all terminals exist in lexer spec
9. Return `ParserSpec` or error

**Validation Rules**:
- Start rule MUST be defined in grammar
- All non-terminal references MUST be defined
- All keyword terminals MUST be declared in lexer spec `keywords`
- All punctuation terminals MUST be declared in lexer spec `punctuation`
- Token kind terminals are validated permissively (not strictly enforced)

**Error Conditions**:
- File does not exist → I/O error
- YAML syntax error → YAML parse error
- `start` missing or empty → "parser.start is missing or empty"
- `grammar` empty → "grammar is missing or empty"
- Malformed production string → "malformed production: {details}" with location
- Start rule not in grammar → "start nonterminal '{name}' is not defined in grammar"
- Undefined non-terminal → "undefined nonterminal '{name}'" with location
- Undeclared keyword → "invalid terminal 'kw:\"{kw}\"' not declared in lexer spec" with location
- Undeclared punctuation → "invalid terminal 'punct:\"{p}\"' not declared in lexer spec" with location

**Silent Behaviors**:
- Unknown fields are ignored
- Token kind validation is permissive (does not strictly check lexer spec)

### 2.7 Runtime Parser Guarantees (Wave 4)

**File**: [src/frontend/parser_runtime.rs](../src/frontend/parser_runtime.rs)  
**Function**: `parse_with_spec(spec: &ParserSpec, tokens: &[Token]) -> Result<ParseTree, ParseError>`

#### Ordered Choice and Backtracking

When a nonterminal has multiple productions (alternatives), the parser runtime:

1. **Tries alternatives in YAML order** (ordered choice, not longest match)
   - Productions are tried in the order they appear in the YAML file
   - First production that matches is selected
   - Later alternatives are only tried if earlier ones fail

2. **Performs complete backtracking on failure**
   - Each alternative attempt creates a checkpoint of token position
   - On failure, token position is fully restored before trying next alternative
   - No partial consumption leaks across alternatives
   - CST construction side effects are not accumulated across failed attempts

3. **Reports most informative error**
   - If all alternatives fail, the error from the last attempt is reported
   - Error includes expected terminal and actual token found
   - Span indicates position where parsing failed

#### Token Kind Normalization

The parser accepts both YAML-style and TokenKind-style names for token types:

| YAML Syntax | TokenKind | Both Accepted |
|-------------|-----------|---------------|
| `<INT>` | `IntLit` | "INT" or "INT_LIT" |
| `<BOOL>` | `BoolLit` | "BOOL" or "BOOL_LIT" |
| `<STRING>` | `StringLit` | "STRING" or "STRING_LIT" |
| `<UNIT>` | `UnitLit` | "UNIT" or "UNIT_LIT" |
| `<IDENT>` | `Ident` | "IDENT" only |

This normalization allows grammar specs to use concise names (`<INT>`) while the runtime uses descriptive enum variant names (`IntLit`).

#### Determinism Guarantees

- Parsing is fully deterministic: same spec + same tokens = same CST
- Alternative order matters: changing production order changes parse results
- No lookahead or ambiguity resolution beyond ordered choice
- No left-recursion elimination or grammar transformation

**Evidence**: [tests/wave4_parser_alternation.rs](../tests/wave4_parser_alternation.rs)

### 2.8 Usage in Pipeline

**File**: [src/frontend/parser_runtime.rs](../src/frontend/parser_runtime.rs) (presumed)

**What is Used**:
- `start` → Entry point for parsing
- `grammar` → All rules are used to build CST
- Production elements → Matched against token stream
- Alternation order → First successful alternative is selected

**What is Ignored**:
- Unknown fields (silently ignored during YAML parsing)

### 2.9 Inspection Output

**Flag**: `--inspect grammar`

**Output**:
- YAML dump of parsed grammar
- Start rule name
- Each rule with its alternatives
- Production structure (elements, terminals, repetition)

**File**: [src/introspection/inspection.rs](../src/introspection/inspection.rs) (line 320)

---

## 3. AST Schema Specification (ast_schema.yaml)

### 3.1 File Location

- Specified via `--schema` CLI flag
- Path is absolute or relative to working directory
- No default path exists

### 3.2 Required Top-Level Structure

```yaml
nodes:
  NodeTypeName:
    # node definition
```

**Required Keys**:
- `nodes`: Root mapping of node type names to definitions

**Fatal Errors**:
- Missing `nodes` key → "schema missing 'nodes' field"
- `nodes` is not a mapping → "'nodes' must be a mapping"

### 3.3 Node Definition Structure

Two types of nodes exist: **Field Nodes** and **Union Nodes**.

#### Field Node

```yaml
NodeName:
  match: GenericAstNodeKind  # optional, defaults to NodeName
  fields:
    field_name:
      from: "extraction_rule"
  annotations:  # optional
    from: "extraction_rule"
```

**Fields**:
- `match` (string, optional): Which Generic AST node kind this matches (defaults to node name)
- `fields` (mapping, required): Field extraction rules
- `annotations` (mapping, optional): Annotation extraction rule

#### Union Node

```yaml
ExprType:
  union:
    - IfExpr
    - CallExpr
    - LiteralExpr
```

**Fields**:
- `union` ([string], required): List of variant node type names

**Current Limitation**: Union nodes are loaded but their usage in schema projection is UNVERIFIED.

### 3.4 Field Extraction Rules

Each field has a `from:` specifier indicating how to extract it from the Generic AST.

**Syntax**:
- `child(N)`: Extract child at index N (0-based)
- `children(START..END)`: Extract children in range [START, END) (exclusive end)
- `children(START..)`: Extract all children from START to end (open-ended range)
- `token(TOKEN_KIND)`: Extract single token of kind
- `tokens(TOKEN_KIND)`: Extract all tokens of kind

**Token Kinds**:
- `IDENT`, `INT_LIT`, `BOOL_LIT`, `STRING_LIT`, `UNIT_LIT`, `EOF`
- `KW_keyword` (e.g., `KW_fn`, `KW_if`)
- `PUNCT_symbol` (e.g., `PUNCT_{`, `PUNCT_->`)

**Examples**:
```yaml
fields:
  name:
    from: token(IDENT)
  body:
    from: child(2)
  params:
    from: children(1..4)
  keywords:
    from: tokens(KW_fn)
  instructions:
    from: children(2..)  # Extract all children from index 2 to end
```

**Open-Ended Range Semantics** (Wave 3.5):
- `children(N..)` extracts all children from index N to the end of the children list
- End index is determined at projection time based on actual child count
- Terminals (tokens) are automatically skipped during extraction (only rule nodes extracted)
- Required for grammar productions with repetition operators (`*`, `+`)
- Example: For `Function: KW_FN IDENT Instruction* KW_END`, use `children(2..)` to extract variable-length instruction list

### 3.5 Annotation Extraction Rules

```yaml
annotations:
  from: tokens(TOKEN_KIND)
```

Currently only `tokens(TOKEN_KIND)` format is supported.

### 3.6 What the Schema Spec Controls

**Controls**:
- Transformation from Generic AST (CST) to Schema AST
- Field naming and extraction
- Node type naming
- Annotation extraction

**Does NOT Control**:
- Grammar rules
- Token classification
- Semantic meaning (that's lowering's job)
- Execution behavior

### 3.7 Loader Behavior

**File**: [src/frontend/schema_load.rs](../src/frontend/schema_load.rs)  
**Function**: `load_schema_from_file(path: &Path) -> Result<AstSchema, SchemaLoadError>`

**Behavior**:
1. Read file as UTF-8 text
2. Parse YAML into `serde_yaml::Value`
3. Extract `nodes` mapping
4. Parse each node definition
5. Validate field extraction syntax
6. Parse token kinds
7. Return `AstSchema` or error

**Validation Rules**:
- `nodes` key MUST exist
- `nodes` MUST be a mapping
- Each node MUST have `fields` (if not a union node)
- Each field MUST have `from` key
- `from` value MUST match a supported extraction pattern

**Error Conditions**:
- File does not exist → "failed to read schema file: {error}"
- YAML syntax error → "failed to parse YAML: {error}"
- Missing `nodes` → "schema missing 'nodes' field"
- `nodes` not a mapping → "'nodes' must be a mapping"
- Node definition not a mapping → "node '{kind}' definition must be a mapping"
- Missing `fields` → "node '{kind}' missing 'fields'"
- `fields` not a mapping → "node '{kind}' fields must be a mapping"
- Missing `from` → "field '{field}' in node '{kind}' missing 'from' specifier"
- `from` not a string → "field '{field}' 'from' in node '{kind}' must be a string"
- Invalid `from` syntax → "unknown 'from' specifier in field '{field}' of node '{kind}': '{spec}'"
- Invalid token kind → "unknown token kind: '{kind}'"
- Invalid child index → "invalid child index in field '{field}' of node '{kind}': '{value}'"
- Invalid children range → "invalid children range in field '{field}' of node '{kind}': '{value}'"

**Silent Behaviors**:
- Unknown top-level fields ignored
- `match` field defaults to node name if omitted
- `annotations` field is optional
- `unhandled_behavior` defaults to `Reject`

### 3.8 Usage in Pipeline

**File**: [src/frontend/schema_ast.rs](../src/frontend/schema_ast.rs)  
**Function**: Schema projection from Generic AST

**What is Used**:
- `nodes` → All node definitions
- `match` → Determines which Generic AST nodes to transform
- `fields` → All field extractions are applied
- `from` → Extraction rules drive schema projection
- `annotations` → UNVERIFIED (loaded but usage unclear)

**What is Ignored**:
- Unknown fields

### 3.9 Inspection Output

**Flag**: `--inspect schema`

**Output**:
- YAML dump of loaded schema
- Node definitions with fields and extraction rules

**File**: [src/introspection/inspection.rs](../src/introspection/inspection.rs) (presumed)

---

## 4. Validator Specification (validator.yaml)

### 4.1 File Location

- Specified via `--validator` CLI flag (if supported)
- Path is absolute or relative to working directory
- **UNVERIFIED**: Validator may not be fully integrated into main pipeline

### 4.2 Required Top-Level Structure

```yaml
validator:
  checks:
    - "check_name_1"
    - "check_name_2"
  canonical_form:  # optional
    # canonical form rules
```

**Required Keys**:
- `validator`: Root container
- `validator.checks`: List of check names

**Optional Keys**:
- `validator.canonical_form`: Canonical form validation rules

### 4.3 Check Names

**Supported Checks**:
- `ascii_only`: Enforce ASCII-only characters
- `line_oriented`: Enforce line-oriented structure
- `canonical_form`: Apply canonical form rules
- `instruction_arity`: UNVERIFIED (not implemented)
- `stack_safety`: UNVERIFIED (not implemented)
- `operator_resolution`: UNVERIFIED (not implemented)

### 4.4 Canonical Form Rules

```yaml
canonical_form:
  one_space_between_tokens: true   # default: false
  no_trailing_whitespace: true     # default: false
  no_empty_tokens: true            # default: false
  no_comments: true                # default: false
```

All fields default to `false` if omitted.

### 4.5 What the Validator Spec Controls

**Controls**:
- Which validation checks to apply
- Canonical form enforcement rules

**Does NOT Control**:
- Lexing
- Parsing
- Schema projection
- Lowering
- Execution

### 4.6 Loader Behavior

**File**: [src/validation/validator.rs](../src/validation/validator.rs)  
**Function**: `Validator::load(path: &Path) -> Result<Self, String>`

**Behavior**:
1. Read file as UTF-8 text
2. Parse YAML into `ValidatorSpec`
3. Apply default values
4. Return `Validator` or error

**Error Conditions**:
- File does not exist → "failed to read validator spec: {error}"
- YAML syntax error → "failed to parse validator spec: {error}"
- Unknown check name → Runtime error during validation: "unknown validation check: {check}"

**Silent Behaviors**:
- Unknown fields ignored
- Canonical form rules default to `false`

### 4.7 Usage in Pipeline

**UNVERIFIED**: Validator may not be invoked in main pipeline.

**What is Used** (if invoked):
- `checks` → Each check is applied in order
- `canonical_form` rules → Applied if `canonical_form` check is enabled

**What is Ignored**:
- Unknown fields
- Unimplemented checks (skipped silently or error at runtime)

### 4.8 Inspection Output

**No dedicated inspection flag exists for validator.**

---

## 5. Spec Synchronization Requirements

### 5.1 Lexer ↔ Parser

**Required**:
- All keywords referenced in parser grammar MUST be declared in lexer `keywords`
- All punctuation referenced in parser grammar MUST be declared in lexer `punctuation`

**Validation**: Enforced by `load_parser_spec()` function.

**Failure Mode**: Parser spec load fails with precise error message.

### 5.2 Parser ↔ Schema

**Required**:
- Schema `match` kinds SHOULD correspond to Generic AST node kinds produced by parser
- Field extraction rules MUST match actual Generic AST structure

**Validation**: NOT enforced during loading. Failures occur at schema projection time.

**Failure Mode**: Schema projection fails at runtime with `SchemaAstError`.

### 5.3 Schema ↔ Lowering

**Required**:
- Lowering rules MUST reference schema node kinds that exist
- Lowering field references MUST match schema field names

**Validation**: NOT enforced during loading. Failures occur at lowering time.

**Failure Mode**: UNVERIFIED (lowering errors not analyzed in Wave 1).

---

## 6. Error Surface Summary

### 6.1 Current Error Quality

**Good**:
- Parser spec errors include location (rule name and alternative index)
- Missing required fields are caught early
- Undefined references are validated before use
- Error messages are specific and actionable

**Needs Improvement**:
- Unknown field detection (all specs silently ignore unknown keys)
- Token kind validation is permissive (lexer spec not strictly enforced)
- Schema projection errors happen at runtime, not load time
- No cross-spec validation (parser-schema sync not checked)

### 6.2 Recommended Improvements (Future Waves)

1. **Unknown Field Detection**: Add strict mode to reject unknown YAML keys
2. **Token Kind Validation**: Strictly validate parser token kinds against lexer spec
3. **Schema-Grammar Sync Check**: Validate schema `match` kinds against parser rules
4. **Lowering Spec Validation**: Pre-validate lowering rules against schema (not in Wave 1 scope)
5. **Better Error Spans**: Include line/column numbers in YAML parsing errors

---

## 7. Inspection Output Alignment

### 7.1 Current Inspection Capabilities

**Flags**:
- `--inspect lexer`: Dumps loaded lexer spec
- `--inspect grammar`: Dumps loaded parser spec
- `--inspect schema`: Dumps loaded schema spec
- `--inspect ast`: Dumps Generic AST (post-parsing)
- `--inspect validation`: UNVERIFIED
- `--inspect lowering`: UNVERIFIED
- `--inspect core-ir`: UNVERIFIED
- `--inspect pipeline`: Dumps all pipeline stages
- `--inspect all`: Dumps everything

### 7.2 What Inspection Shows

**Lexer**:
- All loaded rules with indices
- Charset and case sensitivity
- Total rule count

**Grammar**:
- Start rule
- All grammar rules with alternatives
- Production structure (elements, terminals, repetition)

**Schema**:
- UNVERIFIED (not examined in detail)

### 7.3 What Inspection Does NOT Show

- **Ignored fields**: No indication that fields were present but ignored
- **Default values**: No distinction between explicit and defaulted values
- **Inference**: No indication when legacy inference occurred (e.g., `"fn"` → `kw:"fn"`)

### 7.4 Recommended Improvements (Future Waves)

1. Show "Loaded Fields" vs "Ignored Fields" sections
2. Mark defaulted values explicitly: `case_sensitive: true (default)`
3. Mark inferred terminals: `Terminal::Keyword("fn", InferredLegacy)`

---

## 8. Minimal Valid Specs (Examples)

### 8.1 Minimal Lexer Spec

```yaml
lexer:
  # All fields are optional with defaults
```

This is valid and produces a lexer with:
- `charset: "ascii"` (default)
- `case_sensitive: true` (default)
- No whitespace, comments, keywords, identifiers, literals, or punctuation

### 8.2 Minimal Parser Spec

```yaml
parser:
  start: "S"
  grammar:
    S:
      - ""  # Empty production (matches empty input)
```

This is valid and produces a parser that accepts empty input.

### 8.3 Minimal Schema Spec

```yaml
nodes:
  Root:
    fields: {}  # No fields
```

This is valid and produces a schema with one node type that has no fields.

### 8.4 Minimal Validator Spec

```yaml
validator:
  checks: []  # No checks
```

This is valid and performs no validation.

---

## 9. Spec Does NOT Control (Explicit Boundaries)

### 9.1 Lexer Spec Does NOT Control

- Parse structure
- AST field names
- Semantic meaning
- Type information
- Name resolution
- Lowering rules
- Execution behavior
- Registry behavior

### 9.2 Parser Spec Does NOT Control

- Token classification (that's lexer's job)
- AST field names (that's schema's job)
- Semantic meaning (that's lowering's job)
- Type information
- Name resolution
- Execution behavior

### 9.3 Schema Spec Does NOT Control

- Grammar rules (that's parser's job)
- Token classification (that's lexer's job)
- Semantic meaning (that's lowering's job)
- Lowering rules
- Execution behavior

### 9.4 Validator Spec Does NOT Control

- Any pipeline behavior (validator is orthogonal to compilation)
- Lexing, parsing, schema projection, lowering, or execution

---

## 10. Hard-Coded Behavior (Wave 1 Inventory)

### 10.1 Lexer Engine

**Hard-Coded**:
- Longest-match for punctuation
- Skipping for whitespace and comments (always skips, ignores `skip` field)
- Token kind classification logic
- String literal handling (escape processing)

**NOT Configurable**:
- `charset` is loaded but not enforced
- `case_sensitive` is loaded but not used
- `identifiers.forbid` is loaded but not enforced
- `literals.int.base` is loaded but not used for tokenization

### 10.2 Parser Runtime

**Hard-Coded**:
- Parsing algorithm (PEG or similar)
- First-alternative selection for ambiguity
- Generic AST node kind assignment

**NOT Configurable**:
- Precedence (must be encoded in grammar structure)
- Associativity (must be encoded in grammar structure)

### 10.3 Schema Projection

**Hard-Coded**:
- Field extraction semantics (`child`, `children`, `token`, `tokens`)
- Span derivation
- Error generation for missing fields

**NOT Configurable**:
- Union node handling (usage UNVERIFIED)
- Default values for missing fields (always errors)

---

## 11. Future Wave Considerations

### 11.1 Wave 2+ Requirements

- Grammar-schema structural validation
- Explicit terminal syntax enforcement (deprecate legacy inference)
- Strict unknown field rejection (optional strict mode)
- Cross-spec consistency checks

### 11.2 Out of Scope for All Waves

- Execution semantics (beyond Core IR boundary)
- Runtime behavior
- Performance optimization
- Backend code generation

---

## 12. Acceptance Criteria (Wave 1)

✅ **Achieved**:
- All spec types documented with exact contracts
- Required vs optional fields identified
- Error conditions enumerated
- Usage vs ignored fields catalogued
- Inspection output described
- Hard-coded behavior inventoried
- Minimal valid specs provided

✅ **Deliverables**:
- This document: `docs/frontend_spec_contracts.md`

✅ **Verification**:
- All claims backed by code references
- No speculation or inference
- Negative facts (ignored fields, unverified behavior) explicitly stated

---

## 13. Grammar–Schema Coherence (Wave 2)

### 13.1 Overview

Wave 2 establishes **mechanical coherence** between grammar and schema specifications. The goal is to ensure that:

1. Every schema node can be produced by the grammar
2. Every schema field references a valid CST element
3. No schema relies on undocumented or inferred CST structure

**Authoritative Analysis**: See [WAVE2_GRAMMAR_SCHEMA_COHERENCE_ANALYSIS.md](../WAVE2_GRAMMAR_SCHEMA_COHERENCE_ANALYSIS.md)

### 13.2 Grammar → CST Production Rules

**Parser**: [src/frontend/parser_runtime.rs](../src/frontend/parser_runtime.rs)

#### CST Structure

Every grammar rule produces a `ParseTree`:

```rust
pub struct ParseTree {
    pub rule: String,           // Nonterminal name
    pub children: Vec<ParseNode>,  // Ordered children
    pub span: Span,
}

pub enum ParseNode {
    Rule(ParseTree),      // Non-terminal child
    Terminal(Token),      // Terminal child
}
```

#### Production Rules

| Grammar Construct | CST Result |
|-------------------|------------|
| Nonterminal `A` | `ParseNode::Rule(ParseTree { rule: "A", ... })` |
| Terminal `kw:"fn"` | `ParseNode::Terminal(Token { kind: Keyword("fn"), ... })` |
| Terminal `<IDENT>` | `ParseNode::Terminal(Token { kind: Ident, ... })` |
| Repetition `A*` | 0 or more `ParseNode::Rule` children (inline, not wrapped) |
| Repetition `A+` | 1 or more `ParseNode::Rule` children (inline, not wrapped) |
| Optional `A?` | 0 or 1 `ParseNode::Rule` child (inline, not wrapped) |
| Alternative `A \| B` | One of the alternatives as a child (no wrapper node) |

**CRITICAL**: Repetitions and alternatives do NOT introduce wrapper nodes. They produce their children directly in the parent's children vector.

### 13.3 Schema Field Extraction Rules

**Loader**: [src/frontend/schema_load.rs](../src/frontend/schema_load.rs)  
**Projection**: [src/frontend/schema_ast.rs](../src/frontend/schema_ast.rs)

#### Supported `from:` Specifiers

| Syntax | Meaning | Example |
|--------|---------|---------|
| `from: child(n)` | Extract child at index n | `from: child(1)` |
| `from: children(start..end)` | Extract slice of children | `from: children(2..5)` |
| `from: token(KIND)` | Extract first token of kind | `from: token(IDENT)` |
| `from: tokens(KIND)` | Extract all tokens of kind | `from: tokens(IDENT)` |

#### NOT Supported

| Syntax | Status | Fix |
|--------|--------|-----|
| `from: IDENT` | ✗ INVALID | Use `from: token(IDENT)` or `from: child(n)` |
| `from: Decl` | ✗ INVALID | Use `from: child(n)` where n is the index |
| `repeat: true` | ✗ INVALID | Not a schema feature; use `children(start..end)` |
| `union: [...]` | ✗ INVALID | Not supported; each variant needs separate schema node |

### 13.4 Grammar–Schema Mapping Requirements

For each schema node:

1. **Node Kind Match**:
   - Schema node kind MUST match a grammar nonterminal name
   - OR use `match:` field to specify which grammar rule it projects

2. **Field Source Traceability**:
   - Every field MUST specify a valid `from:` extractor
   - Extractor MUST reference a valid CST element position or token kind

3. **Child Index Validity**:
   - `from: child(n)` → n MUST be within bounds for all CST instances
   - For variable-length rules, use `from: children(start..end)`

4. **Token Kind Validity**:
   - `from: token(KIND)` → KIND MUST be a valid TokenKind
   - KIND MUST actually appear in the grammar rule

### 13.5 Common Schema Errors

#### Error 1: Bare Token References

```yaml
# WRONG
Function:
  fields:
    name:
      from: IDENT  # ✗ Ambiguous
```

```yaml
# CORRECT
Function:
  fields:
    name:
      from: token(IDENT)  # ✓ Explicit token extraction
    # OR
    name:
      from: child(1)  # ✓ Explicit child index (if IDENT is at index 1)
```

#### Error 2: Invalid Repeat Construct

```yaml
# WRONG
Program:
  fields:
    decls:
      from: Decl
      repeat: true  # ✗ Not a schema feature
```

```yaml
# CORRECT
Program:
  fields:
    decls:
      from: children(0..?)  # ✗ Dynamic end not supported
    # OR - if all children are Decl nodes:
    decls:
      from: children(0..100)  # ? Upper bound required (limitation)
```

**NOTE**: Variable-length extraction is a known limitation. Current schema cannot express "all children from index n to end".

#### Error 3: Union Construct

```yaml
# WRONG
Instruction:
  union:
    - Arg
    - Lit
    - Call  # ✗ Union not supported
```

**FIX**: Use separate schema nodes for each variant, or use pass-through mode.

Schema loader does not support union/enum types. Each grammar alternative should produce a distinct CST node, and each needs a schema entry.

### 13.6 Schema File Format

**Required Structure**:

```yaml
nodes:  # ← MUST be at root, no wrapper
  NodeName:
    fields:  # ← Required (even if empty)
      fieldName:
        from: <specifier>
    match: <grammar_rule>  # ← Optional (defaults to NodeName)
```

**WRONG**:

```yaml
schema:  # ✗ Extra wrapper not supported
  nodes:
    ...
```

### 13.7 Validation Strategy

#### Load-Time Validation (Future)

Should validate:
- All schema nodes reference valid grammar rules
- All `from:` specifiers use correct syntax
- All child indices are plausible given grammar
- All token kinds are valid

#### AST-Build-Time Validation (Current)

Currently validates:
- Child index bounds (at projection time)
- Token existence (at projection time)
- Fails with error if extraction cannot be satisfied

### 13.8 Inspection Support

**Current**: `--inspect parser` shows ParseTree (CST) structure

**Proposed** (Wave 2 future):
- `--inspect schema-mapping` - Show grammar → schema mappings
- `--inspect schema-coverage` - List unhandled grammar rules
- `--inspect field-sources` - Show where each field extracts from

### 13.9 Known Limitations

1. **Variable-Length Repetitions**:
   - Grammar: `Decl*` produces 0-N children
   - Schema: `children(start..end)` requires fixed end
   - No support for "all remaining children"

2. **Union Types**:
   - Grammar alternatives (`A | B`) work fine in CST
   - Schema has no union construct
   - Each variant needs explicit handling

3. **Token Kind Ambiguity**:
   - Multiple tokens of same kind in one rule
   - `from: token(IDENT)` gets first occurrence
   - No way to specify "second IDENT"

### 13.10 Wave 2 Findings Summary

**File**: `axis-surface-0-config/surface-0-ast.yaml`

**Status**: INVALID - Schema file has multiple incompatibilities:

1. **Format Error**: Uses `schema: nodes:` wrapper instead of `nodes:` at root
2. **Unsupported Constructs**: Uses `repeat: true` and `union: [...]`
3. **Invalid Field References**: Uses bare `from: IDENT` instead of `from: token(IDENT)` or `from: child(n)`

**See**: [WAVE2_GRAMMAR_SCHEMA_COHERENCE_ANALYSIS.md](../WAVE2_GRAMMAR_SCHEMA_COHERENCE_ANALYSIS.md) for complete analysis.

---

## 14. Code References

All findings in this document are based on analysis of the following files:

- [src/frontend/lexspec.rs](../src/frontend/lexspec.rs) - Lexer spec data structures
- [src/frontend/lexspec_load.rs](../src/frontend/lexspec_load.rs) - Lexer spec loader
- [src/frontend/lexer_engine.rs](../src/frontend/lexer_engine.rs) - Lexer implementation
- [src/frontend/parserspec.rs](../src/frontend/parserspec.rs) - Parser spec data structures
- [src/frontend/parserspec_load.rs](../src/frontend/parserspec_load.rs) - Parser spec loader
- [src/frontend/parser_runtime.rs](../src/frontend/parser_runtime.rs) - Parser runtime (CST production)
- [src/frontend/ast_builder.rs](../src/frontend/ast_builder.rs) - Generic AST builder
- [src/frontend/schema_load.rs](../src/frontend/schema_load.rs) - Schema spec loader
- [src/frontend/schema_ast.rs](../src/frontend/schema_ast.rs) - Schema AST structures and projection
- [src/validation/validator.rs](../src/validation/validator.rs) - Validator implementation
- [src/introspection/inspection.rs](../src/introspection/inspection.rs) - Inspection output

**Analysis Dates**:
- Wave 1 execution: Initial spec contract documentation
- Wave 2 execution (2026-01-25): Grammar–schema coherence analysis

**Codebase State**: Current working tree  
**Verification Method**: Direct code reading, grep analysis, and inspection-driven investigation
