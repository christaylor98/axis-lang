//! How often is an argument in a `Value`-declared parameter slot STATICALLY
//! concrete?
//!
//! The question behind it: the runtime panics inside the accessor/coercion
//! family (`value_to_int`, `tuple_field`, `option_unwrap`, `list_get`, …) fire
//! when the `Value` they are handed holds the wrong variant. Those fns declare
//! `in (Value)`, and `Value` is a sum containing every primitive, so the
//! verifier's Step 2b admits ANY argument — it passes, and the program panics
//! at run time.
//!
//! But the verifier already knows each node's static type. Where the producing
//! node's type is concrete (`Text`, say) and the fn requires `Int`, that panic
//! was detectable at build time; a per-argument narrowing rule would convert it
//! into a `TypeMismatch`. Where the producing node's type genuinely IS `Value`
//! — a list element, a `loop_count` accumulator — no type system without
//! refinement types can do better, and the check is irreducibly dynamic.
//!
//! So the value of building that rule is exactly the ratio between those two
//! cases in real code. This counts it.
//!
//! RESULT, axMachina's 342 driver bundles (2026-09-23)
//! ----------------------------------------------------
//! 388 arguments land in a `Value`-declared slot. 79.4% are statically
//! concrete — but that headline is misleading and the per-callee breakdown is
//! the actual answer:
//!
//!   io_println   240 concrete    0 dynamic     <- accepts anything by design
//!   io_print      24 concrete    0 dynamic     <- same
//!   loop_while    41 concrete    3 dynamic     <- the loop seed, polymorphic
//!   value_to_int   0 concrete   43 dynamic
//!   value_to_text  0 concrete    6 dynamic
//!   tuple_field    0 concrete    4 dynamic
//!   ctor_field     0 concrete    6 dynamic   8 param
//!   value_0/1/2    0 concrete    0 dynamic  10 param
//!
//! 86% of the "concrete" column is print calls, where there is no narrowing to
//! enforce because there is no wrong answer. For the accessor and coercion
//! family — the fns that actually panic — the count of statically concrete
//! arguments is ZERO, across all ~77 call sites.
//!
//! That is not an accident of this codebase. You reach for `value_to_int`
//! precisely when the static type is `Value`; if it were already `Int` the
//! call would be a no-op nobody writes. The coercion's presence is evidence
//! that the type is unknown.
//!
//! CONCLUSION: a per-argument narrowing rule would have caught none of these
//! panics. It is not worth building. The remaining compile-time gaps are
//! elsewhere — `variadic true` switching off arity AND per-argument type
//! checks for 4 fns, and `--no-verify` disabling the gate entirely in five
//! axVerity build scripts.
//!
//! Usage:
//!   cargo run --release --example value_slot_census -- <bundle-dir> <reg>...

use std::collections::HashMap;
use std::path::PathBuf;

use axis_lang::fabric::hash::Hash256;
use axis_lang::ir::core_ir_05::{read_axbi, CoreBundle, Node, NodeRef};
use axis_lang::registry::core05::fold::Scope;

/// Identity -> declared name, scraped from the `.axreg` text. The scope maps
/// names to identities for TYPES but not for fns, and §5b identity is
/// `SHA-256(name)` anyway, so the text is the honest source for reporting.
fn fn_names(regs: &[PathBuf]) -> HashMap<Hash256, String> {
    let mut m = HashMap::new();
    for p in regs {
        let Ok(text) = std::fs::read_to_string(p) else { continue };
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("fn ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                if !name.is_empty() {
                    m.insert(
                        axis_lang::fabric::hash::content_hash(name.as_bytes()),
                        name.to_string(),
                    );
                }
            }
        }
    }
    m
}

/// Per-node result types, in index order. A simplified re-derivation of what
/// the verifier computes: a CCall produces its declared `out`, a CDeterminate
/// produces Unit, a CIf produces its then-arm's type. Good enough to classify
/// an argument's provenance, which is all this census needs.
fn result_types(bundle: &CoreBundle, scope: &Scope) -> Vec<Hash256> {
    let mut types: Vec<Hash256> = Vec::with_capacity(bundle.nodes.len());
    let unit = axis_lang::lowering::emit_05::unit_type_id();
    for node in &bundle.nodes {
        let t = match node {
            Node::CDeterminate | Node::CBlock => unit,
            Node::CEnd { value } => ref_type(*value, bundle, &types),
            Node::CCall { target_identity, .. } => scope
                .resolve_fn(*target_identity)
                .map(|r| r.genesis.output)
                .unwrap_or(Hash256::ZERO),
            Node::CIf { then_, .. } => ref_type(*then_, bundle, &types),
        };
        types.push(t);
    }
    types
}

fn ref_type(r: NodeRef, bundle: &CoreBundle, types: &[Hash256]) -> Hash256 {
    match r {
        NodeRef::Node(j) => types.get(j as usize).copied().unwrap_or(Hash256::ZERO),
        NodeRef::Pool(k) => bundle
            .constant_pool
            .get(k as usize)
            .map(|e| e.def_hash)
            .unwrap_or(Hash256::ZERO),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: value_slot_census <bundle-dir> <reg.axreg>...");
        std::process::exit(2);
    }
    let dir = PathBuf::from(&args[0]);
    let regs: Vec<PathBuf> = args[1..].iter().map(PathBuf::from).collect();

    let scope = axis_lang::cli_util::load_and_merge_registries(&regs)
        .unwrap_or_else(|e| { eprintln!("registry load failed: {e}"); std::process::exit(1) });
    let names = fn_names(&regs);

    let value_id = scope.lookup_name("Value").expect("registry declares no `Value` type");
    let param_id = axis_lang::lowering::emit_05::param_type_id();
    let fn_id = axis_lang::lowering::emit_05::fn_type_id();

    // (concrete, dynamic, param) per callee name.
    let mut per_fn: HashMap<String, (usize, usize, usize)> = HashMap::new();
    let (mut concrete, mut dynamic, mut param) = (0usize, 0usize, 0usize);
    let mut bundles = 0usize;

    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read bundle dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("coreir"))
        .collect();
    paths.sort();

    for path in &paths {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(bundle) = read_axbi(&bytes) else { continue };
        bundles += 1;
        let types = result_types(&bundle, &scope);

        for node in &bundle.nodes {
            let Node::CCall { target_identity, args, .. } = node else { continue };
            let Ok(resolved) = scope.resolve_fn(*target_identity) else { continue };
            if resolved.genesis.variadic { continue; }
            let inputs = &resolved.genesis.inputs;
            let name = names
                .get(target_identity)
                .cloned()
                .unwrap_or_else(|| target_identity.to_hex()[..8].to_string());

            for (pos, &arg) in args.iter().enumerate() {
                // Only slots the registry declares as the `Value` union: those
                // are the ones Step 2b cannot check.
                if inputs.get(pos) != Some(&value_id) { continue; }
                let t = ref_type(arg, &bundle, &types);
                let e = per_fn.entry(name.clone()).or_default();
                if t == param_id {
                    // A parameter of the enclosing composite: its type is
                    // declared elsewhere, so this census cannot see it.
                    param += 1; e.2 += 1;
                } else if t == value_id || t == Hash256::ZERO || t == fn_id {
                    dynamic += 1; e.1 += 1;
                } else {
                    concrete += 1; e.0 += 1;
                }
            }
        }
    }

    let total = concrete + dynamic + param;
    println!("bundles scanned: {bundles}");
    println!("arguments landing in a `Value`-declared slot: {total}");
    if total == 0 { return; }
    println!("  statically CONCRETE (a narrowing rule could check these) : {concrete:5}  ({:.1}%)",
             concrete as f64 / total as f64 * 100.0);
    println!("  statically Value    (irreducibly dynamic)                : {dynamic:5}  ({:.1}%)",
             dynamic as f64 / total as f64 * 100.0);
    println!("  param slot          (declared in the enclosing fn)       : {param:5}  ({:.1}%)",
             param as f64 / total as f64 * 100.0);

    let mut rows: Vec<_> = per_fn.into_iter().collect();
    rows.sort_by_key(|(_, (c, d, p))| std::cmp::Reverse(c + d + p));
    println!("\nby callee (concrete / dynamic / param):");
    for (name, (c, d, p)) in rows.iter().take(25) {
        println!("  {name:28} {c:5} {d:5} {p:5}");
    }
}
