use std::collections::HashMap;

use crate::{
    ast::{self, BinOp, Pattern, Type},
    error::TypeError,
    token::Span,
    typed_ast::{
        TypedDeclaration, TypedExpr, TypedExprKind, TypedFunction, TypedMatchArm, TypedParam, TypedProgram,
        TypedStatement, TypedValueDecl,
    },
};

/// Type-checks a parsed [`Program`] and returns a [`TypedProgram`].
///
/// # Errors
///
/// Returns [`TypeError`] if any expression has an incompatible type.
///
/// ```
/// # use cortex::{lexer::lex, parser::parse, checker::check};
/// let tokens = lex("function f() -> Int\n  returns 42\n").unwrap();
/// let ast = parse(&tokens).unwrap();
/// let typed = check(&ast).unwrap();
/// assert_eq!(typed.declarations.len(), 1);
/// ```
///
/// [`Program`]: crate::ast::Program
/// [`TypedProgram`]: crate::typed_ast::TypedProgram
/// [`TypeError`]: crate::error::TypeError
pub fn check(program: &ast::Program) -> Result<TypedProgram, TypeError> {
    Checker::new().check_program(program)
}

// ---------------------------------------------------------------------------
// Function Signatures
// ---------------------------------------------------------------------------

/// Stores parameter types and return type for a function.
#[derive(Debug, Clone)]
struct FnSig {
    /// Parameter types in declaration order.
    params: Vec<Type>,
    /// Declared return type.
    return_type: Type,
}

// ---------------------------------------------------------------------------
// Type Environment
// ---------------------------------------------------------------------------

/// Scoped variable and function type environment.
struct TypeEnv {
    /// Stack of variable scopes (innermost last).
    scopes: Vec<HashMap<String, Type>>,
    /// Registered function signatures.
    functions: HashMap<String, FnSig>,
}

impl TypeEnv {
    /// Creates an empty environment with one scope.
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
        }
    }

    /// Pushes a new variable scope.
    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pops the innermost variable scope.
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Defines a variable in the current scope.
    fn define(&mut self, name: String, ty: Type) {
        self.scopes.last_mut().unwrap().insert(name, ty);
    }

    /// Looks up a variable by name, searching from innermost scope outward.
    fn lookup(&self, name: &str) -> Option<&Type> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
}

// ---------------------------------------------------------------------------
// Checker
// ---------------------------------------------------------------------------

/// Type checker state.
struct Checker {
    /// The type environment with scopes and function signatures.
    env: TypeEnv,
}

impl Checker {
    /// Creates a new checker with an empty environment.
    fn new() -> Self {
        Self { env: TypeEnv::new() }
    }

    /// Registers built-in function signatures (print, `http_get`, concat).
    fn register_builtins(&mut self) {
        self.env.functions.insert(
            "print".to_owned(),
            FnSig {
                params: vec![Type::Str],
                return_type: Type::Int,
            },
        );
        self.env.functions.insert(
            "http_get".to_owned(),
            FnSig {
                params: vec![Type::Str],
                return_type: Type::Str,
            },
        );
        self.env.functions.insert(
            "concat".to_owned(),
            FnSig {
                params: vec![Type::Str, Type::Str],
                return_type: Type::Str,
            },
        );
    }

    /// Checks a complete program (two-pass: register signatures, then check bodies).
    fn check_program(&mut self, program: &ast::Program) -> Result<TypedProgram, TypeError> {
        self.register_builtins();

        for decl in &program.declarations {
            if let ast::Declaration::Function(f) = decl {
                if self.env.functions.contains_key(&f.name) {
                    return Err(TypeError::DuplicateFn {
                        span: f.span,
                        name: f.name.clone(),
                    });
                }
                let sig = FnSig {
                    params: f.params.iter().map(|p| p.ty.clone()).collect(),
                    return_type: f.return_type.clone(),
                };
                self.env.functions.insert(f.name.clone(), sig);
            }
        }

        let mut declarations = Vec::new();
        for decl in &program.declarations {
            declarations.push(self.check_declaration(decl)?);
        }
        Ok(TypedProgram { declarations })
    }

    /// Checks a single declaration.
    fn check_declaration(&mut self, decl: &ast::Declaration) -> Result<TypedDeclaration, TypeError> {
        match decl {
            ast::Declaration::Function(f) => self.check_function(f).map(TypedDeclaration::Function),
            ast::Declaration::Value(v) => self.check_top_value(v).map(TypedDeclaration::Value),
        }
    }

    /// Checks a function declaration.
    fn check_function(&mut self, func: &ast::Function) -> Result<TypedFunction, TypeError> {
        self.env.push_scope();
        for param in &func.params {
            self.env.define(param.name.clone(), param.ty.clone());
        }

        let mut body = Vec::new();
        for stmt in &func.body {
            body.push(self.check_statement(stmt, &func.return_type)?);
        }

        match body.last() {
            Some(TypedStatement::Returns(expr)) => {
                Self::expect_type(&expr.ty, &func.return_type, expr.span)?;
            },
            _ => return Err(TypeError::MissingReturn { span: func.span }),
        }

        self.env.pop_scope();

        let params = func
            .params
            .iter()
            .map(|p| TypedParam {
                name: p.name.clone(),
                span: p.span,
                ty: p.ty.clone(),
            })
            .collect();

        Ok(TypedFunction {
            name: func.name.clone(),
            body,
            params,
            return_type: func.return_type.clone(),
            span: func.span,
        })
    }

    /// Checks a top-level value declaration.
    fn check_top_value(&mut self, decl: &ast::ValueDecl) -> Result<TypedValueDecl, TypeError> {
        let value = self.check_expr(&decl.value, None)?;
        let ty = value.ty.clone();
        self.env.define(decl.name.clone(), ty.clone());
        Ok(TypedValueDecl {
            name: decl.name.clone(),
            span: decl.span,
            ty,
            value,
        })
    }

    /// Checks a statement within a function body.
    fn check_statement(&mut self, stmt: &ast::Statement, ret_ty: &Type) -> Result<TypedStatement, TypeError> {
        match stmt {
            ast::Statement::Value(v) => {
                let value = self.check_expr(&v.value, None)?;
                let ty = value.ty.clone();
                self.env.define(v.name.clone(), ty.clone());
                Ok(TypedStatement::Value(TypedValueDecl {
                    name: v.name.clone(),
                    span: v.span,
                    ty,
                    value,
                }))
            },
            ast::Statement::Returns(expr) => {
                let typed = self.check_expr(expr, Some(ret_ty))?;
                Ok(TypedStatement::Returns(typed))
            },
        }
    }

    // ---------------------------------------------------------------------------
    // Expression Checking
    // ---------------------------------------------------------------------------

    /// Checks an expression, optionally with an expected type for inference.
    fn check_expr(&mut self, expr: &ast::Expression, expected: Option<&Type>) -> Result<TypedExpr, TypeError> {
        match expr {
            ast::Expression::IntLit(v, span) => Ok(TypedExpr {
                kind: TypedExprKind::IntLit(*v),
                span: *span,
                ty: Type::Int,
            }),
            ast::Expression::BoolLit(v, span) => Ok(TypedExpr {
                kind: TypedExprKind::BoolLit(*v),
                span: *span,
                ty: Type::Bool,
            }),
            ast::Expression::StringLit(s, span) => Ok(TypedExpr {
                kind: TypedExprKind::StringLit(s.clone()),
                span: *span,
                ty: Type::Str,
            }),
            ast::Expression::Identifier(name, span) => {
                let ty = self.env.lookup(name).cloned().ok_or_else(|| TypeError::UndefinedVar {
                    span: *span,
                    name: name.clone(),
                })?;
                Ok(TypedExpr {
                    kind: TypedExprKind::Identifier(name.clone()),
                    span: *span,
                    ty,
                })
            },
            ast::Expression::BinaryOp(left, op, right, span) => {
                let l = self.check_expr(left, None)?;
                let r = self.check_expr(right, None)?;
                let ty = Self::check_binop(*op, &l.ty, &r.ty, *span)?;
                Ok(TypedExpr {
                    kind: TypedExprKind::BinaryOp(Box::new(l), *op, Box::new(r)),
                    span: *span,
                    ty,
                })
            },
            ast::Expression::Call(name, args, span) => self.check_call(name, args, *span),
            ast::Expression::Match(scrutinee, arms, span) => self.check_match(scrutinee, arms, *span, expected),
            ast::Expression::Cons(head, tail, span) => self.check_cons(head, tail, *span),
            ast::Expression::Nil(span) => {
                let ty = match expected {
                    Some(t @ Type::List(_)) => t.clone(),
                    _ => Type::List(Box::new(Type::Int)),
                };
                Ok(TypedExpr {
                    kind: TypedExprKind::Nil,
                    span: *span,
                    ty,
                })
            },
        }
    }

    /// Checks a function call expression.
    fn check_call(&mut self, name: &str, args: &[ast::Expression], span: Span) -> Result<TypedExpr, TypeError> {
        let sig = self
            .env
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| TypeError::UndefinedFn {
                span,
                name: name.to_owned(),
            })?;

        if args.len() != sig.params.len() {
            return Err(TypeError::ArgCount {
                span,
                name: name.to_owned(),
                expected: sig.params.len(),
                found: args.len(),
            });
        }

        let mut typed_args = Vec::new();
        for (arg, param_ty) in args.iter().zip(&sig.params) {
            let typed = self.check_expr(arg, Some(param_ty))?;
            Self::expect_type(&typed.ty, param_ty, typed.span)?;
            typed_args.push(typed);
        }

        Ok(TypedExpr {
            kind: TypedExprKind::Call(name.to_owned(), typed_args),
            span,
            ty: sig.return_type,
        })
    }

    /// Checks a match expression.
    fn check_match(
        &mut self,
        scrutinee: &ast::Expression,
        arms: &[ast::MatchArm],
        span: Span,
        expected: Option<&Type>,
    ) -> Result<TypedExpr, TypeError> {
        let typed_scrutinee = self.check_expr(scrutinee, None)?;
        let scrutinee_ty = typed_scrutinee.ty.clone();

        let mut typed_arms = Vec::new();
        let mut result_ty: Option<Type> = None;

        for arm in arms {
            self.env.push_scope();
            self.bind_pattern(&arm.pattern, &scrutinee_ty)?;
            let body = self.check_expr(&arm.body, expected.or(result_ty.as_ref()))?;

            match &result_ty {
                None => result_ty = Some(body.ty.clone()),
                Some(t) => Self::expect_type(&body.ty, t, body.span)?,
            }

            self.env.pop_scope();
            typed_arms.push(TypedMatchArm {
                body,
                pattern: arm.pattern.clone(),
                span: arm.span,
            });
        }

        let ty = result_ty.unwrap_or(Type::Int);
        Ok(TypedExpr {
            kind: TypedExprKind::Match(Box::new(typed_scrutinee), typed_arms),
            span,
            ty,
        })
    }

    /// Checks a Cons expression.
    fn check_cons(
        &mut self,
        head: &ast::Expression,
        tail: &ast::Expression,
        span: Span,
    ) -> Result<TypedExpr, TypeError> {
        let typed_head = self.check_expr(head, None)?;
        let expected_tail = Type::List(Box::new(typed_head.ty.clone()));
        let typed_tail = self.check_expr(tail, Some(&expected_tail))?;

        if !Self::types_compatible(&typed_tail.ty, &expected_tail) {
            return Err(TypeError::Mismatch {
                span: typed_tail.span,
                expected: expected_tail,
                found: typed_tail.ty.clone(),
            });
        }

        let ty = Type::List(Box::new(typed_head.ty.clone()));
        Ok(TypedExpr {
            kind: TypedExprKind::Cons(Box::new(typed_head), Box::new(typed_tail)),
            span,
            ty,
        })
    }

    /// Binds pattern variables into the current scope.
    fn bind_pattern(&mut self, pattern: &Pattern, ty: &Type) -> Result<(), TypeError> {
        match pattern {
            Pattern::Identifier(name, _) => {
                self.env.define(name.clone(), ty.clone());
            },
            Pattern::Cons(head, tail, _) => {
                if let Type::List(elem_ty) = ty {
                    self.bind_pattern(head, elem_ty)?;
                    self.bind_pattern(tail, ty)?;
                }
            },
            Pattern::Wildcard(_)
            | Pattern::Nil(_)
            | Pattern::IntLit(..)
            | Pattern::BoolLit(..)
            | Pattern::StringLit(..) => {},
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------
    // Type Utilities
    // ---------------------------------------------------------------------------

    /// Checks a binary operation and returns its result type.
    fn check_binop(op: BinOp, left: &Type, right: &Type, span: Span) -> Result<Type, TypeError> {
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                Self::expect_type(left, &Type::Int, span)?;
                Self::expect_type(right, &Type::Int, span)?;
                Ok(Type::Int)
            },
            BinOp::Eq | BinOp::Ne => {
                Self::expect_type(right, left, span)?;
                Ok(Type::Bool)
            },
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                Self::expect_type(left, &Type::Int, span)?;
                Self::expect_type(right, &Type::Int, span)?;
                Ok(Type::Bool)
            },
            BinOp::And | BinOp::Or => {
                Self::expect_type(left, &Type::Bool, span)?;
                Self::expect_type(right, &Type::Bool, span)?;
                Ok(Type::Bool)
            },
        }
    }

    /// Asserts that `actual` matches `expected`, returning an error otherwise.
    fn expect_type(actual: &Type, expected: &Type, span: Span) -> Result<(), TypeError> {
        if Self::types_compatible(actual, expected) {
            Ok(())
        } else {
            Err(TypeError::Mismatch {
                span,
                expected: expected.clone(),
                found: actual.clone(),
            })
        }
    }

    /// Two types are compatible if they are equal, or if either is a List
    /// containing the default element type (for Nil inference).
    fn types_compatible(a: &Type, b: &Type) -> bool {
        if a == b {
            return true;
        }
        matches!((a, b), (Type::List(_), Type::List(_)))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_typed_factorial() {
        let source = "function factorial(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * factorial(n - 1)\n";
        check_ok(source);
    }

    #[test]
    fn well_typed_fibonacci() {
        let source = "function fib(Int n) -> Int\n  returns match n\n    when 0 -> 0\n    when 1 -> 1\n    otherwise -> fib(n - 1) + fib(n - 2)\n";
        check_ok(source);
    }

    #[test]
    fn value_binding_type() {
        check_ok("function f() -> Int\n  value x = 42\n  returns x\n");
    }

    #[test]
    fn list_cons_type() {
        check_ok("function f() -> List<Int>\n  returns Cons(1, Cons(2, Nil))\n");
    }

    #[test]
    fn type_mismatch_arithmetic() {
        let err = check_err("function f() -> Int\n  returns 1 + true\n");
        assert!(matches!(err, TypeError::Mismatch { .. }), "{err:?}");
    }

    #[test]
    fn undefined_variable() {
        let err = check_err("function f() -> Int\n  returns x\n");
        assert!(matches!(err, TypeError::UndefinedVar { .. }), "{err:?}");
    }

    #[test]
    fn wrong_arg_count() {
        let source = "function g(Int a) -> Int\n  returns a\nfunction f() -> Int\n  returns g(1, 2)\n";
        let err = check_err(source);
        assert!(matches!(err, TypeError::ArgCount { .. }), "{err:?}");
    }

    #[test]
    fn return_type_mismatch() {
        let err = check_err("function f() -> Bool\n  returns 42\n");
        assert!(matches!(err, TypeError::Mismatch { .. }), "{err:?}");
    }

    #[test]
    fn missing_return() {
        let err = check_err("function f() -> Int\n  value x = 42\n");
        assert!(matches!(err, TypeError::MissingReturn { .. }), "{err:?}");
    }

    #[test]
    fn duplicate_function() {
        let err = check_err("function f() -> Int\n  returns 1\nfunction f() -> Int\n  returns 2\n");
        assert!(matches!(err, TypeError::DuplicateFn { .. }), "{err:?}");
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Lexes, parses, and type-checks source, panicking on error.
    fn check_ok(source: &str) -> TypedProgram {
        let tokens = crate::lexer::lex(source).unwrap();
        let ast = crate::parser::parse(&tokens).unwrap();
        check(&ast).unwrap()
    }

    /// Lexes, parses, and type-checks source, returning the type error.
    fn check_err(source: &str) -> TypeError {
        let tokens = crate::lexer::lex(source).unwrap();
        let ast = crate::parser::parse(&tokens).unwrap();
        check(&ast).unwrap_err()
    }
}
