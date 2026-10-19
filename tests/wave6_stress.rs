// Additional stress test for invariants - edge cases and boundary conditions

use axis_lang::frontend::lexer_engine::lex_with_spec;
use axis_lang::frontend::lexspec_load::load_spec;
use std::path::PathBuf;

#[path = "wave6_invariants.rs"]
mod invariants;
use invariants::assert_lexer_invariants;

#[test]
fn minimal_spec_only_identifiers() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_minimal.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "hello world foo bar";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Should have 4 identifiers + EOF
    assert_eq!(tokens.len(), 5);
}

#[test]
fn minimal_spec_rejects_numbers() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_minimal.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "hello 123";
    let result = lex_with_spec(&spec, src);

    // Should error because no int literal rule
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.span.is_some(), "Error must have span");
}

#[test]
fn spec_evolution_add_rule() {
    // Start with minimal spec
    let spec_path = PathBuf::from("tests/fixtures/wave6_minimal.yaml");
    let mut spec = load_spec(&spec_path).unwrap();

    let src = "test true";

    // Initially, 'true' should be an identifier
    let tokens = lex_with_spec(&spec, src).unwrap();
    assert_lexer_invariants(&spec, src, &tokens);
    assert_eq!(tokens.len(), 3); // test, true (ident), EOF

    // Add 'true' as keyword
    spec.lexer.keywords.push("true".to_string());

    // Now 'true' should be a keyword
    let tokens = lex_with_spec(&spec, src).unwrap();
    assert_lexer_invariants(&spec, src, &tokens);
    // Behavior changed as expected
}

#[test]
fn unicode_safety_ascii_only_spec() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    // UTF-8 characters should either error or be handled gracefully
    let src = "fn tëst() {}";
    let result = lex_with_spec(&spec, src);

    // Either succeeds with correct spans or errors cleanly
    match result {
        Ok(tokens) => {
            // If it lexes, invariants must hold
            assert_lexer_invariants(&spec, src, &tokens);
        }
        Err(err) => {
            // Error must be well-formed
            assert!(err.span.is_some());
        }
    }
}

#[test]
fn very_long_input_no_panic() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    // Generate long input
    let mut src = String::new();
    for i in 0..1000 {
        src.push_str(&format!("ident{} ", i));
    }

    let result = lex_with_spec(&spec, &src);

    // Should not panic
    assert!(result.is_ok());
    let tokens = result.unwrap();
    assert_lexer_invariants(&spec, &src, &tokens);
}

#[test]
fn deeply_nested_not_applicable_but_many_tokens() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    // Many operators in sequence
    let src = "-> => -> => -> => -> => -> =>";
    let tokens = lex_with_spec(&spec, &src).unwrap();

    assert_lexer_invariants(&spec, &src, &tokens);

    // Should have 10 operators + EOF = 11
    assert_eq!(tokens.len(), 11);
}

#[test]
fn consecutive_string_literals() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = r#""first" "second" "third""#;
    let tokens = lex_with_spec(&spec, &src).unwrap();

    assert_lexer_invariants(&spec, &src, &tokens);

    // 3 strings + EOF = 4
    assert_eq!(tokens.len(), 4);
}
