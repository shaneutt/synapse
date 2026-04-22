#![deny(unsafe_code)]
//! Cortex: the Synapse compiler library.
//!
//! Provides lexing, parsing, type-checking, and Rust code emission
//! for the Synapse programming language.

/// Abstract syntax tree for parsed Synapse programs.
pub mod ast;
/// Type checker: validates types and produces a typed AST.
pub mod checker;
/// Rust code emitter: translates a typed AST to Rust source.
pub mod emitter;
/// Error types for each compilation phase.
pub mod error;
/// Lexer: tokenizes Synapse source with indentation tracking.
pub mod lexer;
/// Module discovery and API extraction for multi-file programs.
pub mod module;
/// Recursive-descent parser: tokens to AST.
pub mod parser;
/// Token and span types used by the lexer and parser.
pub mod token;
/// Typed AST produced by the type checker.
pub mod typed_ast;

use std::fmt;

use typed_ast::TypedProgram;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Errors from [`compile_check`].
///
/// Wraps lex, parse, and type errors into a single type for callers
/// that only care whether the source compiles.
///
/// ```
/// # use cortex::compile_check;
/// let err = compile_check("function f() -> Int\n  returns true\n").unwrap_err();
/// assert!(err.to_string().contains("type mismatch"));
/// ```
///
/// [`compile_check`]: crate::compile_check
#[derive(Debug)]
pub enum CompileCheckError {
    /// A lexing error.
    Lex(error::LexError),
    /// A parsing error.
    Parse(error::ParseError),
    /// A type-checking error.
    Type(error::TypeError),
}

impl fmt::Display for CompileCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(e) => write!(f, "lex error: {e}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Type(e) => write!(f, "type error: {e}"),
        }
    }
}

impl std::error::Error for CompileCheckError {}

/// Runs the full cortex front-end (lex, parse, type-check) on source code.
///
/// Returns the [`TypedProgram`] on success, or a [`CompileCheckError`]
/// describing the first failure.
///
/// # Errors
///
/// Returns [`CompileCheckError`] if lexing, parsing, or type-checking fails.
///
/// ```
/// # use cortex::compile_check;
/// let typed = compile_check("function f() -> Int\n  returns 42\n").unwrap();
/// assert_eq!(typed.declarations.len(), 1);
/// ```
///
/// [`TypedProgram`]: crate::typed_ast::TypedProgram
/// [`CompileCheckError`]: crate::CompileCheckError
pub fn compile_check(source: &str) -> Result<TypedProgram, CompileCheckError> {
    let tokens = lexer::lex(source).map_err(CompileCheckError::Lex)?;
    let ast = parser::parse(&tokens).map_err(CompileCheckError::Parse)?;
    checker::check(&ast).map_err(CompileCheckError::Type)
}
