// Wave 4: Parser Alternation and Backtracking Tests
//
// These tests verify that the parser correctly handles:
// - Ordered choice (try alternatives in YAML order)
// - Complete backtracking (restore position on failure)
// - Correct error reporting

use axis_lang::frontend::parser_runtime::parse_with_spec;
use axis_lang::frontend::parserspec::{
    Element, ParserSpec, Production, Terminal, TerminalSource,
};
use axis_lang::frontend::token::{Span, Token, TokenKind};
use std::collections::HashMap;

// Helper to create tokens
fn make_token(kind: TokenKind, lexeme: &str, start: usize) -> Token {
    Token::new(
        kind,
        lexeme.to_string(),
        Span::new(start, start + lexeme.len()),
    )
}

fn make_eof(pos: usize) -> Token {
    Token::new(TokenKind::Eof, "".to_string(), Span::new(pos, pos))
}

/// Test that alternation tries alternatives in order
/// This reproduces the bug where "arg 1" fails to parse
#[test]
fn test_alternation_ordered_choice() {
    // Grammar (matches surface-0 structure):
    // Instruction -> Arg | Lit | Call
    // Arg -> kw:"arg" INT
    // Lit -> kw:"lit" IDENT INT
    // Call -> kw:"call" IDENT

    let mut grammar = HashMap::new();

    // Instruction: three alternatives (order matters!)
    grammar.insert(
        "Instruction".to_string(),
        vec![
            Production::Sequence(vec![Element::NonTerminal("Arg".to_string())]),
            Production::Sequence(vec![Element::NonTerminal("Lit".to_string())]),
            Production::Sequence(vec![Element::NonTerminal("Call".to_string())]),
        ],
    );

    // Arg: kw:"arg" INT
    grammar.insert(
        "Arg".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "arg".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    // Lit: kw:"lit" IDENT INT
    grammar.insert(
        "Lit".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "lit".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    // Call: kw:"call" IDENT
    grammar.insert(
        "Call".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "call".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    let spec = ParserSpec {
        start: "Instruction".to_string(),
        grammar,
    };

    // Test parsing "arg 1" - should match Arg (first alternative)
    let tokens = vec![
        make_token(TokenKind::Keyword("arg".to_string()), "arg", 0),
        make_token(TokenKind::IntLit, "1", 4),
        make_eof(5),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(
        result.is_ok(),
        "Expected successful parse of 'arg 1', got: {:?}",
        result.err()
    );

    let tree = result.unwrap();
    assert_eq!(tree.rule, "Instruction");
    // Should have parsed Arg successfully
}

/// Test that backtracking restores position correctly
#[test]
fn test_alternation_backtracking() {
    // Same grammar as above
    let mut grammar = HashMap::new();

    grammar.insert(
        "Instruction".to_string(),
        vec![
            Production::Sequence(vec![Element::NonTerminal("Arg".to_string())]),
            Production::Sequence(vec![Element::NonTerminal("Lit".to_string())]),
            Production::Sequence(vec![Element::NonTerminal("Call".to_string())]),
        ],
    );

    grammar.insert(
        "Arg".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "arg".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    grammar.insert(
        "Lit".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "lit".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    grammar.insert(
        "Call".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "call".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    let spec = ParserSpec {
        start: "Instruction".to_string(),
        grammar,
    };

    // Test parsing "lit i32 42" - should match Lit (second alternative)
    // This tests that after failing to match Arg, we correctly backtrack and try Lit
    let tokens = vec![
        make_token(TokenKind::Keyword("lit".to_string()), "lit", 0),
        make_token(TokenKind::Ident, "i32", 4),
        make_token(TokenKind::IntLit, "42", 8),
        make_eof(10),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(
        result.is_ok(),
        "Expected successful parse of 'lit i32 42', got: {:?}",
        result.err()
    );

    let tree = result.unwrap();
    assert_eq!(tree.rule, "Instruction");
}

/// Test that "call" instruction (third alternative) works
#[test]
fn test_alternation_third_alternative() {
    let mut grammar = HashMap::new();

    grammar.insert(
        "Instruction".to_string(),
        vec![
            Production::Sequence(vec![Element::NonTerminal("Arg".to_string())]),
            Production::Sequence(vec![Element::NonTerminal("Lit".to_string())]),
            Production::Sequence(vec![Element::NonTerminal("Call".to_string())]),
        ],
    );

    grammar.insert(
        "Arg".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "arg".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    grammar.insert(
        "Lit".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "lit".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    grammar.insert(
        "Call".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "call".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    let spec = ParserSpec {
        start: "Instruction".to_string(),
        grammar,
    };

    // Test parsing "call foo" - should match Call (third alternative)
    let tokens = vec![
        make_token(TokenKind::Keyword("call".to_string()), "call", 0),
        make_token(TokenKind::Ident, "foo", 5),
        make_eof(8),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(
        result.is_ok(),
        "Expected successful parse of 'call foo', got: {:?}",
        result.err()
    );

    let tree = result.unwrap();
    assert_eq!(tree.rule, "Instruction");
}

/// Test that token kind aliases work (INT vs INT_LIT)
/// This tests the fix for Wave 4 where YAML uses <INT> but runtime expects INT_LIT
#[test]
fn test_token_kind_aliases() {
    let mut grammar = HashMap::new();

    // Use "INT" (YAML form) instead of "INT_LIT" (TokenKind form)
    grammar.insert(
        "Number".to_string(),
        vec![Production::Sequence(vec![Element::Terminal(
            Terminal::TokenKind("INT".to_string(), TerminalSource::Explicit),
        )])],
    );

    let spec = ParserSpec {
        start: "Number".to_string(),
        grammar,
    };

    let tokens = vec![make_token(TokenKind::IntLit, "42", 0), make_eof(2)];

    let result = parse_with_spec(&spec, &tokens);
    assert!(
        result.is_ok(),
        "Expected INT alias to work, got: {:?}",
        result.err()
    );
}
