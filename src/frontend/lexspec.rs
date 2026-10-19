// LexSpec data model - loaded from YAML specification

use serde::{Deserialize, Serialize};

/// Root lexer specification
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LexerSpec {
    pub lexer: LexerConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LexerConfig {
    #[serde(default = "default_charset")]
    pub charset: String,

    #[serde(default = "default_case_sensitive")]
    pub case_sensitive: bool,

    #[serde(default)]
    pub whitespace: Option<WhitespaceRule>,

    #[serde(default)]
    pub comments: Vec<CommentRule>,

    #[serde(default)]
    pub keywords: Vec<String>,

    #[serde(default)]
    pub identifiers: Option<IdentifierRule>,

    #[serde(default)]
    pub literals: Option<LiteralRules>,

    #[serde(default)]
    pub punctuation: Vec<String>,

    /// Opt-in layout tokens. Absent = the lexer behaves exactly as before.
    #[serde(default)]
    pub layout: Option<LayoutRule>,
}

/// Off-side-rule layout (PYAX_SURFACE_EXPAND_V1 d1).
///
/// After ordinary lexing, a post-pass reads the line and column of each token
/// and inserts three zero-width tokens: `newline` at the end of every logical
/// line, `indent` when a line starts further right than the enclosing block,
/// and `dedent` for each block it closes. Inside an open bracket, lines join
/// and nothing is inserted. The tokens are `Punct(<name>)`, so a grammar
/// refers to them as `<PUNCT_NEWLINE>` etc.; the names are config, not code.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutRule {
    /// Only `offside` is defined.
    pub mode: String,
    #[serde(default = "default_newline")]
    pub newline: String,
    #[serde(default = "default_indent")]
    pub indent: String,
    #[serde(default = "default_dedent")]
    pub dedent: String,
    /// Bracket pairs that suspend layout while open, e.g. `["(", ")"]`.
    #[serde(default)]
    pub brackets: Vec<(String, String)>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhitespaceRule {
    pub pattern: String,
    #[serde(default = "default_true")]
    pub skip: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommentRule {
    pub pattern: String,
    #[serde(default = "default_true")]
    pub skip: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IdentifierRule {
    pub pattern: String,
    #[serde(default)]
    pub forbid: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralRules {
    #[serde(default)]
    pub int: Option<IntLiteralRule>,

    #[serde(default)]
    pub float: Option<FloatLiteralRule>,

    #[serde(default)]
    pub bool: Option<BoolLiteralRule>,

    #[serde(default)]
    pub unit: Option<UnitLiteralRule>,

    #[serde(default)]
    pub string: Option<StringLiteralRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FloatLiteralRule {
    pub pattern: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IntLiteralRule {
    pub pattern: String,
    #[serde(default = "default_base_10")]
    pub base: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoolLiteralRule {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UnitLiteralRule {
    pub literal: String,
    #[serde(default = "default_true")]
    pub single_token: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StringLiteralRule {
    pub delimiter: String,
    #[serde(default)]
    pub escapes: Vec<String>,
    #[serde(default = "default_true")]
    pub forbid_newlines: bool,
    /// Further delimiters (e.g. `'`). A string written with one is emitted in
    /// the primary delimiter's form — its own delimiter escape unescaped, the
    /// primary delimiter escaped — so everything downstream sees one form.
    #[serde(default)]
    pub alternate_delimiters: Vec<String>,
    /// Delimiters whose strings may span lines (`"""`); emitted in the
    /// primary form with newlines as `\n` escapes.
    #[serde(default)]
    pub multiline_delimiters: Vec<String>,
    /// Interpolated strings (e.g. `f"a{x}b"`). Absent = none.
    #[serde(default)]
    pub interpolation: Option<InterpolationRule>,
}

/// `f"text{expr}text"` lexes to
/// `Punct(begin) StringLit Punct(open) <expr tokens> Punct(close) StringLit Punct(end)`:
/// the begin/end/open/close tokens are zero-width, the text parts are string
/// literals in the primary delimiter's form, and the expression is lexed in
/// place. `{{` / `}}` are literal braces.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InterpolationRule {
    pub prefixes: Vec<String>,
    pub begin: String,
    pub end: String,
    pub open: String,
    pub close: String,
}

// Default values for serde
fn default_charset() -> String {
    "ascii".to_string()
}

fn default_case_sensitive() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_newline() -> String {
    "NEWLINE".to_string()
}

fn default_indent() -> String {
    "INDENT".to_string()
}

fn default_dedent() -> String {
    "DEDENT".to_string()
}

fn default_base_10() -> u32 {
    10
}
