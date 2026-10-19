// Wave 6: Spec-driven lexer tests
// Tests load spec from fixture, lex samples, assert token sequences with spans

use axis_lang::frontend::lexer_engine::lex_with_spec;
use axis_lang::frontend::lexspec_load::load_spec;
use axis_lang::frontend::token::TokenKind;
use axis_lang::native_lex::{lex_file, lex_str};
use std::path::PathBuf;

// Import invariant helper from wave6_invariants module
#[path = "wave6_invariants.rs"]
mod invariants;
use invariants::assert_lexer_invariants;

#[test]
fn test_load_spec_basic() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load basic spec");

    assert_eq!(spec.lexer.charset, "ascii");
    assert!(spec.lexer.case_sensitive);
    assert!(spec.lexer.keywords.contains(&"fn".to_string()));
    assert!(spec.lexer.keywords.contains(&"if".to_string()));
}

#[test]
fn test_load_spec_full() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).expect("should load full spec");

    assert!(spec.lexer.keywords.contains(&"match".to_string()));
    assert!(spec.lexer.punctuation.contains(&"=>".to_string()));
    assert!(spec.lexer.punctuation.contains(&"->".to_string()));
}

#[test]
fn test_lex_simple_function() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "fn test() {}";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // Verify invariants first
    assert_lexer_invariants(&spec, src, &tokens);

    // fn, test, (), {, }, EOF = 6 tokens (note: () is single unit literal token)
    assert_eq!(tokens.len(), 6);

    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "fn"));
    assert_eq!(tokens[0].lexeme, "fn");
    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 2);

    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "test");
    assert_eq!(tokens[1].span.start, 3);
    assert_eq!(tokens[1].span.end, 7);

    assert!(matches!(tokens[2].kind, TokenKind::UnitLit));
    assert_eq!(tokens[2].lexeme, "()");
    assert!(matches!(&tokens[3].kind, TokenKind::Punct(p) if p == "{"));
    assert!(matches!(&tokens[4].kind, TokenKind::Punct(p) if p == "}"));
    assert!(matches!(tokens[5].kind, TokenKind::Eof));
}

#[test]
fn test_lex_with_literals() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "let x = 42;";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // Verify invariants
    assert_lexer_invariants(&spec, src, &tokens);

    // let, x, =, 42, ;, EOF = 6 tokens
    assert_eq!(tokens.len(), 6);

    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "let"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "x");
    assert!(matches!(&tokens[2].kind, TokenKind::Punct(p) if p == "="));
    assert!(matches!(tokens[3].kind, TokenKind::IntLit));
    assert_eq!(tokens[3].lexeme, "42");
}

#[test]
fn test_lex_bool_literals() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "true false";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    assert_lexer_invariants(&spec, src, &tokens);

    assert_eq!(tokens.len(), 3); // true, false, EOF
    assert!(matches!(tokens[0].kind, TokenKind::BoolLit));
    assert_eq!(tokens[0].lexeme, "true");
    assert!(matches!(tokens[1].kind, TokenKind::BoolLit));
    assert_eq!(tokens[1].lexeme, "false");
}

#[test]
fn test_lex_unit_literal() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "()";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    assert_lexer_invariants(&spec, src, &tokens);

    assert_eq!(tokens.len(), 2); // (), EOF
    assert!(matches!(tokens[0].kind, TokenKind::UnitLit));
    assert_eq!(tokens[0].lexeme, "()");
    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 2);
}

#[test]
fn test_lex_operators_longest_match() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "-> =>";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    assert_lexer_invariants(&spec, src, &tokens);

    assert_eq!(tokens.len(), 3); // ->, =>, EOF
    assert!(matches!(&tokens[0].kind, TokenKind::Punct(p) if p == "->"));
    assert_eq!(tokens[0].lexeme, "->");
    assert!(matches!(&tokens[1].kind, TokenKind::Punct(p) if p == "=>"));
    assert_eq!(tokens[1].lexeme, "=>");
}

#[test]
fn test_lex_comments_skipped() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "// comment\nfn // inline\ntest";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // Should have: fn, test, EOF (comments skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "test");
}

#[test]
fn test_lex_whitespace_skipped() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "  fn  \n\t test  ";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // Should have: fn, test, EOF (whitespace skipped)
    assert_eq!(tokens.len(), 3);
}

#[test]
fn test_lex_string_literal() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = r#""hello world""#;
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    assert_lexer_invariants(&spec, src, &tokens);

    assert_eq!(tokens.len(), 2); // string, EOF
    assert!(matches!(tokens[0].kind, TokenKind::StringLit));
    assert_eq!(tokens[0].lexeme, r#""hello world""#);
}

#[test]
fn test_lex_string_with_escapes() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = r#""quote: \" slash: \\""#;
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    assert_eq!(tokens.len(), 2);
    assert!(matches!(tokens[0].kind, TokenKind::StringLit));
    assert_eq!(tokens[0].lexeme, r#""quote: \" slash: \\""#);
}

#[test]
fn test_lex_unterminated_string_error() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = r#""unterminated"#;
    let result = lex_with_spec(&spec, src);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unterminated"));
}

#[test]
fn test_lex_unknown_character_error() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "fn test() { @ }";
    let result = lex_with_spec(&spec, src);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unexpected character"));
    assert!(err.span.is_some());
}

#[test]
fn test_lex_keywords_vs_identifiers() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "fn fnord if ifx let letter";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // fn (kw), fnord (id), if (kw), ifx (id), let (kw), letter (id), EOF
    assert_eq!(tokens.len(), 7);

    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "fnord");

    assert!(matches!(&tokens[2].kind, TokenKind::Keyword(k) if k == "if"));
    assert!(matches!(tokens[3].kind, TokenKind::Ident));
    assert_eq!(tokens[3].lexeme, "ifx");

    assert!(matches!(&tokens[4].kind, TokenKind::Keyword(k) if k == "let"));
    assert!(matches!(tokens[5].kind, TokenKind::Ident));
    assert_eq!(tokens[5].lexeme, "letter");
}

#[test]
fn test_lex_sample_simple() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let sample_path = PathBuf::from("tests/samples/simple.ax");

    let tokens = lex_file(&spec_path, &sample_path).expect("should lex sample");

    // Verify we got tokens (not checking exact count due to complexity)
    assert!(tokens.len() > 5);

    // First token should be 'fn'
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "fn"));
}

#[test]
fn test_lex_sample_operators() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_full.yaml");
    let sample_path = PathBuf::from("tests/samples/operators.ax");

    let tokens = lex_file(&spec_path, &sample_path).expect("should lex sample");

    // Should contain -> and =>
    let has_arrow = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Punct(p) if p == "->"));
    let has_fat_arrow = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Punct(p) if p == "=>"));

    assert!(has_arrow, "should lex ->");
    assert!(has_fat_arrow, "should lex =>");
}

#[test]
fn test_lex_sample_comments() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let sample_path = PathBuf::from("tests/samples/comments.ax");

    let tokens = lex_file(&spec_path, &sample_path).expect("should lex sample");

    // Comments should be skipped - check we have fn, test, etc.
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Keyword(k) if k == "fn")));
    assert!(tokens.iter().any(|t| t.lexeme == "test"));

    // Should NOT have any comment tokens
    assert!(!tokens.iter().any(|t| matches!(t.kind, TokenKind::Comment)));
}

#[test]
fn test_lex_str_api() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let src = "fn main() {}";

    let tokens = lex_str(&spec_path, src).expect("should lex string");

    // fn, main, (), {}, EOF = 6 tokens (note: () is single unit literal token)
    assert_eq!(tokens.len(), 6);
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "fn"));
}

#[test]
fn test_spans_are_contiguous() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "fn test() {}";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // Verify spans make sense (no overlaps, reasonable positions)
    for (i, token) in tokens.iter().enumerate() {
        assert!(
            token.span.start <= token.span.end,
            "token {} has invalid span",
            i
        );
        if i > 0 && !matches!(token.kind, TokenKind::Eof) {
            // Tokens should be at or after previous token end (due to skipped whitespace)
            assert!(
                token.span.start >= tokens[i - 1].span.end,
                "token {} overlaps previous",
                i
            );
        }
    }
}

#[test]
fn test_span_extraction() {
    let spec_path = PathBuf::from("tests/fixtures/wave6_basic.yaml");
    let spec = load_spec(&spec_path).expect("should load spec");

    let src = "fn test() {}";
    let tokens = lex_with_spec(&spec, src).expect("should lex");

    // Extract lexeme from source using span
    for token in &tokens {
        if !matches!(token.kind, TokenKind::Eof) {
            let extracted = &src[token.span.start..token.span.end];
            assert_eq!(extracted, token.lexeme, "span should match lexeme");
        }
    }
}
