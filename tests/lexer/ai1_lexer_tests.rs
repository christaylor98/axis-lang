// AI-1 (RPN) Lexer Tests - Verify spec compliance

use axis_lang::frontend::{lexer_engine, lexspec_load, token::TokenKind};
use std::path::PathBuf;

fn load_ai1_spec() -> axis_lang::frontend::lexspec::LexerSpec {
    lexspec_load::load_spec(&PathBuf::from("axis-surface-ai1-config/ai1-rpn-lexer.yaml"))
        .expect("load ai1 lexer spec")
}

#[test]
fn ai1_lex_keywords() {
    let spec = load_ai1_spec();
    let src = "app lam let if true false unit";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 7 keywords + EOF
    assert_eq!(tokens.len(), 8);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "app"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "lam"));
    assert!(matches!(tokens[2].kind, TokenKind::Keyword(ref k) if k == "let"));
    assert!(matches!(tokens[3].kind, TokenKind::Keyword(ref k) if k == "if"));
    assert!(matches!(tokens[4].kind, TokenKind::Keyword(ref k) if k == "true"));
    assert!(matches!(tokens[5].kind, TokenKind::Keyword(ref k) if k == "false"));
    assert!(matches!(tokens[6].kind, TokenKind::Keyword(ref k) if k == "unit"));
}

#[test]
fn ai1_lex_identifiers() {
    let spec = load_ai1_spec();
    let src = "x foo_bar CamelCase _under";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 identifiers + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
    assert_eq!(tokens[0].lexeme, "x");
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "foo_bar");
    assert!(matches!(tokens[2].kind, TokenKind::Ident));
    assert_eq!(tokens[2].lexeme, "CamelCase");
    assert!(matches!(tokens[3].kind, TokenKind::Ident));
    assert_eq!(tokens[3].lexeme, "_under");
}

#[test]
fn ai1_lex_int_literals() {
    let spec = load_ai1_spec();
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
fn ai1_lex_no_punctuation() {
    let spec = load_ai1_spec();
    let src = "42 app";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // AI-1 has NO punctuation - should lex cleanly
    assert_eq!(tokens.len(), 3);
}

#[test]
fn ai1_lex_rejects_parens() {
    let spec = load_ai1_spec();
    let src = "42 (";
    let result = lexer_engine::lex_with_spec(&spec, src);

    // Parentheses are not allowed in AI-1
    assert!(result.is_err());
}

#[test]
fn ai1_lex_comments_skip() {
    let spec = load_ai1_spec();
    let src = "42 // comment\napp";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 tokens + EOF (comment skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::IntLit));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "app"));
}

#[test]
fn ai1_lex_bool_as_keyword() {
    let spec = load_ai1_spec();
    let src = "true false";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // In AI-1, true/false are keywords
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "true"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "false"));
}

#[test]
fn ai1_lex_whitespace_separated() {
    let spec = load_ai1_spec();
    let src = "1 2 app x lam";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // AI-1 is purely whitespace-separated
    assert_eq!(tokens.len(), 6);
}
