// Wave 2 Parser Runtime — Example Usage
// This demonstrates how to use the runtime parser engine

use axis_lang::frontend::parser_runtime::{parse_with_spec, ParseNode};
use axis_lang::frontend::parserspec::{
    Element, ParserSpec, Production, Terminal, TerminalSource,
};
use axis_lang::frontend::token::{Span, Token, TokenKind};
use std::collections::HashMap;

fn main() {
    // Example 1: Simple function declaration grammar
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

    // Token stream: fn test()
    let tokens = vec![
        Token::new(
            TokenKind::Keyword("fn".to_string()),
            "fn".to_string(),
            Span::new(0, 2),
        ),
        Token::new(TokenKind::Ident, "test".to_string(), Span::new(3, 7)),
        Token::new(
            TokenKind::Punct("(".to_string()),
            "(".to_string(),
            Span::new(7, 8),
        ),
        Token::new(
            TokenKind::Punct(")".to_string()),
            ")".to_string(),
            Span::new(8, 9),
        ),
        Token::new(TokenKind::Eof, "".to_string(), Span::new(9, 9)),
    ];

    // Parse
    match parse_with_spec(&spec, &tokens) {
        Ok(tree) => {
            println!("Parse successful!");
            println!("Rule: {}", tree.rule);
            println!("Span: {}..{}", tree.span.start, tree.span.end);
            println!("Children: {}", tree.children.len());

            // Print structure
            for (i, child) in tree.children.iter().enumerate() {
                match child {
                    ParseNode::Terminal(tok) => {
                        println!("  [{}] Terminal: {} '{}'", i, tok.kind, tok.lexeme);
                    }
                    ParseNode::Rule(subtree) => {
                        println!("  [{}] Rule: {}", i, subtree.rule);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
        }
    }

    println!("\n---\n");

    // Example 2: Error case - legacy terminal rejection
    let mut grammar2 = HashMap::new();
    grammar2.insert(
        "start".to_string(),
        vec![Production::Sequence(vec![
            // INTENTIONAL: Using InferredLegacy source (FORBIDDEN)
            Element::Terminal(Terminal::Keyword(
                "fn".to_string(),
                TerminalSource::InferredLegacy,
            )),
        ])],
    );

    let spec2 = ParserSpec {
        start: "start".to_string(),
        grammar: grammar2,
    };

    let tokens2 = vec![
        Token::new(
            TokenKind::Keyword("fn".to_string()),
            "fn".to_string(),
            Span::new(0, 2),
        ),
        Token::new(TokenKind::Eof, "".to_string(), Span::new(2, 2)),
    ];

    match parse_with_spec(&spec2, &tokens2) {
        Ok(_) => {
            eprintln!("ERROR: Should have rejected InferredLegacy terminal!");
        }
        Err(e) => {
            println!("Expected error (legacy terminal rejection):");
            println!("  {}", e);
        }
    }
}
