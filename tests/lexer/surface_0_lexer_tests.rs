// Surface-0 Lexer Tests - Verify spec compliance

use axis_lang::frontend::{lexer_engine, lexspec_load, token::TokenKind};
use std::path::PathBuf;

fn load_surface_0_spec() -> axis_lang::frontend::lexspec::LexerSpec {
    lexspec_load::load_spec(&PathBuf::from("axis-surface-0-config/surface-0-lexer.yaml"))
        .expect("load surface-0 lexer spec")
}

#[test]
fn surface_0_lex_keywords() {
    let spec = load_surface_0_spec();
    let src = "fn end arg lit call";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 5 keywords + EOF
    assert_eq!(tokens.len(), 6);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "end"));
    assert!(matches!(tokens[2].kind, TokenKind::Keyword(ref k) if k == "arg"));
    assert!(matches!(tokens[3].kind, TokenKind::Keyword(ref k) if k == "lit"));
    assert!(matches!(tokens[4].kind, TokenKind::Keyword(ref k) if k == "call"));
}

#[test]
fn surface_0_lex_identifiers() {
    let spec = load_surface_0_spec();
    let src = "demo.compute core.math.mul foo_bar";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 3 identifiers + EOF
    assert_eq!(tokens.len(), 4);
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
    assert_eq!(tokens[0].lexeme, "demo.compute");
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "core.math.mul");
    assert!(matches!(tokens[2].kind, TokenKind::Ident));
    assert_eq!(tokens[2].lexeme, "foo_bar");
}

#[test]
fn surface_0_lex_int_literals() {
    let spec = load_surface_0_spec();
    let src = "0 1 42 999";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 int literals + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::IntLit));
    assert_eq!(tokens[0].lexeme, "0");
    assert!(matches!(tokens[1].kind, TokenKind::IntLit));
    assert_eq!(tokens[1].lexeme, "1");
    assert!(matches!(tokens[2].kind, TokenKind::IntLit));
    assert_eq!(tokens[2].lexeme, "42");
    assert!(matches!(tokens[3].kind, TokenKind::IntLit));
    assert_eq!(tokens[3].lexeme, "999");
}

#[test]
fn surface_0_lex_whitespace_skip() {
    let spec = load_surface_0_spec();
    let src = "fn  \t\n  arg";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 keywords + EOF (whitespace skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "arg"));
}

#[test]
fn surface_0_lex_unknown_char_error() {
    let spec = load_surface_0_spec();
    let src = "fn @";
    let result = lexer_engine::lex_with_spec(&spec, src);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unexpected character"));
}

#[test]
fn surface_0_lex_preserves_spans() {
    let spec = load_surface_0_spec();
    let src = "fn demo";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 2);
    assert_eq!(tokens[1].span.start, 3);
    assert_eq!(tokens[1].span.end, 7);
}

#[test]
fn surface_0_lex_real_example() {
    let spec = load_surface_0_spec();
    let src = r#"fn demo.compute
arg 1
arg 2
call core.math.mul
end"#;
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // fn, demo.compute, arg, 1, arg, 2, call, core.math.mul, end, EOF = 10
    assert_eq!(tokens.len(), 10);
}
