// Semantic-Surface-0 Lexer Tests - Verify spec compliance

use axis_lang::frontend::{lexer_engine, lexspec_load, token::TokenKind};
use std::path::PathBuf;

fn load_semantic_s0_spec() -> axis_lang::frontend::lexspec::LexerSpec {
    lexspec_load::load_spec(&PathBuf::from(
        "axis-surface-0-config/semantic-surface-0-lexer.yaml",
    ))
    .expect("load semantic-surface-0 lexer spec")
}

#[test]
fn semantic_s0_lex_keywords() {
    let spec = load_semantic_s0_spec();
    let src = "let in fn if then else";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 6 keywords + EOF
    assert_eq!(tokens.len(), 7);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "let"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "in"));
    assert!(matches!(tokens[2].kind, TokenKind::Keyword(ref k) if k == "fn"));
    assert!(matches!(tokens[3].kind, TokenKind::Keyword(ref k) if k == "if"));
    assert!(matches!(tokens[4].kind, TokenKind::Keyword(ref k) if k == "then"));
    assert!(matches!(tokens[5].kind, TokenKind::Keyword(ref k) if k == "else"));
}

#[test]
fn semantic_s0_lex_identifiers() {
    let spec = load_semantic_s0_spec();
    let src = "foo bar_baz _underscore CamelCase";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 identifiers + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
    assert_eq!(tokens[0].lexeme, "foo");
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "bar_baz");
    assert!(matches!(tokens[2].kind, TokenKind::Ident));
    assert_eq!(tokens[2].lexeme, "_underscore");
    assert!(matches!(tokens[3].kind, TokenKind::Ident));
    assert_eq!(tokens[3].lexeme, "CamelCase");
}

#[test]
fn semantic_s0_lex_int_literals() {
    let spec = load_semantic_s0_spec();
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
fn semantic_s0_lex_bool_literals() {
    let spec = load_semantic_s0_spec();
    let src = "true false";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 bool literals + EOF
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::BoolLit));
    assert_eq!(tokens[0].lexeme, "true");
    assert!(matches!(tokens[1].kind, TokenKind::BoolLit));
    assert_eq!(tokens[1].lexeme, "false");
}

#[test]
fn semantic_s0_lex_unit_literal() {
    let spec = load_semantic_s0_spec();
    let src = "()";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 1 unit literal + EOF
    assert_eq!(tokens.len(), 2);
    assert!(matches!(tokens[0].kind, TokenKind::UnitLit));
    assert_eq!(tokens[0].lexeme, "()");
}

#[test]
fn semantic_s0_lex_punctuation() {
    let spec = load_semantic_s0_spec();
    let src = "( ) = =>";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 punctuation + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::Punct(ref p) if p == "("));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == ")"));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == "="));
    assert!(matches!(tokens[3].kind, TokenKind::Punct(ref p) if p == "=>"));
}

#[test]
fn semantic_s0_lex_comments_skip() {
    let spec = load_semantic_s0_spec();
    let src = "let // this is a comment\nx";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 tokens + EOF (comment skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "let"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "x");
}

#[test]
fn semantic_s0_lex_longest_match_punct() {
    let spec = load_semantic_s0_spec();
    let src = "==>==";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // Current implementation matches left-to-right with longest-first attempt
    // Input "==>=="
    // Position 0: tries "=>", doesn't match (next char is '=')
    //             tries "=", matches
    // Position 1: tries "=>", matches
    // Position 3: tries "=>", doesn't match (only one char left)
    //             tries "=", matches
    // Position 4: tries "=>", doesn't match (only one char left)
    //             tries "=", matches
    // Result: =, =>, =, = + EOF = 5 tokens
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::Punct(ref p) if p == "="));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == "=>"));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == "="));
    assert!(matches!(tokens[3].kind, TokenKind::Punct(ref p) if p == "="));
}

#[test]
fn semantic_s0_lex_keyword_vs_ident() {
    let spec = load_semantic_s0_spec();
    let src = "let letter";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // let is keyword, letter is identifier
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "let"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "letter");
}

#[test]
fn semantic_s0_lex_unit_vs_parens() {
    let spec = load_semantic_s0_spec();
    let src = "() ( )";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // () is unit literal, ( ) are separate parens
    assert_eq!(tokens.len(), 4);
    assert!(matches!(tokens[0].kind, TokenKind::UnitLit));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == "("));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == ")"));
}

#[test]
fn semantic_s0_lex_unknown_char_error() {
    let spec = load_semantic_s0_spec();
    let src = "let @";
    let result = lexer_engine::lex_with_spec(&spec, src);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unexpected character"));
    assert!(err.span.is_some());
}
