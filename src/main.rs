use clap::Parser;
use std::path::PathBuf;

/// Axis Language Lab - Spec-Driven Compilation Pipeline
///
/// Compiles Axis source files to Core IR 0.5 bundles.
/// Pipeline: lex → parse → AST → normalize → lower → verify → .coreir
///
/// Every input the pipeline consumes has an explicit flag.
/// --file and --reg each accept a file or a directory (all files included,
/// no extension filtering) and may be repeated.
/// Registries are merged with last-wins collision policy on duplicate names.
/// All source files must compile and verify successfully or the command fails.
/// Verification runs to a fixpoint, so a composite and the body it calls build
/// in one invocation whatever order they are given in; on failure every bundle
/// that did verify is still written and every failure is reported.
///
/// Example:
///   axis --lexer    spec/lexer.yaml \
///        --parser   spec/parser.yaml \
///        --schema   spec/schema.yaml \
///        --lowering spec/lowering.yaml \
///        --normalize spec/normalize.yaml \
///        --file src/a.ax --file src/b.ax \
///        --reg registries/axis.axreg \
///        --out build/
#[derive(Parser)]
#[command(name = "axis")]
#[command(version, about, long_about = None)]
struct Cli {
    /// Path to lexer spec file (YAML) - REQUIRED
    #[arg(long, required = true)]
    lexer: PathBuf,

    /// Path to parser spec file (YAML) - REQUIRED
    #[arg(long, required = true)]
    parser: PathBuf,

    /// Path to AST schema file (YAML) - REQUIRED
    #[arg(long, required = true)]
    schema: PathBuf,

    /// Path to normalization rules file (YAML) - REQUIRED
    #[arg(long, required = true)]
    normalize: PathBuf,

    /// Source file(s) or director(ies) to compile (repeatable) - AT LEAST ONE REQUIRED
    /// Directories are expanded to all contained files (no extension filter).
    #[arg(long = "file", required = true)]
    files: Vec<PathBuf>,

    /// Registry file(s) or director(ies) in axreg format (repeatable, optional)
    /// Directories are expanded to all contained files (no extension filter).
    /// Registries are merged with last-wins collision policy on duplicate names.
    #[arg(long = "reg")]
    registries: Vec<PathBuf>,

    /// Optional entry rule (uses parser spec's start rule if omitted)
    #[arg(long)]
    entry: Option<String>,

    /// Parser mode: "grammar" (default) or "postfix" (for AI-1 RPN)
    #[arg(long)]
    parser_mode: Option<String>,

    /// Inspect pipeline stages (can be specified multiple times)
    /// Valid values: lexer, parser, grammar, cst, schema, ast, normalisation, nf, registry, core-ir, pipeline, code_trace, all
    /// Inspections are observational only and do not skip compilation stages.
    #[arg(long = "inspect", value_name = "STAGE")]
    inspect: Vec<String>,

    /// Output path for Core IR bundles.
    /// If directory: one bundle per source file, each named after its source.
    /// If file path: writes to that exact path — single source only; with
    /// several sources every bundle would land there and all but the last
    /// would be lost, so it is refused.
    /// Default: "coreir" directory, one bundle per source file.
    #[arg(long)]
    out: Option<PathBuf>,

    /// Skip the Core IR 0.5 bundle verifier gate.
    /// Use for AI2 bridge compilation where bridge functions are polymorphic
    /// and cannot be fully typed in the 0.5 type system.
    #[arg(long, default_value = "false")]
    no_verify: bool,

    /// Directories containing composite-body `.coreir` bundles to populate the
    /// verifier's body store. A composite fn declared `body 0xHHHH...` in the
    /// registry resolves to the bundle whose canonical identity matches HHHH.
    /// Without this the verifier raises `CompositeEffectUnderivable` for any
    /// composite call whose body isn't already in the merged scope.
    /// Repeatable. Mirrors `axis verify --body-dir`.
    #[arg(long = "body-dir")]
    body_dirs: Vec<PathBuf>,

    /// Expand rules file (YAML) — PYAX_SURFACE_EXPAND_V1. Opt-in per surface.
    /// Present: the expand stage runs between the schema AST and the Program
    /// split, every top-level fn is written as its own bundle `<out>/<fn>.coreir`
    /// (lifted siblings are fn-refs the bridge links by file stem), and each fn's
    /// declared return type is checked against its body (RETURN_TYPE_CHECKED_V1).
    /// Absent: none of the three happens; output is unchanged.
    #[arg(long = "expand")]
    expand: Option<PathBuf>,

    /// One bundle per top-level fn, `<out dir>/<fn>.coreir`, without an expand
    /// spec — what the bridge needs to link a multi-fn file. `axis build` sets it.
    #[arg(long)]
    split: bool,
}

fn main() {
    // Dispatch `axis verify ...` / `axis bundle-id ...` before clap.
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(|s| s.as_str()) == Some("verify") {
        cmd_verify(&argv[2..]);
        return;
    }
    if argv.get(1).map(|s| s.as_str()) == Some("bundle-id") {
        cmd_bundle_id(&argv[2..]);
        return;
    }
    if argv.get(1).map(|s| s.as_str()) == Some("types") {
        cmd_types(&argv[2..]);
        return;
    }
    if argv.get(1).map(|s| s.as_str()) == Some("symbols") {
        cmd_symbols(&argv[2..]);
        return;
    }
    if argv.get(1).map(|s| s.as_str()) == Some("find") {
        cmd_find(&argv[2..]);
        return;
    }
    if matches!(argv.get(1).map(|s| s.as_str()), Some("build") | Some("run")) {
        cmd_build(&argv[1], &argv[2..]);
        return;
    }
    if argv.get(1).map(|s| s.as_str()) == Some("callsites") {
        cmd_callsites(&argv[2..]);
        return;
    }

    let cli = Cli::parse();

    // Validate all required paths exist on disk
    if !cli.lexer.exists() {
        eprintln!("Error: Lexer spec not found: {}", cli.lexer.display());
        std::process::exit(1);
    }
    if !cli.parser.exists() {
        eprintln!("Error: Parser spec not found: {}", cli.parser.display());
        std::process::exit(1);
    }
    if !cli.schema.exists() {
        eprintln!("Error: Schema spec not found: {}", cli.schema.display());
        std::process::exit(1);
    }
    if !cli.normalize.exists() {
        eprintln!("Error: Normalization config not found: {}", cli.normalize.display());
        std::process::exit(1);
    }

    // Normalize source inputs (expand directories, deduplicate)
    let source_files = match axis_lang::cli_util::normalize_path_inputs(&cli.files, None) {
        Ok(files) => {
            if files.is_empty() {
                eprintln!("Error: No source files found in specified paths");
                std::process::exit(1);
            }
            files
        }
        Err(e) => {
            eprintln!("Error: Failed to load source files: {}", e);
            std::process::exit(1);
        }
    };

    // Normalize registry inputs (expand directories, deduplicate)
    // --reg is optional: empty list produces an empty scope (no foreign-function lookup)
    let registry_files = if cli.registries.is_empty() {
        Vec::new()
    } else {
        match axis_lang::cli_util::normalize_path_inputs(&cli.registries, None) {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Error: Failed to load registry files: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Load and merge .axreg registries into a single core05 Scope (last-wins on conflicts)
    let merged_scope = if registry_files.is_empty() {
        axis_lang::registry::core05::fold::Scope::new()
    } else {
        match axis_lang::cli_util::load_and_merge_registries(&registry_files) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Parse inspect flags
    let mut inspect_stages = std::collections::HashSet::new();
    for stage in &cli.inspect {
        if stage == "all" {
            inspect_stages.insert("lexer".to_string());
            inspect_stages.insert("parser".to_string());
            inspect_stages.insert("grammar".to_string());
            inspect_stages.insert("cst".to_string());
            inspect_stages.insert("schema".to_string());
            inspect_stages.insert("ast".to_string());
            inspect_stages.insert("normalisation".to_string());
            inspect_stages.insert("nf".to_string());
            inspect_stages.insert("registry".to_string());
            inspect_stages.insert("core-ir".to_string());
            inspect_stages.insert("pipeline".to_string());
            inspect_stages.insert("code_trace".to_string());
        } else if matches!(
            stage.as_str(),
            "lexer"
                | "parser"
                | "grammar"
                | "cst"
                | "schema"
                | "ast"
                | "normalisation"
                | "nf"
                | "registry"
                | "core-ir"
                | "pipeline"
                | "code_trace"
        ) {
            inspect_stages.insert(stage.clone());
        } else {
            eprintln!("Error: Unknown inspect stage: {}", stage);
            eprintln!("Valid stages: lexer, parser, grammar, cst, schema, ast, normalisation, nf, registry, core-ir, pipeline, code_trace, all");
            std::process::exit(1);
        }
    }

    // Normalize parser_mode
    let normalized_parser_mode = cli.parser_mode.as_ref().map(|s| s.trim().to_lowercase());

    // Determine the output path FOR EACH source file.
    //
    // `--out` names either one exact file or a directory holding one bundle per
    // source. The single-file form cannot describe an N-file build: every
    // bundle would land on the same path and only the last would survive, with
    // the N-1 clobbered compiles still reporting success and their own correct
    // identities. So the file form is an error when there is more than one
    // source, and the directory form is resolved per source rather than once
    // from `source_files[0]`.
    let out_paths = match resolve_output_paths(cli.out.as_deref(), &source_files) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Expand spec (PYAX_SURFACE_EXPAND_V1): loaded once, applied to every file.
    let expand_engine = match &cli.expand {
        None => None,
        Some(path) => match axis_lang::normalize::expand_engine::ExpandEngine::load(path) {
            Ok(e) => Some(e),
            Err(msg) => {
                eprintln!("Error: {}", msg);
                std::process::exit(1);
            }
        },
    };
    if expand_engine.is_some() || cli.split {
        if let Some(out) = &cli.out {
            if !(out.is_dir() || out.extension().is_none()) {
                eprintln!("Error: with --expand or --split, --out must be a directory: every top-level fn is written as <dir>/<fn>.coreir");
                std::process::exit(1);
            }
        }
    }

    // Compilation loop: process each file
    let mut all_succeeded = true;
    let mut results = Vec::new();

    for source_file in &source_files {
        use axis_lang::pipeline::{run_pipeline, PipelineConfig};
        use axis_lang::registry::Registry;

        let config = PipelineConfig {
            lexer_spec: cli.lexer.clone(),
            parser_spec: cli.parser.clone(),
            ast_schema: cli.schema.clone(),
            source_file: source_file.clone(),
            entry_rule: cli.entry.clone(),
            normalize_spec: cli.normalize.clone(),
            registry: Registry::new(),
            core05_scope: Some(merged_scope.clone()),
            parser_mode: normalized_parser_mode.clone(),
            hook_registry: None,
        };

        // Apply inspections if requested (observational only, does not skip compilation)
        if !inspect_stages.is_empty() {
            use axis_lang::introspection::inspection::run_inspection;
            eprintln!("=== Inspecting file: {} ===", source_file.display());
            if let Err(e) = run_inspection(&config, inspect_stages.clone()) {
                eprintln!("Inspection failed for {}: {}", source_file.display(), e);
                all_succeeded = false;
                continue;
            }
        }

        // Run full pipeline (lex → parse → AST → normalize → lower)
        let compiled = match &expand_engine {
            Some(engine) => axis_lang::pipeline::run_pipeline_expanded(&config, engine),
            None if cli.split => axis_lang::pipeline::run_pipeline_split(&config),
            None => run_pipeline(&config),
        };
        let artifacts = match compiled {
            Ok(a) => a,
            Err(e) => {
                // Under --expand, name the author's line: expand rewrites the
                // program, so a byte offset alone is no help to the author.
                let at = match (&expand_engine, e.span()) {
                    (Some(_), Some(span)) if !e.to_string().contains("in prelude") => {
                        std::fs::read_to_string(source_file).ok().map(|src| {
                            let upto = &src[..span.start.min(src.len())];
                            let line = upto.matches('\n').count() + 1;
                            let col = upto.len() - upto.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
                            format!("{}:{}:{}: ", source_file.display(), line, col)
                        })
                    }
                    _ => None,
                };
                match at {
                    Some(at) => eprintln!("{}{}", at, e),
                    None => eprintln!("Compilation failed for {}: {}", source_file.display(), e),
                }
                all_succeeded = false;
                continue;
            }
        };

        results.push((source_file.clone(), artifacts));
    }

    if !all_succeeded {
        eprintln!("Error: One or more files failed to compile");
        std::process::exit(1);
    }

    // For each compiled file, verify the bundle (GATE) and write .coreir
    use std::collections::HashMap;
    use axis_lang::ir::core_ir_05::{bundle_identity, read_axbi, write_axbi, serialize_canonical, CoreBundle};
    use axis_lang::fabric::hash::Hash256;
    use axis_lang::registry::core05::record::EffectClass;
    use axis_lang::validation::core_ir::verify_bundle_with_params;
    use axis_lang::lowering::nf_lowering::collect_bundle_param_types;

    // Pre-build the body store once for the whole run. .coreir files under any
    // --body-dir are keyed by canonical bundle identity; a composite registry
    // entry's `body 0x<hash>` then resolves via this map at verify time.
    let mut body_store: HashMap<Hash256, CoreBundle> = HashMap::new();
    for dir in &cli.body_dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("Error: cannot read --body-dir {}: {}", dir.display(), err);
                std::process::exit(1);
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("coreir") {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(err) => {
                    eprintln!("Error: cannot read body bundle {}: {}", path.display(), err);
                    std::process::exit(1);
                }
            };
            match read_axbi(&bytes) {
                Ok(b) => { body_store.insert(bundle_identity(&b), b); }
                Err(err) => {
                    eprintln!("Error: malformed body bundle {}: {}", path.display(), err);
                    std::process::exit(1);
                }
            }
        }
    }

    // VERIFY AND WRITE AS A FIXPOINT, NOT A SINGLE PASS.
    //
    // A composite's `body 0x<hash>` resolves through `body_store`, which above
    // is seeded only from `--body-dir`. A single ordered pass therefore cannot
    // compile a composite and the body it calls in one invocation: the body's
    // bundle exists in `results` but has never been put in the store, so the
    // verifier reports `body bundle <hash> not in store` for a file sitting
    // right there in the same command line.
    //
    // So: repeatedly verify whatever resolves, write it, and insert it into the
    // store under its own identity; repeat while a round makes progress. That
    // is a topological sort that reads the dependency edges off the verifier
    // instead of off the registry, so it also copes with a `body 0x…` that has
    // gone stale.
    //
    // A round that verifies nothing new ends the loop. Whatever is left failed
    // for a real reason; every one of them is reported, not just the first, and
    // the bundles already written STAY written. Exiting on the first failure
    // before any write is what made a cold directory build fail identically
    // forever — no invocation could ever leave more on disk than the one
    // before it.
    // One compile unit per bundle to verify and write. Without --expand that is
    // one per source file, exactly as before. With --expand it is one per
    // top-level fn, written as `<out dir>/<fn>.coreir`: the bridge links a
    // fn-ref to the `--lib` bundle whose file stem is the fn's name.
    struct Unit<'a> {
        source: &'a PathBuf,
        out_path: PathBuf,
        bundle: &'a CoreBundle,
        items: &'a [axis_lang::frontend::schema_ast::SchemaAstNode],
        scope: &'a axis_lang::registry::core05::fold::Scope,
        /// Expand mode: the fn this bundle is. Core IR carries no spans, so a
        /// verifier error can name the function but not the line.
        label: Option<String>,
    }
    let mut pending: Vec<Unit> = Vec::new();
    for ((source_file, artifacts), out_path) in results.iter().zip(out_paths.iter()) {
        let scope = artifacts.augmented_scope.as_ref().unwrap_or(&merged_scope);
        if cli.expand.is_some() || cli.split {
            let dir = out_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            for (name, item, bundle) in &artifacts.item_bundles {
                pending.push(Unit {
                    source: source_file,
                    out_path: dir.join(format!("{}.coreir", name)),
                    bundle,
                    items: std::slice::from_ref(item),
                    scope,
                    label: Some(expand_engine.as_ref().map_or_else(|| format!("`{}`", name), |e| e.describe_fn(name))),
                });
            }
        } else {
            let bundle = match &artifacts.core_bundle {
                Some(b) => b,
                None => {
                    eprintln!("Internal error: no Core IR bundle for {}", source_file.display());
                    std::process::exit(1);
                }
            };
            pending.push(Unit { source: source_file, out_path: out_path.clone(), bundle, items: &artifacts.schema_asts, scope, label: None });
        }
    }
    let unit_count = pending.len();

    // Errors from the most recent attempt on each still-unverified file, kept so
    // a stalled fixpoint can report every failure rather than only the first.
    let mut last_errors: Vec<(PathBuf, String)> = Vec::new();

    while !pending.is_empty() {
        let mut deferred = Vec::new();
        let mut progressed = false;
        last_errors.clear();

        for unit in pending {
            let (source_file, out_path, bundle) = (unit.source, &unit.out_path, unit.bundle);

            // VERIFIER GATE (skip if --no-verify).
            // Use the augmented scope (includes user-defined fn signatures) so that
            // cross-function calls within the same file resolve correctly.
            // Param types are extracted from the FnDecl AST items so the verifier
            // can resolve param-slot conditions to their declared types (PASS 2 fix).
            if !cli.no_verify {
                let scope_for_verify = unit.scope;
                let param_types = collect_bundle_param_types(unit.items, scope_for_verify);
                let pass = match verify_bundle_with_params(bundle, scope_for_verify, &body_store, EffectClass::ANY, &param_types) {
                    Ok(pass) => pass,
                    Err(e) => {
                        // Not necessarily fatal: a later round may put the body this
                        // one is waiting on into the store.
                        let shown = match &unit.label {
                            Some(f) => format!("in {}: {:?}", f, e),
                            None => format!("{:?}", e),
                        };
                        last_errors.push((source_file.to_path_buf(), shown));
                        deferred.push(unit);
                        continue;
                    }
                };
                // RETURN_TYPE_CHECKED_V1: the bundle's result is the LAST item's
                // body, so it must be admitted by that fn's declared return type.
                // Before this, `fn f() -> Int { return Text("x") }` verified.
                // Enforced only under --expand: enabling it everywhere rejects
                // two existing M1/AI3 fixtures (`-> Bool { determine }`), which
                // is chris's call, not this spec's.
                if cli.expand.is_some() {
                if let Some(root) = unit.items.last().filter(|n| n.kind == "FnDecl") {
                    if let Some(axis_lang::frontend::schema_ast::SchemaValue::Token(t)) = root.fields.get("name") {
                        use sha2::Digest;
                        let id = Hash256::from_bytes(sha2::Sha256::digest(t.lexeme.as_bytes()).into());
                        if let Some(genesis) = scope_for_verify.fns.get(&id) {
                            if !axis_lang::validation::core_ir::declared_result_admits(&pass, genesis.output, scope_for_verify) {
                                let describe = |h: &Hash256| scope_for_verify.names.iter()
                                    .find(|(_, v)| *v == h).map(|(n, _)| n.clone())
                                    .unwrap_or_else(|| h.to_hex()[..8].to_string());
                                let got: Vec<String> = pass.result_types.iter().map(|h| describe(h)).collect();
                                last_errors.push((source_file.to_path_buf(), format!(
                                    "ResultTypeMismatch {{ fn: {}, declared: {}, body produces: [{}] }}",
                                    unit.label.clone().unwrap_or_else(|| format!("`{}`", t.lexeme)), describe(&genesis.output), got.join(", "))));
                                deferred.push(unit);
                                continue;
                            }
                        }
                    }
                }
                }
            }

            // Identity = content_hash(canonical serialization) — NOT of the file bytes (the 6-byte header is outside it)
            let canonical_bytes = serialize_canonical(bundle);
            let _ = canonical_bytes; // computed for documentation; bundle_identity does the same
            let identity = bundle_identity(bundle);

            // Native binary (.coreir): header + canonical bytes
            let axbi_bytes = write_axbi(bundle);

            if let Err(e) = std::fs::write(out_path, &axbi_bytes) {
                eprintln!("Failed to write {}: {}", out_path.display(), e);
                std::process::exit(1);
            }

            println!(
                "{} → {} (identity: {})",
                source_file.display(),
                out_path.display(),
                identity.to_hex()
            );

            // This bundle is now a body a later composite in this same run can
            // resolve against, exactly as if it had been read from --body-dir.
            body_store.insert(identity, bundle.clone());
            progressed = true;
        }

        if !progressed {
            for (source_file, err) in &last_errors {
                eprintln!("Verification failed for {}: {}", source_file.display(), err);
            }
            eprintln!(
                "Error: {} file(s) failed verification; {} bundle(s) written",
                last_errors.len(),
                unit_count - last_errors.len()
            );
            std::process::exit(1);
        }

        pending = deferred;
    }

    std::process::exit(0);
}

// ── output path resolution ────────────────────────────────────────────────────

/// One output path per source file.
///
/// `out` is `--out` as given, or `None` for the default `coreir` directory.
/// It names a directory when it has no extension or already exists as one; in
/// that case each source gets `<dir>/<stem>.coreir`. Otherwise it names one
/// exact file, which only describes a build of exactly one source.
///
/// Two sources with the same stem map to the same path — `a/x.ai3` and
/// `b/x.ai3` both want `out/x.coreir`. That is the same silent overwrite the
/// per-file resolution exists to remove, so it is rejected here rather than
/// resolved by whichever compiled last.
fn resolve_output_paths(
    out: Option<&std::path::Path>,
    source_files: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let default_dir = PathBuf::from("coreir");
    let (dir, exact_file) = match out {
        Some(path) => {
            let is_dir = path.is_dir() || path.extension().is_none();
            if is_dir {
                (path.to_path_buf(), None)
            } else {
                (PathBuf::new(), Some(path.to_path_buf()))
            }
        }
        None => (default_dir, None),
    };

    if let Some(file) = exact_file {
        if source_files.len() != 1 {
            return Err(format!(
                "--out {} names a single file but {} source files were given; \
                 every bundle would be written to that one path and only the last \
                 would survive. Pass a directory as --out, or compile one file per \
                 invocation.",
                file.display(),
                source_files.len()
            ));
        }
        if let Some(parent) = file.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create output directory {}: {}", parent.display(), e)
                })?;
            }
        }
        return Ok(vec![file]);
    }

    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create output directory {}: {}", dir.display(), e))?;
    }

    let mut paths = Vec::with_capacity(source_files.len());
    let mut seen: std::collections::HashMap<String, &PathBuf> = std::collections::HashMap::new();
    for source in source_files {
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("source file has no usable name: {}", source.display()))?;
        if let Some(previous) = seen.insert(stem.to_string(), source) {
            return Err(format!(
                "source files {} and {} share the stem '{}' and would both be written \
                 to {}; rename one or compile them into separate --out directories.",
                previous.display(),
                source.display(),
                stem,
                dir.join(format!("{}.coreir", stem)).display()
            ));
        }
        paths.push(dir.join(format!("{}.coreir", stem)));
    }
    Ok(paths)
}

// ── `axis verify` subcommand ──────────────────────────────────────────────────
//
// Usage: axis verify <bundle> --reg <path>... [--body-dir d] [--effect-ceiling c] [--param-types h0,h1,..] [--result-type h] [--effect-ceiling <level>]
//   level: pure | reads | writes | fullio  (default: fullio)
//
// Exit 0 → PASS; prints {"effect_class":"..."} to stdout.
// Exit 1 → FAIL (non-composite); prints VerifyError to stderr.
// Exit 2 → FAIL (CompositeEffectUnderivable); prints error to stderr.
// ── `axis bundle-id` subcommand ──────────────────────────────────────────────
//
// Usage: axis bundle-id <bundle.coreir>
//
// Prints the canonical identity hash (hex) of a Core IR 0.5 bundle.
// Exit 0 on success; exit 1 on error; exit 3 if not a 0.5 .coreir bundle.
fn cmd_bundle_id(args: &[String]) {
    use axis_lang::ir::core_ir_05::{bundle_identity, read_axbi};

    let path = match args.first() {
        Some(p) => p,
        None => {
            eprintln!("axis bundle-id: usage: axis bundle-id <bundle.coreir>");
            std::process::exit(1);
        }
    };
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("axis bundle-id: cannot read {}: {}", path, e);
            std::process::exit(1);
        }
    };
    let bundle = match read_axbi(&raw) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("axis bundle-id: cannot decode {} as Core IR 0.6: {}", path, e);
            std::process::exit(3);
        }
    };
    println!("{}", bundle_identity(&bundle).to_hex());
    std::process::exit(0);
}

// ── `axis types` subcommand ────────────────────────────────────────────────────
//
// Usage: axis types <hash-hex> --reg <path>...
//
// Resolves a Hash256 type identity to its structural TypeDef shape, one level
// deep: Product/Sum/List element identities are printed as hex, not expanded —
// call `axis types` again on a child hash to descend. Read-only: only reads
// the given registry files.
//
// Exit 0 → identity found (as a type, or as a fn — reported either way).
// Exit 1 → identity not present in any given registry, or bad input.
fn cmd_types(args: &[String]) {
    use axis_lang::fabric::hash::Hash256;
    use axis_lang::registry::core05::record::TypeShape;

    let mut hash_arg: Option<String> = None;
    let mut reg_paths: Vec<std::path::PathBuf> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--reg" => {
                if i + 1 >= args.len() {
                    eprintln!("axis types: --reg requires a path argument");
                    std::process::exit(1);
                }
                reg_paths.push(std::path::PathBuf::from(&args[i + 1]));
                i += 2;
            }
            arg if !arg.starts_with('-') && hash_arg.is_none() => {
                hash_arg = Some(arg.to_string());
                i += 1;
            }
            other => {
                eprintln!("axis types: unknown argument '{}'", other);
                std::process::exit(1);
            }
        }
    }

    let hash_arg = match hash_arg {
        Some(h) => h,
        None => {
            eprintln!("axis types: usage: axis types <hash-hex> --reg <path>...");
            std::process::exit(1);
        }
    };
    if reg_paths.is_empty() {
        eprintln!("axis types: at least one --reg is required");
        std::process::exit(1);
    }

    let hash = match Hash256::from_hex(&hash_arg) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("axis types: invalid hash '{}': {:?}", hash_arg, e);
            std::process::exit(1);
        }
    };

    let scope = match axis_lang::cli_util::load_and_merge_registries(&reg_paths) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("axis types: {}", e);
            std::process::exit(1);
        }
    };

    let name = scope
        .names
        .iter()
        .find(|(_, v)| **v == hash)
        .map(|(k, _)| k.clone());
    let name_suffix = name.map(|n| format!(" name={}", n)).unwrap_or_default();

    if let Some(def) = scope.types.get(&hash) {
        let shape = match &def.shape {
            TypeShape::Primitive(p) => format!("primitive {:?}", p),
            TypeShape::Product(fields) => format!(
                "product [{}]",
                fields.iter().map(|h| h.to_hex()).collect::<Vec<_>>().join(", ")
            ),
            TypeShape::Sum(ctors) => format!(
                "sum [{}]",
                ctors
                    .iter()
                    .map(|c| format!(
                        "[{}]",
                        c.iter().map(|h| h.to_hex()).collect::<Vec<_>>().join(", ")
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            TypeShape::List(elem) => format!("list {}", elem.to_hex()),
        };
        println!("TYPE {}{} {}", hash.to_hex(), name_suffix, shape);
        std::process::exit(0);
    }

    if scope.fns.contains_key(&hash) {
        println!(
            "NOT_A_TYPE {}{}: identity belongs to a fn, not a type",
            hash.to_hex(),
            name_suffix
        );
        std::process::exit(1);
    }

    println!("UNKNOWN {}: not found in given registries", hash.to_hex());
    std::process::exit(1);
}

// ── `axis build` / `axis run` subcommands ─────────────────────────────────────
//
// Usage: axis build <src> [-o <exe>] [--surface <name>] [--keep <dir>]
//        axis run   [--surface <name>] [--keep <dir>] <src> [<program args>...]
//
// For `run`, axis's own options go before <src>; everything after it is the
// program's argv, unparsed.
//
// One command from source to a native executable, with no spec flags. The
// surface comes from the file extension (.m1, .ai3, .pyax) or --surface, and
// names that surface's config directory. The compile is this same binary run
// with the full flag set plus --split (or --expand for PyAx), so it is the
// production pipeline and verifier gate, byte for byte. The bundles are then
// linked by axis-codegen-bridge. Nothing here is a second compile path.
//
// Located at run time, each overridable:
//   AXIS_HOME          this repo: the axis-surface-*-config dirs (default: where axis was built)
//   AXIS_REGISTRY_DIR  the five registry files (default: $AXIS_HOME/../axRegistry)
//   AXIS_BRIDGE        the bridge binary, which must stay in its own build dir
//                      (default: $AXIS_HOME/../axis-codegen-bridge-rs/target/release/)
//
// The entry is the bundle the executable calls with argv: `pyax_main` for PyAx
// (expand renames `main`), and the LAST fn in the file for M1 and AI3.
struct BuildSurface {
    name: &'static str,
    config: &'static str,
    /// lexer, parse, ast, normalize — in that order.
    specs: [&'static str; 4],
    expand: Option<&'static str>,
    /// Registries of the surface's own, after the shared five.
    regs: &'static [&'static str],
    dispatch: Option<&'static str>,
    entry: Option<&'static str>,
}

const BUILD_SURFACES: &[BuildSurface] = &[
    BuildSurface {
        name: "m1",
        config: "axis-surface-m1-config",
        specs: ["m1-lexer.yaml", "m1-parse.yaml", "m1-ast.yaml", "m1-normalize.yaml"],
        expand: None,
        regs: &[],
        dispatch: None,
        entry: None,
    },
    BuildSurface {
        name: "ai3",
        config: "axis-surface-ai3-config",
        specs: ["ai3-lexer.yaml", "ai3-parse.yaml", "ai3-ast.yaml", "ai3-normalize.yaml"],
        expand: None,
        regs: &[],
        dispatch: None,
        entry: None,
    },
    BuildSurface {
        name: "pyax",
        config: "axis-surface-pyax-config",
        specs: ["pyax-lexer.yaml", "pyax-parse.yaml", "pyax-ast.yaml", "pyax-normalize.yaml"],
        expand: Some("pyax-expand.yaml"),
        regs: &["pyax.axreg"],
        dispatch: Some("pyax-dispatch.toml"),
        entry: Some("pyax_main"),
    },
];

const BUILD_REGISTRIES: &[&str] = &["axis-types", "axis", "axis-bridge", "m1-builtins", "gen-working"];

fn cmd_build(mode: &str, args: &[String]) {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    let die = |msg: String| -> ! {
        eprintln!("axis {}: {}", mode, msg);
        std::process::exit(1);
    };
    let usage = || -> ! {
        eprintln!("usage: axis build <src> [-o <exe>] [--surface m1|ai3|pyax] [--keep <dir>]");
        eprintln!("       axis run   [--surface m1|ai3|pyax] [--keep <dir>] <src> [<program args>...]");
        std::process::exit(1);
    };

    let mut src: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut surface: Option<String> = None;
    let mut keep: Option<PathBuf> = None;
    let mut prog_args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let need = |i: usize| -> String {
            args.get(i + 1).cloned().unwrap_or_else(|| die(format!("{} requires a value", args[i])))
        };
        // `axis run prog.pyax -3 x` — after the source, everything is the
        // program's argv, flags included.
        if mode == "run" && src.is_some() {
            let rest = if args[i] == "--" { &args[i + 1..] } else { &args[i..] };
            prog_args.extend_from_slice(rest);
            break;
        }
        match args[i].as_str() {
            "-o" | "--out" if mode == "build" => { out = Some(PathBuf::from(need(i))); i += 2; }
            "--surface" => { surface = Some(need(i)); i += 2; }
            "--keep" => { keep = Some(PathBuf::from(need(i))); i += 2; }
            "-h" | "--help" => usage(),
            a if a.starts_with('-') => die(format!("unknown argument '{}'", a)),
            a if src.is_none() => { src = Some(PathBuf::from(a)); i += 1; }
            a => die(format!("unexpected argument '{}'", a)),
        }
    }
    let src = src.unwrap_or_else(|| usage());
    if !src.is_file() {
        die(format!("source not found: {}", src.display()));
    }

    let surface_name = surface.unwrap_or_else(|| {
        src.extension().and_then(|e| e.to_str()).unwrap_or("").to_string()
    });
    let s = BUILD_SURFACES.iter().find(|s| s.name == surface_name).unwrap_or_else(|| {
        let known: Vec<&str> = BUILD_SURFACES.iter().map(|s| s.name).collect();
        die(format!(
            "no surface for '{}': use a .{} file or --surface",
            src.display(),
            known.join(" / .")
        ))
    });

    let env_or = |var: &str, default: PathBuf| std::env::var_os(var).map(PathBuf::from).unwrap_or(default);
    let home = env_or("AXIS_HOME", PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    let reg_dir = env_or("AXIS_REGISTRY_DIR", home.join("../axRegistry"));
    let bridge = env_or(
        "AXIS_BRIDGE",
        home.join("../axis-codegen-bridge-rs/target/release/axis-codegen-bridge"),
    );
    let cfg = home.join(s.config);
    let existing = |p: PathBuf, what: &str, var: &str| -> PathBuf {
        if !p.exists() {
            die(format!("{} not found at {} (set {})", what, p.display(), var));
        }
        p
    };

    let mut regs: Vec<PathBuf> = BUILD_REGISTRIES
        .iter()
        .map(|r| existing(reg_dir.join(format!("{}.axreg", r)), "registry", "AXIS_REGISTRY_DIR"))
        .collect();
    regs.extend(s.regs.iter().map(|r| existing(cfg.join(r), "surface registry", "AXIS_HOME")));
    let reg_args: Vec<std::ffi::OsString> = regs
        .iter()
        .flat_map(|r| [std::ffi::OsString::from("--reg"), r.clone().into_os_string()])
        .collect();
    let bridge = existing(bridge, "axis-codegen-bridge", "AXIS_BRIDGE");
    let [lexer, parser, schema, normalize] =
        s.specs.map(|f| existing(cfg.join(f), &format!("{} spec", s.name), "AXIS_HOME"));

    // Scratch: the bundles, and the bridge's output (it writes lib<name>.a
    // beside the executable, which must not land next to the user's).
    let work = std::env::temp_dir().join(format!("axis-build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let bundles = keep.clone().unwrap_or_else(|| work.join("coreir"));
    for d in [&work, &bundles] {
        std::fs::create_dir_all(d).unwrap_or_else(|e| die(format!("cannot create {}: {}", d.display(), e)));
    }
    let cleanup = || { let _ = std::fs::remove_dir_all(&work); };

    // 1. Compile + verify: this binary, the full flag set.
    let me = std::env::current_exe().unwrap_or_else(|e| die(format!("cannot locate axis itself: {}", e)));
    let mut c = Command::new(&me);
    c.arg("--file").arg(&src)
        .arg("--lexer").arg(&lexer)
        .arg("--parser").arg(&parser)
        .arg("--schema").arg(&schema)
        .arg("--normalize").arg(&normalize)
        .args(&reg_args)
        .arg("--out").arg(format!("{}/", bundles.display()));
    match s.expand {
        Some(e) => { c.arg("--expand").arg(existing(cfg.join(e), "expand spec", "AXIS_HOME")); }
        None => { c.arg("--split"); }
    }
    let compiled = c.output().unwrap_or_else(|e| die(format!("cannot run {}: {}", me.display(), e)));
    if !compiled.status.success() {
        eprint!("{}", String::from_utf8_lossy(&compiled.stderr));
        cleanup();
        std::process::exit(1);
    }

    // 2. The entry bundle.
    let entry = match s.entry {
        Some(e) => e.to_string(),
        None => last_fn_name(&src, &lexer, &parser, &schema, &normalize)
            .unwrap_or_else(|e| { cleanup(); die(e) }),
    };
    let entry_bundle = bundles.join(format!("{}.coreir", entry));
    if !entry_bundle.is_file() {
        cleanup();
        die(format!("entry bundle {} was not written", entry_bundle.display()));
    }

    // 3. Link.
    let linked = work.join("exe").join(src.file_stem().unwrap_or_default());
    let mut b = Command::new(&bridge);
    b.arg("build").arg(&entry_bundle)
        .arg("--out").arg(&linked)
        .arg("--exe")
        .arg("--lib-dir").arg(&bundles)
        .args(&reg_args);
    if let Some(d) = s.dispatch {
        b.arg("--dispatch").arg(existing(cfg.join(d), "dispatch table", "AXIS_HOME"));
    }
    let built = b.output().unwrap_or_else(|e| die(format!("cannot run {}: {}", bridge.display(), e)));
    if !built.status.success() || !linked.is_file() {
        eprint!("{}", String::from_utf8_lossy(&built.stderr));
        cleanup();
        die("bridge link failed".to_string());
    }

    if mode == "run" {
        let status = Command::new(&linked).args(&prog_args).status();
        cleanup();
        match status {
            Ok(st) => std::process::exit(st.code().unwrap_or(1)),
            Err(e) => die(format!("cannot run {}: {}", linked.display(), e)),
        }
    }

    let dest = out.unwrap_or_else(|| PathBuf::from(src.file_stem().unwrap_or_default()));
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| die(format!("cannot create {}: {}", parent.display(), e)));
    }
    // copy, not rename: the scratch dir may sit on another filesystem.
    if let Err(e) = std::fs::copy(&linked, &dest) {
        cleanup();
        die(format!("cannot write {}: {}", dest.display(), e));
    }
    cleanup();
    println!("{} → {} ({}, entry `{}`)", src.display(), Path::new(&dest).display(), s.name, entry);
}

/// The name of the last top-level fn in `src`: the M1/AI3 entry. Read off the
/// compiler's own NF (no registry, so lowering is skipped), not off the text.
fn last_fn_name(
    src: &std::path::Path,
    lexer: &std::path::Path,
    parser: &std::path::Path,
    schema: &std::path::Path,
    normalize: &std::path::Path,
) -> Result<String, String> {
    use axis_lang::frontend::schema_ast::SchemaValue;
    let cfg = axis_lang::pipeline::PipelineConfig {
        lexer_spec: lexer.to_path_buf(),
        parser_spec: parser.to_path_buf(),
        ast_schema: schema.to_path_buf(),
        source_file: src.to_path_buf(),
        entry_rule: None,
        normalize_spec: normalize.to_path_buf(),
        registry: axis_lang::registry::Registry::new(),
        core05_scope: None,
        parser_mode: None,
        hook_registry: None,
    };
    let artifacts = axis_lang::pipeline::run_pipeline(&cfg).map_err(|e| e.to_string())?;
    artifacts
        .schema_asts
        .iter()
        .rev()
        .filter(|n| n.kind == "FnDecl")
        .find_map(|n| match n.fields.get("name") {
            Some(SchemaValue::Token(t)) => Some(t.lexeme.clone()),
            _ => None,
        })
        .ok_or_else(|| format!("{} declares no fn to use as the entry", src.display()))
}

// ── `axis callsites` subcommand ───────────────────────────────────────────────
//
// Usage: axis callsites --file <src> --lexer <l> --parser <p> --schema <s> --normalize <n>
//
// Dumps every call site in a source file, one per line, classified. Runs the
// real lexer/parser/schema/normalize stages and walks the resulting NF, so the
// classification is the compiler's own — not a regex over source text.
//
// NO REGISTRY IS LOADED. `core05_scope: None` skips lowering entirely, which is
// the whole point: lowering resolves names and aborts on the first unknown one
// ("UNKNOWN gate: callable 'x' not found in scope"), so a post-lowering dump can
// only ever show call sites that already resolve. Measuring a resolution RATE
// needs the denominator to include the targets that do not.
//
// Kinds emitted, mirroring the dispatch in lowering/nf_lowering.rs so this stays
// the compiler's classification rather than a second opinion:
//
//   DECL     <name>  a fn declared in this file
//   CALL     <name>  CallExpr / AppExpr callee — an emitted call target
//   FNREF    <name>  a bare identifier in argument position bound to no param
//                    or let in scope: a fn-reference, which lowers to an
//                    Fn-typed pool constant, NOT a CCall
//   VAR      <name>  a bare identifier that IS a param or let binding
//   TYPECTOR <Type>  surface type-constructor syntax — Int(1), Text("a"),
//                    Value(..)(..), ValueList(T)[..]. These look like calls and
//                    are NOT: the model did not name a callable, the surface
//                    lowers them to value_make/list_make. Counting them would
//                    inflate the denominator with targets that cannot fail.
//
// Both arms of an `if` are emitted — "syntactically emitted" is a property of
// the text, not of which branch a particular input would take.
fn cmd_callsites(args: &[String]) {
    use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};

    let mut file: Option<std::path::PathBuf> = None;
    let mut lexer: Option<std::path::PathBuf> = None;
    let mut parser: Option<std::path::PathBuf> = None;
    let mut schema: Option<std::path::PathBuf> = None;
    let mut normalize: Option<std::path::PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        let need = |i: usize| -> std::path::PathBuf {
            if i + 1 >= args.len() {
                eprintln!("axis callsites: {} requires a path", args[i]);
                std::process::exit(1);
            }
            std::path::PathBuf::from(&args[i + 1])
        };
        match args[i].as_str() {
            "--file" => { file = Some(need(i)); i += 2; }
            "--lexer" => { lexer = Some(need(i)); i += 2; }
            "--parser" => { parser = Some(need(i)); i += 2; }
            "--schema" => { schema = Some(need(i)); i += 2; }
            "--normalize" => { normalize = Some(need(i)); i += 2; }
            other => {
                eprintln!("axis callsites: unknown argument '{}'", other);
                std::process::exit(1);
            }
        }
    }

    let (file, lexer, parser, schema, normalize) =
        match (file, lexer, parser, schema, normalize) {
            (Some(f), Some(l), Some(p), Some(s), Some(n)) => (f, l, p, s, n),
            _ => {
                eprintln!(
                    "axis callsites: --file, --lexer, --parser, --schema and \
                     --normalize are all required"
                );
                std::process::exit(1);
            }
        };

    let cfg = axis_lang::pipeline::PipelineConfig {
        lexer_spec: lexer,
        parser_spec: parser,
        ast_schema: schema,
        source_file: file,
        entry_rule: None,
        normalize_spec: normalize,
        registry: axis_lang::registry::Registry::new(),
        // None => stop before lowering. See the note above.
        core05_scope: None,
        parser_mode: None,
        hook_registry: None,
    };

    // PREFIX RECOVERY.
    //
    // A source that does not parse in full still has call sites in the part
    // that does. Reporting nothing for it would drop the whole sample out of
    // a resolution-rate denominator silently, which is not the same as it
    // having no calls — and it is not symmetric across languages, so it
    // corrupts any cross-arm comparison.
    //
    // PipelineError::Parse carries the byte offset where the parser stopped.
    // Truncating there and re-parsing yields the longest valid prefix. The
    // loop repeats because one truncation can expose an earlier stop; it is
    // bounded so a pathological source cannot spin.
    //
    // Recovery is ANNOUNCED, never silent: a `PARTIAL <parsed>/<total>` line
    // is emitted so a consumer can weight or exclude these deliberately
    // rather than mistaking a partial parse for a complete one.
    let full_src = match std::fs::read_to_string(&cfg.source_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("axis callsites: cannot read source: {}", e);
            std::process::exit(1);
        }
    };

    let mut cfg = cfg;
    let mut cut = full_src.len();
    let mut tmp: Option<std::path::PathBuf> = None;
    let mut artifacts = None;
    let mut last_err = String::new();

    for _ in 0..8 {
        match axis_lang::pipeline::run_pipeline(&cfg) {
            Ok(a) => {
                artifacts = Some(a);
                break;
            }
            Err(e) => {
                last_err = format!("{}", e);
                let stop = match &e {
                    axis_lang::pipeline::PipelineError::Parse { span, .. } => span.start,
                    _ => 0,
                };
                // No usable offset, or no forward progress: stop trying.
                if stop == 0 || stop >= cut {
                    break;
                }
                // Land on a char boundary so the slice is valid UTF-8.
                let mut b = stop;
                while b > 0 && !full_src.is_char_boundary(b) {
                    b -= 1;
                }
                cut = b;
                let p = std::env::temp_dir().join(format!(
                    "axis_callsites_prefix_{}_{}.m1",
                    std::process::id(),
                    cut
                ));
                if std::fs::write(&p, &full_src[..cut]).is_err() {
                    break;
                }
                cfg.source_file = p.clone();
                tmp = Some(p);
            }
        }
    }

    if let Some(p) = &tmp {
        let _ = std::fs::remove_file(p);
    }

    let artifacts = match artifacts {
        Some(a) => {
            if cut < full_src.len() {
                println!("PARTIAL {}/{}", cut, full_src.len());
            }
            a
        }
        None => {
            // Not even a prefix parsed — genuinely nothing to report. Exit 2
            // so a harness can tell "unparseable" from "parsed, zero calls".
            eprintln!("axis callsites: {}", last_err);
            std::process::exit(2);
        }
    };

    fn tok(node: &SchemaAstNode, field: &str) -> Option<String> {
        match node.fields.get(field) {
            Some(SchemaValue::Token(t)) => Some(t.lexeme.clone()),
            _ => None,
        }
    }

    // Collect the declared fn names first: a call to a fn declared later in the
    // same file is still resolvable, and a bare reference to one is an fn-ref.
    fn collect_decls(node: &SchemaAstNode, out: &mut Vec<String>) {
        if node.kind == "FnDecl" {
            if let Some(n) = tok(node, "name") {
                out.push(n);
            }
        }
        for v in node.fields.values() {
            match v {
                SchemaValue::Node(c) => collect_decls(c, out),
                SchemaValue::Nodes(cs) => cs.iter().for_each(|c| collect_decls(c, out)),
                _ => {}
            }
        }
    }

    fn walk(node: &SchemaAstNode, bound: &mut Vec<String>, out: &mut Vec<(&'static str, String)>) {
        let mut pushed = 0usize;

        match node.kind.as_str() {
            "FnDecl" => {
                if let Some(SchemaValue::Node(params)) = node.fields.get("params") {
                    fn params_of(n: &SchemaAstNode, acc: &mut Vec<String>) {
                        if n.kind == "TypedParam" {
                            if let Some(SchemaValue::Token(t)) = n.fields.get("name") {
                                acc.push(t.lexeme.clone());
                            }
                        }
                        for v in n.fields.values() {
                            match v {
                                SchemaValue::Node(c) => params_of(c, acc),
                                SchemaValue::Nodes(cs) => cs.iter().for_each(|c| params_of(c, acc)),
                                _ => {}
                            }
                        }
                    }
                    let mut ps = Vec::new();
                    params_of(params, &mut ps);
                    pushed += ps.len();
                    bound.extend(ps);
                }
            }
            "LetExpr" | "LetDecl" => {
                if let Some(n) = tok(node, "name") {
                    bound.push(n);
                    pushed += 1;
                }
            }
            "CallExpr" => {
                if let Some(n) = tok(node, "name") {
                    out.push(("CALL", n));
                }
            }
            "VarRef" => {
                if let Some(n) = tok(node, "name") {
                    if bound.contains(&n) {
                        out.push(("VAR", n));
                    } else {
                        out.push(("FNREF", n));
                    }
                }
            }
            "IntLit" => out.push(("TYPECTOR", "Int".into())),
            "TextLit" => out.push(("TYPECTOR", "Text".into())),
            "BoolLit" => out.push(("TYPECTOR", "Bool".into())),
            "UnitLit" => out.push(("TYPECTOR", "Unit".into())),
            "Literal" => out.push(("TYPECTOR", "Literal".into())),
            "ValueConstructExpr" => out.push(("TYPECTOR", "Value".into())),
            "ValueListConstructExpr" => out.push(("TYPECTOR", "ValueList".into())),
            _ => {}
        }

        // AppExpr's callee is the leftmost VarRef of the application spine; the
        // VarRef arm above would otherwise report it as an fn-ref.
        if node.kind == "AppExpr" {
            fn head<'a>(n: &'a SchemaAstNode) -> Option<&'a SchemaAstNode> {
                if n.kind == "VarRef" {
                    return Some(n);
                }
                match n.fields.get("func").or_else(|| n.fields.get("fn")) {
                    Some(SchemaValue::Node(c)) => head(c),
                    _ => None,
                }
            }
            if let Some(h) = head(node) {
                if let Some(n) = tok(h, "name") {
                    out.push(("CALL", n));
                }
            }
        }

        let mut keys: Vec<&String> = node.fields.keys().collect();
        keys.sort(); // deterministic order — the output is compared across runs
        for k in keys {
            match &node.fields[k] {
                SchemaValue::Node(c) => walk(c, bound, out),
                SchemaValue::Nodes(cs) => cs.iter().for_each(|c| walk(c, bound, out)),
                _ => {}
            }
        }

        for _ in 0..pushed {
            bound.pop();
        }
    }

    let mut decls: Vec<String> = Vec::new();
    for item in &artifacts.schema_asts {
        collect_decls(item, &mut decls);
    }
    for d in &decls {
        println!("DECL {}", d);
    }

    let mut out: Vec<(&'static str, String)> = Vec::new();
    for item in &artifacts.schema_asts {
        // Declared fns are in scope for every item in the file.
        let mut bound: Vec<String> = decls.clone();
        walk(item, &mut bound, &mut out);
    }

    // A bare reference to a fn declared in this file is an fn-ref, not a VAR.
    for (kind, name) in &out {
        let k = if *kind == "VAR" && decls.contains(name) { "FNREF" } else { *kind };
        println!("{} {}", k, name);
    }

    std::process::exit(0);
}

// ── `axis symbols` subcommand ──────────────────────────────────────────────────
//
// Usage: axis symbols [<substring>] --reg <path>...
//
// Case-insensitive substring search over every projected name (type or fn)
// in the given registries. Bounded: at most SYMBOLS_LIMIT matches are
// printed; if more exist, a final line reports the drop count so truncation
// is never silent. Read-only: only reads the given registry files.
//
// Exit 0 → zero or more matches printed (zero matches is not an error).
// Exit 1 → bad input (missing --reg, unreadable registry).
fn cmd_symbols(args: &[String]) {
    use axis_lang::registry::core05::record::TypeShape;

    const SYMBOLS_LIMIT: usize = 50;

    let mut query: Option<String> = None;
    let mut reg_paths: Vec<std::path::PathBuf> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--reg" => {
                if i + 1 >= args.len() {
                    eprintln!("axis symbols: --reg requires a path argument");
                    std::process::exit(1);
                }
                reg_paths.push(std::path::PathBuf::from(&args[i + 1]));
                i += 2;
            }
            arg if !arg.starts_with('-') && query.is_none() => {
                query = Some(arg.to_string());
                i += 1;
            }
            other => {
                eprintln!("axis symbols: unknown argument '{}'", other);
                std::process::exit(1);
            }
        }
    }

    if reg_paths.is_empty() {
        eprintln!("axis symbols: at least one --reg is required");
        std::process::exit(1);
    }

    let scope = match axis_lang::cli_util::load_and_merge_registries(&reg_paths) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("axis symbols: {}", e);
            std::process::exit(1);
        }
    };

    let query_lower = query.as_deref().unwrap_or("").to_lowercase();
    let mut matches: Vec<&String> = scope
        .names
        .keys()
        .filter(|name| name.to_lowercase().contains(&query_lower))
        .collect();
    matches.sort();

    let total = matches.len();
    for name in matches.iter().take(SYMBOLS_LIMIT) {
        let hash = scope.names[*name];
        if let Some(def) = scope.types.get(&hash) {
            let kind = match &def.shape {
                TypeShape::Primitive(p) => format!("primitive {:?}", p),
                TypeShape::Product(_) => "product".to_string(),
                TypeShape::Sum(_) => "sum".to_string(),
                TypeShape::List(_) => "list".to_string(),
            };
            println!("TYPE {} id={} {}", name, hash.to_hex(), kind);
        } else if let Some(fngen) = scope.fns.get(&hash) {
            let ins = fngen
                .inputs
                .iter()
                .map(|h| h.to_hex())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "FN {} id={} kind={:?} in=[{}] out={}",
                name,
                hash.to_hex(),
                fngen.kind,
                ins,
                fngen.output.to_hex()
            );
        } else {
            println!("NAME {} id={} (unresolved kind)", name, hash.to_hex());
        }
    }

    if total > SYMBOLS_LIMIT {
        println!(
            "... {} more match(es) not shown; refine the query",
            total - SYMBOLS_LIMIT
        );
    }

    std::process::exit(0);
}

// ── `axis find` subcommand ───────────────────────────────────────────────────
//
// Usage: axis find --out <hash-hex> [--in <hash-hex>[,<hash-hex>...]] --reg <path>...
//
// Exact-match contract search: finds every fn in the given registries whose
// (inputs, output) signature exactly equals the one given, regardless of
// name. This is the dedup gate (NO_DUPLICATE_CONTRACT_MINTING) — consult
// before authoring a new fn, whatever name you intend to give it. Omit --in
// (or pass an empty string) for a nullary signature. Read-only: only reads
// the given registry files.
//
// Exit 0 → one or more matches printed.
// Exit 1 → NO_MATCH, or bad input.
fn cmd_find(args: &[String]) {
    use axis_lang::fabric::hash::Hash256;

    let mut out_arg: Option<String> = None;
    let mut in_arg: Option<String> = None;
    let mut reg_paths: Vec<std::path::PathBuf> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                if i + 1 >= args.len() {
                    eprintln!("axis find: --out requires a hash argument");
                    std::process::exit(1);
                }
                out_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--in" => {
                if i + 1 >= args.len() {
                    eprintln!("axis find: --in requires a comma-separated hash list");
                    std::process::exit(1);
                }
                in_arg = Some(args[i + 1].clone());
                i += 2;
            }
            "--reg" => {
                if i + 1 >= args.len() {
                    eprintln!("axis find: --reg requires a path argument");
                    std::process::exit(1);
                }
                reg_paths.push(std::path::PathBuf::from(&args[i + 1]));
                i += 2;
            }
            other => {
                eprintln!("axis find: unknown argument '{}'", other);
                std::process::exit(1);
            }
        }
    }

    let out_arg = match out_arg {
        Some(h) => h,
        None => {
            eprintln!(
                "axis find: usage: axis find --out <hash-hex> [--in <hash-hex>[,<hash-hex>...]] --reg <path>..."
            );
            std::process::exit(1);
        }
    };
    if reg_paths.is_empty() {
        eprintln!("axis find: at least one --reg is required");
        std::process::exit(1);
    }

    let out_hash = match Hash256::from_hex(&out_arg) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("axis find: invalid --out hash '{}': {:?}", out_arg, e);
            std::process::exit(1);
        }
    };
    let in_hashes: Vec<Hash256> = match &in_arg {
        None => Vec::new(),
        Some(s) if s.is_empty() => Vec::new(),
        Some(s) => {
            let mut hs = Vec::new();
            for piece in s.split(',') {
                match Hash256::from_hex(piece) {
                    Ok(h) => hs.push(h),
                    Err(e) => {
                        eprintln!("axis find: invalid --in hash '{}': {:?}", piece, e);
                        std::process::exit(1);
                    }
                }
            }
            hs
        }
    };

    let scope = match axis_lang::cli_util::load_and_merge_registries(&reg_paths) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("axis find: {}", e);
            std::process::exit(1);
        }
    };

    let mut found = false;
    for (id, fngen) in &scope.fns {
        if fngen.output == out_hash && fngen.inputs == in_hashes {
            let name = scope
                .names
                .iter()
                .find(|(_, v)| *v == id)
                .map(|(k, _)| k.clone())
                .unwrap_or_else(|| "<unnamed>".to_string());
            println!(
                "MATCH {} id={} kind={:?} disposition={:?} variadic={}",
                name,
                id.to_hex(),
                fngen.kind,
                fngen.disposition,
                fngen.variadic
            );
            found = true;
        }
    }

    if !found {
        println!("NO_MATCH: no fn with this exact signature in given registries");
        std::process::exit(1);
    }
    std::process::exit(0);
}

// ── `axis verify` subcommand ──────────────────────────────────────────────────
//
// Usage: axis verify <bundle> --reg <path>... [--body-dir d] [--effect-ceiling c] [--param-types h0,h1,..] [--result-type h] [--effect-ceiling <level>]
//                                    [--body-dir <dir>...]
//   level: pure | reads | writes | fullio  (default: fullio)
//
// Exit 0 → PASS; prints {"effect_class":"..."} to stdout.
// Exit 1 → FAIL (non-composite); prints VerifyError to stderr.
// Exit 2 → FAIL (CompositeEffectUnderivable); prints error to stderr.
fn cmd_verify(args: &[String]) {
    use std::collections::HashMap;
    use axis_lang::ir::core_ir_05::{bundle_identity, read_axbi, CoreBundle};
    use axis_lang::fabric::hash::Hash256;
    use axis_lang::registry::core05::record::EffectClass;
    use axis_lang::validation::core_ir::VerifyError;
    use axis_lang::validation::core_ir::{declared_result_admits, verify_bundle_with_params};

    let mut bundle_path: Option<std::path::PathBuf> = None;
    let mut reg_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut body_dirs: Vec<std::path::PathBuf> = Vec::new();
    let mut ceiling = EffectClass::ANY;
    let mut param_types_raw: Vec<String> = Vec::new();
    let mut declared_result_raw: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--reg" => {
                if i + 1 >= args.len() {
                    eprintln!("axis verify: --reg requires a path argument");
                    std::process::exit(1);
                }
                reg_paths.push(std::path::PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--body-dir" => {
                if i + 1 >= args.len() {
                    eprintln!("axis verify: --body-dir requires a path argument");
                    std::process::exit(1);
                }
                body_dirs.push(std::path::PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--param-types" => {
                // one type per parameter slot, comma separated, by registry name (Text, TextList, ...) or 64-hex hash
                // the types a solver-written bundle cannot carry itself
                if i + 1 >= args.len() {
                    eprintln!("axis verify: --param-types requires type names, comma separated");
                    std::process::exit(1);
                }
                for h in args[i + 1].split(',').filter(|s| !s.is_empty()) {
                    param_types_raw.push(h.to_string());
                }
                i += 2;
            }
            "--result-type" => {
                if i + 1 >= args.len() {
                    eprintln!("axis verify: --result-type requires a hex hash");
                    std::process::exit(1);
                }
                declared_result_raw = Some(args[i + 1].clone());
                i += 2;
            }
            "--effect-ceiling" => {
                if i + 1 >= args.len() {
                    eprintln!("axis verify: --effect-ceiling requires a level argument");
                    std::process::exit(1);
                }
                ceiling = match args[i + 1].to_lowercase().as_str() {
                    "pure"   => EffectClass::Pure,
                    "reads"  => EffectClass::Reads,
                    "writes" => EffectClass::Writes,
                    "fullio" => EffectClass::ANY,
                    other => {
                        eprintln!("axis verify: unknown effect ceiling '{}' (pure|reads|writes|fullio)", other);
                        std::process::exit(1);
                    }
                };
                i += 2;
            }
            arg if !arg.starts_with('-') && bundle_path.is_none() => {
                bundle_path = Some(std::path::PathBuf::from(arg));
                i += 1;
            }
            other => {
                eprintln!("axis verify: unknown argument '{}'", other);
                std::process::exit(1);
            }
        }
    }

    let bundle_path = match bundle_path {
        Some(p) => p,
        None => {
            eprintln!("axis verify: usage: axis verify <bundle> --reg <path>... [--body-dir d] [--effect-ceiling c] [--param-types h0,h1,..] [--result-type h]");
            std::process::exit(1);
        }
    };

    if reg_paths.is_empty() {
        eprintln!("axis verify: at least one --reg is required");
        std::process::exit(1);
    }

    let raw = match std::fs::read(&bundle_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("axis verify: cannot read {}: {}", bundle_path.display(), e);
            std::process::exit(1);
        }
    };
    let bundle = match read_axbi(&raw) {
        Ok(b) => b,
        Err(e @ axis_lang::ir::core_ir_05::AxbiError::Version05) => {
            // Core IR 0.6: a 0.5 bundle is refused, not passed through as an unknown format
            eprintln!("axis verify: {}: {}", bundle_path.display(), e);
            std::process::exit(1);
        }
        Err(e) => {
            // Exit 3 = bundle is not a 0.6 .coreir file (old format or corrupt).
            // MCP gate treats exit 3 as format-mismatch pass-through, not a verify failure.
            eprintln!("axis verify: cannot decode bundle {} as Core IR 0.6: {}", bundle_path.display(), e);
            std::process::exit(3);
        }
    };

    let scope = match axis_lang::cli_util::load_and_merge_registries(&reg_paths) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("axis verify: {}", e);
            std::process::exit(1);
        }
    };

    // Type names given to --param-types / --result-type resolve against the loaded registry (or are 64-hex hashes).
    let resolve_type = |s: &str| -> axis_lang::fabric::hash::Hash256 {
        match axis_lang::fabric::hash::Hash256::from_hex(s).ok().or_else(|| scope.lookup_name(s)) {
            Some(t) => t,
            None => {
                eprintln!("axis verify: unknown type '{}' (not in the registry, not a hash)", s);
                std::process::exit(1);
            }
        }
    };
    let param_types: Vec<Vec<axis_lang::fabric::hash::Hash256>> = param_types_raw.iter().map(|s| vec![resolve_type(s)]).collect();
    let declared_result = declared_result_raw.as_deref().map(|s| resolve_type(s));

    // Build body store from --body-dir paths.
    let mut body_store: HashMap<Hash256, CoreBundle> = HashMap::new();
    for dir in &body_dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("axis verify: cannot read body dir {}: {}", dir.display(), err);
                std::process::exit(1);
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("coreir") {
                if let Ok(body_bytes) = std::fs::read(&path) {
                    if let Ok(body_bundle) = read_axbi(&body_bytes) {
                        let hash = bundle_identity(&body_bundle);
                        body_store.insert(hash, body_bundle);
                    }
                }
            }
        }
    }

    match verify_bundle_with_params(&bundle, &scope, &body_store, ceiling, &param_types) {
        Ok(pass) => {
            if let Some(want) = declared_result {
                if !declared_result_admits(&pass, want, &scope) {
                    eprintln!("VERIFY_FAIL: result type: the program produces {:?}, the need declares {}", pass.result_types.iter().map(|t| t.to_hex()).collect::<Vec<_>>(), want.to_hex());
                    std::process::exit(1);
                }
            }
            let ec = pass.effect_class;
            let effect = if ec == EffectClass::Pure               { "Pure" }
                else if ec == EffectClass::Reads                  { "Reads" }
                else if ec == EffectClass::Writes                 { "Writes" }
                else if ec == EffectClass::Nondeterministic       { "Nondeterministic" }
                else if ec == EffectClass::FullIo                 { "FullIo" }
                else                                              { "mixed" };
            println!(
                "{{\"effect_class\":\"{}\",\"node_count\":{}}}",
                effect,
                bundle.nodes.len()
            );
            std::process::exit(0);
        }
        Err(VerifyError::CompositeEffectUnderivable { node_index, target, reason }) => {
            eprintln!(
                "COMPOSITE_EFFECT_UNDERIVABLE: node[{}] target {} — {}",
                node_index, target.to_hex(), reason
            );
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("VERIFY_FAIL: {}", e);
            std::process::exit(1);
        }
    }
}
