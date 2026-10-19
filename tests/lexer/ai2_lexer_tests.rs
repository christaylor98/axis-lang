// AI-2 (Explicit Prefix) Lexer Tests - Verify spec compliance

use axis_lang::frontend::{lexer_engine, lexspec_load, token::TokenKind};
use std::path::PathBuf;

fn load_ai2_spec() -> axis_lang::frontend::lexspec::LexerSpec {
    lexspec_load::load_spec(&PathBuf::from(
        "axis-surface-ai2-config/ai2-explicit-lexer.yaml",
    ))
    .expect("load ai2 lexer spec")
}

#[test]
fn ai2_lex_keywords() {
    let spec = load_ai2_spec();
    let src = "let lam app if var int bool unit true false";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 10 keywords + EOF
    assert_eq!(tokens.len(), 11);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "let"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "lam"));
    assert!(matches!(tokens[2].kind, TokenKind::Keyword(ref k) if k == "app"));
    assert!(matches!(tokens[3].kind, TokenKind::Keyword(ref k) if k == "if"));
    assert!(matches!(tokens[4].kind, TokenKind::Keyword(ref k) if k == "var"));
}

#[test]
fn ai2_lex_identifiers() {
    let spec = load_ai2_spec();
    let src = "x foo_bar CamelCase";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 3 identifiers + EOF
    assert_eq!(tokens.len(), 4);
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
    assert_eq!(tokens[0].lexeme, "x");
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "foo_bar");
    assert!(matches!(tokens[2].kind, TokenKind::Ident));
    assert_eq!(tokens[2].lexeme, "CamelCase");
}

#[test]
fn ai2_lex_int_literals() {
    let spec = load_ai2_spec();
    let src = "0 42 -1 -999";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 int literals + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::IntLit));
    assert_eq!(tokens[0].lexeme, "0");
    assert!(matches!(tokens[1].kind, TokenKind::IntLit));
    assert_eq!(tokens[1].lexeme, "42");
    assert!(matches!(tokens[2].kind, TokenKind::IntLit));
    assert_eq!(tokens[2].lexeme, "-1");
    assert!(matches!(tokens[3].kind, TokenKind::IntLit));
    assert_eq!(tokens[3].lexeme, "-999");
}

#[test]
fn ai2_lex_punctuation() {
    let spec = load_ai2_spec();
    let src = "( ) , =";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 punctuation + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::Punct(ref p) if p == "("));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == ")"));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == ","));
    assert!(matches!(tokens[3].kind, TokenKind::Punct(ref p) if p == "="));
}

#[test]
fn ai2_lex_comments_skip() {
    let spec = load_ai2_spec();
    let src = "int // comment\n(";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 tokens + EOF (comment skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "int"));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == "("));
}

#[test]
fn ai2_lex_keyword_vs_ident() {
    let spec = load_ai2_spec();
    let src = "int integer";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // int is keyword, integer is identifier
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "int"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "integer");
}

#[test]
fn ai2_lex_real_example() {
    let spec = load_ai2_spec();
    let src = "int(value = 42)";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // int, (, value, =, 42, ), EOF = 7
    assert_eq!(tokens.len(), 7);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "int"));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == "("));
    assert!(matches!(tokens[2].kind, TokenKind::Ident));
    assert!(matches!(tokens[3].kind, TokenKind::Punct(ref p) if p == "="));
    assert!(matches!(tokens[4].kind, TokenKind::IntLit));
    assert!(matches!(tokens[5].kind, TokenKind::Punct(ref p) if p == ")"));
}

#[test]
fn ai2_lex_unknown_char_error() {
    let spec = load_ai2_spec();
    let src = "int @";
    let result = lexer_engine::lex_with_spec(&spec, src);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unexpected character"));
}
