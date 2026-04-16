use crate::{
    error::IntentError,
    token::{Span, Token, TokenKind},
};

/// Tokenizes intent source into a stream of [`Token`]s.
///
/// Handles YAML-like indentation via [`Indent`]/[`Dedent`] tokens.
/// After `intent:`, everything to end-of-line is captured as
/// [`FreeText`].
///
/// # Errors
///
/// Returns [`IntentError`] on invalid input.
///
/// ```
/// # use intent::lexer::lex;
/// # use intent::token::TokenKind;
/// let tokens = lex("types:\n").unwrap();
/// assert_eq!(tokens[0].kind, TokenKind::Types);
/// ```
///
/// [`Token`]: crate::token::Token
/// [`Indent`]: TokenKind::Indent
/// [`Dedent`]: TokenKind::Dedent
/// [`FreeText`]: TokenKind::FreeText
/// [`IntentError`]: crate::error::IntentError
pub fn lex(source: &str) -> Result<Vec<Token>, IntentError> {
    tracing::debug!(len = source.len(), "lexing intent source");
    Lexer::new(source).tokenize()
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// Internal lexer state for [`lex`].
struct Lexer<'src> {
    /// Source code string.
    source: &'src str,
    /// Current byte offset.
    pos: usize,
    /// Current 1-based line number.
    line: u32,
    /// Current 1-based column number.
    column: u32,
    /// Stack of indentation levels.
    indent_stack: Vec<u32>,
    /// Set when we just emitted the `intent` keyword + colon;
    /// signals that the rest of the line is free text.
    intent_mode: bool,
}

impl<'src> Lexer<'src> {
    /// Creates a new lexer positioned at the start of `source`.
    fn new(source: &'src str) -> Self {
        Self {
            source,
            pos: 0,
            line: 1,
            column: 1,
            indent_stack: vec![0],
            intent_mode: false,
        }
    }

    /// Consumes all input and returns the complete token stream.
    fn tokenize(mut self) -> Result<Vec<Token>, IntentError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.lex_line(&mut tokens)?;
        }

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            tokens.push(Token {
                kind: TokenKind::Dedent,
                span: self.span(),
            });
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.span(),
        });
        Ok(tokens)
    }

    /// Processes a single source line: indentation, tokens, newline.
    fn lex_line(&mut self, tokens: &mut Vec<Token>) -> Result<(), IntentError> {
        let indent = self.consume_leading_spaces();

        if self.is_at_end() || self.at_newline() {
            self.consume_newline();
            return Ok(());
        }

        self.emit_indent_tokens(indent, tokens);

        while !self.is_at_end() && !self.at_newline() {
            self.skip_inline_spaces();
            if self.is_at_end() || self.at_newline() {
                break;
            }

            if self.intent_mode {
                tokens.push(self.lex_free_text());
                self.intent_mode = false;
                break;
            }

            tokens.push(self.lex_token()?);
        }

        self.intent_mode = false;

        tokens.push(Token {
            kind: TokenKind::Newline,
            span: self.span(),
        });
        self.consume_newline();
        Ok(())
    }

    /// Emits [`Indent`] or [`Dedent`] tokens for a change in indentation.
    fn emit_indent_tokens(&mut self, indent: u32, tokens: &mut Vec<Token>) {
        let current = *self.indent_stack.last().unwrap();

        if indent > current {
            tracing::trace!(from = current, to = indent, "indent");
            self.indent_stack.push(indent);
            tokens.push(Token {
                kind: TokenKind::Indent,
                span: self.span(),
            });
        } else if indent < current {
            while *self.indent_stack.last().unwrap() > indent {
                self.indent_stack.pop();
                tracing::trace!(to = indent, "dedent");
                tokens.push(Token {
                    kind: TokenKind::Dedent,
                    span: self.span(),
                });
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Token Dispatch
    // ---------------------------------------------------------------------------

    /// Dispatches to the appropriate sub-lexer for the next token.
    fn lex_token(&mut self) -> Result<Token, IntentError> {
        match self.peek().unwrap() {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => Ok(self.lex_word()),
            b'0'..=b'9' => Ok(self.lex_number()),
            b'.' => Ok(self.lex_path()),
            b'"' => Ok(self.lex_quoted_string()),
            b':' => Ok(self.single_char(TokenKind::Colon)),
            b',' => Ok(self.single_char(TokenKind::Comma)),
            b'(' => Ok(self.single_char(TokenKind::OpenParen)),
            b')' => Ok(self.single_char(TokenKind::CloseParen)),
            b'<' => Ok(self.single_char(TokenKind::LessThan)),
            b'>' => Ok(self.single_char(TokenKind::GreaterThan)),
            b'-' => Ok(self.lex_dash_or_arrow()),
            byte => {
                let span = self.span();
                self.advance();
                Err(IntentError::UnexpectedChar {
                    line: span.line,
                    column: span.column,
                    ch: byte as char,
                })
            },
        }
    }

    // ---------------------------------------------------------------------------
    // Word, Dash, and FreeText Lexers
    // ---------------------------------------------------------------------------

    /// Lexes an identifier or keyword token.
    ///
    /// After the first pure-alpha word, if the next character is
    /// `/` or `.` followed by more word characters, continues
    /// consuming to form a path-like identifier (e.g.
    /// `lib/utils.synapse`, `../math-lib`).
    fn lex_word(&mut self) -> Token {
        let start_col = self.column;
        let start_pos = self.pos;

        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }

        let word = &self.source[start_pos..self.pos];
        let span = Span {
            line: self.line,
            column: start_col,
        };

        let kind = match word {
            "application" => TokenKind::Application,
            "types" => TokenKind::Types,
            "module" => TokenKind::Module,
            "capability" => TokenKind::Capability,
            "pipeline" => TokenKind::Pipeline,
            "input" => TokenKind::Input,
            "output" => TokenKind::Output,
            "intent" => TokenKind::Intent,
            "args" => TokenKind::Args,
            "verb" => TokenKind::Verb,
            "flag" => TokenKind::Flag,
            "positional" => TokenKind::Positional,
            "environment" => TokenKind::Environment,
            "from" => TokenKind::From,
            "default" => TokenKind::Default,
            "capabilities" => TokenKind::Capabilities,
            "description" => TokenKind::Description,
            "properties" => TokenKind::Properties,
            "new" => TokenKind::New,
            "import" => TokenKind::Import,
            "crate" => TokenKind::Crate,
            "uses" => TokenKind::Uses,
            "rust" => TokenKind::Rust,
            _ => {
                self.extend_path_chars(start_pos);
                let full = self.source[start_pos..self.pos].to_owned();
                TokenKind::Identifier(full)
            },
        };

        Token { kind, span }
    }

    /// Extends the current token to include path separators and dots.
    fn extend_path_chars(&mut self, start_pos: usize) {
        let _ = start_pos;
        while matches!(self.peek(), Some(b'/' | b'.')) {
            let next_after = self.source.as_bytes().get(self.pos + 1).copied();
            if next_after.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
                self.advance();
                while let Some(b) = self.peek() {
                    if b.is_ascii_alphanumeric() || b == b'_' {
                        self.advance();
                    } else {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    /// Lexes a numeric or version literal as an [`Identifier`] token.
    ///
    /// Handles plain integers (`8080`) and dotted versions (`1.0.140`).
    ///
    /// [`Identifier`]: TokenKind::Identifier
    fn lex_number(&mut self) -> Token {
        let start_col = self.column;
        let start_pos = self.pos;

        while let Some(b'0'..=b'9') = self.peek() {
            self.advance();
        }

        while self.peek() == Some(b'.') {
            let next_after = self.source.as_bytes().get(self.pos + 1).copied();
            if next_after.is_some_and(|b| b.is_ascii_digit()) {
                self.advance();
                while let Some(b'0'..=b'9') = self.peek() {
                    self.advance();
                }
            } else {
                break;
            }
        }

        let text = self.source[start_pos..self.pos].to_owned();
        Token {
            kind: TokenKind::Identifier(text),
            span: Span {
                line: self.line,
                column: start_col,
            },
        }
    }

    /// Lexes a `"quoted"` string literal as an [`Identifier`] token (without quotes).
    ///
    /// [`Identifier`]: TokenKind::Identifier
    fn lex_quoted_string(&mut self) -> Token {
        let start_col = self.column;
        self.advance();
        let start_pos = self.pos;

        while let Some(b) = self.peek() {
            if b == b'"' {
                break;
            }
            self.advance();
        }

        let text = self.source[start_pos..self.pos].to_owned();
        if self.peek() == Some(b'"') {
            self.advance();
        }

        Token {
            kind: TokenKind::Identifier(text),
            span: Span {
                line: self.line,
                column: start_col,
            },
        }
    }

    /// Lexes `-` (dash), `->` (arrow), or `--` (dash-dash prefix).
    fn lex_dash_or_arrow(&mut self) -> Token {
        let span = self.span();
        self.advance();
        if self.peek() == Some(b'>') {
            self.advance();
            Token {
                kind: TokenKind::Arrow,
                span,
            }
        } else if self.peek() == Some(b'-') {
            self.advance();
            Token {
                kind: TokenKind::DashDash,
                span,
            }
        } else {
            Token {
                kind: TokenKind::Dash,
                span,
            }
        }
    }

    /// Lexes a path starting with `.` (e.g. `../math-lib`).
    fn lex_path(&mut self) -> Token {
        let start_col = self.column;
        let start_pos = self.pos;

        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'/' | b'-') {
                self.advance();
            } else {
                break;
            }
        }

        let text = self.source[start_pos..self.pos].to_owned();
        Token {
            kind: TokenKind::Identifier(text),
            span: Span {
                line: self.line,
                column: start_col,
            },
        }
    }

    /// Captures all remaining text on the line as a [`FreeText`] token.
    fn lex_free_text(&mut self) -> Token {
        let span = self.span();
        let start = self.pos;

        while !self.is_at_end() && !self.at_newline() {
            self.advance();
        }

        let text = self.source[start..self.pos].trim().to_owned();
        Token {
            kind: TokenKind::FreeText(text),
            span,
        }
    }

    // ---------------------------------------------------------------------------
    // Single-char Helper
    // ---------------------------------------------------------------------------

    /// Lexes a single-character token and advances.
    fn single_char(&mut self, kind: TokenKind) -> Token {
        let span = self.span();
        self.advance();

        if matches!(kind, TokenKind::Colon) {
            self.check_intent_mode();
        }

        Token { kind, span }
    }

    /// Sets `intent_mode` if the most recently produced keyword was
    /// `intent` or `description`.
    fn check_intent_mode(&mut self) {
        let before_colon = self.source[..self.pos - 1].trim_end();
        if before_colon.ends_with("intent") || before_colon.ends_with("description") {
            self.intent_mode = true;
        }
    }

    // ---------------------------------------------------------------------------
    // Primitives
    // ---------------------------------------------------------------------------

    /// Peeks at the current byte without consuming it.
    fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }

    /// Consumes one byte and advances the position.
    fn advance(&mut self) -> u8 {
        let byte = self.source.as_bytes()[self.pos];
        self.pos += 1;
        self.column += 1;
        byte
    }

    /// Returns `true` if all input has been consumed.
    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    /// Returns `true` if the current byte is a newline character.
    fn at_newline(&self) -> bool {
        matches!(self.peek(), Some(b'\n' | b'\r'))
    }

    /// Consumes leading spaces at the start of a line.
    fn consume_leading_spaces(&mut self) -> u32 {
        let mut count = 0u32;
        while self.peek() == Some(b' ') {
            self.advance();
            count += 1;
        }
        count
    }

    /// Skips space characters within a line.
    fn skip_inline_spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.advance();
        }
    }

    /// Consumes a newline sequence (`\n`, `\r`, or `\r\n`).
    fn consume_newline(&mut self) {
        let had_newline = matches!(self.peek(), Some(b'\r' | b'\n'));
        if self.peek() == Some(b'\r') {
            self.pos += 1;
        }
        if self.peek() == Some(b'\n') {
            self.pos += 1;
        }
        if had_newline {
            self.line += 1;
            self.column = 1;
        }
    }

    /// Creates a [`Span`] at the current position.
    fn span(&self) -> Span {
        Span {
            line: self.line,
            column: self.column,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source() {
        let tokens = lex_ok("");
        assert_eq!(kinds(&tokens), vec![TokenKind::Eof]);
    }

    #[test]
    fn types_keyword() {
        let tokens = lex_ok("types:\n");
        assert_eq!(
            kinds(&tokens),
            vec![TokenKind::Types, TokenKind::Colon, TokenKind::Newline, TokenKind::Eof,]
        );
    }

    #[test]
    fn module_keyword() {
        let tokens = lex_ok("module math:\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Module,
                TokenKind::Identifier("math".to_owned()),
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn capability_with_intent() {
        let tokens = lex_ok("    intent: compute factorial using recursion\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Indent,
                TokenKind::Intent,
                TokenKind::Colon,
                TokenKind::FreeText("compute factorial using recursion".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn field_with_dash() {
        let tokens = lex_ok("    - Int field_name\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Indent,
                TokenKind::Dash,
                TokenKind::Identifier("Int".to_owned()),
                TokenKind::Identifier("field_name".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn arrow_token() {
        let tokens = lex_ok("step_one(x) -> step_two(y)\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("step_one".to_owned()),
                TokenKind::OpenParen,
                TokenKind::Identifier("x".to_owned()),
                TokenKind::CloseParen,
                TokenKind::Arrow,
                TokenKind::Identifier("step_two".to_owned()),
                TokenKind::OpenParen,
                TokenKind::Identifier("y".to_owned()),
                TokenKind::CloseParen,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn generic_type() {
        let tokens = lex_ok("    input: List<Int> xs\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Indent,
                TokenKind::Input,
                TokenKind::Colon,
                TokenKind::Identifier("List".to_owned()),
                TokenKind::LessThan,
                TokenKind::Identifier("Int".to_owned()),
                TokenKind::GreaterThan,
                TokenKind::Identifier("xs".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn indent_dedent_tracking() {
        let source = "types:\n  Pair:\n    - Int x\n";
        let tokens = lex_ok(source);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Types,
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("Pair".to_owned()),
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Dash,
                TokenKind::Identifier("Int".to_owned()),
                TokenKind::Identifier("x".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn blank_lines_skipped() {
        let tokens = lex_ok("types:\n\n  Pair:\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Types,
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("Pair".to_owned()),
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn error_unexpected_char() {
        let err = lex("@\n").unwrap_err();
        assert_eq!(
            err,
            IntentError::UnexpectedChar {
                line: 1,
                column: 1,
                ch: '@'
            }
        );
    }

    #[test]
    fn comma_in_params() {
        let tokens = lex_ok("    input: Int a, Int b\n");
        assert!(kinds(&tokens).contains(&TokenKind::Comma), "should contain comma token");
    }

    #[test]
    fn dashdash_token() {
        let tokens = lex_ok("    flag: --verbose\n");
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::Flag), "has flag keyword");
        assert!(k.contains(&TokenKind::DashDash), "has dashdash");
        assert!(
            k.contains(&TokenKind::Identifier("verbose".to_owned())),
            "has identifier after dashdash"
        );
    }

    #[test]
    fn args_keywords() {
        let tokens = lex_ok("args:\n");
        assert_eq!(tokens[0].kind, TokenKind::Args);
    }

    #[test]
    fn verb_keyword() {
        let tokens = lex_ok("    verb: action\n");
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::Verb), "has verb keyword");
    }

    #[test]
    fn positional_keyword() {
        let tokens = lex_ok("    positional: file String\n");
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::Positional), "has positional keyword");
    }

    #[test]
    fn environment_keyword() {
        let tokens = lex_ok("  environment:\n");
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::Environment), "has environment keyword");
    }

    #[test]
    fn from_keyword() {
        let tokens = lex_ok("    - String key from API_KEY\n");
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::From), "has from keyword");
    }

    #[test]
    fn default_keyword() {
        let tokens = lex_ok("    flag: --port Int default 8080\n");
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::Default), "has default keyword");
        assert!(
            k.contains(&TokenKind::Identifier("8080".to_owned())),
            "has numeric literal as identifier"
        );
    }

    #[test]
    fn numeric_literal() {
        let tokens = lex_ok("8080\n");
        assert_eq!(
            tokens[0].kind,
            TokenKind::Identifier("8080".to_owned()),
            "numeric literal lexed as identifier"
        );
    }

    #[test]
    fn quoted_string_literal() {
        let tokens = lex_ok("\"en_US\"\n");
        assert_eq!(
            tokens[0].kind,
            TokenKind::Identifier("en_US".to_owned()),
            "quoted string lexed as identifier without quotes"
        );
    }

    #[test]
    fn full_intent_file() {
        let source = "\
types:
  Pair:
    - Int first
    - Int second

module math:
  capability factorial:
    input: Int n
    output: Int
    intent: compute factorial using recursion
";
        let tokens = lex_ok(source);
        let k = kinds(&tokens);
        assert!(k.contains(&TokenKind::Types), "has types keyword");
        assert!(k.contains(&TokenKind::Module), "has module keyword");
        assert!(k.contains(&TokenKind::Capability), "has capability keyword");
        assert!(k.contains(&TokenKind::Intent), "has intent keyword");
        assert!(
            k.contains(&TokenKind::FreeText("compute factorial using recursion".to_owned())),
            "has free text"
        );
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Lexes source, panicking on error.
    fn lex_ok(source: &str) -> Vec<Token> {
        lex(source).unwrap()
    }

    /// Extracts token kinds from a token slice.
    fn kinds(tokens: &[Token]) -> Vec<TokenKind> {
        tokens.iter().map(|t| t.kind.clone()).collect()
    }
}
