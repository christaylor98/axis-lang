// Surface-H1 Lexer Tests - Verify spec compliance

use axis_lang::frontend::{lexer_engine, lexspec_load, token::TokenKind};
use std::path::PathBuf;

fn load_h1_spec() -> axis_lang::frontend::lexspec::LexerSpec {
    lexspec_load::load_spec(&PathBuf::from(
        "axis-surface-h1-config/surface-h1-lexer.yaml",
    ))
    .expect("load h1 lexer spec")
}

#[test]
fn h1_lex_keywords() {
    let spec = load_h1_spec();
    // H1 has no `let`; type constructors are keywords; true/false/unit are keywords.
    let src = "fn if else Int Text Bool Unit Value List true false unit";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 12 keywords + EOF
    assert_eq!(tokens.len(), 13);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Keyword(ref k) if k == "if"));
    assert!(matches!(tokens[2].kind, TokenKind::Keyword(ref k) if k == "else"));
    assert!(matches!(tokens[3].kind, TokenKind::Keyword(ref k) if k == "Int"));
    assert!(matches!(tokens[4].kind, TokenKind::Keyword(ref k) if k == "Text"));
    assert!(matches!(tokens[5].kind, TokenKind::Keyword(ref k) if k == "Bool"));
    assert!(matches!(tokens[6].kind, TokenKind::Keyword(ref k) if k == "Unit"));
    assert!(matches!(tokens[7].kind, TokenKind::Keyword(ref k) if k == "Value"));
    assert!(matches!(tokens[8].kind, TokenKind::Keyword(ref k) if k == "List"));
    assert!(matches!(tokens[9].kind, TokenKind::Keyword(ref k) if k == "true"));
    assert!(matches!(tokens[10].kind, TokenKind::Keyword(ref k) if k == "false"));
    assert!(matches!(tokens[11].kind, TokenKind::Keyword(ref k) if k == "unit"));
}

#[test]
fn h1_lex_let_is_identifier() {
    let spec = load_h1_spec();
    // `let` was removed from keywords; it now lexes as an identifier
    let src = "let";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    assert_eq!(tokens.len(), 2); // let (ident) + EOF
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
    assert_eq!(tokens[0].lexeme, "let");
}

#[test]
fn h1_lex_identifiers() {
    let spec = load_h1_spec();
    let src = "main foo_bar x CamelCase";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 identifiers + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
    assert_eq!(tokens[0].lexeme, "main");
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
    assert_eq!(tokens[1].lexeme, "foo_bar");
}

#[test]
fn h1_lex_int_literals() {
    let spec = load_h1_spec();
    let src = "0 1 42 999";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 4 int literals + EOF
    assert_eq!(tokens.len(), 5);
    assert!(matches!(tokens[0].kind, TokenKind::IntLit));
    assert_eq!(tokens[0].lexeme, "0");
    assert!(matches!(tokens[1].kind, TokenKind::IntLit));
    assert_eq!(tokens[1].lexeme, "1");
}

#[test]
fn h1_lex_string_literals() {
    let spec = load_h1_spec();
    let src = r#""hello" "world""#;
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 string literals + EOF
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::StringLit));
    assert_eq!(tokens[0].lexeme, r#""hello""#);
    assert!(matches!(tokens[1].kind, TokenKind::StringLit));
    assert_eq!(tokens[1].lexeme, r#""world""#);
}

#[test]
fn h1_lex_string_with_escapes() {
    let spec = load_h1_spec();
    let src = r#""hello\nworld""#;
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    assert_eq!(tokens.len(), 2); // string + EOF
    assert!(matches!(tokens[0].kind, TokenKind::StringLit));
    assert_eq!(tokens[0].lexeme, r#""hello\nworld""#);
}

#[test]
fn h1_lex_punctuation() {
    let spec = load_h1_spec();
    let src = "( ) { } [ ] , ; : .";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 10 punctuation + EOF
    assert_eq!(tokens.len(), 11);
    assert!(matches!(tokens[0].kind, TokenKind::Punct(ref p) if p == "("));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == ")"));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == "{"));
    assert!(matches!(tokens[3].kind, TokenKind::Punct(ref p) if p == "}"));
}

#[test]
fn h1_lex_operators() {
    let spec = load_h1_spec();
    let src = "== != <= >= + - * /";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 8 operators + EOF
    assert_eq!(tokens.len(), 9);
    assert!(matches!(tokens[0].kind, TokenKind::Punct(ref p) if p == "=="));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == "!="));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == "<="));
    assert!(matches!(tokens[3].kind, TokenKind::Punct(ref p) if p == ">="));
}

#[test]
fn h1_lex_longest_match_operators() {
    let spec = load_h1_spec();
    let src = "==>===";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // Actual behavior with longest-first matching and left-to-right scan:
    // Input "==>===" (6 chars)
    // Position 0: tries "==", matches (chars 0-1)
    // Position 2: tries ">=", matches (chars 2-3)
    // Position 4: tries "==", matches (chars 4-5)
    // Result: ==, >=, == + EOF = 4 tokens
    assert_eq!(tokens.len(), 4);
    assert!(matches!(tokens[0].kind, TokenKind::Punct(ref p) if p == "=="));
    assert!(matches!(tokens[1].kind, TokenKind::Punct(ref p) if p == ">="));
    assert!(matches!(tokens[2].kind, TokenKind::Punct(ref p) if p == "=="));
}

#[test]
fn h1_lex_line_comments() {
    let spec = load_h1_spec();
    let src = "fn // comment\nmain";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 tokens + EOF (comment skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
}

#[test]
fn h1_lex_block_comments() {
    let spec = load_h1_spec();
    let src = "fn /* block comment */ main";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // 2 tokens + EOF (comment skipped)
    assert_eq!(tokens.len(), 3);
    assert!(matches!(tokens[0].kind, TokenKind::Keyword(ref k) if k == "fn"));
    assert!(matches!(tokens[1].kind, TokenKind::Ident));
}

#[test]
fn h1_lex_forbid_reserved_words() {
    let spec = load_h1_spec();
    // async is in forbid list - but lexer doesn't enforce forbid, only parser does
    // This is expected behavior - lexer recognizes it as identifier
    let src = "async";
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // Still lexes as identifier (forbid is semantic, not lexical)
    assert_eq!(tokens.len(), 2);
    assert!(matches!(tokens[0].kind, TokenKind::Ident));
}

#[test]
fn h1_lex_real_example() {
    let spec = load_h1_spec();
    let src = r#"fn main() {
  1
}"#;
    let tokens = lexer_engine::lex_with_spec(&spec, src).expect("lex failed");

    // fn, main, (, ), {, 1, }, EOF = 8
    assert_eq!(tokens.len(), 8);
}

#[test]
fn h1_lex_unknown_char_error() {
    let spec = load_h1_spec();
    let src = "fn ^";
    let result = lexer_engine::lex_with_spec(&spec, src);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unexpected character"));
}
