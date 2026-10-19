// Wave 2: Runtime Parser Engine
// Executes validated ParserSpec mechanically against token stream
// Produces structural parse trees (NO semantic interpretation)

use crate::frontend::parserspec::{
    Element, ParserSpec, Production, RepeatKind, Terminal, TerminalSource,
};
use crate::frontend::token::{Span, Token, TokenKind};
use std::fmt;

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Structural parse tree (NOT an AST)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTree {
    pub rule: String,
    pub children: Vec<ParseNode>,
    pub span: Span,
}

/// Node in the parse tree
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseNode {
    /// Parsed non-terminal rule
    Rule(ParseTree),
    /// Terminal token
    Terminal(Token),
}

/// Parse error
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "parse error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// Runtime parser entry point
///
/// Parses tokens using the given spec, producing a structural parse tree.
///
/// DOES NOT:
/// - Infer terminals
/// - Fix grammar bugs
/// - Add semantic meaning
/// - Normalize or simplify trees
pub fn parse_with_spec(spec: &ParserSpec, tokens: &[Token]) -> Result<ParseTree, ParseError> {
    let mut state = ParserState::new(tokens);

    // Parse starting at the start rule
    let tree = parse_rule(&mut state, spec, &spec.start)?;

    // CRITICAL: Ensure we consumed all tokens except EOF
    if !state.at_eof() {
        // Report the HIGH-WATER MARK, not `state.pos`. After backtracking,
        // `pos` is wherever the last surviving alternative stopped — often
        // token 0 — so `current_span()` here used to emit `0..2` regardless of
        // where the real mistake was.
        return Err(ParseError {
            message: format!(
                "unexpected token {}, parsed up to here then could not continue{}",
                state.furthest_lexeme(),
                literal_ctor_hint(tokens, state.furthest)
                    .or_else(|| paren_balance_hint(tokens))
                    .unwrap_or_default()
            ),
            span: state.furthest_span(),
        });
    }

    Ok(tree)
}

/// `Int(...)`, `Text(...)`, `Bool(...)` and `Unit()` take a LITERAL TOKEN, not
/// an expression — the productions are terminal-only
/// (`<KW_INT> <PUNCT_(> <INT_LIT> <PUNCT_)>`, m1-parse.yaml). So `Int(n)` or
/// `Int(int_add(n, Int(1)))` stalls the parse at the inner token, with nothing
/// in the raw message to say that a wrapper is the reason. The wrapper is
/// surface discipline (§4.1 forbids bare literals), which makes this an easy
/// mistake to make and a hard one to read.
///
/// Recognised by looking BACK two tokens from the high-water mark for
/// `<literal keyword> (`. Checked ahead of the bracket scan because a wrapper
/// holding a call expression usually has balanced parens, so the balance hint
/// would find nothing and say nothing.
fn literal_ctor_hint(tokens: &[Token], furthest: usize) -> Option<String> {
    if furthest < 2 {
        return None;
    }
    if tokens.get(furthest - 1)?.lexeme != "(" {
        return None;
    }
    let kw = &tokens.get(furthest - 2)?.lexeme;
    let expects = match kw.as_str() {
        "Int" => "an integer literal",
        "Text" => "a string literal",
        "Bool" => "`true` or `false`",
        "Unit" => "nothing — `Unit()` takes no argument",
        _ => return None,
    };
    Some(format!(
        " — `{kw}(...)` is a literal wrapper and takes {expects}, not an expression"
    ))
}

/// Cheap, exact localisation for the commonest cause of a stalled parse.
///
/// The high-water mark names the token that STOPPED the parse, which for an
/// unbalanced delimiter is not the mistake — a missing `)` stalls at whatever
/// follows, arbitrarily far from the `(` that needed closing. A bracket scan
/// finds the actual culprit, so the message can name both.
///
/// Scans the raw token stream, which is sound here because M1 has no token
/// whose lexeme is a bare delimiter in any other role: no string interpolation,
/// no macro, no operator built from brackets (§4.2 — M1 has no operators at
/// all). The delimiters cannot appear inside a Text literal either; the lexer
/// has already folded those into one token.
fn paren_balance_hint(tokens: &[Token]) -> Option<String> {
    let mut stack: Vec<(&str, &Token)> = Vec::new();
    for tok in tokens {
        match tok.lexeme.as_str() {
            "(" | "[" | "{" => stack.push((tok.lexeme.as_str(), tok)),
            close @ (")" | "]" | "}") => {
                let want = match close {
                    ")" => "(",
                    "]" => "[",
                    _ => "{",
                };
                match stack.pop() {
                    // Mismatched pair, e.g. `( ... ]`.
                    Some((open, open_tok)) if open != want => {
                        return Some(format!(
                            " — `{}` at {}..{} is closed by `{}` at {}..{}",
                            open,
                            open_tok.span.start,
                            open_tok.span.end,
                            close,
                            tok.span.start,
                            tok.span.end
                        ));
                    }
                    Some(_) => {}
                    // A closer with nothing open.
                    None => {
                        return Some(format!(
                            " — unmatched `{}` at {}..{}",
                            close, tok.span.start, tok.span.end
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    // Anything left open is unclosed. Report the innermost (last opened): it is
    // the one whose partner is missing.
    stack.last().map(|(open, tok)| {
        format!(
            " — unclosed `{}` at {}..{}{}",
            open,
            tok.span.start,
            tok.span.end,
            if stack.len() > 1 {
                format!(" ({} delimiters left open in total)", stack.len())
            } else {
                String::new()
            }
        )
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// PARSER STATE
// ═══════════════════════════════════════════════════════════════════════════

struct ParserState<'a> {
    tokens: &'a [Token],
    pos: usize,
    /// High-water mark: the furthest `pos` ever reached, across every
    /// alternative tried, and NOT rewound by `restore`.
    ///
    /// This grammar backtracks (`parse_rule` restores the checkpoint and tries
    /// the next production), so when the whole parse ends without reaching EOF,
    /// `pos` is wherever the last surviving alternative happened to stop —
    /// frequently right back at the top of the file. That is what produced the
    /// useless `parse error at 0..2` for a missing `)` two hundred tokens in,
    /// and cost a full bisect to localise by hand.
    ///
    /// The furthest position is the standard localisation for a backtracking
    /// parser: the alternative that got deepest is the one that was nearly
    /// right, so the token that stopped it is the token to name. It is an
    /// approximation — the real mistake can be earlier (an opening `(` whose
    /// partner is missing) — so the diagnostic points at it and the
    /// paren-balance hint below names the earlier cause when it can.
    furthest: usize,
}

impl<'a> ParserState<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        ParserState { tokens, pos: 0, furthest: 0 }
    }

    /// Span of the token at the high-water mark.
    fn furthest_span(&self) -> Span {
        match self.tokens.get(self.furthest) {
            Some(tok) => tok.span.clone(),
            None => self.current_span(),
        }
    }

    /// Lexeme at the high-water mark, for the message.
    fn furthest_lexeme(&self) -> String {
        match self.tokens.get(self.furthest) {
            Some(tok) if matches!(tok.kind, TokenKind::Eof) => "end of file".to_string(),
            Some(tok) => format!("`{}`", tok.lexeme),
            None => "end of file".to_string(),
        }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn current_span(&self) -> Span {
        if let Some(tok) = self.current() {
            tok.span.clone()
        } else {
            // At EOF - use end of last token or 0
            if let Some(last) = self.tokens.last() {
                Span::new(last.span.end, last.span.end)
            } else {
                Span::new(0, 0)
            }
        }
    }

    fn at_eof(&self) -> bool {
        match self.current() {
            Some(tok) => matches!(tok.kind, TokenKind::Eof),
            None => true,
        }
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
            if self.pos > self.furthest {
                self.furthest = self.pos;
            }
        }
    }

    fn checkpoint(&self) -> usize {
        self.pos
    }

    /// Rewind for the next alternative. Deliberately does NOT touch
    /// `furthest`: the whole point of the high-water mark is that it survives
    /// backtracking.
    fn restore(&mut self, checkpoint: usize) {
        self.pos = checkpoint;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CORE PARSER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Parse a non-terminal rule
fn parse_rule(
    state: &mut ParserState,
    spec: &ParserSpec,
    rule: &str,
) -> Result<ParseTree, ParseError> {
    let start_span = state.current_span();

    // Lookup productions for this rule
    let productions = spec.grammar.get(rule).ok_or_else(|| ParseError {
        message: format!("undefined nonterminal: {}", rule),
        span: start_span.clone(),
    })?;

    // Try productions in order (backtracking across alternatives)
    let mut last_error: Option<ParseError> = None;

    for production in productions {
        let checkpoint = state.checkpoint();

        match parse_production(state, spec, production) {
            Ok(children) => {
                let end_span = if !children.is_empty() {
                    match children.last().unwrap() {
                        ParseNode::Rule(t) => t.span.clone(),
                        ParseNode::Terminal(tok) => tok.span.clone(),
                    }
                } else {
                    start_span.clone()
                };

                let span = Span::new(start_span.start, end_span.end);

                return Ok(ParseTree {
                    rule: rule.to_string(),
                    children,
                    span,
                });
            }
            Err(e) => {
                // Backtrack and try next alternative
                state.restore(checkpoint);
                last_error = Some(e);
            }
        }
    }

    // All alternatives failed
    Err(last_error.unwrap_or_else(|| ParseError {
        message: format!("no production matched for rule {}", rule),
        span: start_span,
    }))
}

/// Parse a production (sequence or alternation)
fn parse_production(
    state: &mut ParserState,
    spec: &ParserSpec,
    production: &Production,
) -> Result<Vec<ParseNode>, ParseError> {
    match production {
        Production::Sequence(elements) => {
            let mut children = Vec::new();
            for element in elements {
                let mut nodes = parse_element(state, spec, element)?;
                children.append(&mut nodes);
            }
            Ok(children)
        }

        Production::Alternation(alternatives) => {
            // Try each alternative in order
            let mut last_error: Option<ParseError> = None;

            for alt in alternatives {
                let checkpoint = state.checkpoint();

                match parse_production(state, spec, alt) {
                    Ok(children) => return Ok(children),
                    Err(e) => {
                        state.restore(checkpoint);
                        last_error = Some(e);
                    }
                }
            }

            Err(last_error.unwrap_or_else(|| ParseError {
                message: "no alternative matched".to_string(),
                span: state.current_span(),
            }))
        }
    }
}

/// Parse an element (NonTerminal, Terminal, Repeat, Group)
fn parse_element(
    state: &mut ParserState,
    spec: &ParserSpec,
    element: &Element,
) -> Result<Vec<ParseNode>, ParseError> {
    match element {
        Element::NonTerminal(name) => {
            let tree = parse_rule(state, spec, name)?;
            Ok(vec![ParseNode::Rule(tree)])
        }

        Element::Terminal(terminal) => {
            let tok = parse_terminal(state, terminal)?;
            Ok(vec![ParseNode::Terminal(tok)])
        }

        Element::Repeat(inner, kind) => parse_repeat(state, spec, inner, kind),

        Element::Group(elements) => {
            // Inline parse of grouped elements
            let mut children = Vec::new();
            for element in elements {
                let mut nodes = parse_element(state, spec, element)?;
                children.append(&mut nodes);
            }
            Ok(children)
        }
    }
}

/// Parse a terminal token
fn parse_terminal(state: &mut ParserState, terminal: &Terminal) -> Result<Token, ParseError> {
    // CRITICAL: Reject legacy inferred terminals
    let source = match terminal {
        Terminal::TokenKind(_, src) => src,
        Terminal::Keyword(_, src) => src,
        Terminal::Punct(_, src) => src,
    };

    if matches!(source, TerminalSource::InferredLegacy) {
        return Err(ParseError {
            message: format!(
                "terminal {:?} marked InferredLegacy is forbidden at runtime",
                terminal
            ),
            span: state.current_span(),
        });
    }

    let current = state.current().ok_or_else(|| ParseError {
        message: format!("expected {:?}, found EOF", terminal),
        span: state.current_span(),
    })?;

    // Match terminal against token
    let matches = match terminal {
        Terminal::TokenKind(expected_kind, _) => {
            // Normalize token kind names to support both forms:
            // YAML uses INT, BOOL, STRING, UNIT
            // TokenKind uses IntLit, BoolLit, StringLit, UnitLit
            // Accept both "INT" and "INT_LIT", etc.
            match &current.kind {
                TokenKind::Ident if expected_kind == "IDENT" => true,
                TokenKind::IntLit if expected_kind == "INT_LIT" || expected_kind == "INT" => true,
                TokenKind::FloatLit
                    if expected_kind == "FLOAT_LIT" || expected_kind == "FLOAT" =>
                {
                    true
                }
                TokenKind::BoolLit if expected_kind == "BOOL_LIT" || expected_kind == "BOOL" => {
                    true
                }
                TokenKind::StringLit
                    if expected_kind == "STRING_LIT" || expected_kind == "STRING" =>
                {
                    true
                }
                TokenKind::UnitLit if expected_kind == "UNIT_LIT" || expected_kind == "UNIT" => {
                    true
                }
                TokenKind::Keyword(kw) if expected_kind == &format!("KW_{}", kw.to_uppercase()) => {
                    true
                }
                TokenKind::Punct(p) if expected_kind == &format!("PUNCT_{}", p) => true,
                _ => false,
            }
        }

        Terminal::Keyword(expected_kw, _) => {
            matches!(&current.kind, TokenKind::Keyword(kw) if kw == expected_kw)
        }

        Terminal::Punct(expected_p, _) => {
            matches!(&current.kind, TokenKind::Punct(p) if p == expected_p)
        }
    };

    if !matches {
        return Err(ParseError {
            message: format!("expected {:?}, found {}", terminal, current.kind),
            span: current.span.clone(),
        });
    }

    let tok = current.clone();
    state.advance();
    Ok(tok)
}

/// Parse repeated element (* + ?)
fn parse_repeat(
    state: &mut ParserState,
    spec: &ParserSpec,
    element: &Element,
    kind: &RepeatKind,
) -> Result<Vec<ParseNode>, ParseError> {
    let mut children = Vec::new();

    match kind {
        RepeatKind::ZeroOrMore => {
            // Loop until failure
            loop {
                let checkpoint = state.checkpoint();
                match parse_element(state, spec, element) {
                    Ok(mut nodes) => {
                        children.append(&mut nodes);
                    }
                    Err(_) => {
                        // Backtrack and stop
                        state.restore(checkpoint);
                        break;
                    }
                }
            }
            Ok(children)
        }

        RepeatKind::OneOrMore => {
            // First occurrence is required
            let mut nodes = parse_element(state, spec, element)?;
            children.append(&mut nodes);

            // Then zero or more additional
            loop {
                let checkpoint = state.checkpoint();
                match parse_element(state, spec, element) {
                    Ok(mut nodes) => {
                        children.append(&mut nodes);
                    }
                    Err(_) => {
                        state.restore(checkpoint);
                        break;
                    }
                }
            }
            Ok(children)
        }

        RepeatKind::Optional => {
            // Try once, skip on failure
            let checkpoint = state.checkpoint();
            match parse_element(state, spec, element) {
                Ok(mut nodes) => {
                    children.append(&mut nodes);
                }
                Err(_) => {
                    state.restore(checkpoint);
                }
            }
            Ok(children)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parserspec::Production;
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

    #[test]
    fn test_simple_terminal_match() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Keyword("fn".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
            grammar,
        };

        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(
            result.is_ok(),
            "Expected successful parse, got: {:?}",
            result
        );

        let tree = result.unwrap();
        assert_eq!(tree.rule, "start");
        assert_eq!(tree.children.len(), 1);
    }

    #[test]
    fn test_terminal_mismatch() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Keyword("if".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
            grammar,
        };

        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("expected"));
    }

    #[test]
    fn test_legacy_terminal_rejection() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Keyword("fn".to_string(), TerminalSource::InferredLegacy),
            )])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
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

    #[test]
    fn test_eof_enforcement() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![Production::Sequence(vec![Element::Terminal(
                Terminal::Keyword("fn".to_string(), TerminalSource::Explicit),
            )])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
            grammar,
        };

        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_token(TokenKind::Keyword("if".to_string()), "if", 3),
            make_eof(),
        ];

        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // The message names the offending token and points at ITS span, not at
        // wherever backtracking left `pos`. The old text was the bare
        // "unexpected token after end of parse" with `current_span()`.
        assert!(err.message.contains("unexpected token `if`"), "{}", err.message);
        assert_eq!(err.span.start, 3, "must point at `if`, not at token 0");
        assert_eq!(err.span.end, 5);
    }

    /// The high-water mark must survive backtracking: an alternative that gets
    /// deep and then fails must still set the reported position, even though
    /// `restore` rewinds `pos` behind it. Without this, a grammar with
    /// alternatives reports wherever the LAST alternative stopped — which is
    /// how a missing `)` 148 bytes into a file reported `0..2`.
    #[test]
    fn furthest_position_survives_backtracking() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![
                // Tried first, consumes three tokens, then fails on the fourth.
                Production::Sequence(vec![
                    Element::Terminal(Terminal::Keyword("fn".into(), TerminalSource::Explicit)),
                    Element::Terminal(Terminal::Keyword("if".into(), TerminalSource::Explicit)),
                    Element::Terminal(Terminal::Keyword("else".into(), TerminalSource::Explicit)),
                    Element::Terminal(Terminal::Keyword("nope".into(), TerminalSource::Explicit)),
                ]),
                // Falls back to this, which consumes exactly one and succeeds,
                // leaving `pos` at 1 while the deep attempt reached 3.
                Production::Sequence(vec![Element::Terminal(Terminal::Keyword(
                    "fn".into(),
                    TerminalSource::Explicit,
                ))]),
            ],
        );
        let spec = ParserSpec { start: "start".to_string(), grammar };
        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_token(TokenKind::Keyword("if".to_string()), "if", 10),
            make_token(TokenKind::Keyword("else".to_string()), "else", 20),
            make_token(TokenKind::Keyword("end".to_string()), "end", 30),
            make_eof(),
        ];

        let err = parse_with_spec(&spec, &tokens).unwrap_err();
        // `pos` after the surviving alternative is 1 (span 10). The furthest
        // reached is 3 (span 30) — that is what must be reported.
        assert_eq!(err.span.start, 30, "reported the shallow position: {}", err.message);
        assert!(err.message.contains("unexpected token `end`"), "{}", err.message);
    }

    #[test]
    fn unclosed_delimiter_is_named_in_the_hint() {
        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_token(TokenKind::Punct("(".to_string()), "(", 10),
            make_token(TokenKind::Keyword("x".to_string()), "x", 11),
            make_eof(),
        ];
        let hint = paren_balance_hint(&tokens).expect("an unclosed `(` must be reported");
        assert!(hint.contains("unclosed `(` at 10..11"), "{hint}");
    }

    #[test]
    fn unmatched_closer_is_named_in_the_hint() {
        let tokens = vec![
            make_token(TokenKind::Punct(")".to_string()), ")", 7),
            make_eof(),
        ];
        let hint = paren_balance_hint(&tokens).expect("an unmatched `)` must be reported");
        assert!(hint.contains("unmatched `)` at 7..8"), "{hint}");
    }

    #[test]
    fn balanced_delimiters_produce_no_hint() {
        let tokens = vec![
            make_token(TokenKind::Punct("(".to_string()), "(", 0),
            make_token(TokenKind::Punct(")".to_string()), ")", 1),
            make_eof(),
        ];
        assert!(paren_balance_hint(&tokens).is_none());
    }

    /// `Int(expr)` is a parse failure because the production is terminal-only.
    /// The hint has to look BACK from the stall point to say why.
    #[test]
    fn literal_wrapper_around_an_expression_is_explained() {
        let tokens = vec![
            make_token(TokenKind::Keyword("Int".to_string()), "Int", 0),
            make_token(TokenKind::Punct("(".to_string()), "(", 3),
            make_token(TokenKind::Keyword("int_add".to_string()), "int_add", 4),
            make_eof(),
        ];
        let hint = literal_ctor_hint(&tokens, 2).expect("Int( must be recognised");
        assert!(hint.contains("literal wrapper"), "{hint}");
        assert!(hint.contains("an integer literal"), "{hint}");

        // A plain call in the same position must NOT be mistaken for one.
        let plain = vec![
            make_token(TokenKind::Keyword("str_concat".to_string()), "str_concat", 0),
            make_token(TokenKind::Punct("(".to_string()), "(", 10),
            make_token(TokenKind::Keyword("x".to_string()), "x", 11),
            make_eof(),
        ];
        assert!(literal_ctor_hint(&plain, 2).is_none());
    }

    #[test]
    fn test_repeat_zero_or_more() {
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

        // Zero occurrences
        let tokens = vec![make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        let tree = result.unwrap();
        assert_eq!(tree.children.len(), 0);

        // Multiple occurrences
        let tokens = vec![
            make_token(TokenKind::Ident, "x", 0),
            make_token(TokenKind::Ident, "y", 2),
            make_eof(),
        ];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        let tree = result.unwrap();
        assert_eq!(tree.children.len(), 2);
    }

    #[test]
    fn test_repeat_one_or_more() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![Production::Sequence(vec![Element::Repeat(
                Box::new(Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                ))),
                RepeatKind::OneOrMore,
            )])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
            grammar,
        };

        // Zero occurrences - should fail
        let tokens = vec![make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_err());

        // One occurrence - should succeed
        let tokens = vec![make_token(TokenKind::Ident, "x", 0), make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        let tree = result.unwrap();
        assert_eq!(tree.children.len(), 1);
    }

    #[test]
    fn test_optional_element() {
        let mut grammar = HashMap::new();
        grammar.insert(
            "start".to_string(),
            vec![Production::Sequence(vec![Element::Repeat(
                Box::new(Element::Terminal(Terminal::TokenKind(
                    "IDENT".to_string(),
                    TerminalSource::Explicit,
                ))),
                RepeatKind::Optional,
            )])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
            grammar,
        };

        // Present
        let tokens = vec![make_token(TokenKind::Ident, "x", 0), make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().children.len(), 1);

        // Absent
        let tokens = vec![make_eof()];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().children.len(), 0);
    }

    #[test]
    fn test_backtracking_alternation() {
        let mut grammar = HashMap::new();

        // start -> "fn" IDENT | "if" IDENT
        grammar.insert(
            "start".to_string(),
            vec![Production::Alternation(vec![
                Production::Sequence(vec![
                    Element::Terminal(Terminal::Keyword(
                        "fn".to_string(),
                        TerminalSource::Explicit,
                    )),
                    Element::Terminal(Terminal::TokenKind(
                        "IDENT".to_string(),
                        TerminalSource::Explicit,
                    )),
                ]),
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
            ])],
        );

        let spec = ParserSpec {
            start: "start".to_string(),
            grammar,
        };

        // First alternative
        let tokens = vec![
            make_token(TokenKind::Keyword("fn".to_string()), "fn", 0),
            make_token(TokenKind::Ident, "x", 3),
            make_eof(),
        ];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());

        // Second alternative
        let tokens = vec![
            make_token(TokenKind::Keyword("if".to_string()), "if", 0),
            make_token(TokenKind::Ident, "x", 3),
            make_eof(),
        ];
        let result = parse_with_spec(&spec, &tokens);
        assert!(result.is_ok());
    }

    #[test]
    fn test_determinism() {
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
            make_eof(),
        ];

        // Parse twice and compare
        let result1 = parse_with_spec(&spec, &tokens);
        let result2 = parse_with_spec(&spec, &tokens);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert_eq!(result1.unwrap(), result2.unwrap());
    }
}
