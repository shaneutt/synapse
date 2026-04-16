use crate::{
    ast::{BinOp, Import, Pattern, Type},
    token::Span,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A type-checked Synapse program.
///
/// Every expression carries its resolved [`Type`].
///
/// [`Type`]: crate::ast::Type
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedProgram {
    /// Top-level declarations in source order.
    pub declarations: Vec<TypedDeclaration>,
    /// Import statements from the source.
    pub imports: Vec<Import>,
}

/// A type-checked top-level declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedDeclaration {
    /// A type-checked function definition.
    Function(TypedFunction),
    /// A type-checked top-level value binding.
    Value(TypedValueDecl),
}

/// A type-checked function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedFunction {
    /// The function name.
    pub name: String,
    /// Type-checked body statements.
    pub body: Vec<TypedStatement>,
    /// Whether this function is declared `pub`.
    pub is_public: bool,
    /// Type-checked parameters.
    pub params: Vec<TypedParam>,
    /// Declared return type.
    pub return_type: Type,
    /// Source location.
    pub span: Span,
}

/// A type-checked parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedParam {
    /// The parameter name.
    pub name: String,
    /// Source location.
    pub span: Span,
    /// The resolved type.
    pub ty: Type,
}

/// A type-checked statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedStatement {
    /// A type-checked value binding.
    Value(TypedValueDecl),
    /// A type-checked return expression.
    Returns(TypedExpr),
}

/// A type-checked value binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedValueDecl {
    /// The bound name.
    pub name: String,
    /// Whether this value is declared `pub`.
    pub is_public: bool,
    /// Source location.
    pub span: Span,
    /// The inferred type.
    pub ty: Type,
    /// The bound expression.
    pub value: TypedExpr,
}

/// A type-checked expression with its resolved type.
///
/// ```
/// # use cortex::{typed_ast::{TypedExpr, TypedExprKind}, ast::Type, token::Span};
/// let e = TypedExpr {
///     kind: TypedExprKind::IntLit(42),
///     span: Span {
///         line: 1,
///         column: 1,
///         length: 2,
///     },
///     ty: Type::Int,
/// };
/// assert_eq!(e.ty, Type::Int);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedExpr {
    /// The expression variant.
    pub kind: TypedExprKind,
    /// Source location.
    pub span: Span,
    /// The resolved type of this expression.
    pub ty: Type,
}

/// The kind of a type-checked expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypedExprKind {
    /// Integer literal.
    IntLit(i64),
    /// Boolean literal.
    BoolLit(bool),
    /// String literal.
    StringLit(String),
    /// Variable reference.
    Identifier(String),
    /// Binary operation.
    BinaryOp(Box<TypedExpr>, BinOp, Box<TypedExpr>),
    /// Function call.
    Call(String, Vec<TypedExpr>),
    /// Qualified function call (`module.function`).
    QualifiedCall(String, String, Vec<TypedExpr>),
    /// Qualified identifier (`module.name`).
    QualifiedIdentifier(String, String),
    /// Match expression.
    Match(Box<TypedExpr>, Vec<TypedMatchArm>),
    /// List constructor.
    Cons(Box<TypedExpr>, Box<TypedExpr>),
    /// Empty list.
    Nil,
}

/// A type-checked match arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedMatchArm {
    /// The expression to evaluate when matched.
    pub body: TypedExpr,
    /// The pattern to match against.
    pub pattern: Pattern,
    /// Source location.
    pub span: Span,
}
