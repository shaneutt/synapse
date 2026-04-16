use std::mem::discriminant;

use crate::{
    ast::{BinOp, Declaration, Expression, Function, MatchArm, Param, Pattern, Program, Statement, Type, ValueDecl},
    error::ParseError,
    token::{Span, Token, TokenKind},
};

/// Parses a token stream into a Synapse [`Program`].
///
/// # Errors
///
/// Returns [`ParseError`] if the token stream does not match the grammar.
///
/// ```
/// # use cortex::{lexer::lex, parser::parse, ast::Type};
/// let tokens = lex("function f() -> Int\n  returns 0\n").unwrap();
/// let program = parse(&tokens).unwrap();
/// assert_eq!(program.declarations.len(), 1);
/// ```
///
/// [`Program`]: crate::ast::Program
/// [`ParseError`]: crate::error::ParseError
pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    Parser::new(tokens).parse_program()
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Recursive-descent parser state.
struct Parser<'t> {
    /// The full token slice.
    tokens: &'t [Token],
    /// Current position in the token slice.
    pos: usize,
}

impl<'t> Parser<'t> {
    /// Creates a parser over the given token slice.
    fn new(tokens: &'t [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    // ---------------------------------------------------------------------------
    // Top-Level
    // ---------------------------------------------------------------------------

    /// Parses a complete program (one or more declarations).
    fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut declarations = Vec::new();
        while !self.at_eof() {
            declarations.push(self.parse_declaration()?);
        }
        if declarations.is_empty() {
            return Err(self.unexpected("declaration"));
        }
        Ok(Program { declarations })
    }

    /// Parses a single declaration (function or top-level value).
    fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        match self.peek().kind {
            TokenKind::Function => self.parse_function().map(Declaration::Function),
            TokenKind::Value => self.parse_value_decl().map(Declaration::Value),
            _ => Err(self.unexpected("'function' or 'value'")),
        }
    }

    /// Parses a function declaration.
    fn parse_function(&mut self) -> Result<Function, ParseError> {
        let span = self.expect(&TokenKind::Function)?.span;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::OpenParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::CloseParen)?;
        self.expect(&TokenKind::Arrow)?;
        let return_type = self.parse_type()?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;
        let body = self.parse_body()?;
        self.expect(&TokenKind::Dedent)?;
        Ok(Function {
            name,
            body,
            params,
            return_type,
            span,
        })
    }

    /// Parses a comma-separated parameter list.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        if self.at(&TokenKind::CloseParen) {
            return Ok(Vec::new());
        }
        let mut params = vec![self.parse_param()?];
        while self.at(&TokenKind::Comma) {
            self.advance();
            params.push(self.parse_param()?);
        }
        Ok(params)
    }

    /// Parses a single typed parameter.
    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let ty = self.parse_type()?;
        let (name, span) = self.expect_identifier()?;
        Ok(Param { name, span, ty })
    }

    /// Parses a type annotation.
    fn parse_type(&mut self) -> Result<Type, ParseError> {
        let (name, span) = self.expect_identifier()?;
        match name.as_str() {
            "Int" => Ok(Type::Int),
            "Bool" => Ok(Type::Bool),
            "String" => Ok(Type::Str),
            "List" => {
                self.expect(&TokenKind::LessThan)?;
                let inner = self.parse_type()?;
                self.expect(&TokenKind::GreaterThan)?;
                Ok(Type::List(Box::new(inner)))
            },
            _ => Err(ParseError::Unexpected {
                span,
                expected: "type (Int, Bool, String, List)".to_owned(),
                found: TokenKind::Identifier(name),
            }),
        }
    }

    // ---------------------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------------------

    /// Parses a function body (one or more statements until Dedent).
    fn parse_body(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut stmts = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            stmts.push(self.parse_statement()?);
        }
        if stmts.is_empty() {
            return Err(self.unexpected("statement"));
        }
        Ok(stmts)
    }

    /// Parses a single statement.
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.peek().kind {
            TokenKind::Value => self.parse_value_decl().map(Statement::Value),
            TokenKind::Returns => self.parse_returns(),
            _ => Err(self.unexpected("'value' or 'returns'")),
        }
    }

    /// Parses a value binding.
    fn parse_value_decl(&mut self) -> Result<ValueDecl, ParseError> {
        let span = self.expect(&TokenKind::Value)?.span;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Equals)?;
        let value = self.parse_expression()?;
        self.expect_statement_end()?;
        Ok(ValueDecl { name, span, value })
    }

    /// Parses a returns statement.
    fn parse_returns(&mut self) -> Result<Statement, ParseError> {
        self.expect(&TokenKind::Returns)?;
        let expr = self.parse_expression()?;
        self.expect_statement_end()?;
        Ok(Statement::Returns(expr))
    }

    // ---------------------------------------------------------------------------
    // Expressions
    // ---------------------------------------------------------------------------

    /// Parses an expression (match or binary).
    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        if self.at(&TokenKind::Match) {
            self.parse_match_expr()
        } else {
            self.parse_binary_expr(0)
        }
    }

    /// Parses a match expression with indented arms.
    fn parse_match_expr(&mut self) -> Result<Expression, ParseError> {
        let span = self.expect(&TokenKind::Match)?.span;
        let scrutinee = self.parse_binary_expr(0)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut arms = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            arms.push(self.parse_match_arm()?);
        }
        if arms.is_empty() {
            return Err(self.unexpected("'when' or 'otherwise'"));
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(Expression::Match(Box::new(scrutinee), arms, span))
    }

    /// Parses one arm of a match expression.
    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let span = self.peek().span;
        let pattern = if self.at(&TokenKind::Otherwise) {
            self.advance();
            Pattern::Wildcard(span)
        } else {
            self.expect(&TokenKind::When)?;
            self.parse_pattern()?
        };
        self.expect(&TokenKind::Arrow)?;
        let body = self.parse_expression()?;
        if self.at(&TokenKind::Newline) {
            self.advance();
        }
        Ok(MatchArm { body, pattern, span })
    }

    /// Parses a binary expression with precedence climbing.
    fn parse_binary_expr(&mut self, min_prec: u8) -> Result<Expression, ParseError> {
        let mut left = self.parse_unary_expr()?;

        while let Some(op) = self.try_binop() {
            if op.precedence() < min_prec {
                break;
            }
            self.advance();
            let right = self.parse_binary_expr(op.precedence() + 1)?;
            let span = left.span();
            left = Expression::BinaryOp(Box::new(left), op, Box::new(right), span);
        }

        Ok(left)
    }

    /// Tries to interpret the current token as a binary operator.
    fn try_binop(&self) -> Option<BinOp> {
        match self.peek().kind {
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            TokenKind::Percent => Some(BinOp::Mod),
            TokenKind::EqualEqual => Some(BinOp::Eq),
            TokenKind::BangEqual => Some(BinOp::Ne),
            TokenKind::LessThan => Some(BinOp::Lt),
            TokenKind::GreaterThan => Some(BinOp::Gt),
            TokenKind::LessEqual => Some(BinOp::Le),
            TokenKind::GreaterEqual => Some(BinOp::Ge),
            TokenKind::AmpAmp => Some(BinOp::And),
            TokenKind::PipePipe => Some(BinOp::Or),
            _ => None,
        }
    }

    /// Parses a unary expression (call or atom).
    fn parse_unary_expr(&mut self) -> Result<Expression, ParseError> {
        let is_call = matches!(self.peek().kind, TokenKind::Identifier(_))
            && self.peek_at(1).is_some_and(|t| matches!(t.kind, TokenKind::OpenParen));

        if is_call { self.parse_call() } else { self.parse_atom() }
    }

    /// Parses a function call.
    fn parse_call(&mut self) -> Result<Expression, ParseError> {
        let (name, span) = self.expect_identifier()?;
        self.expect(&TokenKind::OpenParen)?;
        let args = self.parse_arguments()?;
        self.expect(&TokenKind::CloseParen)?;
        Ok(Expression::Call(name, args, span))
    }

    /// Parses a comma-separated argument list.
    fn parse_arguments(&mut self) -> Result<Vec<Expression>, ParseError> {
        if self.at(&TokenKind::CloseParen) {
            return Ok(Vec::new());
        }
        let mut args = vec![self.parse_expression()?];
        while self.at(&TokenKind::Comma) {
            self.advance();
            args.push(self.parse_expression()?);
        }
        Ok(args)
    }

    /// Parses an atomic expression.
    fn parse_atom(&mut self) -> Result<Expression, ParseError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::IntLit(v) => Ok(Expression::IntLit(v, tok.span)),
            TokenKind::BoolLit(v) => Ok(Expression::BoolLit(v, tok.span)),
            TokenKind::StringLit(s) => Ok(Expression::StringLit(s, tok.span)),
            TokenKind::Identifier(s) => Ok(Expression::Identifier(s, tok.span)),
            TokenKind::Nil => Ok(Expression::Nil(tok.span)),
            TokenKind::Cons => {
                self.expect(&TokenKind::OpenParen)?;
                let head = self.parse_expression()?;
                self.expect(&TokenKind::Comma)?;
                let tail = self.parse_expression()?;
                self.expect(&TokenKind::CloseParen)?;
                Ok(Expression::Cons(Box::new(head), Box::new(tail), tok.span))
            },
            TokenKind::OpenParen => {
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::CloseParen)?;
                Ok(expr)
            },
            _ => Err(ParseError::Unexpected {
                span: tok.span,
                expected: "expression".to_owned(),
                found: tok.kind,
            }),
        }
    }

    // ---------------------------------------------------------------------------
    // Patterns
    // ---------------------------------------------------------------------------

    /// Parses a match pattern.
    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::IntLit(v) => Ok(Pattern::IntLit(v, tok.span)),
            TokenKind::BoolLit(v) => Ok(Pattern::BoolLit(v, tok.span)),
            TokenKind::StringLit(s) => Ok(Pattern::StringLit(s, tok.span)),
            TokenKind::Nil => Ok(Pattern::Nil(tok.span)),
            TokenKind::Cons => {
                self.expect(&TokenKind::OpenParen)?;
                let head = self.parse_pattern()?;
                self.expect(&TokenKind::Comma)?;
                let tail = self.parse_pattern()?;
                self.expect(&TokenKind::CloseParen)?;
                Ok(Pattern::Cons(Box::new(head), Box::new(tail), tok.span))
            },
            TokenKind::Identifier(s) if s == "_" => Ok(Pattern::Wildcard(tok.span)),
            TokenKind::Identifier(s) => Ok(Pattern::Identifier(s, tok.span)),
            _ => Err(ParseError::Unexpected {
                span: tok.span,
                expected: "pattern".to_owned(),
                found: tok.kind,
            }),
        }
    }

    // ---------------------------------------------------------------------------
    // Token Helpers
    // ---------------------------------------------------------------------------

    /// Returns the current token without consuming it.
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    /// Returns a token at `offset` positions ahead, if it exists.
    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    /// Consumes and returns the current token.
    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        self.pos += 1;
        tok
    }

    /// Returns `true` if the current token matches the given kind.
    fn at(&self, kind: &TokenKind) -> bool {
        discriminant(&self.peek().kind) == discriminant(kind)
    }

    /// Returns `true` if the current token is [`Eof`].
    ///
    /// [`Eof`]: TokenKind::Eof
    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    /// Consumes the current token if it matches `kind`, or returns an error.
    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        let tok = self.advance();
        if discriminant(&tok.kind) == discriminant(kind) {
            Ok(tok)
        } else {
            Err(ParseError::Unexpected {
                span: tok.span,
                expected: kind.describe().to_owned(),
                found: tok.kind,
            })
        }
    }

    /// Consumes an identifier token and returns its name and span.
    fn expect_identifier(&mut self) -> Result<(String, Span), ParseError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::Identifier(name) => Ok((name, tok.span)),
            _ => Err(ParseError::Unexpected {
                span: tok.span,
                expected: "identifier".to_owned(),
                found: tok.kind,
            }),
        }
    }

    /// Consumes a newline, or accepts Dedent/Eof as implicit end-of-statement.
    fn expect_statement_end(&mut self) -> Result<(), ParseError> {
        match self.peek().kind {
            TokenKind::Newline => {
                self.advance();
                Ok(())
            },
            TokenKind::Dedent | TokenKind::Eof => Ok(()),
            _ => Err(self.unexpected("end of statement")),
        }
    }

    /// Builds an error for the current token.
    fn unexpected(&self, expected: &str) -> ParseError {
        let tok = self.peek();
        ParseError::Unexpected {
            span: tok.span,
            expected: expected.to_owned(),
            found: tok.kind.clone(),
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
    fn simple_function() {
        let prog = parse_ok("function foo() -> Int\n  returns 42\n");
        assert_eq!(prog.declarations.len(), 1, "one declaration");
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        assert_eq!(f.name, "foo");
        assert!(f.params.is_empty(), "no params");
        assert_eq!(f.return_type, Type::Int);
        assert_eq!(f.body.len(), 1, "one statement");
        let Statement::Returns(ref expr) = f.body[0] else {
            panic!("expected returns");
        };
        assert!(matches!(expr, Expression::IntLit(42, _)), "returns 42");
    }

    #[test]
    fn function_with_params() {
        let prog = parse_ok("function add(Int a, Int b) -> Int\n  returns a + b\n");
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        assert_eq!(f.params.len(), 2, "two params");
        assert_eq!(f.params[0].name, "a");
        assert_eq!(f.params[0].ty, Type::Int);
        assert_eq!(f.params[1].name, "b");
    }

    #[test]
    fn value_binding() {
        let prog = parse_ok("function f() -> Int\n  value x = 42\n  returns x\n");
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        assert_eq!(f.body.len(), 2, "two statements");
        assert!(matches!(f.body[0], Statement::Value(_)), "value decl");
        assert!(matches!(f.body[1], Statement::Returns(_)), "returns");
    }

    #[test]
    fn match_expression() {
        let source =
            "function fact(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * fact(n - 1)\n";
        let prog = parse_ok(source);
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        let Statement::Returns(ref expr) = f.body[0] else {
            panic!("expected returns");
        };
        let Expression::Match(_, arms, _) = expr else {
            panic!("expected match");
        };
        assert_eq!(arms.len(), 2, "two arms");
        assert!(matches!(arms[0].pattern, Pattern::IntLit(0, _)), "when 0");
        assert!(matches!(arms[1].pattern, Pattern::Wildcard(_)), "otherwise");
    }

    #[test]
    fn operator_precedence() {
        let prog = parse_ok("function f() -> Int\n  returns 1 + 2 * 3\n");
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        let Statement::Returns(ref expr) = f.body[0] else {
            panic!("expected returns");
        };
        let Expression::BinaryOp(left, BinOp::Add, right, _) = expr else {
            panic!("expected Add at top, got {expr:?}");
        };
        assert!(matches!(left.as_ref(), Expression::IntLit(1, _)), "left is 1");
        let Expression::BinaryOp(rl, BinOp::Mul, rr, _) = right.as_ref() else {
            panic!("expected Mul on right");
        };
        assert!(matches!(rl.as_ref(), Expression::IntLit(2, _)), "2");
        assert!(matches!(rr.as_ref(), Expression::IntLit(3, _)), "3");
    }

    #[test]
    fn function_call() {
        let prog = parse_ok("function f() -> Int\n  returns g(1, 2)\n");
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        let Statement::Returns(ref expr) = f.body[0] else {
            panic!("expected returns");
        };
        let Expression::Call(name, args, _) = expr else {
            panic!("expected call");
        };
        assert_eq!(name, "g");
        assert_eq!(args.len(), 2, "two arguments");
    }

    #[test]
    fn cons_and_nil() {
        let prog = parse_ok("function f() -> List<Int>\n  returns Cons(1, Nil)\n");
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        assert_eq!(f.return_type, Type::List(Box::new(Type::Int)));
        let Statement::Returns(ref expr) = f.body[0] else {
            panic!("expected returns");
        };
        assert!(matches!(expr, Expression::Cons(_, _, _)), "expected Cons");
    }

    #[test]
    fn cons_pattern() {
        let source =
            "function f(List<Int> xs) -> Int\n  returns match xs\n    when Cons(x, rest) -> x\n    when Nil -> 0\n";
        let prog = parse_ok(source);
        let Declaration::Function(f) = &prog.declarations[0] else {
            panic!("expected function");
        };
        let Statement::Returns(ref expr) = f.body[0] else {
            panic!("expected returns");
        };
        let Expression::Match(_, arms, _) = expr else {
            panic!("expected match");
        };
        assert!(matches!(arms[0].pattern, Pattern::Cons(_, _, _)), "Cons pattern");
        assert!(matches!(arms[1].pattern, Pattern::Nil(_)), "Nil pattern");
    }

    #[test]
    fn multiple_functions() {
        let source = "function a() -> Int\n  returns 1\nfunction b() -> Int\n  returns 2\n";
        let prog = parse_ok(source);
        assert_eq!(prog.declarations.len(), 2, "two functions");
    }

    #[test]
    fn error_missing_paren() {
        let err = parse_err("function f( -> Int\n  returns 0\n");
        assert!(matches!(err, ParseError::Unexpected { .. }), "{err:?}");
    }

    #[test]
    fn error_unexpected_token() {
        let err = parse_err("42\n");
        assert!(matches!(err, ParseError::Unexpected { .. }), "{err:?}");
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Lexes and parses source, panicking on error.
    fn parse_ok(source: &str) -> Program {
        let tokens = crate::lexer::lex(source).unwrap();
        parse(&tokens).unwrap()
    }

    /// Lexes and parses source, returning the parse error.
    fn parse_err(source: &str) -> ParseError {
        let tokens = crate::lexer::lex(source).unwrap();
        parse(&tokens).unwrap_err()
    }
}
