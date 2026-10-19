// Wave A: End-to-End Pipeline Integration (Phase 3d: legacy Core IR 0.3 retired)
//
// This module integrates all waves into a single executable pipeline:
// spec files + source → lex → parse → generic AST → schema AST → NF
//
// FORBIDDEN:
// - Redesigning existing waves
// - Adding semantic changes
// - Creating new abstractions
// - Bypassing existing wave implementations
// - Logic duplication
//
// This is PURE INTEGRATION.

use crate::frontend::ast_builder;
use crate::frontend::ast_builder::ASTNode;
use crate::frontend::lexer_engine;
use crate::frontend::lexspec_load;
use crate::frontend::parser_runtime;
use crate::frontend::parser_runtime::ParseTree;
use crate::frontend::parserspec_load;
use crate::frontend::postfix_parser;
use crate::frontend::schema_ast;
use crate::frontend::schema_ast::SchemaAstNode;
use crate::frontend::schema_load;
use crate::frontend::token::{Span, Token};
use crate::normalize::{NormalizationContext, NormalizationError};
use crate::registry::Registry;
use crate::registry::core05::fold::Scope;

use std::fmt;
use std::fs;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Pipeline configuration - specifies all required spec files and source
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Path to lexer specification (lexer.yaml)
    pub lexer_spec: PathBuf,
    /// Path to parser specification (parsing.yaml)
    pub parser_spec: PathBuf,
    /// Path to AST schema (ast_schema.yaml)
    pub ast_schema: PathBuf,
    /// Path to source file to compile and execute
    pub source_file: PathBuf,
    /// Optional entry rule (if None, uses parser spec's start rule)
    pub entry_rule: Option<String>,
    /// Path to normalization rules spec (REQUIRED)
    pub normalize_spec: PathBuf,
    /// Registry for foreign function validation (REQUIRED)
    /// This is the authoritative registry from CLI --reg arguments
    pub registry: Registry,
    /// Core IR 0.5 scope loaded from .axreg files (Phase 5).
    /// When `Some`, the pipeline lowers the NF AST to a CoreBundle after normalization.
    pub core05_scope: Option<Scope>,
    /// Parser mode: "grammar" (default) or "postfix" (for AI-1 RPN)
    pub parser_mode: Option<String>,
    /// Optional hook registry (WAVE 4)
    pub hook_registry: Option<crate::hooks::HookRegistry>,
}

/// Pipeline artifacts - all intermediate products from pipeline execution
#[derive(Debug, Clone)]
pub struct PipelineArtifacts {
    /// Tokens from lexer
    pub tokens: Vec<Token>,
    /// Parse tree from parser
    pub parse_tree: ParseTree,
    /// Generic AST from structural projection
    pub generic_ast: ASTNode,
    /// NF items from normalization (one per top-level declaration)
    pub schema_asts: Vec<SchemaAstNode>,
    /// Core IR 0.5 bundle (present when `PipelineConfig.core05_scope` is `Some`)
    pub core_bundle: Option<crate::ir::core_ir_05::CoreBundle>,
    /// Scope augmented with user-defined function signatures (set by spec-driven path).
    /// Use this for verification so cross-function calls resolve correctly.
    pub augmented_scope: Option<crate::registry::core05::fold::Scope>,
    /// Expand and split modes only: one bundle per top-level fn, `(fn name, NF item, bundle)`.
    /// Empty otherwise, where `core_bundle` is the whole file.
    pub item_bundles: Vec<(String, SchemaAstNode, crate::ir::core_ir_05::CoreBundle)>,
    /// Expand mode only: the Program expand produced (prelude, author and
    /// lifted fns), before the split — the surface AST the goldens print.
    pub expanded: Option<SchemaAstNode>,
}

/// Pipeline error - wraps all possible failures with span information
#[derive(Debug)]
pub enum PipelineError {
    /// Failed to read a file
    Io { message: String, path: PathBuf },
    /// Failed to load lexer spec
    LexerSpecLoad { message: String, span: Option<Span> },
    /// Failed to load parser spec
    ParserSpecLoad { message: String, span: Option<Span> },
    /// Failed to load AST schema
    SchemaLoad { message: String, span: Option<Span> },
    /// Failed to load registry
    RegistryLoad { message: String },
    /// Lexing failed
    Lex { message: String, span: Span },
    /// Parsing failed
    Parse { message: String, span: Span },
    /// Generic AST building failed
    AstBuild { message: String, span: Option<Span> },
    /// Schema AST projection failed
    SchemaProject { message: String, span: Span },
    /// Normalization failed (YAML-driven engine)
    Normalization { message: String, span: Span },
    /// Expand failed (PYAX_SURFACE_EXPAND_V1; only with an expand spec)
    Expand { message: String, span: Span },
    /// NF validation failed (WAVE 1)
    NfValidation { message: String, span: Span },
    /// Lowering to Core IR failed
    Lower { message: String, span: Span },
    /// Inspection failed (Wave T1)
    Inspection { message: String },
}

impl PipelineError {
    /// The source span the error points at, when it has one.
    pub fn span(&self) -> Option<&Span> {
        match self {
            PipelineError::Lex { span, .. }
            | PipelineError::Parse { span, .. }
            | PipelineError::SchemaProject { span, .. }
            | PipelineError::Normalization { span, .. }
            | PipelineError::Expand { span, .. }
            | PipelineError::NfValidation { span, .. }
            | PipelineError::Lower { span, .. } => Some(span),
            _ => None,
        }
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineError::Io { message, path } => {
                write!(f, "I/O error reading {}: {}", path.display(), message)
            }
            PipelineError::LexerSpecLoad { message, span } => {
                if let Some(sp) = span {
                    write!(
                        f,
                        "lexer spec load error at {}..{}: {}",
                        sp.start, sp.end, message
                    )
                } else {
                    write!(f, "lexer spec load error: {}", message)
                }
            }
            PipelineError::ParserSpecLoad { message, span } => {
                if let Some(sp) = span {
                    write!(
                        f,
                        "parser spec load error at {}..{}: {}",
                        sp.start, sp.end, message
                    )
                } else {
                    write!(f, "parser spec load error: {}", message)
                }
            }
            PipelineError::SchemaLoad { message, span } => {
                if let Some(sp) = span {
                    write!(
                        f,
                        "schema load error at {}..{}: {}",
                        sp.start, sp.end, message
                    )
                } else {
                    write!(f, "schema load error: {}", message)
                }
            }
            PipelineError::RegistryLoad { message } => {
                write!(f, "registry load error: {}", message)
            }
            PipelineError::Lex { message, span } => {
                write!(f, "lex error at {}..{}: {}", span.start, span.end, message)
            }
            PipelineError::Parse { message, span } => {
                write!(
                    f,
                    "parse error at {}..{}: {}",
                    span.start, span.end, message
                )
            }
            PipelineError::AstBuild { message, span } => {
                if let Some(sp) = span {
                    write!(
                        f,
                        "AST build error at {}..{}: {}",
                        sp.start, sp.end, message
                    )
                } else {
                    write!(f, "AST build error: {}", message)
                }
            }
            PipelineError::SchemaProject { message, span } => {
                write!(
                    f,
                    "schema projection error at {}..{}: {}",
                    span.start, span.end, message
                )
            }
            PipelineError::Normalization { message, span } => {
                write!(
                    f,
                    "normalization error at {}..{}: {}",
                    span.start, span.end, message
                )
            }
            PipelineError::Expand { message, span } => {
                write!(f, "expand error at {}..{}: {}", span.start, span.end, message)
            }
            PipelineError::NfValidation { message, span } => {
                write!(
                    f,
                    "NF validation error at {}..{}: {}",
                    span.start, span.end, message
                )
            }
            PipelineError::Lower { message, span } => {
                write!(
                    f,
                    "lowering error at {}..{}: {}",
                    span.start, span.end, message
                )
            }
            PipelineError::Inspection { message } => {
                write!(f, "inspection error: {}", message)
            }
        }
    }
}

impl std::error::Error for PipelineError {}

/// Run the full pipeline from specs to NF AST, then lower with the hardcoded path.
///
/// Stages: lex → parse → AST → schema → normalize → lower (nf_lowering).
/// Stage 6 only runs when `cfg.core05_scope` is `Some`.
/// All errors are fatal — no partial execution.
pub fn run_pipeline(cfg: &PipelineConfig) -> Result<PipelineArtifacts, PipelineError> {
    run_pipeline_inner(cfg, None, false)
}

/// `run_pipeline` with every top-level fn lowered into its OWN bundle
/// (`item_bundles`), as under expand but without the expand stage. `axis build`
/// uses it: the bridge links one bundle per fn, and a file's shared bundle
/// holds only its last fn, so a multi-fn M1/AI3 file cannot be linked from it.
pub fn run_pipeline_split(cfg: &PipelineConfig) -> Result<PipelineArtifacts, PipelineError> {
    run_pipeline_inner(cfg, None, true)
}

/// `run_pipeline` with the expand stage (PYAX_SURFACE_EXPAND_V1).
///
/// Three differences, all confined to this entry point:
/// 1. the expand engine rewrites the projected Program — prelude items first,
///    then the author's — before the Program split;
/// 2. every top-level fn is lowered into its OWN bundle (`item_bundles`). A
///    shared bundle has one root, so a sibling fn-ref cannot be linked, and the
///    siblings' param slots leak into the root's arity at run time;
/// 3. `core_bundle` is the last item's bundle, for callers that want one.
pub fn run_pipeline_expanded(
    cfg: &PipelineConfig,
    expand: &crate::normalize::expand_engine::ExpandEngine,
) -> Result<PipelineArtifacts, PipelineError> {
    run_pipeline_inner(cfg, Some(expand), true)
}

/// Every registry function's signature by type name, for expand: the same
/// scope lowering resolves calls through, so a surface with an expand stage
/// types registry calls by the same rules as every other surface.
fn registry_sigs(scope: &crate::registry::core05::fold::Scope) -> crate::normalize::expand_engine::RegistrySigs {
    let type_names: std::collections::HashMap<_, _> = scope
        .names
        .iter()
        .filter(|(_, h)| scope.types.contains_key(*h))
        .map(|(n, h)| (*h, n.clone()))
        .collect();
    let name_of = |h: &crate::fabric::hash::Hash256| type_names.get(h).cloned().unwrap_or_else(|| h.to_hex());
    scope
        .names
        .iter()
        .filter_map(|(n, h)| {
            scope.fns.get(h).map(|g| {
                (n.clone(), crate::normalize::expand_engine::RegSig {
                    params: g.inputs.iter().map(name_of).collect(),
                    ret: name_of(&g.output),
                    variadic: g.variadic,
                })
            })
        })
        .collect()
}

fn run_pipeline_inner(
    cfg: &PipelineConfig,
    expand: Option<&crate::normalize::expand_engine::ExpandEngine>,
    split: bool,
) -> Result<PipelineArtifacts, PipelineError> {
    // STAGE 0: Load all specs
    let lexer_spec =
        lexspec_load::load_spec(&cfg.lexer_spec).map_err(|e| PipelineError::LexerSpecLoad {
            message: e.message,
            span: None,
        })?;

    // Only load parser spec if NOT in postfix mode
    let parser_spec = if cfg.parser_mode.as_deref() != Some("postfix") {
        parserspec_load::load_parser_spec(&cfg.parser_spec, &lexer_spec).map_err(|e| {
            PipelineError::ParserSpecLoad {
                message: e.message,
                span: None,
            }
        })?
    } else {
        // Dummy parser spec for postfix mode (not used)
        use crate::frontend::parserspec::ParserSpec;
        use std::collections::HashMap;
        ParserSpec {
            start: "Program".to_string(),
            grammar: HashMap::new(),
        }
    };

    let ast_schema = schema_load::load_schema_from_file(&cfg.ast_schema).map_err(|e| {
        PipelineError::SchemaLoad {
            message: e.message,
            span: None,
        }
    })?;

    // STAGE 1: Load source file
    let source = fs::read_to_string(&cfg.source_file).map_err(|e| PipelineError::Io {
        message: e.to_string(),
        path: cfg.source_file.clone(),
    })?;

    // STAGE 2: Lex source
    let tokens =
        lexer_engine::lex_with_spec(&lexer_spec, &source).map_err(|e| PipelineError::Lex {
            message: e.message,
            span: e.span.unwrap_or(Span::new(0, 0)),
        })?;

    // STAGE 3: Parse tokens
    // HARD GUARD: Verify parser_mode is honored (no silent fallback)
    let parse_tree = match cfg.parser_mode.as_deref() {
        Some("postfix") => {
            // AI-1 postfix parsing (no parser spec needed, uses stack reduction)
            postfix_parser::parse_postfix(&tokens).map_err(|e| PipelineError::Parse {
                message: e.message,
                span: e.span,
            })?
        }
        None | Some("grammar") => {
            // Grammar-based parsing (default)
            parser_runtime::parse_with_spec(&parser_spec, &tokens).map_err(|e| {
                PipelineError::Parse {
                    message: e.message,
                    span: e.span,
                }
            })?
        }
        Some(unknown) => {
            // HARD ERROR: Unknown parser mode
            return Err(PipelineError::Parse {
                message: format!(
                    "unknown parser_mode '{}': expected 'postfix' or 'grammar'",
                    unknown
                ),
                span: Span::new(0, 0),
            });
        }
    };

    // STAGE 4: Build generic AST
    let generic_ast =
        ast_builder::build_generic_ast(&parse_tree).map_err(|e| PipelineError::AstBuild {
            message: e.message,
            span: None,
        })?;

    // STAGE 5: Project schema AST
    let raw_schema = schema_ast::project_schema_ast(&generic_ast, &ast_schema).map_err(|e| {
        PipelineError::SchemaProject {
            message: e.message,
            span: e.span,
        }
    })?;

    let mut expanded = None;
    // EXPAND (opt-in): AST -> AST, before the Program split, so the items it
    // adds (the prelude now, lifted fns in phase 2) go through the same split.
    let raw_schema = match expand {
        None => raw_schema,
        Some(engine) => {
            let prelude_items = match engine.prelude_path() {
                None => Vec::new(),
                Some(path) => {
                    let text = fs::read_to_string(&path).map_err(|e| PipelineError::Io {
                        message: e.to_string(),
                        path: path.clone(),
                    })?;
                    let at = |m: String| format!("in prelude {}: {}", path.display(), m);
                    let toks = lexer_engine::lex_with_spec(&lexer_spec, &text).map_err(|e| PipelineError::Expand {
                        message: at(e.message),
                        span: e.span.unwrap_or(Span::new(0, 0)),
                    })?;
                    let tree = parser_runtime::parse_with_spec(&parser_spec, &toks).map_err(|e| PipelineError::Expand {
                        message: at(e.message),
                        span: e.span,
                    })?;
                    let gen = ast_builder::build_generic_ast(&tree).map_err(|e| PipelineError::Expand {
                        message: at(e.message),
                        span: Span::new(0, 0),
                    })?;
                    let proj = schema_ast::project_schema_ast(&gen, &ast_schema).map_err(|e| PipelineError::Expand {
                        message: at(e.message),
                        span: e.span,
                    })?;
                    match proj.fields.get("items") {
                        Some(schema_ast::SchemaValue::Nodes(ns)) => ns.clone(),
                        _ => vec![proj],
                    }
                }
            };
            let registry = cfg.core05_scope.as_ref().map(registry_sigs);
            let program = engine.expand_program_with_registry(raw_schema, prelude_items, registry.as_ref()).map_err(|e| PipelineError::Expand {
                message: e.message,
                span: e.span,
            })?;
            expanded = Some(program.clone());
            program
        }
    };

    // Split Program items before normalization — Program must never exist in NF
    let raw_items: Vec<SchemaAstNode> = if raw_schema.kind == "Program" {
        match raw_schema.fields.get("items") {
            Some(schema_ast::SchemaValue::Nodes(ns)) => ns.clone(),
            _ => vec![raw_schema],
        }
    } else {
        vec![raw_schema]
    };

    // STAGE 5.5: YAML-driven Normalization (MANDATORY)
    // Transform Schema AST to Normal Form using YAML rules
    let normalization_ctx = NormalizationContext::load(&cfg.normalize_spec).map_err(|e| {
        PipelineError::Normalization {
            message: format!("{}", e),
            span: Span::new(0, 0),
        }
    })?;

    let schema_asts: Vec<SchemaAstNode> = raw_items
        .into_iter()
        .map(|item| {
            normalization_ctx
                .normalize(item)
                .map_err(|e| match e {
                    NormalizationError::ValidationError(v) => PipelineError::NfValidation {
                        message: v.message.clone(),
                        span: v.span,
                    },
                    _ => PipelineError::Normalization {
                        message: format!("{}", e),
                        span: Span::new(0, 0),
                    },
                })
                .map(|nf| nf.into_node())
        })
        .collect::<Result<Vec<_>, _>>()?;

    // STAGE 6: Core IR 0.5 lowering (when a scope is provided)
    //
    // When a scope is supplied, every FnDecl in the normalized program is first
    // entered into a per-call augmented scope (§5b identity, Leaf/Pure default)
    // so cross-function calls within the same file resolve at lowering and
    // verification time. All items are then lowered into one shared bundle.
    let mut item_bundles = Vec::new();
    let (core_bundle, augmented_scope) = if let Some(scope) = &cfg.core05_scope {
        let aug = crate::lowering::nf_lowering::augment_scope_with_user_fns(scope, &schema_asts)
            .map_err(|e| PipelineError::Lower {
                message: e.to_string(),
                span: Span::new(0, 0),
            })?;
        if split {
            for item in &schema_asts {
                let name = match item.fields.get("name") {
                    Some(schema_ast::SchemaValue::Token(t)) => t.lexeme.clone(),
                    _ => {
                        return Err(PipelineError::Lower {
                            message: format!("split mode: top-level item '{}' has no name", item.kind),
                            span: item.span.clone(),
                        })
                    }
                };
                let bundle = crate::lowering::nf_lowering::lower_nf_to_bundle_05(item, &aug)
                    .map_err(|e| PipelineError::Lower {
                        message: match expand {
                            // The author's name (`main`, not `pyax_main`), or
                            // "a loop body in `f`" for a lifted fn.
                            Some(engine) => format!("in {}: {}", engine.describe_fn(&name), e),
                            None => format!("in `{}`: {}", name, e),
                        },
                        span: item.span.clone(),
                    })?;
                item_bundles.push((name, item.clone(), bundle));
            }
            let last = item_bundles.last().map(|(_, _, b)| b.clone());
            (last, Some(aug))
        } else {
            let bundle = crate::lowering::nf_lowering::lower_nf_items_to_bundle_05(&schema_asts, &aug)
                .map_err(|e| PipelineError::Lower {
                    message: e.to_string(),
                    span: Span::new(0, 0),
                })?;
            (Some(bundle), Some(aug))
        }
    } else {
        (None, None)
    };

    Ok(PipelineArtifacts {
        tokens,
        parse_tree,
        generic_ast,
        schema_asts,
        core_bundle,
        augmented_scope,
        item_bundles,
        expanded,
    })
}
