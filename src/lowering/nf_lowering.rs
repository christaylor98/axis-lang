// NF Lowering — Core IR 0.5 path
//
// Supported constructs:
//   UnitLit    → intern_constant (unit type identity, empty payload)
//   IntLit     → intern_constant (int type identity, zigzag+LEB128 payload per codec §10)
//   BoolLit    → intern_constant (bool type identity, [0x01] or [0x00])
//   TextLit    → intern_constant (text type identity, UTF-8 LEB128 payload)
//   Literal    → dispatch on token kind → correct Core IR literal (Int/Bool/Unit/Text)
//   VarRef     → env lookup (param slot or fn-ref-by-name) → pool constant
//   LetExpr    → lower value, extend env, lower body
//   LetDecl    → lower value field (top-level binding → its value's Core IR)
//   AppExpr    → uncurry nested App/VarRef chain → CCall
//   IfExpr     → CIf (fields: condition, then_branch, else_branch)
//   DetermineExpr → CDeterminate gate node
//   CallExpr   → CCall (two-step: CallExpr.args → ArgList → ArgList.args)
//   FnDecl     → param slots interned into pool; body lowered to CCall graph
//   ValueConstructExpr   → CCall on "value_make" (type-erased; tags checked at surface)
//   ValueListConstructExpr → CCall on "list_make" (type-erased; element type checked at surface)
//   Program    → split upstream by run_pipeline; arm present but unreachable in practice
//                (RESIDUAL — to be removed in a future micro-pass once split is enforced)

use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use crate::frontend::token::TokenKind;
use std::collections::HashMap;

// ─── PARAM EXTRACTION ────────────────────────────────────────────────────────

/// Resolve a surface type expression node to its registry type identity.
///
/// The M1 grammar has three type-expression kinds (m1-parse.yaml `TypeExpr`):
/// `PrimTypeExpr`, `ValueListTypeExpr` (`ValueList(T)`) and `ValueTypeExpr`
/// (`Value(T1, ..., Tn)`).
///
/// `PrimTypeExpr` looks up the named primitive in the scope.
///
/// The two COMPOUND kinds erase to their registry constructor type. The
/// element/slot tags are a surface-only concern: the NF validator checks them
/// before lowering (`validate_value_construct_types`) and the IR carries no
/// tag at all — which is exactly why `value_make`/`list_make` are declared
/// type-erased and variadic. A parameter declared `ValueList(Int)` therefore
/// satisfies a registry contract asking for `ValueList`, and `Value(Text, Int)`
/// satisfies one asking for `Value`.
///
/// Before this, every compound type expression fell through to `Hash256::ZERO`,
/// so a compound-typed parameter had NO declared type and every use of it
/// failed the verifier's argument check against a null hash — indistinguishable
/// from an undeclared type name. That made list-taking functions unwritable.
///
/// An unknown name, or a kind not listed above, still yields `Hash256::ZERO`,
/// which the verifier treats as a hard fail rather than an auto-pass.
fn resolve_type_hash(
    type_expr: &SchemaAstNode,
    scope: &crate::registry::core05::fold::Scope,
) -> crate::fabric::hash::Hash256 {
    if type_expr.kind == "PrimTypeExpr" {
        if let Some(SchemaValue::Token(t)) = type_expr.fields.get("name") {
            if let Some(&hash) = scope.names.get(&t.lexeme) {
                return hash;
            }
        }
        return crate::fabric::hash::Hash256::ZERO;
    }

    // Compound tags erase to the registry constructor type of the same name.
    let erased = match type_expr.kind.as_str() {
        "ValueListTypeExpr" => Some("ValueList"),
        "ValueTypeExpr" => Some("Value"),
        _ => None,
    };
    if let Some(name) = erased {
        if let Some(&hash) = scope.names.get(name) {
            return hash;
        }
    }
    crate::fabric::hash::Hash256::ZERO
}

/// Extract typed params from a TypedParamList NF node (for FnDecl).
/// Each param's type identity is resolved against `scope`; unknown type names
/// yield `Hash256::ZERO` (later checked at verify time, not here).
/// Handles `TypedParam`, `TypedParamRest`, and `Empty`.
fn extract_typed_params(
    params_node: &SchemaAstNode,
    scope: &crate::registry::core05::fold::Scope,
) -> Result<Vec<(String, crate::fabric::hash::Hash256)>, crate::lowering::emit_05::SchemaLower05Error> {
    use crate::lowering::emit_05::{schema_field_node, schema_field_token, SchemaLower05Error};

    let type_of = |p: &SchemaAstNode| -> crate::fabric::hash::Hash256 {
        match p.fields.get("type_expr") {
            Some(SchemaValue::Node(te)) => resolve_type_hash(te, scope),
            _ => crate::fabric::hash::Hash256::ZERO,
        }
    };

    match params_node.kind.as_str() {
        "TypedParamList" => {
            let raw = match params_node.fields.get("params") {
                Some(SchemaValue::Nodes(ns)) => ns.as_slice(),
                _ => return Ok(vec![]),
            };
            let mut params = vec![];
            for p in raw {
                match p.kind.as_str() {
                    "TypedParam" => {
                        params.push((schema_field_token(p, "name")?, type_of(p)));
                    }
                    "TypedParamRest" => {
                        let inner = schema_field_node(p, "param")?;
                        params.push((schema_field_token(inner, "name")?, type_of(inner)));
                    }
                    "Empty" => {}
                    other => return Err(SchemaLower05Error::UnsupportedNode(other.to_string())),
                }
            }
            Ok(params)
        }
        "Empty" => Ok(vec![]),
        other => Err(SchemaLower05Error::UnsupportedNode(
            format!("expected TypedParamList, got '{}'", other),
        )),
    }
}

/// Extract lowerable expression nodes from an ArgList NF node.
/// ArgList.args may contain direct expressions and ArgListRest wrappers.
fn extract_arglist_exprs<'a>(
    arglist: &'a SchemaAstNode,
) -> Result<Vec<&'a SchemaAstNode>, crate::lowering::emit_05::SchemaLower05Error> {
    use crate::lowering::emit_05::schema_field_node;

    match arglist.fields.get("args") {
        Some(SchemaValue::Nodes(raw)) => {
            let mut exprs = vec![];
            for node in raw {
                match node.kind.as_str() {
                    "Empty" => {}
                    "ArgListRest" => exprs.push(schema_field_node(node, "arg")?),
                    _ => exprs.push(node),
                }
            }
            Ok(exprs)
        }
        _ => Ok(vec![]),
    }
}

// ─── CURRIED LAMBDA LOWERING ──────────────────────────────────────────────────

/// Lower FnDecl params as param-slot pool entries, then lower the body.
///
/// For N params: interns each param as a pool slot constant, extends env,
/// then lowers the body (which can reference params via env lookups).
/// For 0 params: lowers body directly.
fn lower_curried_lam<'a>(
    params: &[(String, crate::fabric::hash::Hash256)],
    body_node: &'a SchemaAstNode,
    scope: &crate::registry::core05::fold::Scope,
    builder: &mut crate::ir::core_ir_05::Builder<'_>,
    env: &HashMap<String, crate::ir::core_ir_05::NodeRef>,
) -> Result<crate::ir::core_ir_05::NodeRef, crate::lowering::emit_05::SchemaLower05Error> {
    if params.is_empty() {
        return lower_nf_node_05(body_node, scope, builder, env);
    }

    // Parameter slot pool entries — def_hash = Param type, payload = varint of
    // the 0-based slot index. The bridge substitutes these at eject when
    // resolving CCalls to a `kind composite` (see emit_05::param_type_id /
    // intern_param_slot for the encoding rationale). Replaces the prior
    // empty-payload sentinel mechanism that (a) silently aliased same-typed
    // params in the constant pool and (b) left bodies dependent on a runtime
    // substitution layer that didn't exist.
    let mut extended_env = env.clone();
    for (slot_index, (name, _ty)) in params.iter().enumerate() {
        let slot = crate::lowering::emit_05::intern_param_slot(builder, slot_index as u32);
        extended_env.insert(name.clone(), slot);
    }

    lower_nf_node_05(body_node, scope, builder, &extended_env)
}

// ─── BRANCH EFFECT SCOPING ────────────────────────────────────────────────────

/// All node indices reachable from `root` through CCall args / CIf edges,
/// inclusive of `root` itself. Used to tell whether an `if`-arm binding's value
/// is actually consumed by the arm's result.
fn reachable_node_set(
    builder: &crate::ir::core_ir_05::Builder<'_>,
    root: crate::ir::core_ir_05::NodeRef,
) -> std::collections::HashSet<u32> {
    use crate::ir::core_ir_05::{Node, NodeRef};
    let mut seen = std::collections::HashSet::new();
    let mut stack = Vec::new();
    if let NodeRef::Node(i) = root {
        stack.push(i);
    }
    while let Some(i) = stack.pop() {
        if !seen.insert(i) {
            continue;
        }
        match builder.node_at(i) {
            Node::CCall { args, .. } => {
                for a in args {
                    if let NodeRef::Node(j) = a {
                        stack.push(*j);
                    }
                }
            }
            Node::CIf { cond, then_, else_ } => {
                for r in [cond, then_, else_] {
                    if let NodeRef::Node(j) = r {
                        stack.push(*j);
                    }
                }
            }
            // Core IR 0.6: a block's value is an edge from its CEnd; a CBlock has none
            Node::CEnd { value } => {
                if let NodeRef::Node(j) = value {
                    stack.push(*j);
                }
            }
            Node::CDeterminate | Node::CBlock => {}
        }
    }
    seen
}

/// Thread any discarded side-effecting binding created while lowering one `if`
/// arm into that arm's result value, so the emitter's branch-scoping keeps its
/// effect inside the arm instead of hoisting it to unconditional top level.
///
/// A node created in `[start, nodes_len)` that the arm's `result` does not reach
/// is a discarded value — a `let _ = eff(); …` whose effect has no consumer edge
/// to anchor it to the arm. We rewrite the result as `seq(effect, result)` for
/// each such node (`seq(a, b) -> b`), making the effect a genuine data-dependency
/// of the continuation without changing the value the arm yields. A consumed
/// binding needs no wrapper — its use already anchors it to the arm. Nodes are
/// wrapped in ascending index order, preserving source-order evaluation.
///
/// Only *root* orphans are wrapped. A discarded `if` is itself an orphan, and so
/// is every node the lowerer created inside its arms — nothing outside reaches
/// them. Wrapping those interior nodes too would splice them into the outer arm
/// as siblings of the `CIf`, making them unconditional and destroying the inner
/// branch's guard (an effect under `if d` would fire whenever the outer arm ran,
/// regardless of `d`). So we first collect the interior — everything reachable
/// from some other node in `[start, end)` — and wrap only what remains.
///
/// THIS IS A CONTRACT, NOT AN OPTIMISATION. The bridge emitter carries no
/// fallback for an un-threaded arm effect, and cannot: an arm-local orphan and
/// a top-level orphan lower to byte-identical node sequences, so nothing
/// downstream can recover the distinction. Core IR therefore defines a node
/// with no consumer edge as unconditional and top-level
/// (ORPHAN_IS_TOP_LEVEL_V1 — see `CoreBundle`'s docs and `compute_branch_paths`
/// in axis-codegen-bridge-rs `src/emit/rust_05.rs`). The bridge once tried to
/// guess arm membership from node position and refuse the ambiguous-looking
/// case; that check rejected ordinary AI3 programs and was removed. Branch
/// effect scoping now rests entirely here. `branch_scoping_tests` below is what
/// holds it up — do not weaken it.
fn seq_scope_arm_effects(
    builder: &mut crate::ir::core_ir_05::Builder<'_>,
    scope: &crate::registry::core05::fold::Scope,
    start: u32,
    result: crate::ir::core_ir_05::NodeRef,
) -> Result<crate::ir::core_ir_05::NodeRef, crate::lowering::emit_05::SchemaLower05Error> {
    use crate::ir::core_ir_05::{Node, NodeRef};
    use crate::lowering::emit_05::SchemaLower05Error;

    let end = builder.nodes_len();
    if start == end {
        return Ok(result);
    }
    let reachable = reachable_node_set(builder, result);
    // Nodes in [start, end) that some *other* node in the range already reaches.
    // These are branch/call interiors — their parent anchors them, so wrapping
    // them here would hoist them out of that parent's scope.
    let mut interior: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for i in start..end {
        let mut r = reachable_node_set(builder, NodeRef::Node(i));
        r.remove(&i);
        interior.extend(r.into_iter().filter(|j| (start..end).contains(j)));
    }
    // A CBlock is a place, not a value: nothing reaches it, and it is never threaded (Core IR 0.6).
    let orphans: Vec<u32> = (start..end)
        .filter(|i| !reachable.contains(i) && !interior.contains(i))
        .filter(|i| !matches!(builder.node_at(*i), Node::CBlock))
        .collect();
    if orphans.is_empty() {
        return Ok(result);
    }
    let seq_id = scope
        .lookup_name("seq")
        .ok_or_else(|| SchemaLower05Error::UnknownCallable("seq".to_string()))?;
    let mut current = result;
    for i in orphans {
        current = builder.push_node(Node::CCall {
            target_name: "seq".to_string(),
            target_identity: seq_id,
            args: vec![NodeRef::Node(i), current],
        })?;
    }
    Ok(current)
}

// ─── STRING LITERAL ESCAPES ───────────────────────────────────────────────────

/// Decode the string-literal escape sequences the M1 lexer declares
/// (`m1-lexer.yaml`: `\"`, `\\`, `\n`, `\t`) into their byte values. The lexer
/// preserves escapes verbatim in the token lexeme, so a Text literal's content
/// still carries the raw two-char `\n`; decoding happens here, at the one place a
/// Text literal becomes bytes, right before `encode_text`. An unknown escape
/// (e.g. `\z`) is a hard error rather than a silent passthrough — fail closed.
fn decode_text_escapes(
    content: &str,
) -> Result<String, crate::lowering::emit_05::SchemaLower05Error> {
    use crate::lowering::emit_05::SchemaLower05Error;
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                return Err(SchemaLower05Error::MissingField {
                    node_kind: "TextLit".into(),
                    field: format!("unknown string escape '\\{other}'"),
                });
            }
            None => {
                return Err(SchemaLower05Error::MissingField {
                    node_kind: "TextLit".into(),
                    field: "trailing backslash in string literal".into(),
                });
            }
        }
    }
    Ok(out)
}

// ─── MAIN LOWERING DISPATCH ───────────────────────────────────────────────────

fn lower_nf_node_05(
    node: &SchemaAstNode,
    scope: &crate::registry::core05::fold::Scope,
    builder: &mut crate::ir::core_ir_05::Builder<'_>,
    env: &HashMap<String, crate::ir::core_ir_05::NodeRef>,
) -> Result<crate::ir::core_ir_05::NodeRef, crate::lowering::emit_05::SchemaLower05Error> {
    use crate::ir::core_ir_05::Node;
    use crate::lowering::emit_05::{
        intern_unit, schema_field_node, schema_field_nodes, schema_field_token,
        schema_field_full_token, SchemaLower05Error,
    };
    use crate::registry::core05::{
        codec::type_identity,
        record::{PrimCode, TypeDef, TypeShape},
    };

    match node.kind.as_str() {
        "UnitLit" => Ok(intern_unit(builder)),

        "IntLit" => {
            let lexeme = schema_field_token(node, "value")?;
            let val: i64 = lexeme.parse().map_err(|_| SchemaLower05Error::MissingField {
                node_kind: "IntLit".into(),
                field: format!("value '{}' is not a valid i64", lexeme),
            })?;
            let def_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Int) });
            let mut payload = Vec::new();
            crate::fabric::codec::encode_int(&mut payload, val);
            Ok(builder.intern_constant(def_hash, payload))
        }

        "BoolLit" => {
            let lexeme = schema_field_token(node, "value")?;
            // Fail closed: `lexeme == "true"` alone read every other spelling —
            // `True`, `TRUE`, a typo — as false. NF spells booleans `true` /
            // `false`; a surface with another spelling canonicalises it first.
            let val = match lexeme.as_str() {
                "true" => true,
                "false" => false,
                other => {
                    return Err(SchemaLower05Error::MissingField {
                        node_kind: "BoolLit".into(),
                        field: format!("value '{}' is not a boolean (NF spells them `true` / `false`)", other),
                    })
                }
            };
            let def_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) });
            Ok(builder.intern_constant(def_hash, vec![if val { 1u8 } else { 0u8 }]))
        }

        "TextLit" => {
            let raw = schema_field_token(node, "value")?;
            // Lexeme includes surrounding quotes (e.g. `"hello"`); strip them.
            let content = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                &raw[1..raw.len() - 1]
            } else {
                raw.as_str()
            };
            let decoded = decode_text_escapes(content)?;
            let def_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Text) });
            let mut payload = Vec::new();
            crate::fabric::codec::encode_text(&mut payload, &decoded);
            Ok(builder.intern_constant(def_hash, payload))
        }

        // Literal{value:Token(kind,...)} — dispatch on the token kind to the correct literal.
        "Literal" => {
            let token = schema_field_full_token(node, "value")?;
            match &token.kind {
                TokenKind::IntLit => {
                    let val: i64 = token.lexeme.parse().map_err(|_| {
                        SchemaLower05Error::MissingField {
                            node_kind: "Literal".into(),
                            field: format!(
                                "INT_LIT value '{}' is not a valid i64",
                                token.lexeme
                            ),
                        }
                    })?;
                    let def_hash =
                        type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Int) });
                    let mut payload = Vec::new();
                    crate::fabric::codec::encode_int(&mut payload, val);
                    Ok(builder.intern_constant(def_hash, payload))
                }
                TokenKind::StringLit => {
                    // Lexeme includes surrounding quotes (e.g. `"hello"`); strip them.
                    let raw = &token.lexeme;
                    let content = if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
                        &raw[1..raw.len() - 1]
                    } else {
                        raw.as_str()
                    };
                    let decoded = decode_text_escapes(content)?;
                    let def_hash =
                        type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Text) });
                    let mut payload = Vec::new();
                    crate::fabric::codec::encode_text(&mut payload, &decoded);
                    Ok(builder.intern_constant(def_hash, payload))
                }
                TokenKind::Keyword(kw) if kw == "true" => {
                    let def_hash =
                        type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) });
                    Ok(builder.intern_constant(def_hash, vec![0x01]))
                }
                TokenKind::Keyword(kw) if kw == "false" => {
                    let def_hash =
                        type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) });
                    Ok(builder.intern_constant(def_hash, vec![0x00]))
                }
                TokenKind::Keyword(kw) if kw == "unit" => Ok(intern_unit(builder)),
                other => Err(SchemaLower05Error::UnsupportedNode(format!(
                    "Literal with unsupported token kind {:?}",
                    other
                ))),
            }
        }

        "VarRef" => {
            let var_name = schema_field_token(node, "name")?;
            if let Some(&nr) = env.get(&var_name) {
                return Ok(nr);
            }
            // fn-ref-by-name: name resolved in scope (includes user-defined fns added by
            // augment_scope_with_user_fns before lowering) → Fn-typed pool constant.
            if let Some(fn_identity) = scope.lookup_name(&var_name) {
                return Ok(crate::lowering::emit_05::intern_fn_ref(builder, fn_identity));
            }
            Err(SchemaLower05Error::UnknownCallable(var_name))
        }

        "LetExpr" => {
            let var_name = schema_field_token(node, "name")?;
            let val_node = schema_field_node(node, "value")?;
            let body_node = schema_field_node(node, "body")?;
            let val_ref = lower_nf_node_05(val_node, scope, builder, env)?;
            let mut inner_env = env.clone();
            inner_env.insert(var_name, val_ref);
            lower_nf_node_05(body_node, scope, builder, &inner_env)
        }

        // Top-level let binding: lower to the value's Core IR.
        "LetDecl" => lower_nf_node_05(schema_field_node(node, "value")?, scope, builder, env),

        "AppExpr" => {
            let (fn_name, identity, args) = collect_call(node, scope, builder, env)?;
            Ok(builder.push_node(Node::CCall { target_name: fn_name, target_identity: identity, args })?)
        }

        "Expr" => lower_nf_node_05(schema_field_node(node, "inner")?, scope, builder, env),

        // Program iterates its items — each FnDecl/LetDecl lowered independently.
        // Returns the NodeRef of the last item (multi-item programs produce n CLam nodes).
        "Program" => {
            let items = schema_field_nodes(node, "items")?;
            if items.is_empty() {
                return Err(SchemaLower05Error::UnsupportedNode("Program with no items".into()));
            }
            let mut last_ref = None;
            for item in &items {
                last_ref = Some(lower_nf_node_05(item, scope, builder, env)?);
            }
            Ok(last_ref.expect("items non-empty checked above"))
        }

        "IfExpr" => {
            // A branch is a Body (LetBinding* Expr), so it may contain discarded
            // side-effecting bindings — `if c { let _ = eff(); tail } else { … }`.
            // Core IR 0.5 is a flat data-flow graph and the emitter scopes a node
            // to an `if` arm by whether the arm's result depends on it. A discarded
            // effect has no such data edge, so without help it would be hoisted to
            // unconditional top level and fire in BOTH branches (the BRANCH_SCOPING
            // leak). We fix it at the source: after lowering each arm, thread any
            // node the arm created but its result does not consume into the result
            // via `seq(effect, result) -> result`, making the effect an honest
            // data-dependency of the continuation so the emitter attributes it to
            // exactly that arm. `condition` is a bare Expr (grammar), never a Body,
            // so it needs no such treatment.
            //
            // Core IR 0.6: each arm is a block, CBlock ... CEnd{value}, written where the arm is, and the CIf
            // that follows them names their CEnds. Where a node runs is then in the IR (it runs when every
            // block around it runs), not inferred by a backend; closing a block also ends sharing with it.
            let cond_ref = lower_nf_node_05(schema_field_node(node, "condition")?, scope, builder, env)?;
            builder.push_node(Node::CBlock)?;
            let then_start = builder.nodes_len();
            let then_ref = lower_nf_node_05(schema_field_node(node, "then_branch")?, scope, builder, env)?;
            let then_ref = seq_scope_arm_effects(builder, scope, then_start, then_ref)?;
            let then_end = builder.push_node(Node::CEnd { value: then_ref })?;
            builder.push_node(Node::CBlock)?;
            let else_start = builder.nodes_len();
            let else_ref = lower_nf_node_05(schema_field_node(node, "else_branch")?, scope, builder, env)?;
            let else_ref = seq_scope_arm_effects(builder, scope, else_start, else_ref)?;
            let else_end = builder.push_node(Node::CEnd { value: else_ref })?;
            Ok(builder.push_node(Node::CIf { cond: cond_ref, then_: then_end, else_: else_end })?)
        }

        // Determinacy discharge gate.
        "DetermineExpr" => Ok(builder.push_node(Node::CDeterminate)?),

        // CallExpr: two-step ArgList extraction.
        // CallExpr.args is Node(ArgList); ArgList.args is Nodes([exprs...]).
        "CallExpr" => {
            let name = schema_field_token(node, "name")?;
            let target_identity = scope
                .lookup_name(&name)
                .ok_or_else(|| SchemaLower05Error::UnknownCallable(name.clone()))?;
            let arglist = schema_field_node(node, "args")?;
            let arg_exprs = extract_arglist_exprs(arglist)?;
            // BOOL_SHORT_CIRCUIT_V1: `bool_and(a, b)` / `bool_or(a, b)` evaluate `b` only when it decides
            // the result, as `&&` / `||` do. A CCall evaluates every argument, so `bool_and(int_lt(i, n),
            // int_eq(str_char_code(s, i), Int(32)))` read past the end of `s` exactly when the guard was
            // false. Lowered as `CIf(a, b, false)` / `CIf(a, true, b)`: `b` becomes an arm, which the
            // emitter scopes to that arm, and its discarded effects are threaded into it like any arm's.
            // The verifier still checks both: `a` as the CIf condition (Bool), `b` against the Bool arm.
            // A fn-ref to bool_and (a callback) is not a CallExpr and keeps its eager meaning.
            if (name == "bool_and" || name == "bool_or") && arg_exprs.len() == 2 {
                // Core IR 0.6: both arms are blocks; the one with no source is CBlock, CEnd{false|true}.
                let cond_ref = lower_nf_node_05(arg_exprs[0], scope, builder, env)?;
                let bool_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) });
                let short = builder.intern_constant(bool_hash, vec![if name == "bool_or" { 1u8 } else { 0u8 }]);
                let short_arm = |builder: &mut crate::ir::core_ir_05::Builder<'_>| -> Result<crate::ir::core_ir_05::NodeRef, crate::lowering::emit_05::SchemaLower05Error> {
                    builder.push_node(Node::CBlock)?;
                    Ok(builder.push_node(Node::CEnd { value: short })?)
                };
                let or = name == "bool_or";
                let short_end = if or { Some(short_arm(builder)?) } else { None };
                builder.push_node(Node::CBlock)?;
                let rhs_start = builder.nodes_len();
                let rhs_ref = lower_nf_node_05(arg_exprs[1], scope, builder, env)?;
                let rhs_ref = seq_scope_arm_effects(builder, scope, rhs_start, rhs_ref)?;
                let rhs_end = builder.push_node(Node::CEnd { value: rhs_ref })?;
                let (then_, else_) = match short_end {
                    Some(e) => (e, rhs_end),
                    None => (rhs_end, short_arm(builder)?),
                };
                let _ = target_identity; // the registry still has to declare the name (UnknownCallable above)
                return Ok(builder.push_node(Node::CIf { cond: cond_ref, then_, else_ })?);
            }
            let mut args = Vec::with_capacity(arg_exprs.len());
            for arg in arg_exprs {
                args.push(lower_nf_node_05(arg, scope, builder, env)?);
            }
            Ok(builder.push_node(Node::CCall { target_name: name.clone(), target_identity, args })?)
        }

        // FnDecl → param slots + body lowered.
        // Typed params extracted from TypedParamList; body lowered.
        "FnDecl" => {
            let params_node = schema_field_node(node, "params")?;
            let params = extract_typed_params(params_node, scope)?;
            let body_node = schema_field_node(node, "body")?;
            lower_curried_lam(&params, body_node, scope, builder, env)
        }

        // ValueConstructExpr → CCall on "value_make" registry constructor.
        // Type-erased: tags were checked at the surface; IR carries no type info.
        // Nesting is preserved — each element is lowered as an independent subgraph
        // so the bridge can recover element boundaries from the arg tree.
        "ValueConstructExpr" => {
            let target_identity = scope
                .lookup_name("value_make")
                .ok_or_else(|| SchemaLower05Error::UnknownCallable("value_make".into()))?;
            let arglist = schema_field_node(node, "args")?;
            let arg_exprs = extract_arglist_exprs(arglist)?;
            let mut args = Vec::with_capacity(arg_exprs.len());
            for arg in arg_exprs {
                args.push(lower_nf_node_05(arg, scope, builder, env)?);
            }
            Ok(builder.push_node(Node::CCall { target_name: "value_make".to_string(), target_identity, args })?)
        }

        // ValueListConstructExpr → CCall on "list_make" registry constructor.
        // Same design as ValueConstructExpr: type-erased, nesting preserved.
        "ValueListConstructExpr" => {
            let target_identity = scope
                .lookup_name("list_make")
                .ok_or_else(|| SchemaLower05Error::UnknownCallable("list_make".into()))?;
            let arglist = schema_field_node(node, "args")?;
            let arg_exprs = extract_arglist_exprs(arglist)?;
            let mut args = Vec::with_capacity(arg_exprs.len());
            for arg in arg_exprs {
                args.push(lower_nf_node_05(arg, scope, builder, env)?);
            }
            Ok(builder.push_node(Node::CCall { target_name: "list_make".to_string(), target_identity, args })?)
        }

        other => Err(SchemaLower05Error::UnsupportedNode(other.to_string())),
    }
}

/// Uncurry a nested AppExpr chain into a function identity and ordered arg list.
///
/// `app(fn = app(fn = var(f), arg = a), arg = b)` → `(f_id, [a, b])`
///
/// Only `VarRef` is valid at the head (the innermost `fn` position).
/// Any other head kind → `UnsupportedNode`.
fn collect_call(
    node: &SchemaAstNode,
    scope: &crate::registry::core05::fold::Scope,
    builder: &mut crate::ir::core_ir_05::Builder<'_>,
    env: &HashMap<String, crate::ir::core_ir_05::NodeRef>,
) -> Result<
    (String, crate::fabric::hash::Hash256, Vec<crate::ir::core_ir_05::NodeRef>),
    crate::lowering::emit_05::SchemaLower05Error,
> {
    use crate::lowering::emit_05::{schema_field_node, schema_field_token, SchemaLower05Error};

    match node.kind.as_str() {
        "AppExpr" => {
            let fn_node = schema_field_node(node, "fn")?;
            let arg_node = schema_field_node(node, "arg")?;
            let (fn_name, identity, mut args) = collect_call(fn_node, scope, builder, env)?;
            let arg_ref = lower_nf_node_05(arg_node, scope, builder, env)?;
            args.push(arg_ref);
            Ok((fn_name, identity, args))
        }
        "VarRef" => {
            let fn_name = schema_field_token(node, "name")?;
            let identity = scope
                .lookup_name(&fn_name)
                .ok_or_else(|| SchemaLower05Error::UnknownCallable(fn_name.clone()))?;
            Ok((fn_name, identity, vec![]))
        }
        other => Err(SchemaLower05Error::UnsupportedNode(format!(
            "AppExpr fn must be VarRef or AppExpr, got '{}'",
            other
        ))),
    }
}

/// Lower a NF `SchemaAstNode` to a Core IR 0.5 `CoreBundle`.
///
/// Supported: `UnitLit`, `IntLit`, `BoolLit`, `TextLit`, `Literal`, `VarRef`, `LetExpr`,
/// `LetDecl`, `AppExpr`, `IfExpr`, `DetermineExpr`, `CallExpr`, `FnDecl`,
/// `ValueConstructExpr`, `ValueListConstructExpr`, `Program` (iterates items), `Expr` wrapper.
/// Any other node kind → `SchemaLower05Error::UnsupportedNode` (STOP-and-report).
pub fn lower_nf_to_bundle_05(
    node: &SchemaAstNode,
    scope: &crate::registry::core05::fold::Scope,
) -> Result<crate::ir::core_ir_05::CoreBundle, crate::lowering::emit_05::SchemaLower05Error> {
    let mut builder = crate::ir::core_ir_05::Builder::new().with_scope(scope);
    let env = HashMap::new();
    let result = lower_nf_node_05(node, scope, &mut builder, &env)?;
    Ok(builder.finish(result)?)
}

/// Lower multiple NF items into one shared bundle.
///
/// Each item is lowered through the same builder so all functions in a
/// multi-fn program share one constant pool.
pub fn lower_nf_items_to_bundle_05(
    items: &[SchemaAstNode],
    scope: &crate::registry::core05::fold::Scope,
) -> Result<crate::ir::core_ir_05::CoreBundle, crate::lowering::emit_05::SchemaLower05Error> {
    let mut builder = crate::ir::core_ir_05::Builder::new().with_scope(scope);
    let env = HashMap::new();
    let mut last = None;
    for item in items {
        last = Some(lower_nf_node_05(item, scope, &mut builder, &env)?);
    }
    let result = last.ok_or_else(|| {
        crate::lowering::emit_05::SchemaLower05Error::UnsupportedNode("empty item list".into())
    })?;
    Ok(builder.finish(result)?)
}

/// Augment a registry scope with user-defined function signatures extracted
/// from NF `FnDecl` items. Each function is registered as `Leaf` / `Pure` using
/// the §5b identity rule (SHA-256 of the function name in UTF-8). Type hashes
/// for params and return type are resolved against the existing scope;
/// unresolvable type names default to `Hash256::ZERO`.
///
/// This enables cross-function calls within the same file: when `fn A` calls
/// `fn B`, B's identity and signature are in scope before A is lowered or
/// verified.
///
/// Fails with [`SchemaLower05Error::DuplicateFn`] when a name is declared twice
/// in the file. Identity is `SHA-256(name)`, so a repeated name is a repeated
/// identity — silently keeping the first record would bind call sites to a
/// signature the programmer did not write.
///
/// A name the merged registry already holds is NOT automatically a collision.
/// A composite entry's identity is `SHA-256(name)` too, so a file declaring a
/// blessed composite lands on the registry's own identity: that is the body
/// being recompiled, the registry's records stay authoritative, and only the
/// signature is cross-checked. A registry LEAF of the same name is a real
/// collision — this file is not its body — and fails with
/// [`SchemaLower05Error::RegistryFnCollision`], as does a composite whose
/// declared signature disagrees with the one written here.
pub fn augment_scope_with_user_fns(
    base: &crate::registry::core05::fold::Scope,
    items: &[SchemaAstNode],
) -> Result<crate::registry::core05::fold::Scope, crate::lowering::emit_05::SchemaLower05Error> {
    use crate::lowering::emit_05::SchemaLower05Error;
    use crate::registry::core05::record::{
        Disposition, EffectClass, FnGenesis, FnKind, LeafFacts, Record,
    };
    use sha2::Digest;
    let mut scope = base.clone();
    // Names declared by THIS file. The record-level conflict check below cannot
    // catch two identical signatures with different bodies — `FnGenesis` and
    // `LeafFacts` carry no body, so both records compare equal and `insert`
    // returns `Ok`. A name declared twice in one file is a duplicate
    // declaration regardless of whether the signatures agree.
    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in items {
        if item.kind != "FnDecl" {
            continue;
        }
        let name = match item.fields.get("name") {
            Some(SchemaValue::Token(t)) => t.lexeme.clone(),
            _ => continue,
        };
        if !declared.insert(name.clone()) {
            return Err(SchemaLower05Error::DuplicateFn(name));
        }
        let identity = {
            let mut h = sha2::Sha256::new();
            h.update(name.as_bytes());
            crate::fabric::hash::Hash256::from_bytes(h.finalize().into())
        };
        let inputs = match item.fields.get("params") {
            Some(SchemaValue::Node(n)) => extract_typed_params(n, &scope)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, ty)| ty)
                .collect(),
            _ => vec![],
        };
        let output = match item.fields.get("return_type") {
            Some(SchemaValue::Node(ret)) => resolve_type_hash(ret, &scope),
            _ => crate::fabric::hash::Hash256::ZERO,
        };
        // THIS FILE IS THE BODY OF THE ENTRY THAT ALREADY CARRIES THIS NAME.
        //
        // A registry entry's identity IS `SHA-256(name)`, the same rule used
        // just above, so a file declaring a function the registry already holds
        // lands on the registry's own identity. That is not a collision — it is
        // the body being compiled, which is how both supported patterns work:
        //
        //   * a blessed COMPOSITE being rebuilt (`body 0x<hash>` in the entry);
        //   * a §5b provider declared as a LEAF, which is what it is from the
        //     caller's side — the caller links an extern. axVerity's graphcore
        //     declares 113 of these and zero composites; bench/bridge_crossing
        //     declares 5.
        //
        // The synthesized records below say `FnKind::Leaf` / `FullIo` /
        // `deterministic`, and `Scope::insert` rejects any record that differs
        // from one already held under the same identity. So inserting them over
        // an existing entry reported a duplicate name for a program that had no
        // duplicate, and whether it fired at all depended on whether those
        // constants happened to match the registry's — `gc_frame` is declared
        // `idempotent true`, the synthesized value is `false`, so it did.
        //
        // The registry's records are the authority here and are left in place;
        // this file supplies the body, not the declaration. Only the signature
        // is cross-checked, so a body that no longer matches the contract it
        // was declared under is still caught, and reported as a contract
        // mismatch rather than as a duplicate name.
        //
        // WHAT THIS GIVES UP, deliberately: a file that names a function after
        // an existing bridge built-in (`fn str_len`) is now taken to be that
        // built-in's body rather than refused. That case was only ever caught
        // incidentally — `Scope::insert` errors only when records DIFFER, so a
        // built-in whose facts matched the synthesized ones was already
        // accepted silently. Telling a provider leaf apart from a built-in
        // needs provenance (which registry file a record came from) that the
        // merged `Scope` does not carry.
        if let Some(existing) = base.fns.get(&identity) {
            if existing.inputs.len() != inputs.len() {
                return Err(SchemaLower05Error::RegistryFnCollision {
                    name: name.clone(),
                    detail: format!(
                        "the registry declares {} parameter(s), this file declares {}",
                        existing.inputs.len(),
                        inputs.len()
                    ),
                });
            }
            // ZERO is `resolve_type_hash`'s "not in scope" answer, not a type.
            // Comparing it would reject a file whose type names this scope
            // happens not to carry, so only resolved hashes are held against
            // the registry.
            let zero = crate::fabric::hash::Hash256::ZERO;
            for (i, (want, got)) in existing.inputs.iter().zip(inputs.iter()).enumerate() {
                if *got != zero && got != want {
                    return Err(SchemaLower05Error::RegistryFnCollision {
                        name: name.clone(),
                        detail: format!(
                            "parameter {} has type {} here but {} in the registry",
                            i,
                            got.to_hex(),
                            want.to_hex()
                        ),
                    });
                }
            }
            if output != zero && output != existing.output {
                return Err(SchemaLower05Error::RegistryFnCollision {
                    name: name.clone(),
                    detail: format!(
                        "returns {} here but {} in the registry",
                        output.to_hex(),
                        existing.output.to_hex()
                    ),
                });
            }
            // Same identity, so this is a no-op that documents the binding.
            scope
                .bind_name(name.clone(), identity)
                .map_err(|_| SchemaLower05Error::DuplicateFn(name.clone()))?;
            continue;
        }

        // `bind_name`, not `names.insert`: the projection name table enforces
        // "names within a single scope MUST be unique", and inserting directly
        // bypasses the one function written to enforce it.
        scope
            .bind_name(name.clone(), identity)
            .map_err(|_| SchemaLower05Error::RegistryFnCollision {
                name: name.clone(),
                detail: "the name is already bound to a different identity".to_string(),
            })?;
        // Both inserts are propagated. `Scope::insert` returns `Err` ONLY when
        // an existing record under this identity DIFFERS — an identical
        // re-insert is `Ok` — so there is no benign duplicate to swallow here.
        // Reaching one means the registry holds a LEAF of this name: a bridge
        // built-in or other admitted leaf, which this file is not the body of.
        scope
            .insert(Record::FnGenesis(FnGenesis {
                identity,
                inputs,
                output,
                kind: FnKind::Leaf,
                disposition: Disposition::Unspecified,
                variadic: false,
            }))
            .map_err(|_| SchemaLower05Error::RegistryFnCollision {
                name: name.clone(),
                detail: "the registry declares a leaf of this name with a different \
                         signature; rename the function in this file"
                    .to_string(),
            })?;
        scope
            .insert(Record::LeafFacts(LeafFacts {
                identity,
                effect_class: EffectClass::FullIo,
                deterministic: true,
                idempotent: false,
            }))
            .map_err(|_| SchemaLower05Error::RegistryFnCollision {
                name: name.clone(),
                detail: "the registry declares a leaf of this name with different effect \
                         facts; rename the function in this file"
                    .to_string(),
            })?;
    }
    Ok(scope)
}

/// Build the param-type CANDIDATE map for all FnDecl items sharing a bundle.
///
/// Returns `Vec<Vec<Hash256>>` where entry `i` lists every declared type that
/// any function in the file gives to param slot `i`.
///
/// WHY CANDIDATES AND NOT ONE TYPE PER SLOT
/// ----------------------------------------
/// `intern_param_slot` keys a pool entry on (Param type, varint(slot_index)),
/// and the constant pool de-duplicates. So every function in a file SHARES one
/// pool entry per slot index — `fn a(x: Int)` and `fn b(xs: ValueList(Int))`
/// both reference `pool[0]`. Verified by decoding a two-fn bundle: both fns'
/// CCall nodes take `args = [(pool = 0), ...]`.
///
/// This function previously returned one `Hash256` per slot, taking the first
/// non-ZERO and commenting "all fns in a correct program agree — they share the
/// same pool entry by construction". The sharing is real; the agreement is not.
/// Two functions with differently-typed first parameters is ordinary, correct
/// code — and it is exactly the shape every list task needs (a scalar predicate
/// helper alongside a list-taking function). Collapsing to one type made one of
/// the two fail.
///
/// The bundle records no function boundaries, so a slot cannot be attributed to
/// a single function without reconstructing them from the DAG. Carrying the
/// candidate set instead keeps the check sound in the direction that matters:
/// a type NO function declares at that position is still rejected.
///
/// KNOWN WEAKENING, stated plainly: within one file, an argument that is a
/// param slot is admitted if it matches ANY function's declared type at that
/// position, so passing `a`'s `Int` param where `ValueList` is wanted is not
/// caught when a sibling function declares `ValueList` at the same index. That
/// is a false-accept across functions in one file. It is never a false-reject,
/// and it replaces a scheme that rejected every compound-typed parameter
/// outright. Closing it properly needs per-function slot identity in the IR,
/// which changes emitted bytes and bundle identities.
pub fn collect_bundle_param_types(
    items: &[SchemaAstNode],
    scope: &crate::registry::core05::fold::Scope,
) -> Vec<Vec<crate::fabric::hash::Hash256>> {
    let mut result: Vec<Vec<crate::fabric::hash::Hash256>> = Vec::new();
    for item in items {
        if item.kind != "FnDecl" {
            continue;
        }
        let params_node = match item.fields.get("params") {
            Some(SchemaValue::Node(n)) => n.as_ref(),
            _ => continue,
        };
        let typed_params = match extract_typed_params(params_node, scope) {
            Ok(p) => p,
            Err(_) => continue,
        };
        for (slot_index, (_name, ty)) in typed_params.iter().enumerate() {
            if result.len() <= slot_index {
                result.resize(slot_index + 1, Vec::new());
            }
            // ZERO means "unresolvable at normalize time" and is not a usable
            // candidate; keep it only if the slot has no real candidate, so the
            // verifier still fails closed rather than seeing an empty set.
            if *ty != crate::fabric::hash::Hash256::ZERO && !result[slot_index].contains(ty) {
                result[slot_index].push(*ty);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests_05 {
    use super::lower_nf_to_bundle_05;
    use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
    use crate::frontend::token::{Span, Token, TokenKind};
    use crate::ir::core_ir_05::{bundle_identity, Node, NodeRef};
    use crate::lowering::emit_05::SchemaLower05Error;
    use std::collections::HashMap;

    const PILOT_AXREG: &str = include_str!("../../registries/pilot_lowering.axreg");

    fn pilot_scope() -> crate::registry::core05::fold::Scope {
        crate::registry::core05::text::lower(PILOT_AXREG)
            .expect("pilot registry must load cleanly")
            .scope
    }

    fn span() -> Span { Span::new(0, 4) }

    fn unit_lit() -> SchemaAstNode {
        SchemaAstNode {
            kind: "UnitLit".to_string(),
            fields: HashMap::new(),
            annotations: Default::default(),
            span: span(),
        }
    }

    fn if_expr(cond: SchemaAstNode, then_: SchemaAstNode, else_: SchemaAstNode) -> SchemaAstNode {
        let mut fields = HashMap::new();
        fields.insert("condition".to_string(), SchemaValue::Node(Box::new(cond)));
        fields.insert("then_branch".to_string(), SchemaValue::Node(Box::new(then_)));
        fields.insert("else_branch".to_string(), SchemaValue::Node(Box::new(else_)));
        SchemaAstNode {
            kind: "IfExpr".to_string(),
            fields,
            annotations: Default::default(),
            span: span(),
        }
    }

    fn call_expr(name: &str, args: Vec<SchemaAstNode>) -> SchemaAstNode {
        let tok = Token { kind: TokenKind::Ident, lexeme: name.to_string(), span: span() };
        // Wrap args in ArgList node, matching NF pipeline form (CallExpr.args = Node(ArgList)).
        let arglist = SchemaAstNode {
            kind: "ArgList".to_string(),
            fields: {
                let mut f = HashMap::new();
                f.insert("args".to_string(), SchemaValue::Nodes(args));
                f
            },
            annotations: Default::default(),
            span: span(),
        };
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), SchemaValue::Token(tok));
        fields.insert("args".to_string(), SchemaValue::Node(Box::new(arglist)));
        SchemaAstNode {
            kind: "CallExpr".to_string(),
            fields,
            annotations: Default::default(),
            span: span(),
        }
    }

    // ── 1. UnitLit → pool entry, no nodes ────────────────────────────────────

    #[test]
    fn unit_lit_produces_pool_entry() {
        let scope = pilot_scope();
        let bundle = lower_nf_to_bundle_05(&unit_lit(), &scope).unwrap();
        assert_eq!(bundle.constant_pool.len(), 1);
        assert_eq!(bundle.nodes.len(), 0);
        assert!(bundle.constant_pool[0].payload.is_empty());
    }

    // ── 2. CallExpr → CCall by identity ──────────────────────────────────────

    #[test]
    fn call_expr_produces_ccall_by_identity() {
        let scope = pilot_scope();
        let const_unit_id = scope.lookup_name("const_unit").unwrap();
        let node = call_expr("const_unit", vec![unit_lit()]);
        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        assert_eq!(bundle.nodes.len(), 1);
        match &bundle.nodes[0] {
            Node::CCall { target_identity, args, .. } => {
                assert_eq!(*target_identity, const_unit_id);
                assert_eq!(args, &[NodeRef::Pool(0)]);
            }
            other => panic!("expected CCall, got {:?}", other),
        }
    }

    // ── 3. IfExpr → CIf ──────────────────────────────────────────────────────

    #[test]
    fn if_expr_produces_cif() {
        let scope = pilot_scope();
        let node = if_expr(unit_lit(), unit_lit(), unit_lit());
        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        assert_eq!(bundle.constant_pool.len(), 1, "shared unit constant");
        // Core IR 0.6: [ CEnd unit ] [ CEnd unit ] CIf
        assert_eq!(bundle.nodes.len(), 5, "two arm blocks and one CIf");
        assert!(matches!(bundle.nodes[4], Node::CIf { .. }));
    }

    // ── 4. Literal deduplication ─────────────────────────────────────────────

    #[test]
    fn unit_literals_deduplicated() {
        let scope = pilot_scope();
        let node = call_expr("seq_units", vec![unit_lit(), unit_lit()]);
        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        assert_eq!(bundle.constant_pool.len(), 1, "both units share one pool entry");
        match &bundle.nodes[0] {
            Node::CCall { args, .. } => {
                assert_eq!(args[0], NodeRef::Pool(0));
                assert_eq!(args[1], NodeRef::Pool(0));
            }
            other => panic!("expected CCall, got {:?}", other),
        }
    }

    // ── 5. UNKNOWN callable → hard halt ──────────────────────────────────────

    #[test]
    fn unknown_callable_halts() {
        let scope = pilot_scope();
        let node = call_expr("nonexistent", vec![]);
        let err = lower_nf_to_bundle_05(&node, &scope).unwrap_err();
        assert!(
            matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "nonexistent"),
            "expected UnknownCallable, got {:?}", err
        );
    }

    // ── 6. STOP on unsupported node kind ─────────────────────────────────────

    #[test]
    fn unsupported_node_stops() {
        let scope = pilot_scope();
        let node = SchemaAstNode {
            kind: "ZZZUnknownWidget".to_string(),
            fields: HashMap::new(),
            annotations: Default::default(),
            span: span(),
        };
        let err = lower_nf_to_bundle_05(&node, &scope).unwrap_err();
        assert!(matches!(err, SchemaLower05Error::UnsupportedNode(ref k) if k == "ZZZUnknownWidget"));
    }

    // ── 7. Identity stability ─────────────────────────────────────────────────

    #[test]
    fn bundle_identity_stable() {
        let scope = pilot_scope();
        let mk = || lower_nf_to_bundle_05(&unit_lit(), &scope).unwrap();
        assert_eq!(bundle_identity(&mk()), bundle_identity(&mk()));
    }

    // ── String-literal escape decoding (M1_TEXT_ESCAPE_DECODE_V1) ─────────────

    #[test]
    fn text_escapes_decode_to_bytes() {
        // The four escapes m1-lexer.yaml declares each decode to one byte.
        assert_eq!(super::decode_text_escapes("a\\nb").unwrap(), "a\nb");
        assert_eq!(super::decode_text_escapes("a\\tb").unwrap(), "a\tb");
        assert_eq!(super::decode_text_escapes("a\\\\b").unwrap(), "a\\b");
        assert_eq!(super::decode_text_escapes("a\\\"b").unwrap(), "a\"b");
        // The decoded newline is a single 0x0A, not the two bytes `\` + `n`.
        let nl = super::decode_text_escapes("a\\nb").unwrap();
        assert_eq!(nl.as_bytes(), &[b'a', 0x0A, b'b']);
    }

    #[test]
    fn text_without_escapes_is_unchanged() {
        assert_eq!(super::decode_text_escapes("hello world").unwrap(), "hello world");
    }

    #[test]
    fn unknown_escape_fails_closed() {
        // An undeclared escape is a hard error, never a silent passthrough.
        assert!(super::decode_text_escapes("x\\zy").is_err());
        // A trailing backslash has no following char to decode.
        assert!(super::decode_text_escapes("end\\").is_err());
    }
}

// ─── BRANCH EFFECT SCOPING REGRESSION TESTS ──────────────────────────────────
//
// `seq_scope_arm_effects` threads a discarded (unconsumed) binding created
// inside an `if` arm into that arm's result, so the effect stays scoped to the
// arm. These tests pin the property it exists to guarantee:
//
//   AN EFFECT WRITTEN UNDER A CONDITION MUST BE REACHABLE FROM THE BUNDLE
//   RESULT ONLY THROUGH THAT CONDITION'S `CIf`.
//
// The regression these guard against: when the discarded binding was itself an
// `if`, every node the lowerer created inside its arms was also unreachable
// from the outer arm's result, so each got sequenced into the outer arm
// individually — hoisting the inner branch's effect out from under its own
// guard and making it fire unconditionally whenever the outer arm ran. The
// `CIf` survived in the graph but no longer gated anything, so nothing else in
// the pipeline (including the verifier) noticed.
//
// These build NF nodes directly rather than going through the surface, because
// the callable `seq` these emit is not in any shipped `.axreg` — see
// `branch_scope_registry` below.
#[cfg(test)]
mod branch_scoping_tests {
    use super::lower_nf_to_bundle_05;
    use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
    use crate::frontend::token::{Span, Token, TokenKind};
    use crate::ir::core_ir_05::{CoreBundle, Node, NodeRef};
    use std::collections::HashMap;

    // Self-contained registry. `seq` is the callable `seq_scope_arm_effects`
    // looks up by name; no registry in the repo declares one, so without this
    // every test here would die on `UnknownCallable("seq")`. `eff`/`eff2` are
    // the effectful calls whose scoping is under test.
    const BRANCH_AXREG: &str = "\
registry branch-scoping-test 0.5
type Unit = prim unit
fn seq
  identity 0x3333333333333333333333333333333333333333333333333333333333333333
  kind leaf
  in (Unit, Unit)
  out Unit
  effect pure
  deterministic true
  idempotent true
end
fn eff
  identity 0x4444444444444444444444444444444444444444444444444444444444444444
  kind leaf
  in (Unit)
  out Unit
  effect fullIo
  deterministic false
  idempotent false
end
fn eff2
  identity 0x5555555555555555555555555555555555555555555555555555555555555555
  kind leaf
  in (Unit)
  out Unit
  effect fullIo
  deterministic false
  idempotent false
end
";

    fn branch_scope_registry() -> crate::registry::core05::fold::Scope {
        crate::registry::core05::text::lower(BRANCH_AXREG)
            .expect("branch-scoping test registry must load cleanly")
            .scope
    }

    fn span() -> Span {
        Span::new(0, 4)
    }

    fn unit_lit() -> SchemaAstNode {
        SchemaAstNode {
            kind: "UnitLit".to_string(),
            fields: HashMap::new(),
            annotations: Default::default(),
            span: span(),
        }
    }

    fn call_expr(name: &str, args: Vec<SchemaAstNode>) -> SchemaAstNode {
        let tok = Token { kind: TokenKind::Ident, lexeme: name.to_string(), span: span() };
        let arglist = SchemaAstNode {
            kind: "ArgList".to_string(),
            fields: {
                let mut f = HashMap::new();
                f.insert("args".to_string(), SchemaValue::Nodes(args));
                f
            },
            annotations: Default::default(),
            span: span(),
        };
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), SchemaValue::Token(tok));
        fields.insert("args".to_string(), SchemaValue::Node(Box::new(arglist)));
        SchemaAstNode {
            kind: "CallExpr".to_string(),
            fields,
            annotations: Default::default(),
            span: span(),
        }
    }

    fn if_expr(cond: SchemaAstNode, then_: SchemaAstNode, else_: SchemaAstNode) -> SchemaAstNode {
        let mut fields = HashMap::new();
        fields.insert("condition".to_string(), SchemaValue::Node(Box::new(cond)));
        fields.insert("then_branch".to_string(), SchemaValue::Node(Box::new(then_)));
        fields.insert("else_branch".to_string(), SchemaValue::Node(Box::new(else_)));
        SchemaAstNode {
            kind: "IfExpr".to_string(),
            fields,
            annotations: Default::default(),
            span: span(),
        }
    }

    // Post-body-fold NF form of `let <name> = <value>; <body>`.
    fn let_expr(name: &str, value: SchemaAstNode, body: SchemaAstNode) -> SchemaAstNode {
        let tok = Token { kind: TokenKind::Ident, lexeme: name.to_string(), span: span() };
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), SchemaValue::Token(tok));
        fields.insert("value".to_string(), SchemaValue::Node(Box::new(value)));
        fields.insert("body".to_string(), SchemaValue::Node(Box::new(body)));
        SchemaAstNode {
            kind: "LetExpr".to_string(),
            fields,
            annotations: Default::default(),
            span: span(),
        }
    }

    // ── bundle inspection helpers ───────────────────────────────────────────

    fn node_indices_named(bundle: &CoreBundle, name: &str) -> Vec<usize> {
        bundle
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n, Node::CCall { target_name, .. } if target_name == name))
            .map(|(i, _)| i)
            .collect()
    }

    fn only_node_named(bundle: &CoreBundle, name: &str) -> usize {
        let found = node_indices_named(bundle, name);
        assert_eq!(found.len(), 1, "expected exactly one `{name}` node, got {found:?}");
        found[0]
    }

    fn out_edges(node: &Node) -> Vec<NodeRef> {
        match node {
            Node::CCall { args, .. } => args.clone(),
            Node::CIf { cond, then_, else_ } => vec![*cond, *then_, *else_],
            Node::CEnd { value } => vec![*value],
            Node::CDeterminate | Node::CBlock => Vec::new(),
        }
    }

    /// Indices of every node that references `target` directly.
    fn referrers(bundle: &CoreBundle, target: usize) -> Vec<usize> {
        bundle
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| out_edges(n).contains(&NodeRef::Node(target as u32)))
            .map(|(i, _)| i)
            .collect()
    }

    /// Every node from which `effect` is reachable.
    fn reaches(bundle: &CoreBundle, from: usize, effect: usize) -> bool {
        let mut stack = vec![from];
        let mut seen = std::collections::HashSet::new();
        while let Some(i) = stack.pop() {
            if !seen.insert(i) {
                continue;
            }
            if i == effect {
                return true;
            }
            for r in out_edges(&bundle.nodes[i]) {
                if let NodeRef::Node(j) = r {
                    stack.push(j as usize);
                }
            }
        }
        false
    }

    /// The `CIf` that is supposed to gate `effect`: the innermost one from which
    /// the effect is reachable. Nodes are emitted in topological order, so the
    /// innermost enclosing `CIf` is the lowest-indexed one that reaches it.
    /// Derived from the graph rather than assumed, so it identifies the same
    /// guard whether or not the scoping is correct.
    fn innermost_guard(bundle: &CoreBundle, effect: usize) -> usize {
        (0..bundle.nodes.len())
            .find(|&i| matches!(&bundle.nodes[i], Node::CIf { .. }) && reaches(bundle, i, effect))
            .unwrap_or_else(|| panic!("no CIf reaches the effect at all; bundle: {:?}", bundle.nodes))
    }

    /// THE INVARIANT. Walk from the bundle result, refusing to descend into the
    /// effect's guarding `CIf`. If the effect is still reachable, some other
    /// path bypasses the guard — the effect fires when its condition is false.
    fn assert_effect_is_gated(bundle: &CoreBundle, effect: usize) {
        let guard = innermost_guard(bundle, effect);
        let mut stack = match bundle.result {
            NodeRef::Node(i) => vec![i as usize],
            _ => Vec::new(),
        };
        let mut seen = std::collections::HashSet::new();
        while let Some(i) = stack.pop() {
            if !seen.insert(i) {
                continue;
            }
            assert_ne!(
                i, effect,
                "node {effect} (the effect) is reachable from the bundle result on a \
                 path that bypasses its guarding CIf (node {guard}) — the guard no \
                 longer gates it, so it fires unconditionally. Bundle: {:?}",
                bundle.nodes
            );
            // Stop at the guard: everything below it is legitimately gated.
            if i == guard {
                continue;
            }
            for r in out_edges(&bundle.nodes[i]) {
                if let NodeRef::Node(j) = r {
                    stack.push(j as usize);
                }
            }
        }
    }

    // ── 1. REGRESSION: discarded `if` nested inside another `if` arm ─────────
    //
    //   if c {
    //     let r = if d { eff(unit) } else { unit }   // value discarded
    //     unit
    //   } else { unit }
    //
    // `eff` must fire only when c AND d. The bug spliced `eff` into the outer
    // arm as a sibling of the inner `CIf`, so it fired whenever c held.
    #[test]
    fn nested_discarded_if_keeps_effect_under_its_own_guard() {
        let scope = branch_scope_registry();
        let inner = if_expr(unit_lit(), call_expr("eff", vec![unit_lit()]), unit_lit());
        let arm = let_expr("r", inner, unit_lit());
        let node = if_expr(unit_lit(), arm, unit_lit());

        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        let eff = only_node_named(&bundle, "eff");

        // The invariant itself, checked first: every path from the result to the
        // effect crosses a CIf.
        assert_effect_is_gated(&bundle, eff);

        // The effect is referenced only by its arm's CEnd (Core IR 0.6), which only the inner CIf
        // references — never by a `seq`.
        let refs = referrers(&bundle, eff);
        assert_eq!(
            refs.len(),
            1,
            "effect must have exactly one referrer (its arm's CEnd), got {refs:?}"
        );
        assert!(
            matches!(&bundle.nodes[refs[0]], Node::CEnd { .. }),
            "the effect's only referrer must be its arm's CEnd, got {:?}",
            &bundle.nodes[refs[0]]
        );
        let end_refs = referrers(&bundle, refs[0]);
        assert!(
            end_refs.len() == 1 && matches!(&bundle.nodes[end_refs[0]], Node::CIf { .. }),
            "the arm's CEnd must be referenced by its CIf alone, got {end_refs:?}"
        );

        // Exactly one `seq` — wrapping the discarded inner CIf, not the effect.
        let seq = only_node_named(&bundle, "seq");
        match &bundle.nodes[seq] {
            Node::CCall { args, .. } => {
                let first = args[0];
                assert_ne!(
                    first,
                    NodeRef::Node(eff as u32),
                    "seq must sequence the discarded CIf, not the effect directly"
                );
                match first {
                    NodeRef::Node(i) => assert!(
                        matches!(&bundle.nodes[i as usize], Node::CIf { .. }),
                        "seq's sequenced operand must be the discarded CIf"
                    ),
                    other => panic!("seq's first arg must be a node, got {other:?}"),
                }
            }
            other => panic!("expected CCall, got {other:?}"),
        }
    }

    // ── 2. The machinery still works for a FLAT discarded effect ─────────────
    //
    //   if c { let d = eff(unit); unit } else { unit }
    //
    // Here `eff` genuinely belongs to the arm and must be sequenced into it —
    // a fix that simply stopped emitting `seq` would pass test 1 and break this.
    #[test]
    fn flat_discarded_effect_is_sequenced_into_its_arm() {
        let scope = branch_scope_registry();
        let arm = let_expr("d", call_expr("eff", vec![unit_lit()]), unit_lit());
        let node = if_expr(unit_lit(), arm, unit_lit());

        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        let eff = only_node_named(&bundle, "eff");
        let seq = only_node_named(&bundle, "seq");

        match &bundle.nodes[seq] {
            Node::CCall { args, .. } => assert_eq!(
                args[0],
                NodeRef::Node(eff as u32),
                "the discarded effect must be the sequenced operand"
            ),
            other => panic!("expected CCall, got {other:?}"),
        }
        // Still gated: the seq sits under the arm's CIf, not above it.
        assert_effect_is_gated(&bundle, eff);
    }

    // ── 3. A nested `if` that IS the arm's result needs no sequencing ────────
    //
    //   if c { if d { eff(unit) } else { unit } } else { unit }
    //
    // The inner CIf is consumed as the arm's value, so it is already anchored.
    // Emitting a `seq` here would be spurious.
    #[test]
    fn nested_if_used_as_arm_result_emits_no_seq() {
        let scope = branch_scope_registry();
        let inner = if_expr(unit_lit(), call_expr("eff", vec![unit_lit()]), unit_lit());
        let node = if_expr(unit_lit(), inner, unit_lit());

        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        assert!(
            node_indices_named(&bundle, "seq").is_empty(),
            "a consumed inner CIf needs no seq wrapper; bundle: {:?}",
            bundle.nodes
        );
        assert_effect_is_gated(&bundle, only_node_named(&bundle, "eff"));
    }

    // ── 4. Independent discarded effects are not swallowed ───────────────────
    //
    //   if c { let a = eff(unit); let b = eff2(unit); unit } else { unit }
    //
    // Neither reaches the other, so both are root orphans and both must be
    // sequenced. Guards the interior-subtraction from over-reaching and
    // silently dropping a sibling effect.
    #[test]
    fn two_independent_discarded_effects_are_both_sequenced() {
        let scope = branch_scope_registry();
        let arm = let_expr(
            "a",
            call_expr("eff", vec![unit_lit()]),
            let_expr("b", call_expr("eff2", vec![unit_lit()]), unit_lit()),
        );
        let node = if_expr(unit_lit(), arm, unit_lit());

        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        let eff = only_node_named(&bundle, "eff");
        let eff2 = only_node_named(&bundle, "eff2");

        assert_eq!(
            node_indices_named(&bundle, "seq").len(),
            2,
            "each independent discarded effect needs its own seq; bundle: {:?}",
            bundle.nodes
        );
        for e in [eff, eff2] {
            let refs = referrers(&bundle, e);
            assert_eq!(refs.len(), 1, "effect {e} must be sequenced exactly once, got {refs:?}");
            assert!(
                matches!(&bundle.nodes[refs[0]], Node::CCall { target_name, .. } if target_name == "seq"),
                "effect {e} must be anchored by a seq"
            );
            assert_effect_is_gated(&bundle, e);
        }
    }

    // ── 5. A consumed effect is never double-anchored ────────────────────────
    //
    //   if c { let a = eff(unit); eff2(a) } else { unit }
    //
    // `eff` is reachable from the arm result through `eff2`, so it is already
    // anchored and must not also be sequenced.
    #[test]
    fn consumed_effect_is_not_additionally_sequenced() {
        let scope = branch_scope_registry();
        let use_a = call_expr("eff2", vec![var_ref("a")]);
        let arm = let_expr("a", call_expr("eff", vec![unit_lit()]), use_a);
        let node = if_expr(unit_lit(), arm, unit_lit());

        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        assert!(
            node_indices_named(&bundle, "seq").is_empty(),
            "a consumed binding is anchored by its use; no seq needed. bundle: {:?}",
            bundle.nodes
        );
        let eff = only_node_named(&bundle, "eff");
        assert_eq!(
            referrers(&bundle, eff),
            vec![only_node_named(&bundle, "eff2")],
            "eff must be referenced only by its consumer"
        );
        assert_effect_is_gated(&bundle, eff);
    }

    // ── 6. BOOL_SHORT_CIRCUIT_V1: bool_and / bool_or evaluate their second argument only when needed ──
    //
    //   bool_and(c, effb(unit))   ->   CIf(c, effb(unit), false)
    //   bool_or(c, effb(unit))    ->   CIf(c, true, effb(unit))
    //
    // As a CCall both arguments ran: `bool_and(int_lt(i, n), int_eq(str_char_code(s, i), …))` read past the end
    // of `s` exactly when the guard said not to. The second argument must sit under the CIf, like any arm.
    const SHORT_AXREG: &str = "\
registry short-circuit-test 0.5
type Unit = prim unit
type Bool = prim bool
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
fn effb
  identity 0x8888888888888888888888888888888888888888888888888888888888888888
  kind leaf
  in (Unit)
  out Bool
  effect fullIo
  deterministic false
  idempotent false
end
fn seq
  identity 0x3333333333333333333333333333333333333333333333333333333333333333
  kind leaf
  in (Unit, Unit)
  out Unit
  effect pure
  deterministic true
  idempotent true
end
";

    fn bool_lit(v: bool) -> SchemaAstNode {
        let tok = Token { kind: TokenKind::Ident, lexeme: v.to_string(), span: span() };
        let mut fields = HashMap::new();
        fields.insert("value".to_string(), SchemaValue::Token(tok));
        SchemaAstNode { kind: "BoolLit".to_string(), fields, annotations: Default::default(), span: span() }
    }

    fn short_circuit_case(op: &str) {
        let scope = crate::registry::core05::text::lower(SHORT_AXREG)
            .expect("short-circuit test registry must load cleanly")
            .scope;
        let node = call_expr(op, vec![bool_lit(true), call_expr("effb", vec![unit_lit()])]);
        let bundle = lower_nf_to_bundle_05(&node, &scope).unwrap();
        assert!(
            node_indices_named(&bundle, op).is_empty(),
            "{op}(a, b) must not lower to an eager CCall; bundle: {:?}",
            bundle.nodes
        );
        let effb = only_node_named(&bundle, "effb");
        assert_effect_is_gated(&bundle, effb);
        match bundle.result {
            NodeRef::Node(i) => assert!(
                matches!(&bundle.nodes[i as usize], Node::CIf { .. }),
                "{op} must lower to a CIf; bundle: {:?}",
                bundle.nodes
            ),
            other => panic!("expected a node result, got {other:?}"),
        }
    }

    #[test]
    fn bool_and_evaluates_its_second_argument_only_under_the_condition() {
        short_circuit_case("bool_and");
    }

    #[test]
    fn bool_or_evaluates_its_second_argument_only_when_the_first_is_false() {
        short_circuit_case("bool_or");
    }

    fn var_ref(name: &str) -> SchemaAstNode {
        let tok = Token { kind: TokenKind::Ident, lexeme: name.to_string(), span: span() };
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), SchemaValue::Token(tok));
        SchemaAstNode {
            kind: "VarRef".to_string(),
            fields,
            annotations: Default::default(),
            span: span(),
        }
    }
}
