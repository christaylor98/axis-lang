// PYAX_SURFACE_EXPAND_V1 d5 — the PyAx surface through the real `axis` BINARY.
//
// Every `tests/fixtures/pyax/*.pyax` is compiled by `axis --expand`, and then
// either rejected with the expected diagnostic, or built by axis-codegen-bridge
// and run, with stdout and exit code compared to what the fixture declares.
//
// Fixture header directives (comment lines at the top):
//   # args: a b c                 command line for the program
//   # expect-exit: N              run, and expect this exit code; stdout must
//                                 equal `<name>.out`
//   # expect-error: <substring>   compilation must fail with this text on
//                                 stderr (repeatable: all must appear)
//
// The positive fixtures' `.out` files were cross-checked against CPython
// running the same source (lists.pyax and bytes_loop.pyax excepted: they call registry functions).
//
// Needs the sibling checkouts this repo already builds against:
//   ../axRegistry                       (override: AXIS_REGISTRY_DIR)
//   ../axis-codegen-bridge-rs/target/release/axis-codegen-bridge (override: AXIS_BRIDGE)
// A missing one is a test failure, not a skip.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The binary cargo built for this test run — never a stale one lying in
/// target/ (the wave_s4 footgun in CLAUDE.md).
fn axis_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_axis"))
}

fn registry_dir() -> PathBuf {
    std::env::var_os("AXIS_REGISTRY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("../axRegistry"))
}

fn bridge_bin() -> PathBuf {
    std::env::var_os("AXIS_BRIDGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("../axis-codegen-bridge-rs/target/release/axis-codegen-bridge"))
}

fn reg_args() -> Vec<String> {
    let dir = registry_dir();
    let mut v = Vec::new();
    for r in ["axis-types", "axis", "axis-bridge", "m1-builtins", "gen-working"] {
        let p = dir.join(format!("{}.axreg", r));
        assert!(p.exists(), "registry file missing: {} (set AXIS_REGISTRY_DIR)", p.display());
        v.push("--reg".to_string());
        v.push(p.display().to_string());
    }
    // PyAx's own registry: callables linked to existing runtime fns by
    // pyax-dispatch.toml.
    v.push("--reg".to_string());
    v.push(root().join("axis-surface-pyax-config/pyax.axreg").display().to_string());
    v
}

fn compile(src: &Path, out: &Path) -> std::process::Output {
    let cfg = root().join("axis-surface-pyax-config");
    Command::new(axis_bin())
        .arg("--file").arg(src)
        .arg("--lexer").arg(cfg.join("pyax-lexer.yaml"))
        .arg("--parser").arg(cfg.join("pyax-parse.yaml"))
        .arg("--schema").arg(cfg.join("pyax-ast.yaml"))
        .arg("--normalize").arg(cfg.join("pyax-normalize.yaml"))
        .arg("--expand").arg(cfg.join("pyax-expand.yaml"))
        .args(reg_args())
        .arg("--out").arg(format!("{}/", out.display()))
        .output()
        .expect("run axis")
}

struct Fixture {
    path: PathBuf,
    name: String,
    args: Vec<String>,
    exit: Option<i32>,
    errors: Vec<String>,
}

fn fixtures() -> Vec<Fixture> {
    let dir = root().join("tests/fixtures/pyax");
    let mut out = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/fixtures/pyax")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("pyax"))
        .collect();
    paths.sort();
    for path in paths {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut f = Fixture {
            name: path.file_stem().unwrap().to_string_lossy().into_owned(),
            path: path.clone(),
            args: vec![],
            exit: None,
            errors: vec![],
        };
        for line in text.lines().take_while(|l| l.starts_with('#')) {
            let d = line.trim_start_matches('#').trim();
            if let Some(a) = d.strip_prefix("args:") {
                f.args = a.split_whitespace().map(String::from).collect();
            } else if let Some(n) = d.strip_prefix("expect-exit:") {
                f.exit = Some(n.trim().parse().expect("expect-exit: N"));
            } else if let Some(e) = d.strip_prefix("expect-error:") {
                f.errors.push(e.trim().to_string());
            }
        }
        assert!(
            f.exit.is_some() != !f.errors.is_empty(),
            "{}: give exactly one of `expect-exit` or `expect-error`",
            path.display()
        );
        out.push(f);
    }
    assert!(out.len() >= 25, "expected the phase-2 fixture set, found {}", out.len());
    out
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("pyax_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn check(f: &Fixture) -> Result<(), String> {
    let work = scratch(&f.name);
    let out = work.join("out");
    let c = compile(&f.path, &out);
    let stderr = String::from_utf8_lossy(&c.stderr).into_owned();

    if !f.errors.is_empty() {
        if c.status.success() {
            return Err(format!("{}: compiled, but was expected to fail with {:?}", f.name, f.errors));
        }
        for e in &f.errors {
            if !stderr.contains(e.as_str()) {
                return Err(format!("{}: stderr lacks {:?}\n--- stderr ---\n{}", f.name, e, stderr));
            }
        }
        return Ok(());
    }

    if !c.status.success() {
        return Err(format!("{}: compile failed\n{}", f.name, stderr));
    }
    let bridge = bridge_bin();
    if !bridge.exists() {
        return Err(format!("bridge not found at {} (set AXIS_BRIDGE)", bridge.display()));
    }
    let exe = work.join("exe");
    let b = Command::new(&bridge)
        .arg("build")
        .arg(out.join("pyax_main.coreir"))
        .arg("--out").arg(&exe)
        .arg("--exe")
        .arg("--lib-dir").arg(&out)
        .args(reg_args())
        .arg("--dispatch").arg(root().join("axis-surface-pyax-config/pyax-dispatch.toml"))
        .output()
        .expect("run bridge");
    if !b.status.success() {
        return Err(format!("{}: bridge build failed\n{}", f.name, String::from_utf8_lossy(&b.stderr)));
    }
    let r = Command::new(&exe).args(&f.args).output().expect("run program");
    let got = String::from_utf8_lossy(&r.stdout).into_owned();
    let want = std::fs::read_to_string(f.path.with_extension("out"))
        .map_err(|e| format!("{}: {}.out: {}", f.name, f.name, e))?;
    if r.status.code() != f.exit {
        return Err(format!(
            "{}: exit {:?}, expected {:?}\n--- stderr ---\n{}",
            f.name, r.status.code(), f.exit, String::from_utf8_lossy(&r.stderr)
        ));
    }
    if got != want {
        return Err(format!("{}: stdout differs\n--- got ---\n{}--- want ---\n{}", f.name, got, want));
    }
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}

#[test]
fn pyax_fixtures_through_axis_binary() {
    let failures: Vec<String> = fixtures().iter().filter_map(|f| check(f).err()).collect();
    assert!(failures.is_empty(), "{} fixture(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

/// EXPAND_IS_PURE: the same source gives byte-identical bundles, twice.
#[test]
fn pyax_compile_is_deterministic() {
    let src = root().join("tests/fixtures/pyax/control.pyax");
    let ids = |tag: &str| -> Vec<String> {
        let out = scratch(&format!("det_{}", tag)).join("out");
        let c = compile(&src, &out);
        assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
        let mut v: Vec<String> = String::from_utf8_lossy(&c.stdout)
            .lines()
            .map(|l| l.split(" → ").nth(1).unwrap_or("").rsplit('/').next().unwrap_or("").to_string())
            .collect();
        v.sort();
        v
    };
    let a = ids("a");
    assert!(a.len() >= 3, "expected one bundle per fn: {:?}", a);
    assert_eq!(a, ids("b"));
}

/// The stage is opt-in: without --expand a PyAx file is not PyAx to `axis`
/// (the layout tokens still lex, but nothing rewrites the operators), so it
/// must fail rather than half-compile.
#[test]
fn pyax_without_expand_is_rejected() {
    let cfg = root().join("axis-surface-pyax-config");
    let out = scratch("noexpand").join("out");
    let c = Command::new(axis_bin())
        .arg("--file").arg(root().join("tests/fixtures/pyax/fact.pyax"))
        .arg("--lexer").arg(cfg.join("pyax-lexer.yaml"))
        .arg("--parser").arg(cfg.join("pyax-parse.yaml"))
        .arg("--schema").arg(cfg.join("pyax-ast.yaml"))
        .arg("--normalize").arg(cfg.join("pyax-normalize.yaml"))
        .args(reg_args())
        .arg("--out").arg(format!("{}/", out.display()))
        .output()
        .expect("run axis");
    assert!(!c.status.success(), "PyAx compiled without --expand");
}

// ── d6: golden expand outputs (EXPAND_IS_PURE, EXPAND_OUTPUT_IS_PYAX) ──────────
//
// For each tests/fixtures/pyax/expand/<name>.pyax:
//   1. expand it in-process and print the expanded program as PyAx source;
//      the author's and lifted fns (not the prelude) must equal
//      <name>.expanded.pyax byte for byte (set PYAX_BLESS=1 to rewrite);
//   2. compile that printed source again through the same surface — expand's
//      round-trip variant: no prelude, its own reserved names allowed — and
//      require every fn's bundle identity to equal the original compile's.
// (2) is the claim that expand's output IS PyAx: the surface accepts it and it
// means exactly what the original meant, down to the bundle bytes.

mod golden {
    use super::root;
    use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
    use axis_lang::ir::core_ir_05::bundle_identity;
    use axis_lang::normalize::expand_engine::ExpandEngine;
    use axis_lang::pipeline::{run_pipeline_expanded, PipelineArtifacts, PipelineConfig};
    use std::path::{Path, PathBuf};

    fn compile(src: &Path, engine: &ExpandEngine) -> PipelineArtifacts {
        let cfg_dir = root().join("axis-surface-pyax-config");
        let mut regs: Vec<PathBuf> = ["axis-types", "axis", "axis-bridge", "m1-builtins", "gen-working"]
            .iter()
            .map(|r| super::registry_dir().join(format!("{}.axreg", r)))
            .collect();
        regs.push(cfg_dir.join("pyax.axreg"));
        let scope = axis_lang::cli_util::load_and_merge_registries(&regs).expect("registries");
        let cfg = PipelineConfig {
            lexer_spec: cfg_dir.join("pyax-lexer.yaml"),
            parser_spec: cfg_dir.join("pyax-parse.yaml"),
            ast_schema: cfg_dir.join("pyax-ast.yaml"),
            source_file: src.to_path_buf(),
            entry_rule: None,
            normalize_spec: cfg_dir.join("pyax-normalize.yaml"),
            registry: axis_lang::registry::Registry::new(),
            core05_scope: Some(scope),
            parser_mode: None,
            hook_registry: None,
        };
        run_pipeline_expanded(&cfg, engine).unwrap_or_else(|e| panic!("{}: {}", src.display(), e))
    }

    fn tok<'a>(n: &'a SchemaAstNode, f: &str) -> &'a str {
        match n.fields.get(f) {
            Some(SchemaValue::Token(t)) => &t.lexeme,
            _ => panic!("{} has no token {}", n.kind, f),
        }
    }

    fn node<'a>(n: &'a SchemaAstNode, f: &str) -> &'a SchemaAstNode {
        match n.fields.get(f) {
            Some(SchemaValue::Node(x)) => x,
            _ => panic!("{} has no node {}", n.kind, f),
        }
    }

    fn nodes<'a>(n: &'a SchemaAstNode, f: &str) -> Vec<&'a SchemaAstNode> {
        match n.fields.get(f) {
            Some(SchemaValue::Nodes(xs)) => xs.iter().collect(),
            _ => vec![],
        }
    }

    /// Expanded PyAx (the output vocabulary) -> PyAx source.
    fn expr(n: &SchemaAstNode, e: &ExpandEngine) -> String {
        match n.kind.as_str() {
            "CallExpr" => {
                let args: Vec<String> = nodes(node(n, "args"), "args").into_iter().map(|a| expr(a, e)).collect();
                format!("{}({})", e.printed_call_name(tok(n, "name")), args.join(", "))
            }
            "Name" => tok(n, "name").to_string(),
            "IntLit" | "TextLit" => tok(n, "value").to_string(),
            "BoolLit" => if tok(n, "value") == "true" { "True".into() } else { "False".into() },
            "NoneLit" => "None".into(),
            "Expr" => format!("({} if {} else {})", expr(node(n, "then_branch"), e), expr(node(n, "condition"), e), expr(node(n, "orelse"), e)),
            other => panic!("printer: unexpected expression kind {}", other),
        }
    }

    fn block(b: &SchemaAstNode, ind: usize, out: &mut String, e: &ExpandEngine) {
        let pad = " ".repeat(ind);
        for st in nodes(b, "bindings") {
            out.push_str(&format!("{}{} = {}\n", pad, tok(st, "name"), expr(node(st, "value"), e)));
        }
        let r = node(b, "result");
        if r.kind == "IfStmt" {
            out.push_str(&format!("{}if {}:\n", pad, expr(node(r, "condition"), e)));
            block(node(r, "then_branch"), ind + 4, out, e);
            out.push_str(&format!("{}else:\n", pad));
            block(node(r, "orelse"), ind + 4, out, e);
        } else {
            out.push_str(&format!("{}return {}\n", pad, expr(r, e)));
        }
    }

    fn print_fn(f: &SchemaAstNode, out: &mut String, e: &ExpandEngine) {
        let params: Vec<String> = nodes(node(f, "params"), "params")
            .into_iter()
            .map(|p| format!("{}: {}", tok(p, "name"), tok(node(p, "type_expr"), "name")))
            .collect();
        out.push_str(&format!("def {}({}) -> {}:\n", tok(f, "name"), params.join(", "), tok(node(f, "return_type"), "name")));
        block(node(f, "body"), 4, out, e);
        out.push('\n');
    }

    fn prelude_names() -> Vec<String> {
        let text = std::fs::read_to_string(root().join("axis-surface-pyax-config/pyax-prelude.pyax")).unwrap();
        text.lines().filter_map(|l| l.strip_prefix("def ")).map(|l| l.split('(').next().unwrap().to_string()).collect()
    }

    fn is_prelude(name: &str, prelude: &[String]) -> bool {
        prelude.iter().any(|p| name == p || name.starts_with(&format!("__{}_", p)))
    }

    /// The whole expanded program as source, and the author's part of it.
    fn print_program(p: &SchemaAstNode, e: &ExpandEngine) -> (String, String) {
        let prelude = prelude_names();
        let (mut all, mut author) = (String::new(), String::new());
        for f in nodes(p, "items") {
            print_fn(f, &mut all, e);
            if !is_prelude(tok(f, "name"), &prelude) {
                print_fn(f, &mut author, e);
            }
        }
        (all, author)
    }

    #[test]
    fn pyax_expand_goldens_and_roundtrip() {
        let engine = ExpandEngine::load(&root().join("axis-surface-pyax-config/pyax-expand.yaml")).unwrap();
        let roundtrip = engine.for_expanded_source();
        let dir = root().join("tests/fixtures/pyax/expand");
        let mut srcs: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path())
            .filter(|p| p.to_string_lossy().ends_with(".pyax") && !p.to_string_lossy().ends_with(".expanded.pyax"))
            .collect();
        srcs.sort();
        assert!(srcs.len() >= 3, "expand goldens missing");
        let bless = std::env::var_os("PYAX_BLESS").is_some();
        let mut failures = Vec::new();
        for src in srcs {
            let name = src.file_stem().unwrap().to_string_lossy().into_owned();
            let a = compile(&src, &engine);
            let (all, author) = print_program(a.expanded.as_ref().expect("expanded program"), &engine);
            let golden = dir.join(format!("{}.expanded.pyax", name));
            if bless {
                std::fs::write(&golden, &author).unwrap();
            } else {
                let want = std::fs::read_to_string(&golden).unwrap_or_default();
                if want != author {
                    failures.push(format!("{}: expanded output differs from {}\n--- got ---\n{}", name, golden.display(), author));
                }
            }
            // Round trip: the printed program compiles to the same bundles.
            let printed = std::env::temp_dir().join(format!("pyax_rt_{}_{}.pyax", name, std::process::id()));
            std::fs::write(&printed, &all).unwrap();
            let b = compile(&printed, &roundtrip);
            let ids = |x: &PipelineArtifacts| -> Vec<(String, String)> {
                let mut v: Vec<(String, String)> = x.item_bundles.iter().map(|(n, _, bd)| (n.clone(), bundle_identity(bd).to_hex())).collect();
                v.sort();
                v
            };
            if ids(&a) != ids(&b) {
                failures.push(format!("{}: printed expansion compiles to different bundles\n{:?}\nvs\n{:?}", name, ids(&a), ids(&b)));
            }
            let _ = std::fs::remove_file(&printed);
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }
}

/// `axis run`: source → executable → its output, one command, no spec flags,
/// for each surface the subcommand knows. The M1/AI3 programs are two fns in
/// one file, the case a single shared bundle cannot link (`--split`).
#[test]
fn axis_run_one_command_per_surface() {
    let work = scratch("axis_run");
    let m1 = work.join("two.m1");
    std::fs::write(&m1, "fn greet(name: Text) -> Text {\n  str_concat(Text(\"hi \"), name)\n}\n\n\
        fn hello(args: ValueList(Text)) -> Text {\n  greet(text_list_head(value_list_to_text_list(args)))\n}\n").unwrap();
    let ai3 = work.join("two.ai3");
    std::fs::write(&ai3, "fn greet(name: Text) -> Text {\n  return str_concat(Text(\"hi \"), name);\n}\n\n\
        fn hello(args: ValueList(Text)) -> Text {\n  return greet(text_list_head(value_list_to_text_list(args)));\n}\n").unwrap();
    let cases: [(&Path, &str, &str); 3] = [
        (&m1, "m1", "hi m1\n"),
        (&ai3, "ai3", "hi ai3\n"),
        (&root().join("tests/fixtures/pyax/fact.pyax"), "5", "3628800\n"),
    ];
    for (src, arg, want) in cases {
        let r = Command::new(axis_bin())
            .arg("run").arg(src).arg(arg)
            .env("AXIS_REGISTRY_DIR", registry_dir())
            .env("AXIS_BRIDGE", bridge_bin())
            .output()
            .expect("run axis");
        assert!(r.status.success(), "{}: exit {:?}\n{}", src.display(), r.status.code(), String::from_utf8_lossy(&r.stderr));
        assert_eq!(String::from_utf8_lossy(&r.stdout), want, "{}", src.display());
    }
    let _ = std::fs::remove_dir_all(&work);
}
