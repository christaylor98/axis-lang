// EQUIVALENCE TESTS: Runtime spec vs Generated code
// These tests MUST pass to prove codegen correctness
// Source of truth: YAML spec file
// Generated code: src/generated_lexer.rs

use axis_lang::frontend::lexer_engine::lex_with_spec;
use axis_lang::frontend::lexspec_load::load_spec;
use axis_lang::generated_lexer;
use std::path::PathBuf;

const CANONICAL_SPEC: &str = "lang-lab-poc-userfiles/lexer.yaml";

/// Core equivalence test: same input MUST produce identical tokens
fn assert_equivalent(src: &str) {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    // Runtime spec-driven lexer
    let runtime_tokens = lex_with_spec(&spec, src).expect("runtime lexer should succeed");

    // Generated static lexer
    let generated_tokens = generated_lexer::lex(src).expect("generated lexer should succeed");

    // MUST be identical
    assert_eq!(
        runtime_tokens.len(),
        generated_tokens.len(),
        "Token count mismatch for input: {:?}",
        src
    );

    for (i, (rt, gt)) in runtime_tokens
        .iter()
        .zip(generated_tokens.iter())
        .enumerate()
    {
        assert_eq!(
            rt.kind, gt.kind,
            "Token {} kind mismatch for input: {:?}\nRuntime: {:?}\nGenerated: {:?}",
            i, src, rt.kind, gt.kind
        );
        assert_eq!(
            rt.lexeme, gt.lexeme,
            "Token {} lexeme mismatch for input: {:?}",
            i, src
        );
        assert_eq!(
            rt.span.start, gt.span.start,
            "Token {} span.start mismatch for input: {:?}",
            i, src
        );
        assert_eq!(
            rt.span.end, gt.span.end,
            "Token {} span.end mismatch for input: {:?}",
            i, src
        );
    }
}

#[test]
fn equiv_empty_source() {
    assert_equivalent("");
}

#[test]
fn equiv_single_keyword() {
    assert_equivalent("fn");
    assert_equivalent("match");
    assert_equivalent("enum");
    assert_equivalent("proj");
}

#[test]
fn equiv_all_keywords() {
    assert_equivalent("fn let if else match enum proj");
}

#[test]
fn equiv_identifiers() {
    assert_equivalent("hello world foo_bar test123");
}

#[test]
fn equiv_int_literals() {
    assert_equivalent("0 1 42 999 1234567890");
}

#[test]
fn equiv_bool_literals() {
    assert_equivalent("true false");
}

#[test]
fn equiv_unit_literal() {
    assert_equivalent("()");
}

#[test]
fn equiv_string_literals() {
    assert_equivalent(r#""hello" "world""#);
}

#[test]
fn equiv_string_with_escapes() {
    assert_equivalent(r#""quote: \" slash: \\ newline: \n tab: \t""#);
}

#[test]
fn equiv_punctuation() {
    assert_equivalent("{ } ( ) , : ; =");
}

#[test]
fn equiv_arrow_operators() {
    assert_equivalent("-> =>");
}

#[test]
fn equiv_simple_function() {
    assert_equivalent("fn test() { }");
}

#[test]
fn equiv_function_with_body() {
    assert_equivalent("fn test() { let x = 42; }");
}

#[test]
fn equiv_if_expression() {
    assert_equivalent("if true { () } else { () }");
}

#[test]
fn equiv_match_expression() {
    assert_equivalent(
        r#"match x {
        Some(val) => val,
        None => 0
    }"#,
    );
}

#[test]
fn equiv_enum_declaration() {
    assert_equivalent("enum Option { Some(Int), None }");
}

#[test]
fn equiv_complex_program() {
    let src = r#"fn main() {
        let x = 42;
        let msg = "test";
        if true {
            match x {
                0 => (),
                _ => ()
            }
        } else {
            ()
        }
    }"#;
    assert_equivalent(src);
}

#[test]
fn equiv_comments() {
    assert_equivalent("// comment\nfn test() {}");
}

#[test]
fn equiv_whitespace_variations() {
    assert_equivalent("fn  test  (  )  {  }");
    assert_equivalent("fn\ttest\t(\t)\t{\t}");
    assert_equivalent("fn\ntest\n(\n)\n{\n}");
}

#[test]
fn equiv_proj_keyword() {
    assert_equivalent("proj field");
}

#[test]
fn equiv_type_annotation() {
    assert_equivalent("x: Int");
}

#[test]
fn equiv_error_cases_must_match() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let error_inputs = vec![
        "@",                // Unknown character
        r#""unterminated"#, // Unterminated string
        "123abc",           // Invalid token sequence
    ];

    for src in error_inputs {
        let runtime_result = lex_with_spec(&spec, src);
        let generated_result = generated_lexer::lex(src);

        // Both must succeed or both must fail
        assert_eq!(
            runtime_result.is_ok(),
            generated_result.is_ok(),
            "Error behavior mismatch for input: {:?}",
            src
        );
    }
}

#[test]
fn equiv_determinism() {
    let src = "fn test() { match x { } }";

    // Run multiple times, all must be identical
    for _ in 0..5 {
        assert_equivalent(src);
    }
}

#[test]
fn equiv_all_canonical_samples() {
    // Test against all samples used in canonical spec tests
    let samples = vec![
        "fn test() { }",
        "enum Color { }",
        "match x { }",
        "proj field",
        r#""hello world""#,
        "-> =>",
        "x: Int",
        "fn let if else match enum proj",
        r#"enum Result { Ok(Int), Err(String) }"#,
    ];

    for src in samples {
        assert_equivalent(src);
    }
}

#[test]
fn equiv_stress_long_input() {
    let mut src = String::new();
    for i in 0..100 {
        src.push_str(&format!("ident{} ", i));
    }
    assert_equivalent(&src);
}

#[test]
fn equiv_deeply_nested() {
    let src = "match match match x { } { } { }";
    assert_equivalent(src);
}
