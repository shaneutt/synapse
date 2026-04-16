#![deny(unsafe_code)]
//! Intent: the Synapse intent-layer compiler.
//!
//! Parses `.intent` files and expands them to `.synapse` source
//! via template matching or LLM-assisted code generation.

/// Abstract syntax tree for intent programs.
pub mod ast;
/// Error types for intent processing.
pub mod error;
/// Intent-to-Synapse expansion pipeline.
pub mod expander;
/// Lexer for intent source files.
pub mod lexer;
/// LLM-backed code generation via the `claude` CLI.
pub mod llm;
/// Recursive-descent parser for intent files.
pub mod parser;
/// Prompt construction for LLM expansion.
pub mod prompt;
/// Template-based code generation for common patterns.
pub mod templates;
/// Token and span types for the intent lexer.
pub mod token;
/// Structural validation for intent programs.
pub mod validator;
