// Invariant-based validation for spec-driven lexer
// These tests prove correctness properties that MUST hold for all inputs

use axis_lang::frontend::lexer_engine::lex_with_spec;
use axis_lang::frontend::lexspec::LexerSpec;
use axis_lang::frontend::lexspec_load::load_spec;
use axis_lang::frontend::token::{Token, TokenKind};
use std::path::PathBuf;

/// Core invariant checker - MUST pass for all (spec, src, tokens) triples
pub fn assert_lexer_invariants(_spec: &LexerSpec, src: &str, tokens: &[Token]) {
    // I3: EOF Invariant - exactly one EOF, must be last
    let eof_count = tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::Eof))
        .count();
    assert_eq!(
        eof_count, 1,
        "INVARIANT VIOLATION I3: Must have exactly one EOF token, found {}",
        eof_count
    );

    let last_token = tokens.last().expect("token stream cannot be empty");
    assert!(
        matches!(last_token.kind, TokenKind::Eof),
        "INVARIANT VIOLATION I3: EOF must be the final token, found {:?}",
        last_token.kind
    );

    assert_eq!(
        last_token.span.start,
        src.len(),
        "INVARIANT VIOLATION I3: EOF span.start must equal src.len()"
    );
    assert_eq!(
        last_token.span.end,
        src.len(),
        "INVARIANT VIOLATION I3: EOF span.end must equal src.len()"
    );

    // Iterate through all tokens for remaining invariants
    for (i, token) in tokens.iter().enumerate() {
        // I1: Span Truth Invariant - span must match lexeme
        if !matches!(token.kind, TokenKind::Eof) {
            assert!(
                token.span.start <= token.span.end,
                "INVARIANT VIOLATION I1: Token {} has invalid span: start={} > end={}",
                i,
                token.span.start,
                token.span.end
            );

            assert!(
                token.span.end <= src.len(),
                "INVARIANT VIOLATION I1: Token {} span.end={} exceeds src.len()={}",
                i,
                token.span.end,
                src.len()
            );

            let extracted = &src[token.span.start..token.span.end];
            assert_eq!(extracted, token.lexeme,
                "INVARIANT VIOLATION I1: Token {} span doesn't match lexeme. Span extracts '{}' but lexeme is '{}'",
                i, extracted, token.lexeme);
        }

        // I2: Monotonic Span Invariant - no overlaps, no backwards movement
        if i > 0 {
            let prev = &tokens[i - 1];
            assert!(prev.span.end <= token.span.start,
                "INVARIANT VIOLATION I2: Token {} span.start={} comes before previous token {} span.end={}",
                i, token.span.start, i - 1, prev.span.end);
        }
    }
}

#[test]
fn invariant_i1_span_truth() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn test() { let x = 42; }";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);
}

#[test]
fn invariant_i2_monotonic_spans() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "a b c d e";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Explicitly verify monotonicity
    for i in 1..tokens.len() {
        assert!(
            tokens[i - 1].span.end <= tokens[i].span.start,
            "Spans not monotonic at index {}",
            i
        );
    }
}

#[test]
fn invariant_i3_eof_properties() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn test() {}";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);
}

#[test]
fn invariant_i4_determinism() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn test() { if true { () } else { () } }";

    // Lex the same input 5 times
    let runs: Vec<Vec<Token>> = (0..5).map(|_| lex_with_spec(&spec, src).unwrap()).collect();

    // All runs must produce identical tokens
    for run_idx in 1..runs.len() {
        assert_eq!(
            runs[0].len(),
            runs[run_idx].len(),
            "Run {} has different token count",
            run_idx
        );

        for (i, (t1, t2)) in runs[0].iter().zip(runs[run_idx].iter()).enumerate() {
            assert_eq!(
                t1.kind, t2.kind,
                "INVARIANT VIOLATION I4: Token {} kind differs between runs",
                i
            );
            assert_eq!(
                t1.lexeme, t2.lexeme,
                "INVARIANT VIOLATION I4: Token {} lexeme differs between runs",
                i
            );
            assert_eq!(
                t1.span.start, t2.span.start,
                "INVARIANT VIOLATION I4: Token {} span.start differs between runs",
                i
            );
            assert_eq!(
                t1.span.end, t2.span.end,
                "INVARIANT VIOLATION I4: Token {} span.end differs between runs",
                i
            );
        }
    }
}

#[test]
fn invariant_i5_spec_authority_remove_keyword() {
    // Load original spec
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let mut spec = load_spec(&spec_path).unwrap();

    let src = "fn test";
    let tokens_original = lex_with_spec(&spec, src).unwrap();

    // Verify 'fn' is a keyword
    assert!(matches!(&tokens_original[0].kind, TokenKind::Keyword(k) if k == "fn"));

    // Mutate spec: remove 'fn' from keywords
    spec.lexer.keywords.retain(|k| k != "fn");

    let tokens_mutated = lex_with_spec(&spec, src).unwrap();

    // Behavior MUST change - 'fn' should now be an identifier
    assert!(
        matches!(tokens_mutated[0].kind, TokenKind::Ident),
        "INVARIANT VIOLATION I5: Removing 'fn' from keywords didn't change lexer behavior"
    );
}

#[test]
fn invariant_i5_spec_authority_remove_punctuation() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let mut spec = load_spec(&spec_path).unwrap();

    let src = "test,test";
    let tokens_original = lex_with_spec(&spec, src).unwrap();

    // Should have: ident, comma, ident, EOF
    assert_eq!(tokens_original.len(), 4);
    assert!(matches!(&tokens_original[1].kind, TokenKind::Punct(p) if p == ","));

    // Mutate spec: remove comma
    spec.lexer.punctuation.retain(|p| p != ",");

    let result = lex_with_spec(&spec, src);

    // Behavior MUST change - should error on unknown character
    assert!(
        result.is_err(),
        "INVARIANT VIOLATION I5: Removing comma from punctuation didn't change lexer behavior"
    );
}

#[test]
fn invariant_i5_spec_authority_disable_whitespace_skip() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let mut spec = load_spec(&spec_path).unwrap();

    let src = "a b";
    let tokens_original = lex_with_spec(&spec, src).unwrap();

    // Should skip whitespace
    assert_eq!(tokens_original.len(), 3); // a, b, EOF

    // Mutate spec: remove whitespace rule
    spec.lexer.whitespace = None;

    let result = lex_with_spec(&spec, src);

    // Should error on space character
    assert!(
        result.is_err(),
        "INVARIANT VIOLATION I5: Removing whitespace rule didn't change lexer behavior"
    );
}

#[test]
fn invariant_i6_longest_match_operators() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    // The spec has both "->" and individual chars
    let src = "->";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Should lex as single "->" token, not separate
    assert_eq!(tokens.len(), 2); // ->, EOF
    assert!(
        matches!(&tokens[0].kind, TokenKind::Punct(p) if p == "->"),
        "INVARIANT VIOLATION I6: Did not choose longest match for '->'"
    );
}

#[test]
fn invariant_i6_longest_match_fat_arrow() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "=>";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Should lex as single "=>" token
    assert_eq!(tokens.len(), 2); // =>, EOF
    assert!(
        matches!(&tokens[0].kind, TokenKind::Punct(p) if p == "=>"),
        "INVARIANT VIOLATION I6: Did not choose longest match for '=>'"
    );
}

#[test]
fn invariant_i7_keyword_boundary_start() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let test_cases = vec![
        ("fn", true),     // Just keyword
        ("fnord", false), // Keyword as prefix
        ("fn_", false),   // Keyword with underscore
    ];

    for (src, should_be_keyword) in test_cases {
        let tokens = lex_with_spec(&spec, src).unwrap();
        assert_lexer_invariants(&spec, src, &tokens);

        let is_keyword = matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "fn");
        assert_eq!(
            is_keyword, should_be_keyword,
            "INVARIANT VIOLATION I7: Keyword boundary failed for '{}'",
            src
        );
    }
}

#[test]
fn invariant_i7_keyword_boundary_embedded() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "define"; // Contains 'if' but should be identifier
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    assert!(
        matches!(tokens[0].kind, TokenKind::Ident),
        "INVARIANT VIOLATION I7: 'define' should be identifier, not keyword"
    );
}

#[test]
fn invariant_i8_error_span_unknown_char() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn test @";
    let result = lex_with_spec(&spec, src);

    assert!(result.is_err(), "Should error on '@'");

    let err = result.unwrap_err();
    assert!(
        err.span.is_some(),
        "INVARIANT VIOLATION I8: Error must have span"
    );

    let span = err.span.unwrap();
    assert!(
        span.start < src.len(),
        "INVARIANT VIOLATION I8: Error span.start out of bounds"
    );
    assert!(
        span.end <= src.len(),
        "INVARIANT VIOLATION I8: Error span.end out of bounds"
    );

    // Span should point to the '@' character
    assert_eq!(
        &src[span.start..span.end],
        "@",
        "INVARIANT VIOLATION I8: Error span doesn't point to offending character"
    );
}

#[test]
fn invariant_i8_error_span_unterminated_string() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = r#""unterminated"#;
    let result = lex_with_spec(&spec, src);

    assert!(result.is_err(), "Should error on unterminated string");

    let err = result.unwrap_err();
    assert!(
        err.span.is_some(),
        "INVARIANT VIOLATION I8: Error must have span"
    );

    let span = err.span.unwrap();
    assert!(
        span.start <= src.len(),
        "INVARIANT VIOLATION I8: Error span out of bounds"
    );
}

#[test]
fn garbage_input_random_ascii() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    // Various garbage inputs - lexer must not panic
    let garbage_inputs = vec![
        "",
        " ",
        "!!!",
        "@@@@@@",
        "fn fn fn fn fn",
        "() () () ()",
        "123abc456def",
        "___",
        "if if if",
    ];

    for src in garbage_inputs {
        let result = lex_with_spec(&spec, src);

        match result {
            Ok(tokens) => {
                // If it succeeds, invariants MUST hold
                assert_lexer_invariants(&spec, src, &tokens);
            }
            Err(err) => {
                // If it errors, error span MUST be valid
                if let Some(span) = err.span {
                    assert!(
                        span.start <= span.end,
                        "Error span invalid for input '{}'",
                        src
                    );
                    assert!(
                        span.end <= src.len(),
                        "Error span out of bounds for input '{}'",
                        src
                    );
                }
            }
        }
    }
}

#[test]
fn garbage_input_edge_cases() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let edge_cases = vec![
        ("", "empty string"),
        ("   ", "only whitespace"),
        ("\n\n\n", "only newlines"),
        ("fn", "single keyword"),
        ("(", "single punct - incomplete unit literal"),
    ];

    for (src, description) in edge_cases {
        let result = lex_with_spec(&spec, src);

        match result {
            Ok(tokens) => {
                assert_lexer_invariants(&spec, src, &tokens);
            }
            Err(err) => {
                // Ensure error is well-formed
                assert!(
                    !err.message.is_empty(),
                    "Error message empty for case: {}",
                    description
                );
            }
        }
    }
}

#[test]
fn cli_consistency_same_code_path() {
    use axis_lang::native_lex::lex_str;

    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn test() { let x = 42; }";

    // Direct API call (used by tests)
    let tokens_direct = lex_with_spec(&spec, src).unwrap();

    // Native API call (used by CLI)
    let tokens_cli = lex_str(&spec_path, src).unwrap();

    // Token streams MUST be identical
    assert_eq!(
        tokens_direct.len(),
        tokens_cli.len(),
        "CLI and direct API produce different token counts"
    );

    for (i, (t1, t2)) in tokens_direct.iter().zip(tokens_cli.iter()).enumerate() {
        assert_eq!(
            t1.kind, t2.kind,
            "Token {} kind differs: CLI vs direct API",
            i
        );
        assert_eq!(
            t1.lexeme, t2.lexeme,
            "Token {} lexeme differs: CLI vs direct API",
            i
        );
        assert_eq!(
            t1.span.start, t2.span.start,
            "Token {} span.start differs: CLI vs direct API",
            i
        );
        assert_eq!(
            t1.span.end, t2.span.end,
            "Token {} span.end differs: CLI vs direct API",
            i
        );
    }

    // Both must satisfy invariants
    assert_lexer_invariants(&spec, src, &tokens_direct);
    assert_lexer_invariants(&spec, src, &tokens_cli);
}

#[test]
fn all_existing_samples_satisfy_invariants() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let samples = vec![
        ("tests/samples/simple.ax", include_str!("samples/simple.ax")),
        (
            "tests/samples/operators.ax",
            include_str!("samples/operators.ax"),
        ),
        (
            "tests/samples/comments.ax",
            include_str!("samples/comments.ax"),
        ),
        (
            "tests/samples/strings.ax",
            include_str!("samples/strings.ax"),
        ),
        (
            "tests/samples/keywords.ax",
            include_str!("samples/keywords.ax"),
        ),
    ];

    for (name, src) in samples {
        let tokens =
            lex_with_spec(&spec, src).unwrap_or_else(|e| panic!("Failed to lex {}: {}", name, e));

        assert_lexer_invariants(&spec, src, &tokens);
    }
}

#[test]
fn spec_mutation_reorder_operators() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let mut spec = load_spec(&spec_path).unwrap();

    let src = "-> =>";
    let tokens_original = lex_with_spec(&spec, src).unwrap();

    // Original should have 3 tokens: ->, =>, EOF
    assert_eq!(tokens_original.len(), 3);

    // Reorder punctuation (shortest first - breaks longest match)
    spec.lexer.punctuation.sort_by(|a, b| a.len().cmp(&b.len()));

    let tokens_reordered = lex_with_spec(&spec, src);

    // Behavior might change if internal sorting doesn't compensate
    // At minimum, invariants must still hold if it succeeds
    if let Ok(tokens) = tokens_reordered {
        assert_lexer_invariants(&spec, src, &tokens);
    }
}

#[test]
fn whitespace_preservation_in_spans() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn   test"; // Multiple spaces
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Tokens should not include whitespace in their lexemes
    assert_eq!(tokens[0].lexeme, "fn");
    assert_eq!(tokens[1].lexeme, "test");

    // But spans should account for the gap
    assert!(
        tokens[1].span.start > tokens[0].span.end,
        "Whitespace gap should be reflected in spans"
    );
}

#[test]
fn empty_source_invariants() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).unwrap();

    let src = "";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Should have exactly one EOF token
    assert_eq!(tokens.len(), 1);
    assert!(matches!(tokens[0].kind, TokenKind::Eof));
}
