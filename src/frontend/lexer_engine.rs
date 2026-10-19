// Spec-driven lexer engine - NO hard-coded token rules

use crate::frontend::lexspec::LexerSpec;
use crate::frontend::token::{Span, Token, TokenKind};
use regex::Regex;
use std::fmt;

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub span: Option<Span>,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(s) = &self.span {
            write!(f, "lex error at {}..{}: {}", s.start, s.end, self.message)
        } else {
            write!(f, "lex error: {}", self.message)
        }
    }
}

impl std::error::Error for LexError {}

/// Compiled lexer rules from spec
struct CompiledRules {
    whitespace_re: Option<Regex>,
    comment_res: Vec<Regex>,
    ident_re: Option<Regex>,
    int_re: Option<Regex>,
    float_re: Option<Regex>,
    keywords: Vec<String>,
    bool_values: Vec<String>,
    unit_literal: Option<String>,
    string_delimiter: Option<String>,
    string_escapes: Vec<String>,
    string_forbid_newlines: bool,
    string_alternates: Vec<String>,
    string_multiline: Vec<String>,
    interpolation: Option<crate::frontend::lexspec::InterpolationRule>,
    punctuation: Vec<String>, // Sorted longest-first
}

impl CompiledRules {
    fn from_spec(spec: &LexerSpec) -> Result<Self, LexError> {
        let cfg = &spec.lexer;

        // Compile whitespace pattern
        let whitespace_re = if let Some(ws) = &cfg.whitespace {
            Some(Regex::new(&ws.pattern).map_err(|e| LexError {
                message: format!("invalid whitespace pattern: {}", e),
                span: None,
            })?)
        } else {
            None
        };

        // Compile comment patterns
        let mut comment_res = Vec::new();
        for comment in &cfg.comments {
            let re = Regex::new(&comment.pattern).map_err(|e| LexError {
                message: format!("invalid comment pattern: {}", e),
                span: None,
            })?;
            comment_res.push(re);
        }

        // Compile identifier pattern
        let ident_re = if let Some(ident) = &cfg.identifiers {
            Some(Regex::new(&ident.pattern).map_err(|e| LexError {
                message: format!("invalid identifier pattern: {}", e),
                span: None,
            })?)
        } else {
            None
        };

        // Compile int literal pattern
        let int_re = if let Some(lits) = &cfg.literals {
            if let Some(int_rule) = &lits.int {
                Some(Regex::new(&int_rule.pattern).map_err(|e| LexError {
                    message: format!("invalid int literal pattern: {}", e),
                    span: None,
                })?)
            } else {
                None
            }
        } else {
            None
        };

        // Compile float literal pattern
        let float_re = if let Some(lits) = &cfg.literals {
            if let Some(float_rule) = &lits.float {
                Some(Regex::new(&float_rule.pattern).map_err(|e| LexError {
                    message: format!("invalid float literal pattern: {}", e),
                    span: None,
                })?)
            } else {
                None
            }
        } else {
            None
        };

        // Extract bool values
        let bool_values = if let Some(lits) = &cfg.literals {
            if let Some(bool_rule) = &lits.bool {
                bool_rule.values.clone()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Extract unit literal
        let unit_literal = if let Some(lits) = &cfg.literals {
            lits.unit.as_ref().map(|u| u.literal.clone())
        } else {
            None
        };

        // Extract string literal rules
        let (string_delimiter, string_escapes, string_forbid_newlines) =
            if let Some(lits) = &cfg.literals {
                if let Some(str_rule) = &lits.string {
                    (
                        Some(str_rule.delimiter.clone()),
                        str_rule.escapes.clone(),
                        str_rule.forbid_newlines,
                    )
                } else {
                    (None, Vec::new(), true)
                }
            } else {
                (None, Vec::new(), true)
            };

        // Sort punctuation longest-first for longest-match
        let mut punctuation = cfg.punctuation.clone();
        punctuation.sort_by(|a, b| b.len().cmp(&a.len()));

        Ok(CompiledRules {
            whitespace_re,
            comment_res,
            ident_re,
            int_re,
            float_re,
            keywords: cfg.keywords.clone(),
            bool_values,
            unit_literal,
            string_delimiter,
            string_escapes,
            string_forbid_newlines,
            string_alternates: cfg.literals.as_ref().and_then(|l| l.string.as_ref()).map(|r| r.alternate_delimiters.clone()).unwrap_or_default(),
            string_multiline: cfg.literals.as_ref().and_then(|l| l.string.as_ref()).map(|r| r.multiline_delimiters.clone()).unwrap_or_default(),
            interpolation: cfg.literals.as_ref().and_then(|l| l.string.as_ref()).and_then(|r| r.interpolation.clone()),
            punctuation,
        })
    }
}

/// Lex source code using the provided spec
pub fn lex_with_spec(spec: &LexerSpec, src: &str) -> Result<Vec<Token>, LexError> {
    let rules = CompiledRules::from_spec(spec)?;
    let tokens = lex_range(&rules, src, 0, src.len())?;
    let pos = src.len();

    let mut tokens = match &spec.lexer.layout {
        Some(layout) => apply_layout(layout, src, tokens)?,
        None => tokens,
    };

    // Add EOF token
    tokens.push(Token::new(
        TokenKind::Eof,
        String::new(),
        Span::new(pos, pos),
    ));

    Ok(tokens)
}

/// Lex `src[from..to]` into tokens with absolute spans. The whole file is one
/// call; an interpolated string's expression part is another, in place.
fn lex_range(rules: &CompiledRules, src: &str, from: usize, to: usize) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut pos = from;

    while pos < to {
        let start_pos = pos;

        // Try whitespace
        if let Some(ws_re) = &rules.whitespace_re {
            if let Some(m) = ws_re.find(&src[pos..to]) {
                if m.start() == 0 {
                    pos += m.end();
                    // Skip whitespace - don't emit token
                    continue;
                }
            }
        }

        // Try comments
        let mut comment_matched = false;
        for comment_re in &rules.comment_res {
            if let Some(m) = comment_re.find(&src[pos..to]) {
                if m.start() == 0 {
                    pos += m.end();
                    comment_matched = true;
                    // Skip comment - don't emit token
                    break;
                }
            }
        }
        if comment_matched {
            continue;
        }

        // Interpolated string (`f"..."`): before identifiers, which would
        // otherwise take the prefix.
        if let Some(ip) = &rules.interpolation {
            if let Some((toks, end)) = lex_interpolated(rules, ip, src, pos, to)? {
                tokens.extend(toks);
                pos = end;
                continue;
            }
        }

        // Try unit literal "()" - must come before punctuation
        if let Some(unit_lit) = &rules.unit_literal {
            if src[pos..to].starts_with(unit_lit) {
                let end_pos = pos + unit_lit.len();
                tokens.push(Token::new(
                    TokenKind::UnitLit,
                    unit_lit.clone(),
                    Span::new(start_pos, end_pos),
                ));
                pos = end_pos;
                continue;
            }
        }

        // Try punctuation (longest match)
        let mut punct_matched = false;
        for punct in &rules.punctuation {
            if src[pos..to].starts_with(punct) {
                let end_pos = pos + punct.len();
                tokens.push(Token::new(
                    TokenKind::Punct(punct.clone()),
                    punct.clone(),
                    Span::new(start_pos, end_pos),
                ));
                pos = end_pos;
                punct_matched = true;
                break;
            }
        }
        if punct_matched {
            continue;
        }

        // Try string literal
        if let Some(delim) = &rules.string_delimiter {
            // Multi-line delimiters first: `"""` begins with `"`.
            let mut ml_matched = false;
            for md in &rules.string_multiline {
                if src[pos..to].starts_with(md.as_str()) {
                    match lex_string(&src[pos..to], md, &rules.string_escapes, false) {
                        Ok((lexeme, consumed)) => {
                            let inner = &lexeme[md.len()..lexeme.len() - md.len()];
                            let end_pos = pos + consumed;
                            tokens.push(Token::new(
                                TokenKind::StringLit,
                                canonical_multiline(inner, delim),
                                Span::new(start_pos, end_pos),
                            ));
                            pos = end_pos;
                            ml_matched = true;
                            break;
                        }
                        Err(msg) => {
                            return Err(LexError { message: msg, span: Some(Span::new(start_pos, pos + 1)) });
                        }
                    }
                }
            }
            if ml_matched {
                continue;
            }
            if src[pos..to].starts_with(delim) {
                match lex_string(
                    &src[pos..to],
                    delim,
                    &rules.string_escapes,
                    rules.string_forbid_newlines,
                ) {
                    Ok((lexeme, consumed)) => {
                        let end_pos = pos + consumed;
                        tokens.push(Token::new(
                            TokenKind::StringLit,
                            lexeme,
                            Span::new(start_pos, end_pos),
                        ));
                        pos = end_pos;
                        continue;
                    }
                    Err(msg) => {
                        return Err(LexError {
                            message: msg,
                            span: Some(Span::new(start_pos, pos + 1)),
                        });
                    }
                }
            }
            let mut alt_matched = false;
            for alt in &rules.string_alternates {
                if src[pos..to].starts_with(alt.as_str()) {
                    let escapes = alt_escapes(&rules.string_escapes, delim, alt);
                    match lex_string(&src[pos..to], alt, &escapes, rules.string_forbid_newlines) {
                        Ok((lexeme, consumed)) => {
                            let inner = &lexeme[alt.len()..lexeme.len() - alt.len()];
                            let end_pos = pos + consumed;
                            tokens.push(Token::new(
                                TokenKind::StringLit,
                                canonical_string(inner, delim, alt),
                                Span::new(start_pos, end_pos),
                            ));
                            pos = end_pos;
                            alt_matched = true;
                            break;
                        }
                        Err(msg) => {
                            return Err(LexError {
                                message: msg,
                                span: Some(Span::new(start_pos, pos + 1)),
                            });
                        }
                    }
                }
            }
            if alt_matched {
                continue;
            }
        }

        // Try float literal BEFORE int (float pattern is longer; must win over int prefix)
        if let Some(float_re) = &rules.float_re {
            if let Some(m) = float_re.find(&src[pos..to]) {
                if m.start() == 0 {
                    let lexeme = m.as_str().to_string();
                    let end_pos = pos + m.end();
                    tokens.push(Token::new(
                        TokenKind::FloatLit,
                        lexeme,
                        Span::new(start_pos, end_pos),
                    ));
                    pos = end_pos;
                    continue;
                }
            }
        }

        // Try int literal
        if let Some(int_re) = &rules.int_re {
            if let Some(m) = int_re.find(&src[pos..to]) {
                if m.start() == 0 {
                    let lexeme = m.as_str().to_string();
                    let end_pos = pos + m.end();
                    tokens.push(Token::new(
                        TokenKind::IntLit,
                        lexeme,
                        Span::new(start_pos, end_pos),
                    ));
                    pos = end_pos;
                    continue;
                }
            }
        }

        // Try bool literals (must come BEFORE identifiers to avoid keyword collision)
        let mut bool_matched = false;
        for bool_val in &rules.bool_values {
            if src[pos..to].starts_with(bool_val) {
                // Check boundary (not followed by alphanumeric or _)
                let end_idx = pos + bool_val.len();
                let at_boundary = end_idx >= to || {
                    let next_char = src[end_idx..to].chars().next().unwrap();
                    !next_char.is_alphanumeric() && next_char != '_'
                };

                if at_boundary {
                    tokens.push(Token::new(
                        TokenKind::BoolLit,
                        bool_val.clone(),
                        Span::new(start_pos, end_idx),
                    ));
                    pos = end_idx;
                    bool_matched = true;
                    break;
                }
            }
        }
        if bool_matched {
            continue;
        }

        // Try identifier (also checks for keywords)
        if let Some(ident_re) = &rules.ident_re {
            if let Some(m) = ident_re.find(&src[pos..to]) {
                if m.start() == 0 {
                    let lexeme = m.as_str().to_string();
                    let end_pos = pos + m.end();

                    // Check if it's a keyword OR bool value (keywords take precedence over identifiers)
                    let kind = if rules.keywords.contains(&lexeme) {
                        TokenKind::Keyword(lexeme.clone())
                    } else if rules.bool_values.contains(&lexeme) {
                        // Handle bool values that are also listed as keywords
                        TokenKind::Keyword(lexeme.clone())
                    } else {
                        TokenKind::Ident
                    };

                    tokens.push(Token::new(kind, lexeme, Span::new(start_pos, end_pos)));
                    pos = end_pos;
                    continue;
                }
            }
        }

        // Unknown character - error
        let bad_char = src[pos..to].chars().next().unwrap_or('?');
        return Err(LexError {
            message: format!("unexpected character '{}'", bad_char),
            span: Some(Span::new(pos, pos + bad_char.len_utf8())),
        });
    }

    Ok(tokens)
}

/// Off-side-rule post-pass (see `LayoutRule`). Runs on the finished token
/// stream, so the whitespace and comment paths above are untouched: a spec
/// without `layout` never reaches this function.
///
/// Columns are byte offsets from the line start. A tab in the indentation of
/// a token-bearing line is an error rather than a guess at its width.
fn apply_layout(
    layout: &crate::frontend::lexspec::LayoutRule,
    src: &str,
    tokens: Vec<Token>,
) -> Result<Vec<Token>, LexError> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |at: usize| starts.partition_point(|&s| s <= at) - 1;
    let line_start = |at: usize| starts[line_of(at)];
    let mark = |name: &str, at: usize| Token::new(TokenKind::Punct(name.to_string()), String::new(), Span::new(at, at));

    let mut out = Vec::with_capacity(tokens.len() * 2);
    let mut stack: Vec<usize> = vec![0];
    let mut depth: usize = 0;
    let mut prev: Option<(usize, usize)> = None; // (line, end offset) of the previous token

    for tok in tokens {
        let line = line_of(tok.span.start);
        let starts_line = prev.map_or(true, |(l, _)| line > l);
        if depth == 0 && starts_line {
            let ls = line_start(tok.span.start);
            let indent = &src[ls..tok.span.start];
            if indent.contains('\t') {
                return Err(LexError {
                    message: "tab in indentation; indent with spaces".to_string(),
                    span: Some(Span::new(ls, tok.span.start)),
                });
            }
            let col = tok.span.start - ls;
            if let Some((_, end)) = prev {
                out.push(mark(&layout.newline, end));
            }
            let top = *stack.last().unwrap();
            if col > top {
                if prev.is_none() {
                    return Err(LexError {
                        message: "unexpected indent on the first line".to_string(),
                        span: Some(Span::new(ls, tok.span.start)),
                    });
                }
                stack.push(col);
                out.push(mark(&layout.indent, tok.span.start));
            } else if col < top {
                while *stack.last().unwrap() > col {
                    stack.pop();
                    out.push(mark(&layout.dedent, tok.span.start));
                }
                if *stack.last().unwrap() != col {
                    return Err(LexError {
                        message: format!(
                            "dedent to column {} matches no enclosing block (blocks start at {:?})",
                            col, stack
                        ),
                        span: Some(Span::new(ls, tok.span.start)),
                    });
                }
            }
        }
        if let TokenKind::Punct(p) = &tok.kind {
            if layout.brackets.iter().any(|(open, _)| open == p) {
                depth += 1;
            } else if layout.brackets.iter().any(|(_, close)| close == p) {
                depth = depth.saturating_sub(1);
            }
        }
        prev = Some((line, tok.span.end));
        out.push(tok);
    }

    if let Some((_, end)) = prev {
        out.push(mark(&layout.newline, end));
        while stack.len() > 1 {
            stack.pop();
            out.push(mark(&layout.dedent, end));
        }
    }
    Ok(out)
}

/// The escapes of an alternate delimiter: the primary's delimiter escape
/// (`\"`) becomes the alternate's (`\'`).
fn alt_escapes(escapes: &[String], primary: &str, alt: &str) -> Vec<String> {
    let p = format!("\\{}", primary);
    escapes.iter().map(|e| if *e == p { format!("\\{}", alt) } else { e.clone() }).collect()
}

/// A string body written between `alt` delimiters, re-written between
/// `primary` delimiters: `\'` -> `'`, a bare `"` -> `\"`, other escapes kept.
fn canonical_string(inner: &str, primary: &str, alt: &str) -> String {
    let mut out = String::new();
    let alt_esc = format!("\\{}", alt);
    let mut i = 0;
    while i < inner.len() {
        let rest = &inner[i..];
        if rest.starts_with(alt_esc.as_str()) {
            out.push_str(alt);
            i += alt_esc.len();
        } else if rest.starts_with('\\') {
            let next = rest[1..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
            out.push_str(&rest[..1 + next]);
            i += 1 + next;
        } else if rest.starts_with(primary) {
            out.push('\\');
            out.push_str(primary);
            i += primary.len();
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            i += c.len_utf8();
        }
    }
    format!("{}{}{}", primary, out, primary)
}

/// A multi-line string body in the primary form: escapes kept, bare primary
/// delimiters escaped, newlines as `\n`.
fn canonical_multiline(inner: &str, primary: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push('\\');
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '\n' => out.push_str("\\n"),
            '\r' => {}
            _ if primary.starts_with(c) => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    format!("{}{}{}", primary, out, primary)
}

/// An interpolated string at `pos`, if one starts there.
fn lex_interpolated(
    rules: &CompiledRules,
    ip: &crate::frontend::lexspec::InterpolationRule,
    src: &str,
    pos: usize,
    to: usize,
) -> Result<Option<(Vec<Token>, usize)>, LexError> {
    let primary = match &rules.string_delimiter {
        Some(d) => d.clone(),
        None => return Ok(None),
    };
    let mut delims = vec![primary.clone()];
    delims.extend(rules.string_alternates.iter().cloned());
    let (prefix, d) = match ip.prefixes.iter().find_map(|p| {
        let after = &src[pos..to];
        if !after.starts_with(p.as_str()) {
            return None;
        }
        delims.iter().find(|d| after[p.len()..].starts_with(d.as_str())).map(|d| (p.clone(), d.clone()))
    }) {
        Some(x) => x,
        None => return Ok(None),
    };
    let mark = |name: &str, at: usize| Token::new(TokenKind::Punct(name.to_string()), String::new(), Span::new(at, at));
    let fail = |m: &str, a: usize, b: usize| Err(LexError { message: m.to_string(), span: Some(Span::new(a, b)) });
    let mut toks = vec![mark(&ip.begin, pos)];
    let mut i = pos + prefix.len() + d.len();
    let mut buf = String::new();
    let mut text_start = i;
    let flush = |buf: &mut String, toks: &mut Vec<Token>, a: usize, b: usize| {
        if !buf.is_empty() {
            toks.push(Token::new(TokenKind::StringLit, canonical_string(buf, &primary, &d), Span::new(a, b)));
            buf.clear();
        }
    };
    loop {
        if i >= to {
            return fail("unterminated interpolated string", pos, to);
        }
        let rest = &src[i..to];
        if rest.starts_with(d.as_str()) {
            flush(&mut buf, &mut toks, text_start, i);
            let end = i + d.len();
            toks.push(mark(&ip.end, end));
            return Ok(Some((toks, end)));
        }
        let c = rest.chars().next().unwrap();
        if c == '\n' {
            return fail("unterminated interpolated string (newline not allowed)", pos, i);
        }
        if c == '\\' {
            let n = rest[1..].chars().next().map(|c| c.len_utf8()).unwrap_or(0);
            buf.push_str(&rest[..1 + n]);
            i += 1 + n;
            continue;
        }
        if rest.starts_with("{{") || rest.starts_with("}}") {
            buf.push(c);
            i += 2;
            continue;
        }
        if c == '}' {
            return fail("a single `}` in an interpolated string must be written `}}`", i, i + 1);
        }
        if c != '{' {
            buf.push(c);
            i += c.len_utf8();
            continue;
        }
        // `{ expr }`: find the matching `}`.
        flush(&mut buf, &mut toks, text_start, i);
        let open = i;
        let mut j = i + 1;
        let mut depth = 0i32;
        while j < to {
            let r = &src[j..to];
            let ch = r.chars().next().unwrap();
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' => depth -= 1,
                '}' if depth == 0 => break,
                '}' => depth -= 1,
                '"' | '\'' => {
                    // a string inside the expression: skip to its end
                    let q = ch;
                    j += 1;
                    while j < to && !src[j..to].starts_with(q) {
                        j += src[j..to].chars().next().unwrap().len_utf8();
                    }
                }
                '!' if depth == 0 && !r.starts_with("!=") => {
                    return fail("conversions (`{x!r}`) are not in this language", j, j + 1);
                }
                ':' if depth == 0 => {
                    return fail("format specs (`{x:...}`) are not in this language; format the value first, e.g. `str(x)`", j, j + 1);
                }
                '\n' => return fail("unterminated `{` in an interpolated string", open, j),
                _ => {}
            }
            j += src[j..to].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
        }
        if j >= to {
            return fail("unterminated `{` in an interpolated string", open, to);
        }
        let inner = lex_range(rules, src, open + 1, j)?;
        if inner.is_empty() {
            return fail("empty `{}` in an interpolated string", open, j + 1);
        }
        toks.push(mark(&ip.open, open));
        toks.extend(inner);
        toks.push(mark(&ip.close, j));
        i = j + 1;
        text_start = i;
    }
}

/// Lex a string literal with escape handling
fn lex_string(
    src: &str,
    delimiter: &str,
    escapes: &[String],
    forbid_newlines: bool,
) -> Result<(String, usize), String> {
    if !src.starts_with(delimiter) {
        return Err("not a string literal".to_string());
    }

    let mut pos = delimiter.len();
    let mut result = String::new();
    result.push_str(delimiter);

    while pos < src.len() {
        // Check for closing delimiter
        if src[pos..].starts_with(delimiter) {
            result.push_str(delimiter);
            pos += delimiter.len();
            return Ok((result, pos));
        }

        // Check for newline
        if forbid_newlines {
            let ch = src[pos..].chars().next().unwrap();
            if ch == '\n' || ch == '\r' {
                return Err("unterminated string literal (newline not allowed)".to_string());
            }
        }

        // Check for escape sequences
        let mut escape_matched = false;
        for esc in escapes {
            if src[pos..].starts_with(esc) {
                result.push_str(esc);
                pos += esc.len();
                escape_matched = true;
                break;
            }
        }

        if !escape_matched {
            let ch = src[pos..].chars().next().unwrap();
            result.push(ch);
            pos += ch.len_utf8();
        }
    }

    Err("unterminated string literal".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexspec_load::load_spec;
    use std::path::PathBuf;

    #[test]
    fn lex_simple_code() {
        let spec_path = PathBuf::from("lang-lab-poc-userfiles/lexer.yaml");
        if !spec_path.exists() {
            return; // Skip if spec not available
        }

        let spec = load_spec(&spec_path).unwrap();
        let src = "fn test() {}";
        let tokens = lex_with_spec(&spec, src).unwrap();

        // Should have: fn, test, (), {, }, EOF = 6 tokens (note: () is single unit literal)
        assert_eq!(tokens.len(), 6);
        assert!(matches!(tokens[0].kind, TokenKind::Keyword(_)));
        assert_eq!(tokens[0].lexeme, "fn");
        assert!(matches!(tokens[1].kind, TokenKind::Ident));
        assert_eq!(tokens[1].lexeme, "test");
    }

    #[test]
    fn lex_preserves_spans() {
        let spec_path = PathBuf::from("lang-lab-poc-userfiles/lexer.yaml");
        if !spec_path.exists() {
            return;
        }

        let spec = load_spec(&spec_path).unwrap();
        let src = "fn";
        let tokens = lex_with_spec(&spec, src).unwrap();

        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 2);
    }

    // ── layout: offside (PYAX_SURFACE_EXPAND_V1 d1) ─────────────────────

    fn layout_spec() -> crate::frontend::lexspec::LexerSpec {
        serde_yaml::from_str(
            r#"
lexer:
  whitespace: { pattern: '^[ \t\r\n]+' }
  comments: [ { pattern: '^#[^\n]*' } ]
  keywords: [def, return, if, else]
  identifiers: { pattern: '^[A-Za-z_][A-Za-z0-9_]*' }
  literals: { int: { pattern: '^[0-9]+' } }
  punctuation: [":", "(", ")", ",", "+"]
  layout: { mode: offside, brackets: [["(", ")"]] }
"#,
        )
        .unwrap()
    }

    /// Token names, with layout tokens shown as their names and others as lexemes.
    fn shape(src: &str) -> Vec<String> {
        lex_with_spec(&layout_spec(), src)
            .unwrap()
            .into_iter()
            .map(|t| match t.kind {
                TokenKind::Punct(p) if t.lexeme.is_empty() => p,
                TokenKind::Eof => "EOF".to_string(),
                _ => t.lexeme,
            })
            .collect()
    }

    #[test]
    fn layout_blocks_newlines_and_dedents() {
        let src = "def f(x):\n    if x:\n        return 1\n    return 2\n";
        assert_eq!(
            shape(src).join(" "),
            "def f ( x ) : NEWLINE INDENT if x : NEWLINE INDENT return 1 NEWLINE DEDENT \
             return 2 NEWLINE DEDENT EOF"
        );
    }

    #[test]
    fn layout_ignores_blank_lines_comments_and_bracketed_continuations() {
        let got = shape("def f(x,\n      y):\n\n    # note\n    return x\n").join(" ");
        assert_eq!(got, "def f ( x , y ) : NEWLINE INDENT return x NEWLINE DEDENT EOF");
    }

    #[test]
    fn layout_zero_width_tokens_carry_real_spans() {
        let toks = lex_with_spec(&layout_spec(), "def f():\n    return 1\n").unwrap();
        let indent = toks.iter().find(|t| t.kind == TokenKind::Punct("INDENT".into())).unwrap();
        assert_eq!((indent.span.start, indent.span.end), (13, 13)); // at `return`
    }

    #[test]
    fn layout_rejects_inconsistent_dedent_and_tabs() {
        let spec = layout_spec();
        let e = lex_with_spec(&spec, "def f():\n    if x:\n        return 1\n  return 2\n").unwrap_err();
        assert!(e.message.contains("matches no enclosing block"), "{}", e.message);
        let e = lex_with_spec(&spec, "def f():\n\treturn 1\n").unwrap_err();
        assert!(e.message.contains("tab in indentation"), "{}", e.message);
        let e = lex_with_spec(&spec, "  def f():\n").unwrap_err();
        assert!(e.message.contains("first line"), "{}", e.message);
    }

    #[test]
    fn no_layout_key_means_no_layout_tokens() {
        let mut spec = layout_spec();
        spec.lexer.layout = None;
        let toks = lex_with_spec(&spec, "def f():\n    return 1\n").unwrap();
        assert!(toks.iter().all(|t| !t.lexeme.is_empty() || t.kind == TokenKind::Eof));
    }

    // ── alternate delimiters + interpolated strings (PYAX_SURFACE_EXPAND_V1 2b) ──

    fn string_spec() -> crate::frontend::lexspec::LexerSpec {
        serde_yaml::from_str(
            r#"
lexer:
  whitespace: { pattern: '^[ \t\r\n]+' }
  identifiers: { pattern: '^[A-Za-z_][A-Za-z0-9_]*' }
  literals:
    int: { pattern: '^[0-9]+' }
    string:
      delimiter: '"'
      escapes: ['\"', '\\', '\n']
      alternate_delimiters: ["'"]
      interpolation: { prefixes: [f], begin: FB, end: FE, open: FO, close: FC }
  punctuation: ["+", "[", "]", "(", ")", "!="]
"#,
        )
        .unwrap()
    }

    fn lexemes(src: &str) -> Vec<String> {
        lex_with_spec(&string_spec(), src)
            .unwrap()
            .into_iter()
            .map(|t| match t.kind {
                TokenKind::Punct(p) if t.lexeme.is_empty() => p,
                TokenKind::Eof => "EOF".to_string(),
                _ => t.lexeme,
            })
            .collect()
    }

    #[test]
    fn alternate_delimiter_is_canonicalised() {
        assert_eq!(lexemes(r#"'it\'s "x"'"#), vec![r#""it's \"x\"""#, "EOF"]);
        assert_eq!(lexemes(r#""a" + 'b'"#), vec![r#""a""#, "+", r#""b""#, "EOF"]);
    }

    #[test]
    fn interpolated_string_lexes_in_place() {
        assert_eq!(
            lexemes(r#"f"n={n + 1}!" f'{xs[0]}{{}}'"#),
            vec!["FB", r#""n=""#, "FO", "n", "+", "1", "FC", r#""!""#, "FE", "FB", "FO", "xs", "[", "0", "]", "FC", r#""{}""#, "FE", "EOF"]
        );
        // an identifier merely starting with the prefix is still an identifier
        assert_eq!(lexemes("fx"), vec!["fx", "EOF"]);
        // `!=` inside braces is an operator, not a conversion
        assert_eq!(lexemes(r#"f"{a != b}""#), vec!["FB", "FO", "a", "!=", "b", "FC", "FE", "EOF"]);
    }

    #[test]
    fn interpolated_string_rejects_format_specs() {
        let e = lex_with_spec(&string_spec(), r#"f"{x:.2f}""#).unwrap_err();
        assert!(e.message.contains("format specs"), "{}", e.message);
        let e = lex_with_spec(&string_spec(), r#"f"{x!r}""#).unwrap_err();
        assert!(e.message.contains("conversions"), "{}", e.message);
    }
}
