use crate::{
    ast::Type,
    token::{Span, TokenKind},
};

// ---------------------------------------------------------------------------
// Lex Errors
// ---------------------------------------------------------------------------

/// Errors produced during lexical analysis.
///
/// Each variant carries a [`Span`] pointing to the error location.
///
/// ```
/// # use cortex::{error::LexError, token::Span};
/// let err = LexError::UnexpectedChar {
///     span: Span {
///         line: 1,
///         column: 5,
///         length: 1,
///     },
///     ch: '@',
/// };
/// assert!(format!("{err}").contains("unexpected character"));
/// ```
///
/// [`Span`]: crate::token::Span
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LexError {
    /// An unrecognized character was encountered.
    #[error("{span}: unexpected character '{ch}'")]
    UnexpectedChar {
        /// Error location.
        span: Span,
        /// The offending character.
        ch: char,
    },

    /// A string literal was not closed before end of line or file.
    #[error("{span}: unterminated string literal")]
    UnterminatedString {
        /// Error location.
        span: Span,
    },

    /// A dedent did not match any previous indentation level.
    #[error("{span}: inconsistent indentation (found {found} spaces)")]
    InconsistentIndent {
        /// Error location.
        span: Span,
        /// The indentation level found.
        found: u32,
    },

    /// An integer literal could not be parsed (overflow or invalid).
    #[error("{span}: invalid integer literal")]
    InvalidInteger {
        /// Error location.
        span: Span,
    },

    /// Tab characters are not permitted; use spaces for indentation.
    #[error("{span}: tab characters are not allowed, use spaces")]
    TabNotAllowed {
        /// Error location.
        span: Span,
    },
}

// ---------------------------------------------------------------------------
// Parse Errors
// ---------------------------------------------------------------------------

/// Errors produced during parsing.
///
/// ```
/// # use cortex::{error::ParseError, token::{Span, TokenKind}};
/// let err = ParseError::Unexpected {
///     span: Span {
///         line: 1,
///         column: 1,
///         length: 1,
///     },
///     expected: "'('".to_owned(),
///     found: TokenKind::Eof,
/// };
/// assert!(format!("{err}").contains("expected"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Found a token that does not fit the current grammar rule.
    #[error("{span}: expected {expected}, found {found}")]
    Unexpected {
        /// Error location.
        span: Span,
        /// What the parser expected.
        expected: String,
        /// What was actually found.
        found: TokenKind,
    },
}

// ---------------------------------------------------------------------------
// Type Errors
// ---------------------------------------------------------------------------

/// Errors produced during type checking.
///
/// ```
/// # use cortex::{error::TypeError, token::Span, ast::Type};
/// let err = TypeError::Mismatch {
///     span: Span {
///         line: 1,
///         column: 1,
///         length: 1,
///     },
///     expected: Type::Int,
///     found: Type::Bool,
/// };
/// assert!(format!("{err}").contains("type mismatch"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TypeError {
    /// An expression's type does not match the expected type.
    #[error("{span}: type mismatch: expected {expected}, found {found}")]
    Mismatch {
        /// Error location.
        span: Span,
        /// The expected type.
        expected: Type,
        /// The actual type.
        found: Type,
    },

    /// A variable was used but never declared.
    #[error("{span}: undefined variable '{name}'")]
    UndefinedVar {
        /// Error location.
        span: Span,
        /// The undefined variable name.
        name: String,
    },

    /// A function was called but never declared.
    #[error("{span}: undefined function '{name}'")]
    UndefinedFn {
        /// Error location.
        span: Span,
        /// The undefined function name.
        name: String,
    },

    /// A function was called with the wrong number of arguments.
    #[error("{span}: '{name}' expects {expected} arguments, found {found}")]
    ArgCount {
        /// Error location.
        span: Span,
        /// The function name.
        name: String,
        /// The expected argument count.
        expected: usize,
        /// The actual argument count.
        found: usize,
    },

    /// A function body does not end with a `returns` statement.
    #[error("{span}: function body must end with 'returns'")]
    MissingReturn {
        /// Error location.
        span: Span,
    },

    /// Two functions share the same name.
    #[error("{span}: duplicate function '{name}'")]
    DuplicateFn {
        /// Error location.
        span: Span,
        /// The duplicate function name.
        name: String,
    },
}
