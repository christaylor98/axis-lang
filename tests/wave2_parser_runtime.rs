// Wave 2 Parser Runtime Integration Tests
// Tests runtime parser engine with real specs and tokens

use axis_lang::frontend::parser_runtime::{parse_with_spec, ParseNode};
use axis_lang::frontend::parserspec::{
    Element, ParserSpec, Production, RepeatKind, Terminal, TerminalSource,
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

#[test]
fn test_complex_grammar_with_nesting() {
    // Grammar:
    // expr -> "(" expr ")" | IDENT
    let mut grammar = HashMap::new();

    grammar.insert(
        "expr".to_string(),
        vec![Production::Alternation(vec![
            // Parenthesized expression
            Production::Sequence(vec![
                Element::Terminal(Terminal::Punct("(".to_string(), TerminalSource::Explicit)),
                Element::NonTerminal("expr".to_string()),
                Element::Terminal(Terminal::Punct(")".to_string(), TerminalSource::Explicit)),
            ]),
            // Simple identifier
            Production::Sequence(vec![Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            ))]),
        ])],
    );

    let spec = ParserSpec {
        start: "expr".to_string(),
        grammar,
    };

    // Test: ((x))
    let tokens = vec![
        make_token(TokenKind::Punct("(".to_string()), "(", 0),
        make_token(TokenKind::Punct("(".to_string()), "(", 1),
        make_token(TokenKind::Ident, "x", 2),
        make_token(TokenKind::Punct(")".to_string()), ")", 3),
        make_token(TokenKind::Punct(")".to_string()), ")", 4),
        make_eof(5),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(
        result.is_ok(),
        "Expected successful parse, got: {:?}",
        result
    );

    let tree = result.unwrap();
    assert_eq!(tree.rule, "expr");
    assert_eq!(tree.children.len(), 3); // ( expr )
}

#[test]
fn test_sequence_parsing() {
    // Grammar: start -> kw:"fn" IDENT "(" ")"
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "fn".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::Punct("(".to_string(), TerminalSource::Explicit)),
            Element::Terminal(Terminal::Punct(")".to_string(), TerminalSource::Explicit)),
        ])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    let tokens = vec![
        make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
        make_token(TokenKind::Ident, "test", 3),
        make_token(TokenKind::Punct("(".to_string()), "(", 7),
        make_token(TokenKind::Punct(")".to_string()), ")", 8),
        make_eof(9),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_ok());

    let tree = result.unwrap();
    assert_eq!(tree.children.len(), 4);
}

#[test]
fn test_group_element() {
    // Grammar: start -> IDENT ("," IDENT)*
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Repeat(
                Box::new(Element::Group(vec![
                    Element::Terminal(Terminal::Punct(",".to_string(), TerminalSource::Explicit)),
                    Element::Terminal(Terminal::TokenKind(
                        "IDENT".to_string(),
                        TerminalSource::Explicit,
                    )),
                ])),
                RepeatKind::ZeroOrMore,
            ),
        ])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    // Test: x, y, z
    let tokens = vec![
        make_token(TokenKind::Ident, "x", 0),
        make_token(TokenKind::Punct(",".to_string()), ",", 1),
        make_token(TokenKind::Ident, "y", 3),
        make_token(TokenKind::Punct(",".to_string()), ",", 4),
        make_token(TokenKind::Ident, "z", 6),
        make_eof(7),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_ok());

    let tree = result.unwrap();
    // Should have: 1 IDENT + 4 items from groups (2 commas + 2 idents)
    assert_eq!(tree.children.len(), 5);
}

#[test]
fn test_multiple_productions_for_same_rule() {
    // Grammar:
    // start -> prod1 | prod2 | prod3
    // prod1 -> "if"
    // prod2 -> "fn"
    // prod3 -> "let"
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![
            Production::Sequence(vec![Element::Terminal(Terminal::Keyword(
                "if".to_string(),
                TerminalSource::Explicit,
            ))]),
            Production::Sequence(vec![Element::Terminal(Terminal::Keyword(
                "fn".to_string(),
                TerminalSource::Explicit,
            ))]),
            Production::Sequence(vec![Element::Terminal(Terminal::Keyword(
                "let".to_string(),
                TerminalSource::Explicit,
            ))]),
        ],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    // Test first production
    let tokens = vec![
        make_token(TokenKind::Keyword("if".to_string()), "if", 0),
        make_eof(2),
    ];
    assert!(parse_with_spec(&spec, &tokens).is_ok());

    // Test second production
    let tokens = vec![
        make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
        make_eof(2),
    ];
    assert!(parse_with_spec(&spec, &tokens).is_ok());

    // Test third production
    let tokens = vec![
        make_token(TokenKind::Keyword("let".to_string()), "let", 0),
        make_eof(3),
    ];
    assert!(parse_with_spec(&spec, &tokens).is_ok());
}

#[test]
fn test_nested_nonterminals() {
    // Grammar:
    // start -> decl
    // decl -> kw:"fn" name
    // name -> IDENT
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![Element::NonTerminal(
            "decl".to_string(),
        )])],
    );

    grammar.insert(
        "decl".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "fn".to_string(),
                TerminalSource::Explicit,
            )),
            Element::NonTerminal("name".to_string()),
        ])],
    );

    grammar.insert(
        "name".to_string(),
        vec![Production::Sequence(vec![Element::Terminal(
            Terminal::TokenKind("IDENT".to_string(), TerminalSource::Explicit),
        )])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    let tokens = vec![
        make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
        make_token(TokenKind::Ident, "test", 3),
        make_eof(7),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_ok());

    let tree = result.unwrap();
    assert_eq!(tree.rule, "start");

    // Check nesting
    match &tree.children[0] {
        ParseNode::Rule(decl_tree) => {
            assert_eq!(decl_tree.rule, "decl");
            assert_eq!(decl_tree.children.len(), 2);

            // Second child should be "name" rule
            match &decl_tree.children[1] {
                ParseNode::Rule(name_tree) => {
                    assert_eq!(name_tree.rule, "name");
                }
                _ => panic!("Expected Rule node for name"),
            }
        }
        _ => panic!("Expected Rule node for decl"),
    }
}

#[test]
fn test_span_tracking() {
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "fn".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    let tokens = vec![
        make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
        make_token(TokenKind::Ident, "test", 3),
        make_eof(7),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_ok());

    let tree = result.unwrap();
    // Span should cover from start of first token to end of last token
    assert_eq!(tree.span.start, 0);
    assert_eq!(tree.span.end, 7);
}

#[test]
fn test_empty_production() {
    // Grammar: start -> IDENT*
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![Element::Repeat(
            Box::new(Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            ))),
            RepeatKind::ZeroOrMore,
        )])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    // Empty input (just EOF)
    let tokens = vec![make_eof(0)];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_ok());

    let tree = result.unwrap();
    assert_eq!(tree.children.len(), 0);
}

#[test]
fn test_undefined_nonterminal_error() {
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![Element::NonTerminal(
            "undefined_rule".to_string(),
        )])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    let tokens = vec![make_eof(0)];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.message.contains("undefined nonterminal"));
}

#[test]
fn test_backtracking_preserves_state() {
    // Grammar:
    // start -> "if" IDENT | "if" "(" IDENT ")"
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Alternation(vec![
            // First alternative: if IDENT
            Production::Sequence(vec![
                Element::Terminal(Terminal::Keyword(
                    "if".to_string(),
                    TerminalSource::Explicit,
                )),
                Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                )),
            ]),
            // Second alternative: if ( IDENT )
            Production::Sequence(vec![
                Element::Terminal(Terminal::Keyword(
                    "if".to_string(),
                    TerminalSource::Explicit,
                )),
                Element::Terminal(Terminal::Punct("(".to_string(), TerminalSource::Explicit)),
                Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                )),
                Element::Terminal(Terminal::Punct(")".to_string(), TerminalSource::Explicit)),
            ]),
        ])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    // Input matches second alternative: if ( x )
    // First alternative will partially match ("if") but fail on "("
    // Parser should backtrack and try second alternative
    let tokens = vec![
        make_token(TokenKind::Keyword("if".to_string()), "if", 0),
        make_token(TokenKind::Punct("(".to_string()), "(", 3),
        make_token(TokenKind::Ident, "x", 4),
        make_token(TokenKind::Punct(")".to_string()), ")", 5),
        make_eof(6),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_ok());

    let tree = result.unwrap();
    assert_eq!(tree.children.len(), 4);
}

#[test]
fn test_determinism_with_complex_input() {
    // Complex grammar with multiple rules and alternations
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![
            Element::NonTerminal("item".to_string()),
            Element::Repeat(
                Box::new(Element::Group(vec![
                    Element::Terminal(Terminal::Punct(",".to_string(), TerminalSource::Explicit)),
                    Element::NonTerminal("item".to_string()),
                ])),
                RepeatKind::ZeroOrMore,
            ),
        ])],
    );

    grammar.insert(
        "item".to_string(),
        vec![Production::Alternation(vec![
            Production::Sequence(vec![Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            ))]),
            Production::Sequence(vec![Element::Terminal(Terminal::TokenKind(
                "INT_LIT".to_string(),
                TerminalSource::Explicit,
            ))]),
        ])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    let tokens = vec![
        make_token(TokenKind::Ident, "x", 0),
        make_token(TokenKind::Punct(",".to_string()), ",", 1),
        make_token(TokenKind::IntLit, "42", 3),
        make_token(TokenKind::Punct(",".to_string()), ",", 5),
        make_token(TokenKind::Ident, "y", 7),
        make_eof(8),
    ];

    // Parse multiple times
    let result1 = parse_with_spec(&spec, &tokens);
    let result2 = parse_with_spec(&spec, &tokens);
    let result3 = parse_with_spec(&spec, &tokens);

    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());

    // All results should be identical
    assert_eq!(result1.unwrap(), result2.unwrap());
    let tree3 = result3.unwrap();

    // Verify structure
    assert_eq!(tree3.rule, "start");
}

#[test]
fn test_error_messages_are_clear() {
    let mut grammar = HashMap::new();

    grammar.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![
            Element::Terminal(Terminal::Keyword(
                "fn".to_string(),
                TerminalSource::Explicit,
            )),
            Element::Terminal(Terminal::TokenKind(
                "IDENT".to_string(),
                TerminalSource::Explicit,
            )),
        ])],
    );

    let spec = ParserSpec {
        start: "start".to_string(),
        grammar,
    };

    // Missing IDENT
    let tokens = vec![
        make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
        make_eof(2),
    ];

    let result = parse_with_spec(&spec, &tokens);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(
        err.message.contains("expected")
            || err.message.contains("IDENT")
            || err.message.contains("EOF")
    );
}
