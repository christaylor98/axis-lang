# Pipeline Inspection Examples

This document demonstrates the full observability capabilities of Axis Lang.

## Quick Examples

### Inspect Lexer Rules

See exactly what lexer rules were loaded from your YAML spec:

```bash
axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file samples/pipeline_test.ax \
  --inspect lexer
```

Output shows:
- Total number of rules loaded
- Each rule's index, name, pattern, and skip flag
- Exact order of rule priority

### Inspect Grammar

See the resolved grammar with all productions:

```bash
axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file samples/pipeline_test.ax \
  --inspect grammar
```

Output shows:
- Start symbol
- All nonterminals
- All productions for each nonterminal
- Explicit terminal syntax (kw:"fn", punct:"+", <IDENT>)

### Inspect Registry

See all registered operations:

```bash
axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file samples/pipeline_test.ax \
  --inspect registry
```

Output shows:
- Total operation count
- Each operation's name, arity, determinism flag, profiles, and ID

### Inspect Multiple Stages

Combine multiple inspections in one run:

```bash
axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file samples/pipeline_test.ax \
  --inspect lexer \
  --inspect grammar \
  --inspect pipeline
```

### Inspect Everything

Use `--inspect all` to dump every stage:

```bash
axis run \
  --lexer lang-lab-poc-userfiles/lexer.yaml \
  --parser lang-lab-poc-userfiles/parsing.yaml \
  --schema lang-lab-poc-userfiles/ast_schema.yaml \
  --file samples/pipeline_test.ax \
  --inspect all
```

## Available Stages

| Stage      | What It Shows |
|------------|---------------|
| `lexer`    | All lexer rules with patterns and priority order |
| `grammar`  | Resolved grammar with nonterminals and productions |
| `cst`      | Concrete syntax tree with token spans and values |
| `schema`   | Schema node definitions with fields and types |
| `ast`      | Abstract syntax tree after schema projection |
| `registry` | All registered operations with metadata |
| `lowering` | Lowering transformation steps |
| `core-ir`  | Core IR structure after lowering |
| `pipeline` | Execution summary with spec paths and timing |
| `all`      | Enable all inspection stages |

## Use Cases

### Debugging Lexer Issues

If your lexer isn't tokenizing correctly:

```bash
axis run ... --inspect lexer
```

Check:
- Are all rules present?
- Are they in the right order?
- Do patterns match what you expect?

### Debugging Parser Issues

If parsing fails:

```bash
axis run ... --inspect grammar
```

Check:
- Did the grammar expand correctly?
- Are terminals using explicit syntax?
- Are repetition operators (*, +, ?) expanded as expected?

### Understanding Schema Projection

If AST projection fails:

```bash
axis run ... --inspect schema
```

Check:
- Which fields are defined?
- What extraction rules exist?
- Is the schema complete?

### Performance Analysis

See where time is spent:

```bash
axis run ... --inspect pipeline
```

Check:
- Stage timings
- Total pipeline duration
- Which specs were loaded

## Output Format

All inspection output is deterministic YAML. This means:
- You can diff outputs across runs
- You can parse it programmatically
- You can version control inspection snapshots
- No hidden state or inference

## No Guessing Mode

Inspection shows **exactly** what the compiler loaded. If something looks wrong in the inspection output, it's because the spec was written that way - not because the compiler guessed or inferred something.

This eliminates the most common source of language development frustration: not knowing what the compiler actually sees.
