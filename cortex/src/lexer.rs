use crate::{
    error::LexError,
    token::{Span, Token, TokenKind},
};

/// Tokenizes Synapse source into a stream of [`Token`]s.
///
/// Handles whitespace-significant indentation via
/// [`Indent`]/[`Dedent`] tokens. Blank lines are skipped.
/// The stream always ends with [`Eof`].
///
/// # Errors
///
/// Returns [`LexError`] on invalid input.
///
/// ```
/// # use cortex::lexer::lex;
/// # use cortex::token::TokenKind;
/// let tokens = lex("42\n").unwrap();
/// assert_eq!(tokens[0].kind, TokenKind::IntLit(42));
/// ```
///
/// [`Token`]: crate::token::Token
/// [`Indent`]: TokenKind::Indent
/// [`Dedent`]: TokenKind::Dedent
/// [`Eof`]: TokenKind::Eof
/// [`LexError`]: crate::error::LexError
pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    tracing::debug!(len = source.len(), "lexing source");
    Lexer::new(source).tokenize()
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// Internal lexer state for [`lex`].
struct Lexer<'src> {
    /// Source code bytes.
    source: &'src str,
    /// Current byte offset.
    pos: usize,
    /// Current 1-based line number.
    line: u32,
    /// Current 1-based column number.
    column: u32,
    /// Stack of indentation levels.
    indent_stack: Vec<u32>,
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
        }
    }

    /// Consumes all input and returns the complete token stream.
    fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.lex_line(&mut tokens)?;
        }

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            tokens.push(Token {
                kind: TokenKind::Dedent,
                span: self.span(0),
            });
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            span: self.span(0),
        });
        Ok(tokens)
    }

    /// Processes a single source line: indentation, tokens, newline.
    fn lex_line(&mut self, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        let indent = self.consume_leading_spaces()?;

        if self.is_at_end() || self.at_newline() {
            self.consume_newline();
            return Ok(());
        }

        self.emit_indent_tokens(indent, tokens)?;

        while !self.is_at_end() && !self.at_newline() {
            self.skip_inline_spaces();
            if self.is_at_end() || self.at_newline() {
                break;
            }
            tokens.push(self.lex_token()?);
        }

        tokens.push(Token {
            kind: TokenKind::Newline,
            span: self.span(1),
        });
        self.consume_newline();
        Ok(())
    }

    /// Emits [`Indent`] or [`Dedent`] tokens for a change in indentation level.
    fn emit_indent_tokens(&mut self, indent: u32, tokens: &mut Vec<Token>) -> Result<(), LexError> {
        let current = *self.indent_stack.last().unwrap();

        if indent > current {
            tracing::trace!(from = current, to = indent, "indent");
            self.indent_stack.push(indent);
            tokens.push(Token {
                kind: TokenKind::Indent,
                span: self.span(0),
            });
        } else if indent < current {
            while *self.indent_stack.last().unwrap() > indent {
                let from = self.indent_stack.pop().unwrap();
                tracing::trace!(from, to = indent, "dedent");
                tokens.push(Token {
                    kind: TokenKind::Dedent,
                    span: self.span(0),
                });
            }
            if *self.indent_stack.last().unwrap() != indent {
                return Err(LexError::InconsistentIndent {
                    span: self.span(0),
                    found: indent,
                });
            }
        }

        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Token Dispatch
    // ---------------------------------------------------------------------------

    /// Dispatches to the appropriate sub-lexer for the next token.
    fn lex_token(&mut self) -> Result<Token, LexError> {
        match self.peek().unwrap() {
            b'0'..=b'9' => self.lex_integer(),
            b'"' => self.lex_string(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => Ok(self.lex_word()),
            b'+' => Ok(self.single_char(TokenKind::Plus)),
            b'*' => Ok(self.single_char(TokenKind::Star)),
            b'/' => Ok(self.single_char(TokenKind::Slash)),
            b'%' => Ok(self.single_char(TokenKind::Percent)),
            b'(' => Ok(self.single_char(TokenKind::OpenParen)),
            b')' => Ok(self.single_char(TokenKind::CloseParen)),
            b',' => Ok(self.single_char(TokenKind::Comma)),
            b':' => Ok(self.single_char(TokenKind::Colon)),
            b'-' => Ok(self.maybe_double(b'>', TokenKind::Arrow, TokenKind::Minus)),
            b'=' => Ok(self.maybe_double(b'=', TokenKind::EqualEqual, TokenKind::Equals)),
            b'<' => Ok(self.maybe_double(b'=', TokenKind::LessEqual, TokenKind::LessThan)),
            b'>' => Ok(self.maybe_double(b'=', TokenKind::GreaterEqual, TokenKind::GreaterThan)),
            b'!' => self.require_double(b'=', TokenKind::BangEqual),
            b'&' => self.require_double(b'&', TokenKind::AmpAmp),
            b'|' => self.require_double(b'|', TokenKind::PipePipe),
            byte => {
                let span = self.span(1);
                self.advance();
                Err(LexError::UnexpectedChar { span, ch: byte as char })
            },
        }
    }

    // ---------------------------------------------------------------------------
    // Word, Integer, and String Lexers
    // ---------------------------------------------------------------------------

    /// Lexes an identifier or keyword token.
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
        let length = (self.pos - start_pos) as u32;
        let span = Span {
            line: self.line,
            column: start_col,
            length,
        };

        let kind = match word {
            "function" => TokenKind::Function,
            "value" => TokenKind::Value,
            "returns" => TokenKind::Returns,
            "match" => TokenKind::Match,
            "when" => TokenKind::When,
            "otherwise" => TokenKind::Otherwise,
            "true" => TokenKind::BoolLit(true),
            "false" => TokenKind::BoolLit(false),
            "Cons" => TokenKind::Cons,
            "Nil" => TokenKind::Nil,
            _ => TokenKind::Identifier(word.to_owned()),
        };

        Token { kind, span }
    }

    /// Lexes a decimal integer literal.
    fn lex_integer(&mut self) -> Result<Token, LexError> {
        let start_col = self.column;
        let start_pos = self.pos;

        while let Some(b'0'..=b'9') = self.peek() {
            self.advance();
        }

        let text = &self.source[start_pos..self.pos];
        let length = (self.pos - start_pos) as u32;
        let span = Span {
            line: self.line,
            column: start_col,
            length,
        };

        let value: i64 = text.parse().map_err(|_| LexError::InvalidInteger { span })?;
        Ok(Token {
            kind: TokenKind::IntLit(value),
            span,
        })
    }

    /// Lexes a double-quoted string literal.
    fn lex_string(&mut self) -> Result<Token, LexError> {
        let start_col = self.column;
        let start_line = self.line;
        self.advance();

        let content_start = self.pos;

        loop {
            match self.peek() {
                Some(b'"') => {
                    let content = self.source[content_start..self.pos].to_owned();
                    self.advance();
                    return Ok(Token {
                        kind: TokenKind::StringLit(content),
                        span: Span {
                            line: start_line,
                            column: start_col,
                            length: self.column - start_col,
                        },
                    });
                },
                Some(b'\n' | b'\r') | None => {
                    return Err(LexError::UnterminatedString {
                        span: Span {
                            line: start_line,
                            column: start_col,
                            length: 1,
                        },
                    });
                },
                Some(_) => {
                    self.advance();
                },
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Operator Helpers
    // ---------------------------------------------------------------------------

    /// Lexes a single-character token and advances.
    fn single_char(&mut self, kind: TokenKind) -> Token {
        let span = self.span(1);
        self.advance();
        Token { kind, span }
    }

    /// Lexes a one- or two-character operator depending on the next byte.
    fn maybe_double(&mut self, next: u8, double_kind: TokenKind, single_kind: TokenKind) -> Token {
        let start_col = self.column;
        self.advance();
        if self.peek() == Some(next) {
            self.advance();
            Token {
                kind: double_kind,
                span: Span {
                    line: self.line,
                    column: start_col,
                    length: 2,
                },
            }
        } else {
            Token {
                kind: single_kind,
                span: Span {
                    line: self.line,
                    column: start_col,
                    length: 1,
                },
            }
        }
    }

    /// Lexes a mandatory two-character operator (e.g. `!=`, `&&`, `||`).
    fn require_double(&mut self, next: u8, kind: TokenKind) -> Result<Token, LexError> {
        let start_col = self.column;
        let first = self.advance();
        if self.peek() == Some(next) {
            self.advance();
            Ok(Token {
                kind,
                span: Span {
                    line: self.line,
                    column: start_col,
                    length: 2,
                },
            })
        } else {
            Err(LexError::UnexpectedChar {
                span: Span {
                    line: self.line,
                    column: start_col,
                    length: 1,
                },
                ch: first as char,
            })
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

    /// Consumes leading spaces at the start of a line, rejecting tabs.
    fn consume_leading_spaces(&mut self) -> Result<u32, LexError> {
        let mut count = 0u32;
        while self.peek() == Some(b' ') {
            self.advance();
            count += 1;
        }
        if self.peek() == Some(b'\t') {
            return Err(LexError::TabNotAllowed { span: self.span(1) });
        }
        Ok(count)
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

    /// Creates a [`Span`] at the current position with the given length.
    fn span(&self, length: u32) -> Span {
        Span {
            line: self.line,
            column: self.column,
            length,
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
    fn whitespace_only() {
        let tokens = lex_ok("   \n\n  \n");
        assert_eq!(kinds(&tokens), vec![TokenKind::Eof]);
    }

    #[test]
    fn keywords() {
        let tokens = lex_ok("function value returns match when otherwise\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Function,
                TokenKind::Value,
                TokenKind::Returns,
                TokenKind::Match,
                TokenKind::When,
                TokenKind::Otherwise,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn boolean_literals() {
        let tokens = lex_ok("true false\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::BoolLit(true),
                TokenKind::BoolLit(false),
                TokenKind::Newline,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn cons_and_nil() {
        let tokens = lex_ok("Cons Nil\n");
        assert_eq!(
            kinds(&tokens),
            vec![TokenKind::Cons, TokenKind::Nil, TokenKind::Newline, TokenKind::Eof]
        );
    }

    #[test]
    fn identifiers() {
        let tokens = lex_ok("foo Bar _baz x123\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("foo".to_owned()),
                TokenKind::Identifier("Bar".to_owned()),
                TokenKind::Identifier("_baz".to_owned()),
                TokenKind::Identifier("x123".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn integer_literals() {
        let tokens = lex_ok("0 42 1000\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::IntLit(0),
                TokenKind::IntLit(42),
                TokenKind::IntLit(1000),
                TokenKind::Newline,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn string_literals() {
        let tokens = lex_ok("\"hello\" \"\" \"world\"\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::StringLit("hello".to_owned()),
                TokenKind::StringLit(String::new()),
                TokenKind::StringLit("world".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn arithmetic_operators() {
        let tokens = lex_ok("+ - * / %\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn comparison_operators() {
        let tokens = lex_ok("== != < > <= >=\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::EqualEqual,
                TokenKind::BangEqual,
                TokenKind::LessThan,
                TokenKind::GreaterThan,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn logical_operators() {
        let tokens = lex_ok("&& ||\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::AmpAmp,
                TokenKind::PipePipe,
                TokenKind::Newline,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn assignment_and_arrow() {
        let tokens = lex_ok("= ->\n");
        assert_eq!(
            kinds(&tokens),
            vec![TokenKind::Equals, TokenKind::Arrow, TokenKind::Newline, TokenKind::Eof]
        );
    }

    #[test]
    fn punctuation() {
        let tokens = lex_ok("( ) , :\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::OpenParen,
                TokenKind::CloseParen,
                TokenKind::Comma,
                TokenKind::Colon,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn simple_indent() {
        let tokens = lex_ok("a\n  b\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn nested_indent() {
        let tokens = lex_ok("a\n  b\n    c\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("c".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn dedent_to_base() {
        let tokens = lex_ok("a\n  b\nc\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Identifier("c".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn multiple_dedent() {
        let tokens = lex_ok("a\n  b\n    c\nd\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("c".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Dedent,
                TokenKind::Identifier("d".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn blank_lines_skipped() {
        let tokens = lex_ok("a\n\n\nb\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn blank_lines_with_spaces_skipped() {
        let tokens = lex_ok("a\n   \nb\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn spans_are_correct() {
        let tokens = lex_ok("function foo -> Int\n");
        assert_eq!(
            tokens[0].span,
            Span {
                line: 1,
                column: 1,
                length: 8
            },
            "function keyword"
        );
        assert_eq!(
            tokens[1].span,
            Span {
                line: 1,
                column: 10,
                length: 3
            },
            "foo identifier"
        );
        assert_eq!(
            tokens[2].span,
            Span {
                line: 1,
                column: 14,
                length: 2
            },
            "arrow"
        );
        assert_eq!(
            tokens[3].span,
            Span {
                line: 1,
                column: 17,
                length: 3
            },
            "Int identifier"
        );
    }

    #[test]
    fn no_trailing_newline() {
        let tokens = lex_ok("42");
        assert_eq!(
            kinds(&tokens),
            vec![TokenKind::IntLit(42), TokenKind::Newline, TokenKind::Eof]
        );
    }

    #[test]
    fn windows_line_endings() {
        let tokens = lex_ok("a\r\n  b\r\n");
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn complete_program() {
        let source = "function factorial(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * factorial(n - 1)\n";
        let tokens = lex_ok(source);
        assert_eq!(
            kinds(&tokens),
            vec![
                TokenKind::Function,
                TokenKind::Identifier("factorial".to_owned()),
                TokenKind::OpenParen,
                TokenKind::Identifier("Int".to_owned()),
                TokenKind::Identifier("n".to_owned()),
                TokenKind::CloseParen,
                TokenKind::Arrow,
                TokenKind::Identifier("Int".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Returns,
                TokenKind::Match,
                TokenKind::Identifier("n".to_owned()),
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::When,
                TokenKind::IntLit(0),
                TokenKind::Arrow,
                TokenKind::IntLit(1),
                TokenKind::Newline,
                TokenKind::Otherwise,
                TokenKind::Arrow,
                TokenKind::Identifier("n".to_owned()),
                TokenKind::Star,
                TokenKind::Identifier("factorial".to_owned()),
                TokenKind::OpenParen,
                TokenKind::Identifier("n".to_owned()),
                TokenKind::Minus,
                TokenKind::IntLit(1),
                TokenKind::CloseParen,
                TokenKind::Newline,
                TokenKind::Dedent,
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
            LexError::UnexpectedChar {
                span: Span {
                    line: 1,
                    column: 1,
                    length: 1
                },
                ch: '@'
            }
        );
    }

    #[test]
    fn error_unterminated_string() {
        let err = lex("\"hello\n").unwrap_err();
        assert_eq!(
            err,
            LexError::UnterminatedString {
                span: Span {
                    line: 1,
                    column: 1,
                    length: 1
                }
            }
        );
    }

    #[test]
    fn error_inconsistent_indent() {
        let err = lex("a\n  b\n c\n").unwrap_err();
        assert_eq!(
            err,
            LexError::InconsistentIndent {
                span: Span {
                    line: 3,
                    column: 2,
                    length: 0
                },
                found: 1
            }
        );
    }

    #[test]
    fn error_tab_not_allowed() {
        let err = lex("\tfoo\n").unwrap_err();
        assert_eq!(
            err,
            LexError::TabNotAllowed {
                span: Span {
                    line: 1,
                    column: 1,
                    length: 1
                }
            }
        );
    }

    #[test]
    fn error_integer_overflow() {
        let err = lex("99999999999999999999999\n").unwrap_err();
        assert!(
            matches!(err, LexError::InvalidInteger { .. }),
            "expected InvalidInteger, got {err:?}"
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
