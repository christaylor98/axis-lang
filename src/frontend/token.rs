// Token model with span information for spec-driven lexer

use std::fmt;

/// Byte-based span tracking (start and end offsets)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

/// Token kind - the semantic category of a token
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords
    Keyword(String),

    // Identifiers
    Ident,

    // Literals
    IntLit,
    FloatLit,
    BoolLit,
    StringLit,
    UnitLit,

    // Punctuation/Operators
    Punct(String),

    // Whitespace/Comments (usually skipped)
    Whitespace,
    Comment,

    // End of file
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Keyword(kw) => write!(f, "keyword '{}'", kw),
            TokenKind::Ident => write!(f, "identifier"),
            TokenKind::IntLit => write!(f, "int literal"),
            TokenKind::FloatLit => write!(f, "float literal"),
            TokenKind::BoolLit => write!(f, "bool literal"),
            TokenKind::StringLit => write!(f, "string literal"),
            TokenKind::UnitLit => write!(f, "unit literal"),
            TokenKind::Punct(p) => write!(f, "'{}'", p),
            TokenKind::Whitespace => write!(f, "whitespace"),
            TokenKind::Comment => write!(f, "comment"),
            TokenKind::Eof => write!(f, "end of file"),
        }
    }
}

/// Token with kind, lexeme, and span
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, lexeme: String, span: Span) -> Self {
        Token { kind, lexeme, span }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} '{}' @{}..{}",
            self.kind, self.lexeme, self.span.start, self.span.end
        )
    }
}
