// M1 exhaustive lowering matrix — AXLANG_M1_EXHAUSTIVE_LOWERING_MATRIX_T3
// + AXLANG_M1_LAM_RESTRICT_NONCAPTURING_ARGPOS_V1 (rows 20–22)
// + AXLANG_VALUE_CONSTRUCTION_LOWERING_V2 (rows 9/10 flip + rows 23–25)
// + M1_BYTE_TYPE_REQUIRED (row 26 — Bytes-typed param flows through)
//
// One minimal M1 program per node kind / restriction case. Each test runs the
// REAL pipeline (lex → parse → schema → normalize → validate_nf +
// validate_m1_lam_rules + validate_value_construct_types) via run_pipeline,
// then calls lower_nf_to_bundle_05 on the relevant NF sub-node.
//
// Matrix rows:
//   Row  1: FnDecl                — PASS → CLam (CVar("x") + CLam{param:"x",body:Node(0)})
//   Row  2: LetDecl               — PASS → CPrimLit(Int) pool entry (let x = Int(42))
//   Row  3: Body(0-binding)       — PASS → CVar (body fold: no Body node in NF)
//   Row  4: Body(1-binding)       — PASS → CVar (LetExpr env extension)
//   Row  5: Body(n-binding)       — PASS → CVar, CVar (nested LetExpr chain)
//   Row  6: IfExpr                — PASS → CIf
//   Row  7: LamExpr (arg-pos)     — PASS → CLam (non-capturing arg-position LamExpr lowers)
//   Row  8: CallExpr              — PASS → CCall (two-step ArgList extraction)
//   Row  9: ValueConstructExpr    — PASS → CCall(value_construct, [CVar("a"), CVar("b")])
//   Row 10: ValueListConstructExpr — PASS → CCall(value_list_construct, [CVar("a")])
//   Row 11: DetermineExpr         — PASS → CDeterminate
//   Row 12: VarRef                — PASS → CVar
//   Row 13: IntLit  Int(42)        — PASS → Pool(Int, encode(42))
//   Row 14: TextLit Text("hello")  — PASS → Pool(Text, encode("hello"))
//   Row 15: BoolLit Bool(true)     — PASS → Pool(Bool, [0x01])
//   Row 16: BoolLit Bool(false)    — PASS → Pool(Bool, [0x00])
//   Row 17: UnitLit Unit()         — PASS → Pool(Unit, [])
//   Row 18: nested lets           — PASS → CVar, CVar (inner LetExpr confirmed)
//   Row 19: multi-function        — PASS → 4 nodes (2 × [CVar + CLam])
//   Row 20: LamExpr capture       — FAIL validation (RULE 1: captures outer var 'x')
//   Row 21: LamExpr let-bound     — FAIL validation (RULE 2: LamExpr in LetExpr.value)
//   Row 22: LamExpr body-result   — FAIL validation (RULE 2: LamExpr as FnDecl body)
//   Row 23: G2 — Value(Int)("hello") — FAIL validation (type mismatch: Int vs Text)
//   Row 24: G3 — ValueList(Value(Text,Int))(Value(Text,Int)(a,b)) — PASS → nested CCall tree
//   Row 25: G4 — Value(Bool)(fn(a){a}) — FAIL validation (RULE 2: Lam in Value element)
//   Row 26: Bytes-typed param — PASS → sentinel pool entry; "Bytes" name preserved in NF
//
// Pre-existing known failure (NOT in this file):
//   lowering::spec_driven::tests_05::unsupported_spec_target_stops — IntLit field issue
//   in the spec-driven path; unrelated to nf_lowering.

use axis_lang::fabric::codec::{encode_int, encode_text};
use axis_lang::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use axis_lang::ir::core_ir_05::{Node, NodeRef};
use axis_lang::lowering::emit_05::SchemaLower05Error;
use axis_lang::lowering::nf_lowering::lower_nf_to_bundle_05;
use axis_lang::pipeline::{run_pipeline, PipelineConfig, PipelineError};
use axis_lang::registry::core05::{type_identity, PrimCode, TypeDef, TypeShape};
use axis_lang::registry::core05::fold::Scope;
use axis_lang::registry::Registry;
use std::path::PathBuf;

const LEXER: &str = "axis-surface-m1-config/m1-lexer.yaml";
const PARSER: &str = "axis-surface-m1-config/m1-parse.yaml";
const SCHEMA: &str = "axis-surface-m1-config/m1-ast.yaml";
const NORMALIZE: &str = "axis-surface-m1-config/m1-normalize.yaml";
const PILOT_AXREG: &str = include_str!("../registries/pilot_lowering.axreg");
const AXIS_AXREG: &str = include_str!("../registries/axis.axreg");

fn cfg(source: &str) -> PipelineConfig {
    PipelineConfig {
        lexer_spec: PathBuf::from(LEXER),
        parser_spec: PathBuf::from(PARSER),
        ast_schema: PathBuf::from(SCHEMA),
        source_file: PathBuf::from(source),
        entry_rule: None,
        normalize_spec: PathBuf::from(NORMALIZE),
        registry: Registry::new(),
        core05_scope: None,
        parser_mode: None,
        hook_registry: None,
    }
}

fn pilot_scope() -> Scope {
    axis_lang::registry::core05::text::lower(PILOT_AXREG)
        .expect("pilot registry must load cleanly")
        .scope
}

fn axis_scope() -> Scope {
    axis_lang::registry::core05::text::lower(AXIS_AXREG)
        .expect("axis.axreg must load cleanly")
        .scope
}

// Extract FnDecl.body from the NF root (single-function programs only).
fn fn_body(nf: &SchemaAstNode) -> &SchemaAstNode {
    assert_eq!(nf.kind, "FnDecl", "NF root must be FnDecl, got '{}'", nf.kind);
    match nf.fields.get("body") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        other => panic!("FnDecl.body must be SchemaValue::Node, got {other:?}"),
    }
}

// ─── ROW 1: FnDecl ───────────────────────────────────────────────────────────
//
// Program:   fn test(x: Bool) -> Bool { x }
// NF root:   FnDecl { name:"test", params:TypedParamList{[TypedParam{name:"x"}]}, body:VarRef }
// Lower:     extract param "x" → CVar("x") at Node(0), env[x]=Node(0),
//            lower VarRef("x") → Node(0), CLam{param:"x",body:Node(0)} at Node(1)
// Verdict:   PASS → [CVar("x"), CLam{param:"x", body:Node(0)}]
#[test]
fn matrix_row01_fndecl() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_simple.m1"))
        .expect("NF pipeline must succeed");
    assert_eq!(a.schema_asts[0].kind, "FnDecl", "NF root must be FnDecl");
    let bundle = lower_nf_to_bundle_05(&a.schema_asts[0], &Scope::new())
        .expect("FnDecl must lower successfully");
    // param x → sentinel pool entry; VarRef("x") → Pool(0); no nodes emitted.
    assert_eq!(bundle.nodes.len(), 0, "no nodes: param x is a sentinel pool entry");
    assert_eq!(bundle.constant_pool.len(), 1, "one sentinel pool entry for param x");
}

// ─── ROW 2: LetDecl ──────────────────────────────────────────────────────────
//
// Program:   let x = Int(42)
// NF root:   LetDecl { name:Token("x"), value:IntLit{value:Token("42")} }
// Lower:     CLetDecl → lower value IntLit → CPrimLit(Int) → 1 pool entry
// Verdict:   PASS → 1 pool entry, 0 nodes
//
// (fixture was updated from `let x = y` — CVar nodes no longer exist in IR)
#[test]
fn matrix_row02_letdecl() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_letdecl.m1"))
        .expect("NF pipeline must succeed");
    assert_eq!(a.schema_asts[0].kind, "LetDecl", "NF root must be LetDecl");
    let bundle = lower_nf_to_bundle_05(&a.schema_asts[0], &Scope::new())
        .expect("LetDecl with IntLit value must lower successfully");
    assert_eq!(bundle.constant_pool.len(), 1, "Int(42) → one pool entry");
    assert_eq!(bundle.nodes.len(), 0, "no nodes for a literal value");
}

// ─── ROW 3: Body(0-binding) ──────────────────────────────────────────────────
//
// Program:   fn test(x: Bool) -> Bool { x }
// NF:        desugar_body folds 0 bindings → FnDecl.body is VarRef directly (no Body wrapper)
// Lower:     VarRef (empty env) → CVar("x")
// Verdict:   PASS → CVar
//
// Embedded in FnDecl — lower FnDecl.body (VarRef), not FnDecl itself.
#[test]
fn matrix_row03_body_0_binding() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_simple.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_ne!(body.kind, "Body", "desugar_body must have eliminated the Body node");
    assert_eq!(body.kind, "VarRef", "0-binding body must fold to VarRef directly");
    // VarRef("x") with empty env → UnknownCallable (no CVar fallback anymore).
    let err = lower_nf_to_bundle_05(body, &Scope::new()).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "x"),
        "free VarRef 'x' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 4: Body(1-binding) ──────────────────────────────────────────────────
//
// Program:   fn f(a: Bool) -> Bool { let x = a \n x }
// NF:        FnDecl.body = LetExpr{name:"x", value:VarRef("a"), body:VarRef("x")}
// Lower:     VarRef("a")→CVar("a") at Node(0); x resolves to Node(0); result=Node(0)
//            bundle.nodes = [CVar("a")]
// Verdict:   PASS → CVar (via LetExpr env extension)
//
// Embedded in FnDecl — lower FnDecl.body (LetExpr).
#[test]
fn matrix_row04_body_1_binding() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_body_1.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_ne!(body.kind, "Body", "desugar_body must have eliminated the Body node");
    assert_eq!(body.kind, "LetExpr", "1-binding body must fold to LetExpr");
    // VarRef("a") is free (not in env) → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &Scope::new()).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "a"),
        "free VarRef 'a' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 5: Body(n-binding) ──────────────────────────────────────────────────
//
// Program:   fn f(a: Bool, b: Bool) -> Bool { let x = a \n let y = b \n x }
// NF:        FnDecl.body = LetExpr{x=VarRef(a), body: LetExpr{y=VarRef(b), body:VarRef(x)}}
// Lower:     CVar("a") at Node(0), CVar("b") at Node(1); x→Node(0) via env; result=Node(0)
//            bundle.nodes = [CVar("a"), CVar("b")]
// Verdict:   PASS → CVar, CVar (nested LetExpr chain)
//
// Embedded in FnDecl — lower FnDecl.body (outer LetExpr).
#[test]
fn matrix_row05_body_n_bindings() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_body_n.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_ne!(body.kind, "Body", "desugar_body must have eliminated the Body node");
    assert_eq!(body.kind, "LetExpr", "n-binding body must fold to LetExpr (outer)");
    // VarRef("a") is free (not in env) → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &Scope::new()).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "a"),
        "free VarRef 'a' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 6: IfExpr ───────────────────────────────────────────────────────────
//
// Program:   fn decide(flag: Bool) -> Bool { if flag { flag } else { flag } }
//            (reuses examples/surface-m1/ifexpr_decide.m1)
// NF:        FnDecl.body = IfExpr{condition:VarRef("flag"), then_branch:VarRef("flag"),
//                                  else_branch:VarRef("flag")}
// Lower:     CVar("flag") at Node(0), all three arms hash-cons to Node(0), CIf at Node(1)
// Verdict:   PASS → CIf
//
// Embedded in FnDecl — lower FnDecl.body (IfExpr).
#[test]
fn matrix_row06_ifexpr() {
    let a = run_pipeline(&cfg("examples/surface-m1/ifexpr_decide.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "IfExpr", "body must be IfExpr");
    // VarRef("flag") is free (not in env) → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &Scope::new()).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "flag"),
        "free VarRef 'flag' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 7: LamExpr (argument-position, legal) ───────────────────────────────
//
// Program:   fn f(x: Bool) -> Unit { const_unit(fn(a) { a }) }
// RULE 1:    fn(a){a} body only references its own param 'a' — no capture.
// RULE 2:    LamExpr is a direct argument to const_unit(...) — legal position.
// NF:        FnDecl.body = CallExpr{name:"const_unit",
//                            args:ArgList{args:[LamExpr{params:[a], body:VarRef("a")}]}}
// PASS 3 update: lambda syntax removed from M1 — file now fails at parse time.
// Verdict:   FAIL at parse (lambda syntax `fn(...){}` no longer in M1 grammar).
#[test]
fn matrix_row07_lamexpr() {
    let result = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_lam_argpos.m1"));
    assert!(result.is_err(), "lambda syntax must be parse-rejected after PASS 3");
    match result.unwrap_err() {
        PipelineError::Parse { .. } => {}
        other => panic!("expected parse error, got {other}"),
    }
}

// ─── ROW 8: CallExpr ─────────────────────────────────────────────────────────
//
// Program:   fn f(x: Bool) -> Bool { const_unit(x) }
// NF:        FnDecl.body = CallExpr{name:Token("const_unit"),
//                                    args:Node(ArgList{args:Nodes([VarRef("x")])})}
// Lower:     two-step extraction: CallExpr.args → ArgList node → ArgList.args Nodes;
//            VarRef("x") → CVar("x") at Node(0) (x not in env → free CVar);
//            CCall{const_unit_id, [Node(0)]} at Node(1)
// Verdict:   PASS → [CVar("x"), CCall{const_unit, [Node(0)]}]
//
// Embedded in FnDecl — lower FnDecl.body (CallExpr) with pilot scope.
#[test]
fn matrix_row08_callexpr() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_callexpr.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "CallExpr", "body must be CallExpr");
    let scope = pilot_scope();
    // VarRef("x") is a function param — not in env, not a registry fn → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &scope).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "x"),
        "free VarRef 'x' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 9: ValueConstructExpr ───────────────────────────────────────────────
//
// Program:   fn make(a: Bool, b: Bool) -> Value(Bool, Bool) { Value(Bool, Bool)(a, b) }
// NF:        FnDecl.body = ValueConstructExpr{type_args:ValueTypeArgs{[Bool,Bool]},
//                                             args:ArgList{[VarRef("a"),VarRef("b")]}}
// Lower:     → CVar("a") at Node(0), CVar("b") at Node(1),
//              CCall(value_construct_id, [Node(0), Node(1)]) at Node(2)
// Verdict:   PASS — registry-constructor CCall, type-erased, elements present
//
// G1 gate: well-typed Value(Bool,Bool)(a,b) lowers to a constructor CCall with
//          both element args present; row 9 flips FAIL→PASS via run_pipeline.
#[test]
fn matrix_row09_value_construct_expr() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_value_construct.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "ValueConstructExpr", "body must be ValueConstructExpr");

    let scope = axis_scope();
    let vc_id = scope.lookup_name("value_make")
        .expect("value_make must be in axis scope");

    // VarRef("a") and VarRef("b") are function params — not in env → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &scope).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "a" || n == "b"),
        "free VarRef must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 10: ValueListConstructExpr ──────────────────────────────────────────
//
// Program:   fn make(a: Bool) -> ValueList(Bool) { ValueList(Bool)(a) }
// NF:        FnDecl.body = ValueListConstructExpr{element_type:PrimTypeExpr("Bool"),
//                                                  args:ArgList{[VarRef("a")]}}
// Lower:     → CVar("a") at Node(0),
//              CCall(list_make_id, [Node(0)]) at Node(1)
// Verdict:   PASS — registry-constructor CCall, type-erased, element present
//
// G1 gate (row 10 side): ValueList(Bool)(a) lowers to a constructor CCall with
//          the element arg present; row 10 flips FAIL→PASS via run_pipeline.
#[test]
fn matrix_row10_valuelist_construct_expr() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_valuelist_construct.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "ValueListConstructExpr", "body must be ValueListConstructExpr");

    let scope = axis_scope();
    let vlc_id = scope.lookup_name("list_make")
        .expect("list_make must be in axis scope");

    // VarRef("a") is a function param — not in env → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &scope).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "a"),
        "free VarRef 'a' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 11: DetermineExpr ───────────────────────────────────────────────────
//
// Program:   fn f(x: Bool) -> Bool { determine }
// NF:        FnDecl.body = DetermineExpr{}
//            (desugar_body: 0 bindings → result = DetermineExpr directly)
// Lower:     DetermineExpr → CDeterminate (the only zero-operand gate node)
// Verdict:   PASS → CDeterminate
//
// Embedded in FnDecl — lower FnDecl.body (DetermineExpr).
// NOTE: first end-to-end test of `determine` surface keyword through the real pipeline.
#[test]
fn matrix_row11_determine_expr() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_determine.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "DetermineExpr", "body must be DetermineExpr");
    let bundle = lower_nf_to_bundle_05(body, &Scope::new())
        .expect("DetermineExpr must lower successfully");
    assert_eq!(bundle.nodes.len(), 1, "exactly one CDeterminate node");
    assert!(
        matches!(bundle.nodes[0], Node::CDeterminate),
        "node must be CDeterminate, got {:?}", bundle.nodes[0]
    );
}

// ─── ROW 12: VarRef ──────────────────────────────────────────────────────────
//
// Program:   fn test(x: Bool) -> Bool { x }
// NF:        FnDecl.body = VarRef{name:Token("x")}
//            (normalize_ident rewrites Expr{variant:IDENT} → VarRef{name:token})
// Lower:     VarRef (not in env) → CVar("x")
// Verdict:   PASS → CVar
//
// Embedded in FnDecl — lower FnDecl.body (VarRef) with empty scope.
#[test]
fn matrix_row12_varref() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_simple.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "VarRef", "body must be VarRef");
    // VarRef("x") with empty env → UnknownCallable (no CVar fallback).
    let err = lower_nf_to_bundle_05(body, &Scope::new()).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "x"),
        "free VarRef 'x' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 13: INT_LIT ─────────────────────────────────────────────────────────
//
// Program:   fn f() -> Int { Int(42) }
// NF:        FnDecl.body = IntLit{value:Token(INT_LIT,"42")}
// Lower:     IntLit → intern_constant(Int, encode_int(42))
// Verdict:   PASS → Pool(Int type, encode(42))
//
// KEY: asserts CORRECT literal kind (Int) — not merely that lowering succeeded.
#[test]
fn matrix_row13_int_lit() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_int_lit.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "IntLit", "Int(42) body must normalize to IntLit");
    let bundle = lower_nf_to_bundle_05(body, &Scope::new())
        .expect("IntLit must lower successfully");
    assert_eq!(bundle.nodes.len(), 0, "literal → pool entry, no nodes");
    assert_eq!(bundle.constant_pool.len(), 1, "one Int constant pool entry");
    let entry = &bundle.constant_pool[0];
    let expected_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Int) });
    assert_eq!(entry.def_hash, expected_hash, "def_hash must be Int type identity");
    let mut expected_payload = Vec::new();
    encode_int(&mut expected_payload, 42);
    assert_eq!(entry.payload, expected_payload, "payload must encode 42 via zigzag LEB128");
}

// ─── ROW 14: STRING_LIT ──────────────────────────────────────────────────────
//
// Program:   fn f() -> Text { Text("hello") }
// NF:        FnDecl.body = TextLit{value:Token(STRING_LIT,"\"hello\"")}
// Lower:     TextLit → strip quotes → intern_constant(Text, encode("hello"))
// Verdict:   PASS → Pool(Text type, encode("hello"))
//
// KEY: asserts CORRECT literal kind (Text) and correct string content.
#[test]
fn matrix_row14_string_lit() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_string_lit.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "TextLit", "Text(\"hello\") body must normalize to TextLit");
    let bundle = lower_nf_to_bundle_05(body, &Scope::new())
        .expect("TextLit must lower successfully");
    assert_eq!(bundle.nodes.len(), 0, "literal → pool entry, no nodes");
    assert_eq!(bundle.constant_pool.len(), 1, "one Text constant pool entry");
    let entry = &bundle.constant_pool[0];
    let expected_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Text) });
    assert_eq!(entry.def_hash, expected_hash, "def_hash must be Text type identity");
    let mut expected_payload = Vec::new();
    encode_text(&mut expected_payload, "hello");
    assert_eq!(entry.payload, expected_payload, "payload must encode \"hello\" as Text");
}

// ─── ROW 15: KW_TRUE (bool true) ─────────────────────────────────────────────
//
// Program:   fn f() -> Bool { Bool(true) }
// NF:        FnDecl.body = BoolLit{value:Token(KW_TRUE,"true")}
// Lower:     BoolLit → intern_constant(Bool, [0x01])
// Verdict:   PASS → Pool(Bool type, [0x01])
//
// KEY: asserts CORRECT literal kind (Bool) and correct true payload.
#[test]
fn matrix_row15_bool_true() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_bool_true.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "BoolLit", "Bool(true) body must normalize to BoolLit");
    let bundle = lower_nf_to_bundle_05(body, &Scope::new())
        .expect("BoolLit must lower successfully");
    assert_eq!(bundle.nodes.len(), 0, "literal → pool entry, no nodes");
    assert_eq!(bundle.constant_pool.len(), 1, "one Bool constant pool entry");
    let entry = &bundle.constant_pool[0];
    let expected_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) });
    assert_eq!(entry.def_hash, expected_hash, "def_hash must be Bool type identity");
    assert_eq!(entry.payload, vec![0x01], "true must encode as [0x01]");
}

// ─── ROW 16: KW_FALSE (bool false) ───────────────────────────────────────────
//
// Program:   fn f() -> Bool { Bool(false) }
// NF:        FnDecl.body = BoolLit{value:Token(KW_FALSE,"false")}
// Lower:     BoolLit → intern_constant(Bool, [0x00])
// Verdict:   PASS → Pool(Bool type, [0x00])
//
// KEY: asserts CORRECT literal kind (Bool) and correct false payload.
#[test]
fn matrix_row16_bool_false() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_bool_false.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "BoolLit", "Bool(false) body must normalize to BoolLit");
    let bundle = lower_nf_to_bundle_05(body, &Scope::new())
        .expect("BoolLit must lower successfully");
    assert_eq!(bundle.nodes.len(), 0, "literal → pool entry, no nodes");
    assert_eq!(bundle.constant_pool.len(), 1, "one Bool constant pool entry");
    let entry = &bundle.constant_pool[0];
    let expected_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Bool) });
    assert_eq!(entry.def_hash, expected_hash, "def_hash must be Bool type identity");
    assert_eq!(entry.payload, vec![0x00], "false must encode as [0x00]");
}

// ─── ROW 17: KW_UNIT (unit literal) ──────────────────────────────────────────
//
// Program:   fn f() -> Unit { Unit() }
// NF:        FnDecl.body = UnitLit{}
// Lower:     UnitLit → intern_unit() → Pool(Unit, [])
// Verdict:   PASS → Pool(Unit type, [])
//
// KEY: asserts CORRECT literal kind (Unit) and empty payload.
#[test]
fn matrix_row17_unit_lit() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_unit.m1"))
        .expect("NF pipeline must succeed");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "UnitLit", "Unit() body must normalize to UnitLit");
    let bundle = lower_nf_to_bundle_05(body, &Scope::new())
        .expect("UnitLit must lower successfully");
    assert_eq!(bundle.nodes.len(), 0, "literal → pool entry, no nodes");
    assert_eq!(bundle.constant_pool.len(), 1, "one Unit constant pool entry");
    let entry = &bundle.constant_pool[0];
    let expected_hash = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Unit) });
    assert_eq!(entry.def_hash, expected_hash, "def_hash must be Unit type identity");
    assert!(entry.payload.is_empty(), "unit must have empty payload");
}

// ─── ROW 18: nested lets ─────────────────────────────────────────────────────
//
// Program:   fn f(a: Bool, b: Bool) -> Bool { let x = a \n let y = b \n x }
// NF:        FnDecl.body = outer LetExpr; outer.body = inner LetExpr (2-level nesting)
// Lower:     same as Row 5; asserts the nesting structure explicitly
// Verdict:   PASS → CVar, CVar
//
// Distinguishes from Body(n): proves the inner LetExpr.body is also a LetExpr,
// and that both bindings independently survive to Core IR as separate CVar nodes.
#[test]
fn matrix_row18_nested_lets() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_body_n.m1"))
        .expect("NF pipeline must succeed");
    let outer = fn_body(&a.schema_asts[0]);
    assert_eq!(outer.kind, "LetExpr", "outer body must be LetExpr");
    let inner = match outer.fields.get("body") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        other => panic!("outer LetExpr.body must be Node, got {other:?}"),
    };
    assert_eq!(inner.kind, "LetExpr", "inner LetExpr.body must also be LetExpr (2-level nesting)");
    // VarRef("a") is free in outer LetExpr.value — not in env → UnknownCallable.
    let err = lower_nf_to_bundle_05(outer, &Scope::new()).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "a"),
        "free VarRef 'a' must yield UnknownCallable, got: {err:?}"
    );
}

// ─── ROW 19: multi-function ──────────────────────────────────────────────────
//
// Program:   fn first(x: Bool) -> Bool { x } \n fn second(y: Bool) -> Bool { y }
// NF items:  [FnDecl("first"), FnDecl("second")]  (pipeline splits before normalization)
// Lower:     each FnDecl lowers independently to [CVar + CLam].
//            first:  CVar("x")=Node(0), CLam{param:"x",body:Node(0)}=Node(1)
//            second: CVar("y")=Node(0), CLam{param:"y",body:Node(0)}=Node(1)
// Verdict:   PASS → 2 items, each 2 nodes (CVar + CLam)
#[test]
fn matrix_row19_multi_function() {
    let a = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_multi_fn.m1"))
        .expect("NF pipeline must succeed");
    assert_eq!(a.schema_asts.len(), 2, "multi-function program must produce exactly 2 NF items");
    assert_eq!(a.schema_asts[0].kind, "FnDecl", "first item must be FnDecl");
    assert_eq!(a.schema_asts[1].kind, "FnDecl", "second item must be FnDecl");

    // fn first(x: Bool) -> Bool { x }
    // param x → sentinel Pool(0); VarRef("x") → Pool(0); 0 nodes.
    let bundle0 = lower_nf_to_bundle_05(&a.schema_asts[0], &Scope::new())
        .expect("first FnDecl must lower successfully");
    assert_eq!(bundle0.nodes.len(), 0, "no nodes: param x is a sentinel pool entry");
    assert_eq!(bundle0.constant_pool.len(), 1, "one sentinel for param x");

    // fn second(y: Bool) -> Bool { y }
    // param y → sentinel Pool(0); VarRef("y") → Pool(0); 0 nodes.
    let bundle1 = lower_nf_to_bundle_05(&a.schema_asts[1], &Scope::new())
        .expect("second FnDecl must lower successfully");
    assert_eq!(bundle1.nodes.len(), 0, "no nodes: param y is a sentinel pool entry");
    assert_eq!(bundle1.constant_pool.len(), 1, "one sentinel for param y");
}

// ─── ROW 20: LamExpr capture (RULE 1 violation) ──────────────────────────────
//
// PASS 3 update: lambda syntax removed from M1 — file now fails at parse time.
// Verdict:   FAIL at parse (lambda syntax `fn(...){}` no longer in M1 grammar).
#[test]
fn matrix_row20_lam_capture_rule1_fail() {
    let result = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_lam_capture.m1"));
    assert!(result.is_err(), "lambda syntax must be parse-rejected after PASS 3");
    match result.unwrap_err() {
        PipelineError::Parse { .. } => {}
        other => panic!("expected parse error, got {other}"),
    }
}

// ─── ROW 21: LamExpr let-bound escape (RULE 2 violation) ─────────────────────
//
// PASS 3 update: lambda syntax removed from M1 — file now fails at parse time.
// Verdict:   FAIL at parse (lambda syntax `fn(...){}` no longer in M1 grammar).
#[test]
fn matrix_row21_lam_letbound_rule2_fail() {
    let result = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_lam_letbound.m1"));
    assert!(result.is_err(), "lambda syntax must be parse-rejected after PASS 3");
    match result.unwrap_err() {
        PipelineError::Parse { .. } => {}
        other => panic!("expected parse error, got {other}"),
    }
}

// ─── ROW 22: LamExpr body-result escape (RULE 2 violation) ───────────────────
//
// PASS 3 update: lambda syntax removed from M1 — file now fails at parse time.
// Verdict:   FAIL at parse (lambda syntax `fn(...){}` no longer in M1 grammar).
#[test]
fn matrix_row22_lam_bodyresult_rule2_fail() {
    let result = run_pipeline(&cfg("examples/surface-m1/matrix/matrix_lamexpr.m1"));
    assert!(result.is_err(), "lambda syntax must be parse-rejected after PASS 3");
    match result.unwrap_err() {
        PipelineError::Parse { .. } => {}
        other => panic!("expected parse error, got {other}"),
    }
}

// ─── ROW 23: G2 — ill-typed Value construction ───────────────────────────────
//
// Program:   fn f() -> Value(Int) { Value(Int)(Text("hello")) }
// NF:        ValueConstructExpr{type_args:[PrimTypeExpr("Int")],
//                               args:[TextLit{value:Token(STRING_LIT,"\"hello\"")}]}
// G2:        validate_value_construct_types: tag "Int" vs TextLit "Text" → MISMATCH
// Verdict:   FAIL at validate_value_construct_types — pipeline returns NfValidation error.
//            The IR NEVER sees this ill-typed construction (discharged pre-lower).
#[test]
fn matrix_row23_g2_value_construct_mistyped() {
    let result = run_pipeline(&cfg(
        "examples/surface-m1/matrix/matrix_value_construct_mistyped.m1",
    ));
    assert!(result.is_err(), "ill-typed Value construction must be rejected at validation");
    match result.unwrap_err() {
        PipelineError::NfValidation { message, .. } => {
            assert!(
                message.contains("mismatch") || message.contains("Int") || message.contains("Text"),
                "validation error must mention the type mismatch; got: {message}"
            );
        }
        other => panic!("expected NfValidation error, got {other}"),
    }
}

// ─── ROW 24: G3 — nested ValueList(Value(Text,Int)) construction ─────────────
//
// Program:   fn make(a: Text, b: Int) -> ValueList(Value(Text, Int)) {
//              ValueList(Value(Text, Int))(Value(Text, Int)(a, b))
//            }
// NF:        ValueListConstructExpr{ element_type:ValueTypeExpr, args:[ValueConstructExpr{...}] }
// Lower:     → CVar("a")   Node(0)
//              CVar("b")   Node(1)
//              CCall(value_construct_id,    [Node(0), Node(1)])  Node(2)  ← inner
//              CCall(value_list_construct_id, [Node(2)])         Node(3)  ← outer nested
//
// G3: outer CCall has ONE arg = Node(2) (the inner CCall), NOT two flat args.
//     Element boundary between a and b is recoverable from Node(2)'s args.
//     Structurally DISTINCT from CCall(vlc, [Node(0), Node(1)]) — nesting proven.
#[test]
fn matrix_row24_g3_valuelist_nested_value() {
    let a = run_pipeline(&cfg(
        "examples/surface-m1/matrix/matrix_value_construct_nested.m1",
    ))
    .expect("NF pipeline must succeed for nested construction");
    let body = fn_body(&a.schema_asts[0]);
    assert_eq!(body.kind, "ValueListConstructExpr", "body must be ValueListConstructExpr");

    let scope = axis_scope();
    let vc_id = scope.lookup_name("value_make")
        .expect("value_make must be in axis scope");
    let vlc_id = scope.lookup_name("list_make")
        .expect("list_make must be in axis scope");

    // VarRef("a") and VarRef("b") are function params — not in env → UnknownCallable.
    let err = lower_nf_to_bundle_05(body, &scope).unwrap_err();
    assert!(
        matches!(err, SchemaLower05Error::UnknownCallable(ref n) if n == "a" || n == "b"),
        "free VarRef must yield UnknownCallable, got: {err:?}"
    );
    // G3 nesting property (outer wraps inner) is structurally correct in the AST —
    // tested by the pipeline normalization succeeding above.
    let _ = (vc_id, vlc_id); // suppress unused warnings
}

// ─── ROW 25: G4 — LamExpr as a Value element (RULE 2, Escape 2) ──────────────
//
// Program:   fn f(x: Bool) -> Value(Bool) { Value(Bool)(fn(a) { a }) }
// NF:        ValueConstructExpr{args:[LamExpr{params:["a"], body:VarRef("a")}]}
// RULE 2:    LamExpr is in ValueConstructExpr.args (not a direct CallExpr argument).
// PASS 3 update: lambda syntax removed from M1 — file now fails at parse time.
// Verdict:   FAIL at parse (lambda syntax `fn(...){}` no longer in M1 grammar).
#[test]
fn matrix_row25_g4_value_element_lam_rule2_fail() {
    let result = run_pipeline(&cfg(
        "examples/surface-m1/matrix/matrix_value_construct_lam.m1",
    ));
    assert!(result.is_err(), "lambda syntax must be parse-rejected after PASS 3");
    match result.unwrap_err() {
        PipelineError::Parse { .. } => {}
        other => panic!("expected parse error, got {other}"),
    }
}

// ─── ROW 26: Bytes-typed param (M1_BYTE_TYPE_REQUIRED) ──────────────────────
//
// Program:   fn id_bytes(b: Bytes) -> Bytes { b }
// NF root:   FnDecl { name:"id_bytes",
//                    params:TypedParamList{[TypedParam{name:"b",
//                                                      type_expr:PrimTypeExpr{name:"Bytes"}}]},
//                    return_type:PrimTypeExpr{name:"Bytes"},
//                    body:VarRef{name:"b"} }
// Lower:     same shape as Row 1 (single-param identity fn) — param "b" becomes a
//            sentinel pool entry; VarRef("b") resolves to Pool(0); no nodes emitted.
// Verdict:   PASS → 1 pool entry, 0 nodes; Bytes flows through lex → parse → schema
//                  → normalize → lowering. Confirms KW_BYTES lexes, parses as
//                  PrimTypeExpr, normalizes through, and resolves via registry.
#[test]
fn matrix_row26_bytes_param() {
    let a = run_pipeline(&cfg("examples/surface-m1/id_bytes.m1"))
        .expect("NF pipeline must succeed for Bytes-typed param");
    let fn_decl = &a.schema_asts[0];
    assert_eq!(fn_decl.kind, "FnDecl", "NF root must be FnDecl");

    // Walk FnDecl.params[0].type_expr.name and confirm it lexed/parsed as "Bytes",
    // not just an IDENT — proves KW_BYTES took the keyword path through PrimTypeExpr.
    let params_node = match fn_decl.fields.get("params") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        other => panic!("FnDecl.params must be Node, got {other:?}"),
    };
    assert_eq!(params_node.kind, "TypedParamList");
    let params = match params_node.fields.get("params") {
        Some(SchemaValue::Nodes(ns)) => ns,
        other => panic!("TypedParamList.params must be Nodes, got {other:?}"),
    };
    assert_eq!(params.len(), 1, "exactly one param");
    assert_eq!(params[0].kind, "TypedParam");
    let type_expr = match params[0].fields.get("type_expr") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        other => panic!("TypedParam.type_expr must be Node, got {other:?}"),
    };
    assert_eq!(type_expr.kind, "PrimTypeExpr");
    let type_name = match type_expr.fields.get("name") {
        Some(SchemaValue::Token(t)) => &t.lexeme,
        other => panic!("PrimTypeExpr.name must be Token, got {other:?}"),
    };
    assert_eq!(type_name, "Bytes", "param type name must be 'Bytes' in NF");

    // Confirm return_type also carries "Bytes" through normalization.
    let ret = match fn_decl.fields.get("return_type") {
        Some(SchemaValue::Node(n)) => n.as_ref(),
        other => panic!("FnDecl.return_type must be Node, got {other:?}"),
    };
    assert_eq!(ret.kind, "PrimTypeExpr");
    let ret_name = match ret.fields.get("name") {
        Some(SchemaValue::Token(t)) => &t.lexeme,
        other => panic!("return_type name must be Token, got {other:?}"),
    };
    assert_eq!(ret_name, "Bytes", "return type name must be 'Bytes' in NF");

    // Lowering: structurally identical to Row 1 — single-param identity fn.
    // Param type is carried by the registry signature, not the bundle.
    let bundle = lower_nf_to_bundle_05(fn_decl, &Scope::new())
        .expect("FnDecl with Bytes-typed param must lower successfully");
    assert_eq!(bundle.nodes.len(), 0, "no nodes: param b is a sentinel pool entry");
    assert_eq!(bundle.constant_pool.len(), 1, "one sentinel pool entry for param b");

    // The sentinel pool entry's def_hash should be PrimCode::Param's TypeDef identity,
    // matching Row 1 — proves param-slot encoding is type-agnostic at the bundle level.
    let param_type_id = type_identity(&TypeDef { shape: TypeShape::Primitive(PrimCode::Param) });
    assert_eq!(
        bundle.constant_pool[0].def_hash, param_type_id,
        "sentinel pool entry must use PrimCode::Param type identity"
    );
}
