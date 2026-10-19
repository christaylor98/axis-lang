// Core IR 0.6 (core_ir_spec/axis-core-ir-0.6.md), end to end from source: the compiler writes nodes in source
// order, every node in the block of the branch it was written in, and shares a node only from the block being
// written or one enclosing it. Then the spec's eight worked examples.
//
// Pipeline: M1 source -> lex -> parse -> schema project -> normalize -> lower (nf_lowering).

use axis_lang::ir::core_ir_05::{
    deserialize_canonical, read_axbi, serialize_canonical, verify_blocks, Arm, Builder, CoreBundle, Node, NodeRef,
};
use axis_lang::lowering::nf_lowering::lower_nf_to_bundle_05;
use axis_lang::pipeline::{run_pipeline, PipelineConfig};
use axis_lang::registry::Registry;
use std::path::PathBuf;

const REG: &str = "\
registry core-ir-06-test 0.6
type Unit = prim unit
type Bool = prim bool
type Int = prim int
fn tick
  identity 0x1111111111111111111111111111111111111111111111111111111111111111
  kind leaf
  in (Int)
  out Int
  effect fullIo
  deterministic false
  idempotent false
end
fn pick
  identity 0x2222222222222222222222222222222222222222222222222222222222222222
  kind leaf
  in (Int)
  out Int
  effect pure
  deterministic true
  idempotent true
end
fn int_eq
  identity 0x4444444444444444444444444444444444444444444444444444444444444444
  kind leaf
  in (Int, Int)
  out Bool
  effect pure
  deterministic true
  idempotent true
end
fn seq
  identity 0x3333333333333333333333333333333333333333333333333333333333333333
  kind leaf
  in (Int, Int)
  out Int
  effect pure
  deterministic true
  idempotent true
end
fn bool_and
  identity 0x6666666666666666666666666666666666666666666666666666666666666666
  kind leaf
  in (Bool, Bool)
  out Bool
  effect pure
  deterministic true
  idempotent true
end
fn bool_or
  identity 0x7777777777777777777777777777777777777777777777777777777777777777
  kind leaf
  in (Bool, Bool)
  out Bool
  effect pure
  deterministic true
  idempotent true
end
";

fn compile() -> CoreBundle {
    let cfg = PipelineConfig {
        lexer_spec: PathBuf::from("axis-surface-m1-config/m1-lexer.yaml"),
        parser_spec: PathBuf::from("axis-surface-m1-config/m1-parse.yaml"),
        ast_schema: PathBuf::from("axis-surface-m1-config/m1-ast.yaml"),
        source_file: PathBuf::from("examples/surface-m1/core_ir_06_source_order.m1"),
        entry_rule: None,
        normalize_spec: PathBuf::from("axis-surface-m1-config/m1-normalize.yaml"),
        registry: Registry::new(),
        core05_scope: None,
        parser_mode: None,
        hook_registry: None,
    };
    let artifacts = run_pipeline(&cfg).unwrap_or_else(|e| panic!("M1 pipeline failed: {e}"));
    let scope = axis_lang::registry::core05::lower(REG).expect("test registry").scope;
    lower_nf_to_bundle_05(&artifacts.schema_asts[0], &scope).unwrap_or_else(|e| panic!("lowering failed: {e}"))
}

/// The int a call's literal argument carries (the source position tag), if it is `name(Int(n))`.
fn tag(bundle: &CoreBundle, i: usize, name: &str) -> Option<i64> {
    let Node::CCall { target_name, args, .. } = &bundle.nodes[i] else { return None };
    if target_name != name {
        return None;
    }
    let NodeRef::Pool(p) = args[0] else { return None };
    let payload = &bundle.constant_pool[p as usize].payload;
    // zigzag varint (Int payload)
    let (mut v, mut shift) = (0u64, 0);
    for b in payload {
        v |= ((b & 0x7f) as u64) << shift;
        shift += 7;
        if b & 0x80 == 0 {
            break;
        }
    }
    Some(((v >> 1) as i64) ^ -((v & 1) as i64))
}

/// Every block holding node `i`, outermost first: (CIf, 'T' | 'E') for an arm, (CBlock, 'B') for a plain block.
fn path(bundle: &CoreBundle, i: usize) -> Vec<(usize, char)> {
    let blocks = verify_blocks(bundle).expect("verifies");
    let mut p = Vec::new();
    let mut at = blocks.home[i];
    while let Some(b) = at {
        p.push(match blocks.arm_of_block(b) {
            Some((k, Arm::Then)) => (k as usize, 'T'),
            Some((k, Arm::Else)) => (k as usize, 'E'),
            None => (b as usize, 'B'),
        });
        at = blocks.home[b as usize];
    }
    p.reverse();
    p
}

fn arms(p: &[(usize, char)]) -> String {
    p.iter().map(|x| x.1).collect()
}

#[test]
fn nodes_come_out_in_source_order_each_in_the_block_it_was_written_in() {
    let bundle = compile();
    verify_blocks(&bundle).expect("the compiler's own output obeys the block rules");
    let ticks: Vec<(i64, usize)> =
        (0..bundle.nodes.len()).filter_map(|i| tag(&bundle, i, "tick").map(|t| (t, i))).collect();
    let order: Vec<i64> = ticks.iter().map(|t| t.0).collect();
    assert_eq!(order, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10], "tick nodes in source order; bundle:\n{bundle}");
    let at = |t: i64| ticks.iter().find(|x| x.0 == t).unwrap().1;
    let p = |t: i64| path(&bundle, at(t));
    // where each was written
    assert_eq!(arms(&p(1)), "");
    assert_eq!(arms(&p(2)), "T");
    assert_eq!(arms(&p(3)), "E");
    assert_eq!(arms(&p(4)), "ET");
    assert_eq!(arms(&p(5)), "EE");
    assert_eq!(arms(&p(6)), "T", "bool_and's second argument is its then-arm");
    assert_eq!(arms(&p(7)), "E", "bool_or's second argument is its else-arm");
    assert_eq!(arms(&p(8)), "T");
    assert_eq!(arms(&p(9)), "E");
    assert_eq!(arms(&p(10)), "");
    // the same ifs
    assert_eq!(p(2)[0].0, p(3)[0].0);
    assert_eq!(p(4)[0], p(5)[0]);
    assert_eq!(p(4)[0].0, p(2)[0].0);
    assert_eq!(p(4)[1].0, p(5)[1].0);
    assert_eq!(p(8)[0].0, p(9)[0].0);
    // every CIf's cond is written before its then-block opens
    for (k, n) in bundle.nodes.iter().enumerate() {
        if let Node::CIf { cond: NodeRef::Node(c), then_: NodeRef::Node(t), .. } = n {
            let blocks = verify_blocks(&bundle).unwrap();
            let open = blocks.partner[*t as usize].unwrap();
            assert!(*c < open, "if node {k}: cond {c} is not before its then-block at {open}");
        }
    }
}

#[test]
fn a_pure_call_is_shared_only_from_its_own_block_or_an_enclosing_one() {
    let bundle = compile();
    let picks = |t: i64| -> Vec<usize> { (0..bundle.nodes.len()).filter(|&i| tag(&bundle, i, "pick") == Some(t)).collect() };
    // pick(11): once in each arm of `s` and once after it -- three nodes, none reused across blocks
    let p11 = picks(11);
    assert_eq!(p11.len(), 3, "pick(11) in sibling arms and after the if must be three nodes:\n{bundle}");
    assert_eq!(arms(&path(&bundle, p11[0])), "T");
    assert_eq!(arms(&path(&bundle, p11[1])), "E");
    assert_eq!(arms(&path(&bundle, p11[2])), "");
    // pick(12): written at top level first, so the arm of `u` reuses it -- one node, top level
    let p12 = picks(12);
    assert_eq!(p12.len(), 1, "pick(12) in an arm reuses the enclosing node:\n{bundle}");
    assert_eq!(arms(&path(&bundle, p12[0])), "");
}

#[test]
fn the_blocks_survive_a_canonical_round_trip() {
    let bundle = compile();
    let back = deserialize_canonical(&serialize_canonical(&bundle)).expect("decodes and verifies");
    assert_eq!(back.nodes, bundle.nodes);
}

// ── the worked examples (axis-core-ir-0.6.md), compiled by the `axis` binary against the real registries ──

fn registry_dir() -> PathBuf {
    std::env::var_os("AXIS_REGISTRY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../axRegistry"))
}

fn compile_example(name: &str) -> CoreBundle {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cfg = root.join("axis-surface-m1-config");
    let out = std::env::temp_dir().join(format!("core_ir_06_{}_{}.coreir", name, std::process::id()));
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_axis"));
    cmd.arg("--lexer").arg(cfg.join("m1-lexer.yaml"))
        .arg("--parser").arg(cfg.join("m1-parse.yaml"))
        .arg("--schema").arg(cfg.join("m1-ast.yaml"))
        .arg("--normalize").arg(cfg.join("m1-normalize.yaml"))
        .arg("--file").arg(root.join("examples/surface-m1/core_ir_06").join(format!("{name}.m1")))
        .arg("--out").arg(&out)
        // as the M1 build scripts do: the 0.5 type verifier refuses `tuple_field`'s Value where Int is expected
        .arg("--no-verify");
    for r in ["axis-types.axreg", "axis-bridge.axreg", "gen-working.axreg", "m1-builtins.axreg"] {
        let p = registry_dir().join(r);
        assert!(p.exists(), "registry file missing: {} (set AXIS_REGISTRY_DIR)", p.display());
        cmd.arg("--reg").arg(p);
    }
    let o = cmd.output().expect("run axis");
    assert!(o.status.success(), "{name}: axis failed:\n{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    let bundle = read_axbi(&std::fs::read(&out).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    let _ = std::fs::remove_file(&out);
    bundle
}

fn calls(bundle: &CoreBundle, name: &str) -> Vec<usize> {
    (0..bundle.nodes.len())
        .filter(|&i| matches!(&bundle.nodes[i], Node::CCall { target_name, .. } if target_name == name))
        .collect()
}

/// The call `name` whose Text argument (anywhere in its args) is `text`.
fn call_with_text(bundle: &CoreBundle, name: &str, text: &str) -> usize {
    let found: Vec<usize> = calls(bundle, name)
        .into_iter()
        .filter(|&i| match &bundle.nodes[i] {
            Node::CCall { args, .. } => args.iter().any(|a| match a {
                NodeRef::Pool(p) => bundle.constant_pool[*p as usize].payload.ends_with(text.as_bytes()),
                NodeRef::Node(j) => matches!(&bundle.nodes[*j as usize], Node::CCall { args, .. }
                    if args.iter().any(|b| matches!(b, NodeRef::Pool(p) if bundle.constant_pool[*p as usize].payload.ends_with(text.as_bytes())))),
            }),
            _ => false,
        })
        .collect();
    assert_eq!(found.len(), 1, "{name}({text:?}): {found:?}\n{bundle}");
    found[0]
}

#[test]
fn example_1_nested_if_effect_hoist_write_is_in_its_arm_and_bool_and_is_not_shared() {
    let b = compile_example("ex1_nested_if_effect_hoist");
    let w = calls(&b, "mem_write_int_raw");
    assert_eq!(w.len(), 1);
    let p = path(&b, w[0]);
    assert_eq!(arms(&p), "T", "{b}");
    // that arm's if is decided by bool_and(f, g): itself an if, at top level
    let Node::CIf { cond: NodeRef::Node(c), .. } = &b.nodes[p[0].0] else { panic!("{b}") };
    assert!(matches!(b.nodes[*c as usize], Node::CIf { .. }) && path(&b, *c as usize).is_empty(), "{b}");
    // bool_and(f, h) twice: in _x's else-arm, and again for _y (the first is in a closed block)
    let h = calls(&b, "int_lt")[0] as u32;
    let ifs_over_h: Vec<usize> = (0..b.nodes.len()).filter(|&k| match &b.nodes[k] {
        Node::CIf { then_: NodeRef::Node(t), .. } => matches!(b.nodes[*t as usize], Node::CEnd { value: NodeRef::Node(v) } if v == h),
        _ => false,
    }).collect();
    assert_eq!(ifs_over_h.len(), 2, "{b}");
    assert_eq!(arms(&path(&b, ifs_over_h[0])), "E");
    assert_eq!(arms(&path(&b, ifs_over_h[1])), "");
}

#[test]
fn example_2_toplevel_let_used_only_in_a_branch_is_in_no_block() {
    let b = compile_example("ex2_toplevel_let_used_in_branch");
    let w = calls(&b, "mem_write_int_raw")[0];
    assert!(path(&b, w).is_empty(), "{b}");
    assert!(w < calls(&b, "io_print")[0], "w is written before _o");
}

#[test]
fn example_3_two_deep_ifs() {
    let b = compile_example("ex3_two_deep");
    assert_eq!(arms(&path(&b, call_with_text(&b, "io_println", "w"))), "TT");
    assert_eq!(arms(&path(&b, call_with_text(&b, "io_println", "p-inner"))), "TE");
    assert_eq!(arms(&path(&b, call_with_text(&b, "io_println", "p-outer"))), "E");
    assert_eq!(arms(&path(&b, call_with_text(&b, "io_println", "end"))), "");
}

#[test]
fn example_4_orphan_effect_runs_in_its_arm() {
    let b = compile_example("ex4_orphan_effect");
    assert_eq!(arms(&path(&b, call_with_text(&b, "io_println", "in-then"))), "T", "{b}");
    assert_eq!(arms(&path(&b, call_with_text(&b, "io_println", "top"))), "");
}

#[test]
fn example_5_av_admit_body_runs_where_written_and_its_partial_call_in_its_arm() {
    let b = compile_example("ex5_av_admit");
    let d = calls(&b, "int_div")[0];
    let p = path(&b, d);
    assert_eq!(arms(&p), "T", "{b}");
    assert!(path(&b, p[0].0).is_empty(), "body's if is at top level");
}

#[test]
fn example_6_nv_n_step_lets_are_in_no_block() {
    let b = compile_example("ex6_nv_n_step");
    assert!(path(&b, calls(&b, "mem_write_int_raw")[0]).is_empty(), "{b}");
    assert!(path(&b, calls(&b, "int_add")[0]).is_empty(), "{b}");
}

fn h(s: &str) -> axis_lang::fabric::hash::Hash256 {
    axis_lang::fabric::hash::content_hash(s.as_bytes())
}

fn ccall(name: &str, args: Vec<NodeRef>) -> Node {
    Node::CCall { target_name: name.to_string(), target_identity: h(name), args }
}

#[test]
fn example_7_one_armed_if() {
    // j [ j+1 log(x) j+2 CEnd Unit ] j+3 CIf c then j+2 else Unit
    let mut b = Builder::new();
    let unit = b.intern_constant(axis_lang::lowering::emit_05::unit_type_id(), vec![]);
    let x = b.intern_constant(h("int"), vec![7]);
    let c = b.push_node(ccall("c", vec![x])).unwrap();
    b.push_node(Node::CBlock).unwrap();
    let log = b.push_node(ccall("log", vec![x])).unwrap();
    let e = b.push_node(Node::CEnd { value: unit }).unwrap();
    let k = b.push_node(Node::CIf { cond: c, then_: e, else_: unit }).unwrap();
    let bundle = b.finish(k).expect("a one-armed if verifies");
    let NodeRef::Node(log) = log else { unreachable!() };
    assert_eq!(arms(&path(&bundle, log as usize)), "T");
}

#[test]
fn example_8_plain_grouping_block() {
    // m [ m+1 a = f() m+2 b = g(a) m+3 CEnd b ] m+4 h(m+3): always runs; h may use the CEnd, not a or b
    let mut b = Builder::new();
    let x = b.intern_constant(h("int"), vec![7]);
    b.push_node(Node::CBlock).unwrap();
    let a = b.push_node(ccall("f", vec![x])).unwrap();
    let g = b.push_node(ccall("g", vec![a])).unwrap();
    let e = b.push_node(Node::CEnd { value: g }).unwrap();
    let r = b.push_node(ccall("h", vec![e])).unwrap();
    let bundle = b.finish(r).expect("a plain block verifies");
    assert_eq!(arms(&path(&bundle, 2)), "B");
    assert!(path(&bundle, 4).is_empty());
    let mut bad = bundle.clone();
    bad.nodes[4] = ccall("h", vec![a]);
    assert_eq!(verify_blocks(&bad).unwrap_err().to_string(),
               "block rule 5: node 4 uses node 1, which is inside the closed block opened at node 0");
}
