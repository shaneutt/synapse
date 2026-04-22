use std::fmt;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Position in source code for error reporting and diagnostics.
///
/// ```
/// # use cortex::token::Span;
/// let span = Span {
///     line: 1,
///     column: 5,
///     length: 3,
/// };
/// assert_eq!(format!("{span}"), "1:5");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Length of the spanned text in bytes.
    pub length: u32,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Classifies a [`Token`] by syntactic role.
///
/// ```
/// # use cortex::token::TokenKind;
/// let kw = TokenKind::Function;
/// assert_eq!(format!("{kw:?}"), "Function");
/// ```
///
/// [`Token`]: crate::token::Token
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// `builtins` keyword.
    Builtins,
    /// `Cons` constructor keyword.
    Cons,
    /// `function` keyword.
    Function,
    /// `import` keyword.
    Import,
    /// `match` keyword.
    Match,
    /// `Nil` literal keyword.
    Nil,
    /// `otherwise` keyword.
    Otherwise,
    /// `pub` keyword.
    Pub,
    /// `returns` keyword.
    Returns,
    /// `rust` keyword.
    Rust,
    /// `value` keyword.
    Value,
    /// `when` keyword.
    When,
    /// Integer literal.
    IntLit(i64),
    /// Boolean literal (`true` / `false`).
    BoolLit(bool),
    /// Double-quoted string literal.
    StringLit(String),
    /// A named identifier.
    Identifier(String),
    /// `+` operator.
    Plus,
    /// `-` operator.
    Minus,
    /// `*` operator.
    Star,
    /// `/` operator.
    Slash,
    /// `%` operator.
    Percent,
    /// `==` operator.
    EqualEqual,
    /// `!=` operator.
    BangEqual,
    /// `<` operator.
    LessThan,
    /// `>` operator.
    GreaterThan,
    /// `<=` operator.
    LessEqual,
    /// `>=` operator.
    GreaterEqual,
    /// `&&` operator.
    AmpAmp,
    /// `||` operator.
    PipePipe,
    /// `=` assignment.
    Equals,
    /// `->` arrow.
    Arrow,
    /// `,` separator.
    Comma,
    /// `.` dot for qualified access.
    Dot,
    /// `(` opening parenthesis.
    OpenParen,
    /// `)` closing parenthesis.
    CloseParen,
    /// `:` colon.
    Colon,
    /// Increase in indentation level.
    Indent,
    /// Decrease in indentation level.
    Dedent,
    /// End of line.
    Newline,
    /// End of input.
    Eof,
}

impl TokenKind {
    /// Returns a human-readable label for error messages.
    ///
    /// ```
    /// # use cortex::token::TokenKind;
    /// assert_eq!(TokenKind::Plus.describe(), "'+'");
    /// assert_eq!(TokenKind::Eof.describe(), "end of file");
    /// ```
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Builtins => "'builtins'",
            Self::Cons => "'Cons'",
            Self::Function => "'function'",
            Self::Import => "'import'",
            Self::Match => "'match'",
            Self::Nil => "'Nil'",
            Self::Otherwise => "'otherwise'",
            Self::Pub => "'pub'",
            Self::Returns => "'returns'",
            Self::Rust => "'rust'",
            Self::Value => "'value'",
            Self::When => "'when'",
            Self::IntLit(_) => "integer literal",
            Self::BoolLit(_) => "boolean literal",
            Self::StringLit(_) => "string literal",
            Self::Identifier(_) => "identifier",
            Self::Plus => "'+'",
            Self::Minus => "'-'",
            Self::Star => "'*'",
            Self::Slash => "'/'",
            Self::Percent => "'%'",
            Self::EqualEqual => "'=='",
            Self::BangEqual => "'!='",
            Self::LessThan => "'<'",
            Self::GreaterThan => "'>'",
            Self::LessEqual => "'<='",
            Self::GreaterEqual => "'>='",
            Self::AmpAmp => "'&&'",
            Self::PipePipe => "'||'",
            Self::Equals => "'='",
            Self::Arrow => "'->'",
            Self::Comma => "','",
            Self::Dot => "'.'",
            Self::OpenParen => "'('",
            Self::CloseParen => "')'",
            Self::Colon => "':'",
            Self::Indent => "indent",
            Self::Dedent => "dedent",
            Self::Newline => "newline",
            Self::Eof => "end of file",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.describe())
    }
}

/// A lexical token with its [`TokenKind`] and source [`Span`].
///
/// ```
/// # use cortex::token::{Token, TokenKind, Span};
/// let tok = Token {
///     kind: TokenKind::IntLit(42),
///     span: Span {
///         line: 1,
///         column: 1,
///         length: 2,
///     },
/// };
/// assert_eq!(tok.kind, TokenKind::IntLit(42));
/// ```
///
/// [`TokenKind`]: crate::token::TokenKind
/// [`Span`]: crate::token::Span
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The syntactic classification of this token.
    pub kind: TokenKind,
    /// Source location of this token.
    pub span: Span,
}
