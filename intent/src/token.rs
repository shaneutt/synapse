use std::fmt;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Position in an intent source file.
///
/// ```
/// # use intent::token::Span;
/// let span = Span { line: 1, column: 5 };
/// assert_eq!(format!("{span}"), "1:5");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Classifies an intent [`Token`] by syntactic role.
///
/// ```
/// # use intent::token::TokenKind;
/// let kw = TokenKind::Types;
/// assert_eq!(format!("{kw:?}"), "Types");
/// ```
///
/// [`Token`]: crate::token::Token
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// `application` keyword.
    Application,
    /// `types` keyword.
    Types,
    /// `module` keyword.
    Module,
    /// `capability` keyword.
    Capability,
    /// `pipeline` keyword.
    Pipeline,
    /// `input` keyword.
    Input,
    /// `output` keyword.
    Output,
    /// `intent` keyword.
    Intent,
    /// `args` keyword.
    Args,
    /// `verb` keyword.
    Verb,
    /// `flag` keyword.
    Flag,
    /// `positional` keyword.
    Positional,
    /// `environment` keyword.
    Environment,
    /// `from` keyword.
    From,
    /// `default` keyword.
    Default,
    /// `capabilities` keyword.
    Capabilities,
    /// `description` keyword.
    Description,
    /// `properties` keyword.
    Properties,
    /// `new` keyword.
    New,
    /// `import` keyword.
    Import,
    /// `crate` keyword.
    Crate,
    /// `uses` keyword.
    Uses,
    /// `rust` keyword.
    Rust,
    /// `--` prefix before flag names.
    DashDash,
    /// `:` separator.
    Colon,
    /// `-` list item marker.
    Dash,
    /// `->` arrow for pipeline steps.
    Arrow,
    /// `,` separator.
    Comma,
    /// `(` opening parenthesis.
    OpenParen,
    /// `)` closing parenthesis.
    CloseParen,
    /// `<` opening angle bracket for generics.
    LessThan,
    /// `>` closing angle bracket for generics.
    GreaterThan,
    /// A named identifier.
    Identifier(String),
    /// Free-form text (the intent phrase after `intent:`).
    FreeText(String),
    /// Increase in indentation level.
    Indent,
    /// Decrease in indentation level.
    Dedent,
    /// End of a line.
    Newline,
    /// End of input.
    Eof,
}

impl TokenKind {
    /// Returns a human-readable label for error messages.
    ///
    /// ```
    /// # use intent::token::TokenKind;
    /// assert_eq!(TokenKind::Colon.describe(), "':'");
    /// assert_eq!(TokenKind::Eof.describe(), "end of file");
    /// ```
    pub fn describe(&self) -> &str {
        match self {
            Self::Application => "'application'",
            Self::Types => "'types'",
            Self::Module => "'module'",
            Self::Capability => "'capability'",
            Self::Pipeline => "'pipeline'",
            Self::Input => "'input'",
            Self::Output => "'output'",
            Self::Intent => "'intent'",
            Self::Args => "'args'",
            Self::Verb => "'verb'",
            Self::Flag => "'flag'",
            Self::Positional => "'positional'",
            Self::Environment => "'environment'",
            Self::From => "'from'",
            Self::Default => "'default'",
            Self::Capabilities => "'capabilities'",
            Self::Description => "'description'",
            Self::Properties => "'properties'",
            Self::New => "'new'",
            Self::Import => "'import'",
            Self::Crate => "'crate'",
            Self::Uses => "'uses'",
            Self::Rust => "'rust'",
            Self::DashDash => "'--'",
            Self::Colon => "':'",
            Self::Dash => "'-'",
            Self::Arrow => "'->'",
            Self::Comma => "','",
            Self::OpenParen => "'('",
            Self::CloseParen => "')'",
            Self::LessThan => "'<'",
            Self::GreaterThan => "'>'",
            Self::Identifier(_) => "identifier",
            Self::FreeText(_) => "intent phrase",
            Self::Indent => "indent",
            Self::Dedent => "dedent",
            Self::Newline => "newline",
            Self::Eof => "end of file",
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(s) => write!(f, "identifier '{s}'"),
            Self::FreeText(s) => write!(f, "intent phrase '{s}'"),
            other => f.write_str(other.describe()),
        }
    }
}

/// A lexical token with its [`TokenKind`] and source [`Span`].
///
/// ```
/// # use intent::token::{Token, TokenKind, Span};
/// let tok = Token {
///     kind: TokenKind::Types,
///     span: Span { line: 1, column: 1 },
/// };
/// assert_eq!(tok.kind, TokenKind::Types);
/// ```
///
/// [`TokenKind`]: crate::token::TokenKind
/// [`Span`]: crate::token::Span
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The syntactic classification of this token.
    pub kind: TokenKind,
    /// Source location.
    pub span: Span,
}
