use std::fmt;

use crate::token::Span;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A complete Synapse program.
///
/// ```
/// # use cortex::ast::Program;
/// let prog = Program {
///     declarations: vec![],
/// };
/// assert!(prog.declarations.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// Top-level declarations in source order.
    pub declarations: Vec<Declaration>,
}

/// A top-level declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    /// A function definition.
    Function(Function),
    /// A top-level value binding.
    Value(ValueDecl),
}

/// A function with parameters, return type, and body.
///
/// ```
/// # use cortex::ast::{Function, Param, Statement, Expression, Type};
/// # use cortex::token::Span;
/// let f = Function {
///     name: "id".to_owned(),
///     body: vec![Statement::Returns(Expression::Identifier(
///         "x".to_owned(),
///         Span {
///             line: 1,
///             column: 1,
///             length: 1,
///         },
///     ))],
///     params: vec![Param {
///         name: "x".to_owned(),
///         span: Span {
///             line: 1,
///             column: 1,
///             length: 1,
///         },
///         ty: Type::Int,
///     }],
///     return_type: Type::Int,
///     span: Span {
///         line: 1,
///         column: 1,
///         length: 1,
///     },
/// };
/// assert_eq!(f.name, "id");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// The function name.
    pub name: String,
    /// Statements in the function body.
    pub body: Vec<Statement>,
    /// Typed parameters.
    pub params: Vec<Param>,
    /// Declared return type.
    pub return_type: Type,
    /// Source location of the `function` keyword.
    pub span: Span,
}

/// A typed function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// The parameter name.
    pub name: String,
    /// Source location.
    pub span: Span,
    /// The parameter's type annotation.
    pub ty: Type,
}

/// A Synapse type annotation.
///
/// ```
/// # use cortex::ast::Type;
/// let list_int = Type::List(Box::new(Type::Int));
/// assert_eq!(format!("{list_int}"), "List<Int>");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// 64-bit signed integer.
    Int,
    /// Boolean.
    Bool,
    /// String.
    Str,
    /// Homogeneous linked list.
    List(Box<Type>),
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => f.write_str("Int"),
            Self::Bool => f.write_str("Bool"),
            Self::Str => f.write_str("String"),
            Self::List(inner) => write!(f, "List<{inner}>"),
        }
    }
}

/// A statement within a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// A local value binding.
    Value(ValueDecl),
    /// A return expression.
    Returns(Expression),
}

/// A value binding (`value name = expr`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueDecl {
    /// The bound name.
    pub name: String,
    /// Source location of the `value` keyword.
    pub span: Span,
    /// The bound expression.
    pub value: Expression,
}

/// An expression node in the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    /// Integer literal.
    IntLit(i64, Span),
    /// Boolean literal.
    BoolLit(bool, Span),
    /// String literal.
    StringLit(String, Span),
    /// Variable reference.
    Identifier(String, Span),
    /// Binary operation.
    BinaryOp(Box<Expression>, BinOp, Box<Expression>, Span),
    /// Function call.
    Call(String, Vec<Expression>, Span),
    /// Match expression with arms.
    Match(Box<Expression>, Vec<MatchArm>, Span),
    /// List constructor (`Cons(head, tail)`).
    Cons(Box<Expression>, Box<Expression>, Span),
    /// Empty list.
    Nil(Span),
}

impl Expression {
    /// Returns the source [`Span`] of this expression.
    ///
    /// [`Span`]: crate::token::Span
    pub fn span(&self) -> Span {
        match self {
            Self::IntLit(_, s)
            | Self::BoolLit(_, s)
            | Self::StringLit(_, s)
            | Self::Identifier(_, s)
            | Self::Nil(s)
            | Self::BinaryOp(_, _, _, s)
            | Self::Call(_, _, s)
            | Self::Match(_, _, s)
            | Self::Cons(_, _, s) => *s,
        }
    }
}

/// A branch in a match expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    /// The expression to evaluate when matched.
    pub body: Expression,
    /// The pattern to match against.
    pub pattern: Pattern,
    /// Source location.
    pub span: Span,
}

/// A pattern for destructuring in match arms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// Integer literal pattern.
    IntLit(i64, Span),
    /// Boolean literal pattern.
    BoolLit(bool, Span),
    /// String literal pattern.
    StringLit(String, Span),
    /// Variable binding pattern.
    Identifier(String, Span),
    /// Cons destructuring pattern.
    Cons(Box<Pattern>, Box<Pattern>, Span),
    /// Nil pattern.
    Nil(Span),
    /// Wildcard (`_` or `otherwise`).
    Wildcard(Span),
}

/// A binary operator with defined precedence.
///
/// ```
/// # use cortex::ast::BinOp;
/// assert!(BinOp::Mul.precedence() > BinOp::Add.precedence());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Division (`/`).
    Div,
    /// Modulo (`%`).
    Mod,
    /// Equality (`==`).
    Eq,
    /// Inequality (`!=`).
    Ne,
    /// Less than (`<`).
    Lt,
    /// Greater than (`>`).
    Gt,
    /// Less or equal (`<=`).
    Le,
    /// Greater or equal (`>=`).
    Ge,
    /// Logical and (`&&`).
    And,
    /// Logical or (`||`).
    Or,
}

impl BinOp {
    /// Returns the precedence level (higher binds tighter).
    pub fn precedence(self) -> u8 {
        match self {
            Self::Or => 1,
            Self::And => 2,
            Self::Eq | Self::Ne | Self::Lt | Self::Gt | Self::Le | Self::Ge => 3,
            Self::Add | Self::Sub => 4,
            Self::Mul | Self::Div | Self::Mod => 5,
        }
    }
}
