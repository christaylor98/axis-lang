// Parser-only tests - token-kind enforcement
// Tests parser behavior with explicit token kind matching only

#[cfg(test)]
mod parser_token_kind_tests {
    use axis_lang::frontend::parser_runtime::parse_with_spec;
    use axis_lang::frontend::parserspec::{
        Element, ParserSpec, Production, RepeatKind, Terminal, TerminalSource,
    };
    use axis_lang::frontend::token::{Span, Token, TokenKind};
    use std::collections::HashMap;

    fn make_token(kind: TokenKind, lexeme: &str, start: usize) -> Token {
        Token::new(
            kind,
            lexeme.to_string(),
            Span::new(start, start + lexeme.len()),
        )
    }

    fn make_eof() -> Token {
        Token::new(TokenKind::Eof, "".to_string(), Span::new(0, 0))
    }

    // Test 1: Minimal valid program - single token
    #[test]
    fn test_minimal_valid_program() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::TokenKind("INT".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        let tokens = vec![make_token(TokenKind::IntLit, "42", 0), make_eof()];

        let result = parse_with_spec(&spec, &tokens);
        assert!(
            result.is_ok(),
            "Expected successful parse, got: {:?}",
            result
        );

        let tree = result.unwrap();
        assert_eq!(tree.rule, "Program");
        assert_eq!(tree.children.len(), 1);
    }

    // Test 2: Invalid token kind at rule boundary
    #[test]
    fn test_invalid_token_kind_at_boundary() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
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
            start: "Program".to_string(),
            grammar,
        };

        // Wrong token kind: INT instead of IDENT
        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_token(TokenKind::IntLit, "42", 3),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("expected"));
        assert!(err.message.contains("IDENT"));
    }

    // Test 3: Repetition edge case - ZeroOrMore with zero occurrences
    #[test]
    fn test_repetition_zero_or_more_empty() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Repeat(
                Box::new(Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                ))),
                RepeatKind::ZeroOrMore,
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        // Zero occurrences - should succeed
        let tokens = vec![make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().children.len(), 0);
    }

    // Test 4: Repetition edge case - OneOrMore with zero occurrences
    #[test]
    fn test_repetition_one_or_more_empty() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Repeat(
                Box::new(Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                ))),
                RepeatKind::OneOrMore,
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        // Zero occurrences - should fail
        let tokens = vec![make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());
    }

    // Test 5: Optional element - present
    #[test]
    fn test_optional_present() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Repeat(
                Box::new(Element::Terminal(Terminal::TokenKind(
                    "INT".to_string(),
                    TerminalSource::Explicit,
                ))),
                RepeatKind::Optional,
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        let tokens = vec![make_token(TokenKind::IntLit, "99", 0), make_eof()];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().children.len(), 1);
    }

    // Test 6: Optional element - absent
    #[test]
    fn test_optional_absent() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Repeat(
                Box::new(Element::Terminal(Terminal::TokenKind(
                    "INT".to_string(),
                    TerminalSource::Explicit,
                ))),
                RepeatKind::Optional,
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        let tokens = vec![make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().children.len(), 0);
    }

    // Test 7: Keyword vs identifier boundary - keyword expected
    #[test]
    fn test_keyword_vs_ident_keyword_expected() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Keyword("fn".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        // Identifier instead of keyword
        let tokens = vec![
            make_token(TokenKind::Ident, "fn", 0), // IDENT, not Keyword
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("expected"));
    }

    // Test 8: Keyword vs identifier boundary - identifier expected
    #[test]
    fn test_keyword_vs_ident_ident_expected() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::TokenKind("IDENT".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        // Keyword instead of identifier
        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("expected"));
    }

    // Test 9: Legacy terminal rejection
    #[test]
    fn test_legacy_terminal_rejected() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Keyword("fn".to_string(), TerminalSource::InferredLegacy),
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.message.contains("InferredLegacy"));
        assert!(err.message.contains("forbidden"));
    }

    // Test 10: Error message references token kinds not characters
    #[test]
    fn test_error_message_token_kind_not_char() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Punct("+".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        // Wrong punctuation
        let tokens = vec![
            make_token(TokenKind::Punct("-".to_string()), "-", 0),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());

        let err = result.unwrap_err();
        // Error should mention Punct, not '+' character
        assert!(err.message.contains("expected"));
        assert!(err.message.contains("Punct"));
    }

    // Test 11: Alternation with token kind matching
    #[test]
    fn test_alternation_token_kinds() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Alternation(vec![
                Production::Sequence(vec![Element::Terminal(Terminal::TokenKind(
                    "INT".to_string(),
                    TerminalSource::Explicit,
                ))]),
                Production::Sequence(vec![Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                ))]),
            ])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        // First alternative
        let tokens = vec![make_token(TokenKind::IntLit, "42", 0), make_eof()];
        assert!(parse_with_spec(&spec, &tokens).is_ok());

        // Second alternative
        let tokens = vec![make_token(TokenKind::Ident, "x", 0), make_eof()];
        assert!(parse_with_spec(&spec, &tokens).is_ok());

        // Neither alternative
        let tokens = vec![make_token(TokenKind::BoolLit, "true", 0), make_eof()];
        assert!(parse_with_spec(&spec, &tokens).is_err());
    }

    // Test 12: Deterministic behavior across multiple parses
    #[test]
    fn test_deterministic_parsing() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "Program".to_string(),
            vec![Production::Sequence(vec![
                Element::Terminal(Terminal::Keyword(
                    "fn".to_string(),
                    TerminalSource::Explicit,
                )),
                Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                )),
                Element::Repeat(
                    Box::new(Element::Terminal(Terminal::TokenKind(
                        "INT".to_string(),
                        TerminalSource::Explicit,
                    ))),
                    RepeatKind::ZeroOrMore,
                ),
            ])],
        );

        let spec = ParserSpec {
            start: "Program".to_string(),
            grammar,
        };

        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_token(TokenKind::Ident, "test", 3),
            make_token(TokenKind::IntLit, "1", 8),
            make_token(TokenKind::IntLit, "2", 10),
            make_eof(),
        ];

        let result1 = parse_with_spec(&spec, &tokens);
        let result2 = parse_with_spec(&spec, &tokens);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert_eq!(result1.unwrap(), result2.unwrap());
    }
}
