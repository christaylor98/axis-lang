// Tests for the canonical user-provided lexer spec
// This is the first milestone config file: lang-lab-poc-userfiles/lexer.yaml

use axis_lang::frontend::lexer_engine::lex_with_spec;
use axis_lang::frontend::lexspec_load::load_spec;
use axis_lang::frontend::token::TokenKind;
use axis_lang::native_lex::lex_str;
use std::path::PathBuf;

#[path = "wave6_invariants.rs"]
mod invariants;
use invariants::assert_lexer_invariants;

const CANONICAL_SPEC: &str = "lang-lab-poc-userfiles/lexer.yaml";

#[test]
fn canonical_spec_loads() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).expect("should load canonical spec");

    // Verify extended keyword set
    assert!(spec.lexer.keywords.contains(&"fn".to_string()));
    assert!(spec.lexer.keywords.contains(&"match".to_string()));
    assert!(spec.lexer.keywords.contains(&"enum".to_string()));
    assert!(spec.lexer.keywords.contains(&"proj".to_string()));

    // Verify string literal support
    assert!(spec.lexer.literals.is_some());
    let lits = spec.lexer.literals.as_ref().unwrap();
    assert!(lits.string.is_some());
}

#[test]
fn canonical_match_keyword() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "match x { }";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // match, x, {, }, EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "match"));
}

#[test]
fn canonical_enum_keyword() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "enum Color { }";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // enum, Color, {, }, EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "enum"));
}

#[test]
fn canonical_proj_keyword() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "proj field";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // proj, field, EOF
    assert_eq!(tokens.len(), 3);
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "proj"));
}

#[test]
fn canonical_arrow_operators() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "-> =>";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // ->, =>, EOF
    assert_eq!(tokens.len(), 3);
    assert!(matches!(&tokens[0].kind, TokenKind::Punct(p) if p == "->"));
    assert!(matches!(&tokens[1].kind, TokenKind::Punct(p) if p == "=>"));
}

#[test]
fn canonical_colon_punctuation() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "x: Int";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // x, :, Int, EOF
    assert_eq!(tokens.len(), 4);
    assert!(matches!(&tokens[1].kind, TokenKind::Punct(p) if p == ":"));
}

#[test]
fn canonical_string_literal_basic() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = r#""hello world""#;
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    assert_eq!(tokens.len(), 2); // string, EOF
    assert!(matches!(tokens[0].kind, TokenKind::StringLit));
    assert_eq!(tokens[0].lexeme, r#""hello world""#);
}

#[test]
fn canonical_string_literal_with_escapes() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    // Test all escape sequences defined in canonical spec
    let test_cases = vec![
        (r#""quote: \"text\"""#, "escaped quote"),
        (r#""backslash: \\path\\""#, "escaped backslash"),
        (r#""newline: \n""#, "escaped newline"),
        (r#""tab: \t""#, "escaped tab"),
    ];

    for (src, description) in test_cases {
        let tokens = lex_with_spec(&spec, src).unwrap();
        assert_lexer_invariants(&spec, src, &tokens);

        assert_eq!(tokens.len(), 2, "Failed for: {}", description);
        assert!(
            matches!(tokens[0].kind, TokenKind::StringLit),
            "Failed for: {}",
            description
        );
        assert_eq!(tokens[0].lexeme, src, "Failed for: {}", description);
    }
}

#[test]
fn canonical_string_forbids_newlines() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "\"unterminated\nstring\"";
    let result = lex_with_spec(&spec, src);

    // Should error because newlines are forbidden
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unterminated") || err.message.contains("newline"));
}

#[test]
fn canonical_complex_expression() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = r#"match x {
        Some(val) => val,
        None => 0
    }"#;

    let tokens = lex_with_spec(&spec, src).unwrap();
    assert_lexer_invariants(&spec, src, &tokens);

    // Verify key tokens are present
    let has_match = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Keyword(k) if k == "match"));
    let arrow_count = tokens
        .iter()
        .filter(|t| matches!(&t.kind, TokenKind::Punct(p) if p == "=>"))
        .count();

    assert!(has_match, "Should contain match keyword");
    assert_eq!(arrow_count, 2, "Should have 2 fat arrows");
}

#[test]
fn canonical_enum_declaration() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "enum Option { Some(Int), None }";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // Should have enum keyword
    assert!(matches!(&tokens[0].kind, TokenKind::Keyword(k) if k == "enum"));
}

#[test]
fn canonical_native_api_lex_str() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let src = "fn test() { match x { } }";

    let tokens = lex_str(&spec_path, src).expect("should lex with native API");

    // Verify we got tokens
    assert!(tokens.len() > 5);

    // Verify both fn and match keywords are present
    let has_fn = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Keyword(k) if k == "fn"));
    let has_match = tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Keyword(k) if k == "match"));

    assert!(has_fn);
    assert!(has_match);
}

#[test]
fn canonical_all_keywords() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = "fn let if else match enum proj";
    let tokens = lex_with_spec(&spec, src).unwrap();

    assert_lexer_invariants(&spec, src, &tokens);

    // All 7 keywords + EOF = 8
    assert_eq!(tokens.len(), 8);

    let keywords: Vec<String> = tokens
        .iter()
        .filter_map(|t| {
            if let TokenKind::Keyword(k) = &t.kind {
                Some(k.clone())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(keywords.len(), 7);
    assert!(keywords.contains(&"fn".to_string()));
    assert!(keywords.contains(&"let".to_string()));
    assert!(keywords.contains(&"if".to_string()));
    assert!(keywords.contains(&"else".to_string()));
    assert!(keywords.contains(&"match".to_string()));
    assert!(keywords.contains(&"enum".to_string()));
    assert!(keywords.contains(&"proj".to_string()));
}

#[test]
fn canonical_determinism() {
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let spec = load_spec(&spec_path).unwrap();

    let src = r#"enum Result { Ok(Int), Err(String) } match result { Ok(x) => x, Err(e) => 0 }"#;

    // Lex same input 5 times
    let runs: Vec<_> = (0..5).map(|_| lex_with_spec(&spec, src).unwrap()).collect();

    // All runs must produce identical results
    for run in &runs[1..] {
        assert_eq!(runs[0].len(), run.len());
        for (i, (t1, t2)) in runs[0].iter().zip(run.iter()).enumerate() {
            assert_eq!(t1.kind, t2.kind, "Token {} kind differs", i);
            assert_eq!(t1.lexeme, t2.lexeme, "Token {} lexeme differs", i);
            assert_eq!(
                t1.span.start, t2.span.start,
                "Token {} span.start differs",
                i
            );
            assert_eq!(t1.span.end, t2.span.end, "Token {} span.end differs", i);
        }
    }
}

#[test]
fn canonical_spec_cli_compatibility() {
    // This test ensures the canonical spec works with the CLI path
    let spec_path = PathBuf::from(CANONICAL_SPEC);
    let src = r#"fn main() { let x = "test"; match x { } }"#;

    let tokens = lex_str(&spec_path, src).expect("CLI path should work with canonical spec");

    assert_lexer_invariants(&load_spec(&spec_path).unwrap(), src, &tokens);

    // Should have fn, let, match keywords
    let keyword_count = tokens
        .iter()
        .filter(|t| matches!(t.kind, TokenKind::Keyword(_)))
        .count();

    assert!(keyword_count >= 3, "Should have at least fn, let, match");
}
