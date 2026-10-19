// Pipeline Stage Inspection
//
// This module provides observability and introspection for all pipeline stages.
// Each stage can be inspected independently to dump its exact runtime state.
//
// NO GUESSING MODE: Every loaded spec and intermediate artifact is explicitly
// dumped in deterministic, machine-readable format (YAML).

use crate::frontend::ast_builder;
use crate::frontend::lexer_engine;
use crate::frontend::lexspec_load;
use crate::frontend::parser_runtime;
use crate::frontend::parserspec_load;
use crate::frontend::schema_ast;
use crate::frontend::schema_load;
use crate::frontend::token::Span;
use crate::pipeline::{PipelineConfig, PipelineError};
use crate::registry::core05::fold::Scope;
use crate::registry::core05::record::{EffectClass, FnKind};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;

/// Run inspection mode - dump requested pipeline stages and exit
pub fn run_inspection(cfg: &PipelineConfig, stages: HashSet<String>) -> Result<(), PipelineError> {
    let start_time = std::time::Instant::now();
    let mut stage_timings: Vec<(String, std::time::Duration)> = Vec::new();

    // Track which stages executed
    let mut executed_stages: Vec<String> = Vec::new();

    // STAGE 0: Load lexer spec
    let stage_start = std::time::Instant::now();
    let lexer_spec =
        lexspec_load::load_spec(&cfg.lexer_spec).map_err(|e| PipelineError::LexerSpecLoad {
            message: e.message,
            span: None,
        })?;
    let lexer_load_time = stage_start.elapsed();
    stage_timings.push(("lexer_load".to_string(), lexer_load_time));
    executed_stages.push("lexer_load".to_string());

    if stages.contains("lexer") {
        dump_lexer_inspection(&lexer_spec);
    }

    // STAGE 1: Load parser spec
    let stage_start = std::time::Instant::now();
    let parser_spec =
        parserspec_load::load_parser_spec(&cfg.parser_spec, &lexer_spec).map_err(|e| {
            PipelineError::ParserSpecLoad {
                message: e.message,
                span: None,
            }
        })?;
    let parser_load_time = stage_start.elapsed();
    stage_timings.push(("parser_load".to_string(), parser_load_time));
    executed_stages.push("parser_load".to_string());

    if stages.contains("parser") || stages.contains("grammar") {
        dump_grammar_inspection(&parser_spec);
    }

    // STAGE 2: Load schema
    let stage_start = std::time::Instant::now();
    let ast_schema = schema_load::load_schema_from_file(&cfg.ast_schema).map_err(|e| {
        PipelineError::SchemaLoad {
            message: e.message,
            span: None,
        }
    })?;
    let schema_load_time = stage_start.elapsed();
    stage_timings.push(("schema_load".to_string(), schema_load_time));
    executed_stages.push("schema_load".to_string());

    if stages.contains("schema") {
        dump_schema_inspection(&ast_schema);
    }

    // STAGE 3: Registry — the Core IR 0.5 scope the CLI built from `--reg`.
    //
    // This used to call `Registry::load_active()`, which consults neither
    // `--reg` nor `cfg`: with no langlab config on disk it falls back to
    // `registries/wave3_test.axreg`, a legacy 0.1 projection (`function det0` /
    // `arity 0`) the current parser rejects at line 3. Worse, the call sat
    // OUTSIDE the stage filter, so it failed before ANY stage could be dumped —
    // `--inspect lexer` died on a registry parse error while the very same
    // `--reg` flags compiled the same file cleanly without `--inspect`.
    //
    // `main.rs` passes `registry: Registry::new()` (empty) and puts the real
    // vocabulary in `core05_scope`; the 0.1 loader has not been the registry
    // path since Phase 5. Reading the scope is free, so there is nothing to
    // time or fail here — the stage stays in the timing table for shape.
    let stage_start = std::time::Instant::now();
    let registry_scope = cfg.core05_scope.as_ref();
    let registry_load_time = stage_start.elapsed();
    stage_timings.push(("registry_load".to_string(), registry_load_time));
    executed_stages.push("registry_load".to_string());

    if stages.contains("registry") {
        dump_registry_inspection(registry_scope);
    }

    // STAGE 4: Load source file
    let stage_start = std::time::Instant::now();
    let source = fs::read_to_string(&cfg.source_file).map_err(|e| PipelineError::Io {
        message: e.to_string(),
        path: cfg.source_file.clone(),
    })?;
    let source_load_time = stage_start.elapsed();
    stage_timings.push(("source_load".to_string(), source_load_time));
    executed_stages.push("source_load".to_string());

    // STAGE 5: Lex source
    let stage_start = std::time::Instant::now();
    let tokens =
        lexer_engine::lex_with_spec(&lexer_spec, &source).map_err(|e| PipelineError::Lex {
            message: e.message,
            span: e.span.unwrap_or(Span::new(0, 0)),
        })?;
    let lex_time = stage_start.elapsed();
    stage_timings.push(("lex".to_string(), lex_time));
    executed_stages.push("lex".to_string());

    // STAGE 6: Parse tokens
    let stage_start = std::time::Instant::now();
    let parse_tree =
        match cfg.parser_mode.as_deref() {
            Some("postfix") => {
                use crate::frontend::postfix_parser;
                postfix_parser::parse_postfix(&tokens).map_err(|e| PipelineError::Parse {
                    message: e.message,
                    span: e.span,
                })?
            }
            None | Some("grammar") => parser_runtime::parse_with_spec(&parser_spec, &tokens)
                .map_err(|e| PipelineError::Parse {
                    message: e.message,
                    span: e.span,
                })?,
            Some(unknown) => {
                return Err(PipelineError::Parse {
                    message: format!(
                        "unknown parser_mode '{}': expected 'postfix' or 'grammar'",
                        unknown
                    ),
                    span: Span::new(0, 0),
                });
            }
        };
    let parse_time = stage_start.elapsed();
    stage_timings.push(("parse".to_string(), parse_time));
    executed_stages.push("parse".to_string());

    if stages.contains("cst") {
        dump_cst_inspection(&parse_tree, &tokens);
    }

    // STAGE 7: Build generic AST
    let stage_start = std::time::Instant::now();
    let generic_ast =
        ast_builder::build_generic_ast(&parse_tree).map_err(|e| PipelineError::AstBuild {
            message: e.message,
            span: None,
        })?;
    let ast_build_time = stage_start.elapsed();
    stage_timings.push(("ast_build".to_string(), ast_build_time));
    executed_stages.push("ast_build".to_string());

    // STAGE 8: Project schema AST
    let stage_start = std::time::Instant::now();
    let schema_ast_node =
        schema_ast::project_schema_ast(&generic_ast, &ast_schema).map_err(|e| {
            PipelineError::SchemaProject {
                message: e.message,
                span: e.span,
            }
        })?;
    let schema_project_time = stage_start.elapsed();
    stage_timings.push(("schema_project".to_string(), schema_project_time));
    executed_stages.push("schema_project".to_string());

    if stages.contains("ast") {
        dump_ast_inspection(&schema_ast_node);
    }

    // STAGE 8: Normalization (YAML-driven)
    let stage_start = std::time::Instant::now();
    let normalization_ctx = crate::normalize::NormalizationContext::load(&cfg.normalize_spec)
        .map_err(|e| PipelineError::Normalization {
            message: format!("{}", e),
            span: crate::frontend::token::Span::new(0, 0),
        })?;

    // Split `Program` into its items BEFORE normalizing, exactly as
    // `run_pipeline` does. `Program` is a forbidden node kind: the normalizer
    // rejects it outright ("Program with N items reached normalizer — pipeline
    // must split before normalization"), so without this split `--inspect` on
    // any file with more than one top-level declaration died at normalization
    // even though the same file compiles. The registry bug above hid this one:
    // stage 3 failed first, so no inspection ever reached the normalizer.
    let raw_items: Vec<schema_ast::SchemaAstNode> = if schema_ast_node.kind == "Program" {
        match schema_ast_node.fields.get("items") {
            Some(schema_ast::SchemaValue::Nodes(ns)) => ns.clone(),
            _ => vec![schema_ast_node],
        }
    } else {
        vec![schema_ast_node]
    };

    let nf_asts = raw_items
        .into_iter()
        .map(|item| {
            normalization_ctx.normalize(item).map_err(|e| match e {
                crate::normalize::NormalizationError::ValidationError(v) => {
                    PipelineError::NfValidation {
                        message: v.message.clone(),
                        span: v.span,
                    }
                }
                _ => PipelineError::Normalization {
                    message: format!("{}", e),
                    span: crate::frontend::token::Span::new(0, 0),
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let normalisation_time = stage_start.elapsed();
    stage_timings.push(("normalisation".to_string(), normalisation_time));
    executed_stages.push("normalisation".to_string());

    if stages.contains("normalisation") || stages.contains("nf") {
        eprintln!("=== NORMALIZATION ===");
        eprintln!("Items: {}", nf_asts.len());
        for (i, nf_ast) in nf_asts.iter().enumerate() {
            eprintln!("  [{}] NF version: {}", i, nf_ast.nf_version());
            eprintln!("  [{}] Root node: {}", i, nf_ast.node().kind);
        }
    }

    // NOTE: Legacy Core IR 0.3 lowering stage retired in Phase 3d.
    // The 0.5 lowering path is wired separately. The inspection stages
    // for "core-ir" and "code_trace" that depended on the 0.3 bundle
    // are omitted here.
    if stages.contains("lowering") {
        eprintln!("=== LOWERING ===");
        eprintln!("Legacy Core IR 0.3 lowering retired (Phase 3d). Use 0.5 path.");
    }

    // Pipeline summary
    if stages.contains("pipeline") {
        let total_time = start_time.elapsed();
        dump_pipeline_inspection(&executed_stages, &stage_timings, total_time, cfg);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// INSPECTION DUMP FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

fn dump_lexer_inspection(spec: &crate::frontend::lexspec::LexerSpec) {
    println!("═══════════════════════════════════════════════════════════");
    println!("LEXER INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct LexerRule {
        index: usize,
        name: String,
        pattern: String,
        skip: bool,
    }

    #[derive(Serialize)]
    struct LexerInspection {
        charset: String,
        case_sensitive: bool,
        total_rules: usize,
        rules: Vec<LexerRule>,
    }

    let mut rules = Vec::new();
    let mut index = 0;

    // Whitespace
    if let Some(ws) = &spec.lexer.whitespace {
        rules.push(LexerRule {
            index,
            name: "WHITESPACE".to_string(),
            pattern: ws.pattern.clone(),
            skip: ws.skip,
        });
        index += 1;
    }

    // Comments
    for (i, comment) in spec.lexer.comments.iter().enumerate() {
        rules.push(LexerRule {
            index,
            name: format!("COMMENT_{}", i),
            pattern: comment.pattern.clone(),
            skip: comment.skip,
        });
        index += 1;
    }

    // Keywords
    for kw in &spec.lexer.keywords {
        rules.push(LexerRule {
            index,
            name: format!("KW_{}", kw.to_uppercase()),
            pattern: kw.clone(),
            skip: false,
        });
        index += 1;
    }

    // Literals
    if let Some(literals) = &spec.lexer.literals {
        if let Some(int_lit) = &literals.int {
            rules.push(LexerRule {
                index,
                name: "INT_LIT".to_string(),
                pattern: int_lit.pattern.clone(),
                skip: false,
            });
            index += 1;
        }
        if let Some(bool_lit) = &literals.bool {
            rules.push(LexerRule {
                index,
                name: "BOOL_LIT".to_string(),
                pattern: format!("{:?}", bool_lit.values),
                skip: false,
            });
            index += 1;
        }
        if let Some(unit_lit) = &literals.unit {
            rules.push(LexerRule {
                index,
                name: "UNIT_LIT".to_string(),
                pattern: unit_lit.literal.clone(),
                skip: false,
            });
            index += 1;
        }
        if let Some(string_lit) = &literals.string {
            rules.push(LexerRule {
                index,
                name: "STRING_LIT".to_string(),
                pattern: format!("delim: {}", string_lit.delimiter),
                skip: false,
            });
            index += 1;
        }
    }

    // Identifiers
    if let Some(ident) = &spec.lexer.identifiers {
        rules.push(LexerRule {
            index,
            name: "IDENT".to_string(),
            pattern: ident.pattern.clone(),
            skip: false,
        });
        index += 1;
    }

    // Punctuation
    for punct in &spec.lexer.punctuation {
        rules.push(LexerRule {
            index,
            name: format!("PUNCT_{}", punct),
            pattern: punct.clone(),
            skip: false,
        });
        index += 1;
    }

    let inspection = LexerInspection {
        charset: spec.lexer.charset.clone(),
        case_sensitive: spec.lexer.case_sensitive,
        total_rules: rules.len(),
        rules,
    };

    let yaml = serde_yaml::to_string(&inspection).unwrap();
    println!("{}", yaml);
}

fn dump_grammar_inspection(spec: &crate::frontend::parserspec::ParserSpec) {
    println!("═══════════════════════════════════════════════════════════");
    println!("GRAMMAR INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct GrammarInspection {
        start: String,
        nonterminals: std::collections::BTreeMap<String, Vec<String>>,
    }

    let mut nonterminals = std::collections::BTreeMap::new();

    for (nt, prods) in &spec.grammar {
        let mut prod_strings = Vec::new();
        for prod in prods {
            prod_strings.push(format_production(prod));
        }
        nonterminals.insert(nt.clone(), prod_strings);
    }

    let inspection = GrammarInspection {
        start: spec.start.clone(),
        nonterminals,
    };

    let yaml = serde_yaml::to_string(&inspection).unwrap();
    println!("{}", yaml);
}

fn format_production(prod: &crate::frontend::parserspec::Production) -> String {
    use crate::frontend::parserspec::Production;

    match prod {
        Production::Sequence(elements) => {
            let elem_strs: Vec<String> = elements.iter().map(|e| format_element(e)).collect();
            elem_strs.join(" ")
        }
        Production::Alternation(alts) => {
            let alt_strs: Vec<String> = alts.iter().map(|p| format_production(p)).collect();
            format!("({})", alt_strs.join(" | "))
        }
    }
}

fn format_element(elem: &crate::frontend::parserspec::Element) -> String {
    use crate::frontend::parserspec::{Element, RepeatKind, Terminal};

    match elem {
        Element::NonTerminal(nt) => nt.clone(),
        Element::Terminal(term) => match term {
            Terminal::TokenKind(tk, _) => format!("<{}>", tk),
            Terminal::Keyword(kw, _) => format!("kw:\"{}\"", kw),
            Terminal::Punct(p, _) => format!("punct:\"{}\"", p),
        },
        Element::Repeat(inner, kind) => {
            let inner_str = format_element(inner);
            match kind {
                RepeatKind::ZeroOrMore => format!("{}*", inner_str),
                RepeatKind::OneOrMore => format!("{}+", inner_str),
                RepeatKind::Optional => format!("{}?", inner_str),
            }
        }
        Element::Group(elems) => {
            let elem_strs: Vec<String> = elems.iter().map(|e| format_element(e)).collect();
            format!("({})", elem_strs.join(" "))
        }
    }
}

fn dump_cst_inspection(
    parse_tree: &crate::frontend::parser_runtime::ParseTree,
    tokens: &[crate::frontend::token::Token],
) {
    println!("═══════════════════════════════════════════════════════════");
    println!("CST INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct CSTNode {
        node_type: String,
        span: (usize, usize),
        token_value: Option<String>,
        children: Vec<CSTNode>,
    }

    fn convert_parse_tree(
        pt: &crate::frontend::parser_runtime::ParseTree,
        tokens: &[crate::frontend::token::Token],
    ) -> CSTNode {
        use crate::frontend::parser_runtime::ParseNode;

        let child_nodes: Vec<CSTNode> = pt
            .children
            .iter()
            .map(|child| match child {
                ParseNode::Rule(sub_tree) => convert_parse_tree(sub_tree, tokens),
                ParseNode::Terminal(token) => CSTNode {
                    node_type: token.kind.to_string(),
                    span: (token.span.start, token.span.end),
                    token_value: Some(token.lexeme.clone()),
                    children: Vec::new(),
                },
            })
            .collect();

        CSTNode {
            node_type: pt.rule.clone(),
            span: (pt.span.start, pt.span.end),
            token_value: None,
            children: child_nodes,
        }
    }

    let cst = convert_parse_tree(parse_tree, tokens);
    let yaml = serde_yaml::to_string(&cst).unwrap();
    println!("{}", yaml);
}

fn dump_schema_inspection(schema: &crate::frontend::schema_ast::AstSchema) {
    println!("═══════════════════════════════════════════════════════════");
    println!("SCHEMA INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct SchemaNodeInfo {
        kind: String,
        match_kind: String,
        fields: Vec<String>,
        is_enum: bool,
        variants: Vec<String>,
    }

    #[derive(Serialize)]
    struct SchemaInspection {
        total_nodes: usize,
        nodes: std::collections::BTreeMap<String, SchemaNodeInfo>,
    }

    let mut nodes = std::collections::BTreeMap::new();

    for (name, node_def) in &schema.nodes {
        let field_names: Vec<String> = node_def.fields.keys().cloned().collect();

        nodes.insert(
            name.clone(),
            SchemaNodeInfo {
                kind: name.clone(),
                match_kind: node_def.match_kind.clone(),
                fields: field_names,
                is_enum: false,
                variants: Vec::new(),
            },
        );
    }

    let inspection = SchemaInspection {
        total_nodes: nodes.len(),
        nodes,
    };

    let yaml = serde_yaml::to_string(&inspection).unwrap();
    println!("{}", yaml);
}

fn dump_ast_inspection(ast: &crate::frontend::schema_ast::SchemaAstNode) {
    println!("═══════════════════════════════════════════════════════════");
    println!("AST INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct ASTNodeInfo {
        kind: String,
        span: (usize, usize),
        variant: Option<String>,
        fields: std::collections::BTreeMap<String, String>,
        children_count: usize,
    }

    fn convert_ast_node(node: &crate::frontend::schema_ast::SchemaAstNode) -> ASTNodeInfo {
        use crate::frontend::schema_ast::SchemaValue;

        let mut fields = std::collections::BTreeMap::new();

        for (name, field_value) in &node.fields {
            let field_type = match field_value {
                SchemaValue::Node(_) => "Node".to_string(),
                SchemaValue::Nodes(_) => "Nodes".to_string(),
                SchemaValue::Token(_) => "Token".to_string(),
                SchemaValue::Tokens(_) => "Tokens".to_string(),
            };
            fields.insert(name.clone(), field_type);
        }

        ASTNodeInfo {
            kind: node.kind.clone(),
            span: (node.span.start, node.span.end),
            variant: None,
            fields,
            children_count: 0,
        }
    }

    let ast_info = convert_ast_node(ast);
    let yaml = serde_yaml::to_string(&ast_info).unwrap();
    println!("{}", yaml);
}

fn dump_registry_inspection(scope: Option<&Scope>) {
    println!("═══════════════════════════════════════════════════════════");
    println!("REGISTRY INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct RegistryFn {
        name: String,
        identity: String,
        kind: String,
        inputs: Vec<String>,
        output: String,
        effect: String,
        variadic: bool,
        /// One entry per `callback <pos> (<in>) -> <out>` the fn declares.
        /// Empty for everything that takes no `Fn`-typed parameter.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        callbacks: Vec<String>,
    }

    #[derive(Serialize)]
    struct RegistryInspection {
        total_types: usize,
        total_fns: usize,
        fns: Vec<RegistryFn>,
    }

    let Some(scope) = scope else {
        println!("no registry loaded (the pipeline was configured without --reg)");
        println!();
        return;
    };

    // Reverse the projection name table once: identities are what the scope is
    // keyed by, but names are what a person reads.
    let by_id: std::collections::HashMap<crate::fabric::hash::Hash256, &str> =
        scope.names.iter().map(|(n, id)| (*id, n.as_str())).collect();
    let show = |id: &crate::fabric::hash::Hash256| -> String {
        by_id.get(id).map(|n| n.to_string()).unwrap_or_else(|| id.to_hex())
    };

    let mut fns: Vec<RegistryFn> = scope
        .fns
        .values()
        .map(|g| RegistryFn {
            name: show(&g.identity),
            identity: g.identity.to_hex(),
            kind: match g.kind {
                FnKind::Leaf => "leaf".to_string(),
                FnKind::Composite => "composite".to_string(),
            },
            inputs: g.inputs.iter().map(&show).collect(),
            output: show(&g.output),
            effect: match scope.leaf_facts.get(&g.identity) {
                Some(lf) => render_effect(lf.effect_class),
                // A composite's effect is derived from its body, never declared.
                None => "derived".to_string(),
            },
            variadic: g.variadic,
            callbacks: scope
                .hof_callback_contracts
                .get(&g.identity)
                .map(|cs| {
                    cs.iter()
                        .map(|c| {
                            format!(
                                "{} ({}) -> {}",
                                c.position,
                                c.inputs.iter().map(&show).collect::<Vec<_>>().join(", "),
                                show(&c.output)
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    // Deterministic output: identity order is a hash-map walk, name order is not.
    fns.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.identity.cmp(&b.identity)));

    let inspection = RegistryInspection {
        total_types: scope.types.len(),
        total_fns: fns.len(),
        fns,
    };

    let yaml = serde_yaml::to_string(&inspection).unwrap();
    println!("{}", yaml);
}

/// Render an obligation bitset as the `|`-joined bit names, or `pure` when the
/// set is empty. Not `Debug`, which would print the raw `EffectClass(8)`.
fn render_effect(e: EffectClass) -> String {
    let mut bits = Vec::new();
    if e.has_reads() { bits.push("reads"); }
    if e.has_writes() { bits.push("writes"); }
    if e.has_nondeterministic() { bits.push("nondeterministic"); }
    if e == EffectClass::FullIo || e.union(EffectClass::FullIo) == e { bits.push("fullIo"); }
    if e.has_irreversible() { bits.push("irreversible"); }
    if bits.is_empty() { "pure".to_string() } else { bits.join("|") }
}

// dump_lowering_spec_inspection deleted: Bundle 8 (2026-06-08) — no call-site in run_inspection.
// dump_lowering_inspection deleted: Bundle 8 (2026-06-08) — no call-site in run_inspection.
// dump_core_ir_inspection removed in Phase 3d (legacy Core IR 0.3 model retired).

fn dump_pipeline_inspection(
    executed_stages: &[String],
    timings: &[(String, std::time::Duration)],
    total_time: std::time::Duration,
    config: &PipelineConfig,
) {
    println!("═══════════════════════════════════════════════════════════");
    println!("PIPELINE INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();

    #[derive(Serialize)]
    struct StageTiming {
        stage: String,
        duration_ms: f64,
    }

    #[derive(Serialize)]
    struct PipelineInspection {
        specs_loaded: Vec<String>,
        stages_executed: Vec<String>,
        total_stages: usize,
        timings: Vec<StageTiming>,
        total_duration_ms: f64,
    }

    let specs_loaded = vec![
        config.lexer_spec.display().to_string(),
        config.parser_spec.display().to_string(),
        config.ast_schema.display().to_string(),
    ];

    let stage_timings: Vec<StageTiming> = timings
        .iter()
        .map(|(name, dur)| StageTiming {
            stage: name.clone(),
            duration_ms: dur.as_secs_f64() * 1000.0,
        })
        .collect();

    let inspection = PipelineInspection {
        specs_loaded,
        stages_executed: executed_stages.to_vec(),
        total_stages: executed_stages.len(),
        timings: stage_timings,
        total_duration_ms: total_time.as_secs_f64() * 1000.0,
    };

    let yaml = serde_yaml::to_string(&inspection).unwrap();
    println!("{}", yaml);
}

// dump_normalisation_inspection deleted in Bundle 4 (2026-06-08).
// dump_nf_inspection deleted in Bundle 8 (2026-06-08) — no call-site in run_inspection.
//   The normalisation/nf stages output inline at run_inspection lines 202-206.
// dump_code_trace_inspection removed in Phase 3d (legacy Core IR 0.3 model retired).
// dump_hook_inspection deleted in Bundle 8 (2026-06-08) — no call-site in run_inspection.

// ─────────────────────────────────────────────────────────────────────────────
// Core IR 0.5 inspection (additive — Phase 3c)
//
// Reads the flat node table by index (linear scan — no tree recursion).
// The legacy `dump_core_ir_inspection` (CoreTerm recursive tree) is untouched.
// ─────────────────────────────────────────────────────────────────────────────

/// Dump a human-readable inspection of a Core IR 0.5 `CoreBundle`.
///
/// Additive 0.5 read path — iterates the flat node table linearly.
pub fn dump_core_ir_05_inspection(bundle: &crate::ir::core_ir_05::CoreBundle) {
    use crate::ir::core_ir_05::Node;

    println!("═══════════════════════════════════════════════════════════");
    println!("CORE IR 0.5 INSPECTION");
    println!("═══════════════════════════════════════════════════════════");
    println!();
    println!("version:       {}", bundle.version);
    println!("pool_entries:  {}", bundle.constant_pool.len());
    println!("node_count:    {}", bundle.nodes.len());
    println!();

    for (i, entry) in bundle.constant_pool.iter().enumerate() {
        println!("  pool[{}]  def_hash={}  payload_bytes={}", i, entry.def_hash, entry.payload.len());
    }

    for (i, node) in bundle.nodes.iter().enumerate() {
        match node {
            Node::CCall { target_name, target_identity, args } => {
                println!("  node[{}]  CCall  target={}  ({})  args={}", i, target_name, target_identity, args.len());
            }
            Node::CIf { cond, then_, else_ } => {
                println!("  node[{}]  CIf  cond={:?}  then={:?}  else={:?}", i, cond, then_, else_);
            }
            Node::CDeterminate => {
                println!("  node[{}]  CDeterminate  gate", i);
            }
            Node::CBlock => {
                println!("  node[{}]  CBlock", i);
            }
            Node::CEnd { value } => {
                println!("  node[{}]  CEnd  value={:?}", i, value);
            }
        }
    }
    println!();
}

#[cfg(test)]
mod tests_05 {
    use super::dump_core_ir_05_inspection;

    const PILOT_AXREG: &str = include_str!("../../registries/pilot_lowering.axreg");

    fn pilot_scope() -> crate::registry::core05::fold::Scope {
        crate::registry::core05::text::lower(PILOT_AXREG)
            .expect("pilot registry must load cleanly")
            .scope
    }

    // Verify dump_core_ir_05_inspection doesn't panic on a simple bundle
    #[test]
    fn dump_empty_bundle_does_not_panic() {
        use crate::frontend::schema_ast::SchemaAstNode;
        use crate::frontend::token::Span;
        use crate::lowering::nf_lowering::lower_nf_to_bundle_05;
        use std::collections::HashMap;

        let scope = pilot_scope();
        let unit_node = SchemaAstNode {
            kind: "UnitLit".to_string(),
            fields: HashMap::new(),
            annotations: Default::default(),
            span: Span::new(0, 0),
        };
        let bundle = lower_nf_to_bundle_05(&unit_node, &scope).unwrap();
        dump_core_ir_05_inspection(&bundle);
    }
}
