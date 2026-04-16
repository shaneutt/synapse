use crate::{
    ast::{BinOp, Pattern, Type},
    typed_ast::{
        TypedDeclaration, TypedExpr, TypedExprKind, TypedFunction, TypedMatchArm, TypedParam, TypedProgram,
        TypedStatement, TypedValueDecl,
    },
};

/// Emits a [`TypedProgram`] as valid Rust source code.
///
/// ```
/// # use cortex::{lexer::lex, parser::parse, checker::check, emitter::emit};
/// let tokens = lex("function f() -> Int\n  returns 42\n").unwrap();
/// let ast = parse(&tokens).unwrap();
/// let typed = check(&ast).unwrap();
/// let rust = emit(&typed);
/// assert!(rust.contains("fn f() -> i64"));
/// ```
///
/// [`TypedProgram`]: crate::typed_ast::TypedProgram
pub fn emit(program: &TypedProgram) -> String {
    let mut e = Emitter {
        output: String::new(),
        indent: 0,
    };
    e.emit_program(program);
    e.output
}

/// Emits a [`TypedProgram`] with application-level `main()` that parses CLI args and env vars.
///
/// The generated `main()` handles argument parsing, environment variable reading,
/// type conversion, and calls `synapse_main` with the parsed values.
///
/// ```
/// # use cortex::{lexer::lex, parser::parse, checker::check, emitter::*};
/// let tokens = lex("function main(Bool verbose, String file) -> Int\n  returns 42\n").unwrap();
/// let ast = parse(&tokens).unwrap();
/// let typed = check(&ast).unwrap();
/// let app = AppMeta {
///     flags: vec![AppFlag {
///         long_name: "verbose".to_owned(),
///         default: None,
///         ty: None,
///     }],
///     positionals: vec![AppPositional {
///         binding: "file".to_owned(),
///         ty: "String".to_owned(),
///     }],
///     ..AppMeta::default()
/// };
/// let rust = emit_with_application(&typed, &app);
/// assert!(rust.contains("fn main()"));
/// assert!(rust.contains("--verbose"));
/// ```
///
/// [`TypedProgram`]: crate::typed_ast::TypedProgram
pub fn emit_with_application(program: &TypedProgram, app: &AppMeta) -> String {
    let mut e = Emitter {
        output: String::new(),
        indent: 0,
    };
    e.emit_program_with_app(program, app);
    e.output
}

// ---------------------------------------------------------------------------
// Application Metadata
// ---------------------------------------------------------------------------

/// Application-level metadata for generating an arg-parsing `main()`.
///
/// ```
/// # use cortex::emitter::AppMeta;
/// let meta = AppMeta::default();
/// assert!(meta.verb.is_none());
/// ```
#[derive(Debug, Clone, Default)]
pub struct AppMeta {
    /// Optional verb (subcommand-style first positional).
    pub verb: Option<String>,
    /// CLI flag definitions.
    pub flags: Vec<AppFlag>,
    /// Positional argument definitions.
    pub positionals: Vec<AppPositional>,
    /// Environment variable definitions.
    pub env_vars: Vec<AppEnvVar>,
}

/// A CLI flag: boolean (ty=None) or typed with optional default.
///
/// ```
/// # use cortex::emitter::AppFlag;
/// let f = AppFlag {
///     long_name: "verbose".to_owned(),
///     default: None,
///     ty: None,
/// };
/// assert!(f.ty.is_none());
/// ```
#[derive(Debug, Clone)]
pub struct AppFlag {
    /// The flag name (without `--` prefix).
    pub long_name: String,
    /// Default value (`None` means required).
    pub default: Option<String>,
    /// The type (`None` for boolean flags).
    pub ty: Option<String>,
}

/// A positional argument with binding name and type.
///
/// ```
/// # use cortex::emitter::AppPositional;
/// let p = AppPositional {
///     binding: "file".to_owned(),
///     ty: "String".to_owned(),
/// };
/// assert_eq!(p.ty, "String");
/// ```
#[derive(Debug, Clone)]
pub struct AppPositional {
    /// The variable name in generated code.
    pub binding: String,
    /// The Synapse type name.
    pub ty: String,
}

/// An environment variable binding.
///
/// ```
/// # use cortex::emitter::AppEnvVar;
/// let e = AppEnvVar {
///     binding: "key".to_owned(),
///     default: None,
///     ty: "String".to_owned(),
///     var_name: "API_KEY".to_owned(),
/// };
/// assert_eq!(e.var_name, "API_KEY");
/// ```
#[derive(Debug, Clone)]
pub struct AppEnvVar {
    /// The variable name in generated code.
    pub binding: String,
    /// Default value (`None` means required).
    pub default: Option<String>,
    /// The Synapse type name.
    pub ty: String,
    /// The OS environment variable name.
    pub var_name: String,
}

// ---------------------------------------------------------------------------
// Emitter
// ---------------------------------------------------------------------------

/// Walks the typed AST and produces formatted Rust source.
struct Emitter {
    /// Accumulated Rust source output.
    output: String,
    /// Current indentation level.
    indent: usize,
}

impl Emitter {
    /// Emits a complete program with prelude and optional main wrapper.
    fn emit_program(&mut self, program: &TypedProgram) {
        self.emit_prelude(program);
        self.emit_builtins(program);

        let mut main_params: Option<&[TypedParam]> = None;
        for decl in &program.declarations {
            match decl {
                TypedDeclaration::Function(f) => {
                    if f.name == "main" {
                        main_params = Some(&f.params);
                    }
                    self.emit_function(f);
                    self.line("");
                },
                TypedDeclaration::Value(v) => self.emit_top_value(v),
            }
        }

        if let Some(params) = main_params {
            self.emit_main_wrapper(params);
        }
    }

    /// Emits a program with an application-level `main()` for arg parsing.
    fn emit_program_with_app(&mut self, program: &TypedProgram, app: &AppMeta) {
        self.emit_prelude(program);
        self.emit_builtins(program);

        for decl in &program.declarations {
            match decl {
                TypedDeclaration::Function(f) => {
                    self.emit_function(f);
                    self.line("");
                },
                TypedDeclaration::Value(v) => self.emit_top_value(v),
            }
        }

        self.emit_app_main(program, app);
    }

    /// Emits the List enum and Display impl if any function uses list types.
    fn emit_prelude(&mut self, program: &TypedProgram) {
        let uses_lists = program.declarations.iter().any(|d| {
            if let TypedDeclaration::Function(f) = d {
                f.params.iter().any(|p| matches!(p.ty, Type::List(_))) || matches!(f.return_type, Type::List(_))
            } else {
                false
            }
        });

        if uses_lists {
            self.line("#[derive(Debug, Clone, PartialEq)]");
            self.line("enum List<T> {");
            self.indent();
            self.line("Cons(T, Box<List<T>>),");
            self.line("Nil,");
            self.dedent();
            self.line("}");
            self.line("");
            self.line("impl<T: std::fmt::Display> std::fmt::Display for List<T> {");
            self.indent();
            self.line("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
            self.indent();
            self.line("write!(f, \"[\")?;");
            self.line("let mut current = self;");
            self.line("let mut first = true;");
            self.line("loop {");
            self.indent();
            self.line("match current {");
            self.indent();
            self.line("List::Cons(head, tail) => {");
            self.indent();
            self.line("if !first { write!(f, \", \")?; }");
            self.line("write!(f, \"{head}\")?;");
            self.line("first = false;");
            self.line("current = tail;");
            self.dedent();
            self.line("}");
            self.line("List::Nil => break,");
            self.dedent();
            self.line("}");
            self.dedent();
            self.line("}");
            self.line("write!(f, \"]\")");
            self.dedent();
            self.line("}");
            self.dedent();
            self.line("}");
            self.line("");
        }
    }

    /// Emits a function definition.
    fn emit_function(&mut self, func: &TypedFunction) {
        let name = if func.name == "main" {
            "synapse_main"
        } else {
            &func.name
        };
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, type_to_rust(&p.ty)))
            .collect();
        let ret = type_to_rust(&func.return_type);
        self.line(&format!("fn {name}({}) -> {ret} {{", params.join(", ")));
        self.indent();

        for (i, stmt) in func.body.iter().enumerate() {
            let is_last = i == func.body.len() - 1;
            self.emit_statement(stmt, is_last);
        }

        self.dedent();
        self.line("}");
    }

    /// Emits a top-level value as a const (simple literals only).
    fn emit_top_value(&mut self, decl: &TypedValueDecl) {
        let ty = type_to_rust(&decl.ty);
        self.push_indent();
        self.output
            .push_str(&format!("const {}: {ty} = ", decl.name.to_uppercase()));
        self.emit_expr(&decl.value);
        self.output.push_str(";\n");
    }

    /// Emits a statement. The last statement omits the semicolon (tail expression).
    fn emit_statement(&mut self, stmt: &TypedStatement, is_last: bool) {
        match stmt {
            TypedStatement::Value(v) => {
                self.push_indent();
                self.output.push_str(&format!("let {} = ", v.name));
                self.emit_expr(&v.value);
                self.output.push_str(";\n");
            },
            TypedStatement::Returns(expr) => {
                self.push_indent();
                self.emit_expr(expr);
                if is_last {
                    self.output.push('\n');
                } else {
                    self.output.push_str(";\n");
                }
            },
        }
    }

    /// Emits an expression.
    fn emit_expr(&mut self, expr: &TypedExpr) {
        match &expr.kind {
            TypedExprKind::IntLit(v) => self.output.push_str(&format!("{v}_i64")),
            TypedExprKind::BoolLit(v) => self.output.push_str(&format!("{v}")),
            TypedExprKind::StringLit(s) => self.output.push_str(&format!("\"{s}\".to_owned()")),
            TypedExprKind::Identifier(name) => self.output.push_str(name),
            TypedExprKind::BinaryOp(left, op, right) => {
                self.output.push('(');
                self.emit_expr(left);
                self.output.push_str(&format!(" {} ", binop_to_rust(*op)));
                self.emit_expr(right);
                self.output.push(')');
            },
            TypedExprKind::Call(name, args) => self.emit_call(name, args),
            TypedExprKind::Match(scrutinee, arms) => {
                self.output.push_str("match ");
                self.emit_expr(scrutinee);
                self.output.push_str(" {\n");
                self.indent();
                for arm in arms {
                    self.emit_match_arm(arm);
                }
                self.dedent();
                self.push_indent();
                self.output.push('}');
            },
            TypedExprKind::Cons(head, tail) => {
                self.output.push_str("List::Cons(");
                self.emit_expr(head);
                self.output.push_str(", Box::new(");
                self.emit_expr(tail);
                self.output.push_str("))");
            },
            TypedExprKind::Nil => self.output.push_str("List::Nil"),
        }
    }

    /// Emits a match arm with pattern and body.
    fn emit_match_arm(&mut self, arm: &TypedMatchArm) {
        self.push_indent();
        let box_vars = collect_box_vars(&arm.pattern);
        self.emit_pattern(&arm.pattern);
        self.output.push_str(" => ");

        if box_vars.is_empty() {
            self.emit_expr(&arm.body);
            self.output.push_str(",\n");
        } else {
            self.output.push_str("{\n");
            self.indent();
            for var in &box_vars {
                self.push_indent();
                self.output.push_str(&format!("let {var} = *{var};\n"));
            }
            self.push_indent();
            self.emit_expr(&arm.body);
            self.output.push('\n');
            self.dedent();
            self.push_indent();
            self.output.push_str("},\n");
        }
    }

    /// Emits a pattern.
    fn emit_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::IntLit(v, _) => self.output.push_str(&format!("{v}_i64")),
            Pattern::BoolLit(v, _) => self.output.push_str(&format!("{v}")),
            Pattern::StringLit(s, _) => self.output.push_str(&format!("\"{s}\"")),
            Pattern::Identifier(name, _) => self.output.push_str(name),
            Pattern::Wildcard(_) => self.output.push('_'),
            Pattern::Nil(_) => self.output.push_str("List::Nil"),
            Pattern::Cons(head, tail, _) => {
                self.output.push_str("List::Cons(");
                self.emit_pattern(head);
                self.output.push_str(", ");
                self.emit_pattern(tail);
                self.output.push(')');
            },
        }
    }

    /// Emits the Rust `main()` wrapper that calls `synapse_main`.
    ///
    /// When `params` is non-empty, generates CLI arg parsing that
    /// converts positional arguments to the declared parameter types.
    fn emit_main_wrapper(&mut self, params: &[TypedParam]) {
        self.line("fn main() {");
        self.indent();

        if params.is_empty() {
            self.line("let result = synapse_main();");
        } else {
            self.emit_auto_arg_parsing(params);
        }

        self.line("println!(\"{result}\");");
        self.dedent();
        self.line("}");
    }

    /// Emits positional CLI arg parsing inferred from `synapse_main` parameters.
    fn emit_auto_arg_parsing(&mut self, params: &[TypedParam]) {
        self.line("let args: Vec<String> = std::env::args().skip(1).collect();");

        let count = params.len();
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let usage_args = names.join("> <");
        self.line(&format!("if args.len() < {count} {{"));
        self.indent();
        self.line(&format!("eprintln!(\"usage: <program> <{usage_args}>\");"));
        self.line("std::process::exit(1);");
        self.dedent();
        self.line("}");

        for (i, param) in params.iter().enumerate() {
            let name = &param.name;
            let conversion = match &param.ty {
                Type::Int => format!("args[{i}].parse::<i64>().expect(\"invalid integer\")"),
                Type::Bool => format!("args[{i}].parse::<bool>().expect(\"invalid boolean\")"),
                Type::Str | Type::List(_) => format!("args[{i}].clone()"),
            };
            self.line(&format!("let {name} = {conversion};"));
        }

        let call_args: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        self.line(&format!("let result = synapse_main({});", call_args.join(", ")));
    }

    // ---------------------------------------------------------------------------
    // Application Main Generation
    // ---------------------------------------------------------------------------

    /// Emits a `main()` function that parses CLI args, reads env vars,
    /// and calls `synapse_main` with the parsed values.
    fn emit_app_main(&mut self, _program: &TypedProgram, app: &AppMeta) {
        self.line("fn main() {");
        self.indent();

        self.line("let args: Vec<String> = std::env::args().skip(1).collect();");
        self.line("");

        self.emit_flag_defaults(app);
        if app.verb.is_some() || !app.positionals.is_empty() {
            self.line("let mut positionals: Vec<String> = Vec::new();");
        }
        self.line("");

        self.emit_arg_loop(app);
        self.line("");

        if let Some(ref verb) = app.verb {
            self.emit_verb_extraction(verb);
        }
        self.emit_positional_extraction(app);
        self.emit_env_vars(app);

        self.emit_synapse_call(app);

        self.dedent();
        self.line("}");
    }

    /// Emits `let mut` declarations for flag variables with defaults.
    fn emit_flag_defaults(&mut self, app: &AppMeta) {
        for flag in &app.flags {
            match &flag.ty {
                None => {
                    self.line(&format!("let mut {} = false;", flag.long_name));
                },
                Some(ty) => {
                    let rust_ty = synapse_type_to_rust(ty);
                    match &flag.default {
                        Some(def) => {
                            let val = default_to_rust(ty, def);
                            self.line(&format!("let mut {}: {rust_ty} = {val};", flag.long_name));
                        },
                        None => {
                            self.line(&format!("let mut {}: Option<{rust_ty}> = None;", flag.long_name));
                        },
                    }
                },
            }
        }
    }

    /// Emits the arg-parsing `while` loop.
    fn emit_arg_loop(&mut self, app: &AppMeta) {
        self.line("let mut i = 0;");
        self.line("while i < args.len() {");
        self.indent();
        self.line("match args[i].as_str() {");
        self.indent();

        for flag in &app.flags {
            match &flag.ty {
                None => {
                    self.line(&format!("\"--{}\" => {} = true,", flag.long_name, flag.long_name));
                },
                Some(ty) => {
                    self.line(&format!("\"--{}\" => {{", flag.long_name));
                    self.indent();
                    self.line("i += 1;");
                    self.line("if i >= args.len() {");
                    self.indent();
                    self.line(&format!("eprintln!(\"missing value for --{}\");", flag.long_name));
                    self.line("std::process::exit(1);");
                    self.dedent();
                    self.line("}");
                    let parse = parse_expr_for_type(ty, "args[i].as_str()");
                    match &flag.default {
                        Some(_) => {
                            self.line(&format!("{} = {parse};", flag.long_name));
                        },
                        None => {
                            self.line(&format!("{} = Some({parse});", flag.long_name));
                        },
                    }
                    self.dedent();
                    self.line("},");
                },
            }
        }

        self.line("other if other.starts_with(\"--\") => {");
        self.indent();
        self.line("eprintln!(\"unknown flag: {other}\");");
        self.line("std::process::exit(1);");
        self.dedent();
        self.line("},");

        if app.verb.is_some() || !app.positionals.is_empty() {
            self.line("_ => positionals.push(args[i].clone()),");
        } else {
            self.line("_ => {");
            self.indent();
            self.line("eprintln!(\"unexpected argument: {}\", args[i]);");
            self.line("std::process::exit(1);");
            self.dedent();
            self.line("},");
        }

        self.dedent();
        self.line("}");
        self.line("i += 1;");
        self.dedent();
        self.line("}");
    }

    /// Emits verb extraction from positionals.
    fn emit_verb_extraction(&mut self, verb: &str) {
        self.line("if positionals.is_empty() {");
        self.indent();
        self.line(&format!("eprintln!(\"missing required argument: {verb}\");"));
        self.line("std::process::exit(1);");
        self.dedent();
        self.line("}");
        self.line(&format!("let {verb} = positionals.remove(0);"));
        self.line("");
    }

    /// Emits positional argument extraction.
    fn emit_positional_extraction(&mut self, app: &AppMeta) {
        for (idx, pos) in app.positionals.iter().enumerate() {
            self.line(&format!("if positionals.len() <= {idx} {{"));
            self.indent();
            self.line(&format!("eprintln!(\"missing required argument: {}\");", pos.binding));
            self.line("std::process::exit(1);");
            self.dedent();
            self.line("}");
            let parse = parse_expr_for_type(&pos.ty, &format!("positionals[{idx}].as_str()"));
            self.line(&format!("let {} = {parse};", pos.binding));
            self.line("");
        }
    }

    /// Emits required-flag validation after parsing.
    fn emit_required_flag_validation(&mut self, app: &AppMeta) {
        for flag in &app.flags {
            if flag.ty.is_some() && flag.default.is_none() {
                self.line(&format!("let {} = match {} {{", flag.long_name, flag.long_name));
                self.indent();
                self.line("Some(v) => v,");
                self.line("None => {");
                self.indent();
                self.line(&format!("eprintln!(\"missing required flag: --{}\");", flag.long_name));
                self.line("std::process::exit(1);");
                self.dedent();
                self.line("},");
                self.dedent();
                self.line("};");
            }
        }
    }

    /// Emits environment variable reading.
    fn emit_env_vars(&mut self, app: &AppMeta) {
        for env in &app.env_vars {
            match &env.default {
                Some(def) => {
                    let val = default_to_rust(&env.ty, def);
                    self.line(&format!(
                        "let {binding}_raw = std::env::var(\"{var}\").unwrap_or_else(|_| {val}.to_string());",
                        binding = env.binding,
                        var = env.var_name,
                    ));
                    if env.ty == "String" {
                        self.line(&format!("let {} = {}_raw;", env.binding, env.binding));
                    } else {
                        let parse = parse_expr_for_type(&env.ty, &format!("{}_raw.as_str()", env.binding));
                        self.line(&format!("let {} = {parse};", env.binding));
                    }
                },
                None => {
                    self.line(&format!(
                        "let {binding}_raw = std::env::var(\"{var}\").unwrap_or_else(|_| {{",
                        binding = env.binding,
                        var = env.var_name,
                    ));
                    self.indent();
                    self.line(&format!(
                        "eprintln!(\"missing required environment variable: {}\");",
                        env.var_name
                    ));
                    self.line("std::process::exit(1);");
                    self.dedent();
                    self.line("});");
                    if env.ty == "String" {
                        self.line(&format!("let {} = {}_raw;", env.binding, env.binding));
                    } else {
                        let parse = parse_expr_for_type(&env.ty, &format!("{}_raw.as_str()", env.binding));
                        self.line(&format!("let {} = {parse};", env.binding));
                    }
                },
            }
            self.line("");
        }
    }

    /// Emits the call to `synapse_main` and prints the result.
    fn emit_synapse_call(&mut self, app: &AppMeta) {
        self.emit_required_flag_validation(app);

        let mut call_args = Vec::new();

        if let Some(ref verb) = app.verb {
            call_args.push(verb.clone());
        }
        for flag in &app.flags {
            call_args.push(flag.long_name.clone());
        }
        for pos in &app.positionals {
            call_args.push(pos.binding.clone());
        }
        for env in &app.env_vars {
            call_args.push(env.binding.clone());
        }

        let args_str = call_args.join(", ");
        self.line(&format!("let result = synapse_main({args_str});"));
        self.line("println!(\"{result}\");");
    }

    // ---------------------------------------------------------------------------
    // Built-in Functions
    // ---------------------------------------------------------------------------

    /// Emits Rust implementations for built-in functions used in the program.
    fn emit_builtins(&mut self, program: &TypedProgram) {
        if self.uses_builtin(program, "http_get") {
            self.line("fn __builtin_http_get(url: String) -> String {");
            self.indent();
            self.line("let output = std::process::Command::new(\"curl\")");
            self.indent();
            self.line(".args([\"-s\", &url])");
            self.line(".output()");
            self.line(".expect(\"failed to run curl\");");
            self.dedent();
            self.line("String::from_utf8(output.stdout).unwrap_or_default()");
            self.dedent();
            self.line("}");
            self.line("");
        }
    }

    /// Checks if any function in the program calls a given built-in.
    fn uses_builtin(&self, program: &TypedProgram, name: &str) -> bool {
        program.declarations.iter().any(|d| {
            if let TypedDeclaration::Function(f) = d {
                f.body.iter().any(|s| self.stmt_uses(s, name))
            } else {
                false
            }
        })
    }

    /// Checks if a statement contains a call to the named built-in.
    fn stmt_uses(&self, stmt: &TypedStatement, name: &str) -> bool {
        match stmt {
            TypedStatement::Value(v) => self.expr_uses(&v.value, name),
            TypedStatement::Returns(e) => self.expr_uses(e, name),
        }
    }

    /// Checks if an expression contains a call to the named built-in.
    fn expr_uses(&self, expr: &TypedExpr, name: &str) -> bool {
        match &expr.kind {
            TypedExprKind::Call(n, args) => n == name || args.iter().any(|a| self.expr_uses(a, name)),
            TypedExprKind::BinaryOp(l, _, r) => self.expr_uses(l, name) || self.expr_uses(r, name),
            TypedExprKind::Match(s, arms) => {
                self.expr_uses(s, name) || arms.iter().any(|a| self.expr_uses(&a.body, name))
            },
            TypedExprKind::Cons(h, t) => self.expr_uses(h, name) || self.expr_uses(t, name),
            _ => false,
        }
    }

    /// Emits a function call, with special handling for built-ins.
    fn emit_call(&mut self, name: &str, args: &[TypedExpr]) {
        match name {
            "print" => {
                self.output.push_str("{ println!(\"{}\", ");
                self.emit_expr(&args[0]);
                self.output.push_str("); 0_i64 }");
            },
            "concat" => {
                self.output.push_str("format!(\"{}{}\", ");
                self.emit_expr(&args[0]);
                self.output.push_str(", ");
                self.emit_expr(&args[1]);
                self.output.push(')');
            },
            "http_get" => {
                self.output.push_str("__builtin_http_get(");
                self.emit_expr(&args[0]);
                self.output.push(')');
            },
            _ => {
                let name = if name == "main" { "synapse_main" } else { name };
                self.output.push_str(name);
                self.output.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.emit_expr(arg);
                }
                self.output.push(')');
            },
        }
    }

    // ---------------------------------------------------------------------------
    // Output Helpers
    // ---------------------------------------------------------------------------

    /// Writes an indented line to the output.
    fn line(&mut self, text: &str) {
        self.push_indent();
        self.output.push_str(text);
        self.output.push('\n');
    }

    /// Writes the current indentation prefix.
    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push_str("    ");
        }
    }

    /// Increases the indentation level.
    fn indent(&mut self) {
        self.indent += 1;
    }

    /// Decreases the indentation level.
    fn dedent(&mut self) {
        self.indent -= 1;
    }
}

// ---------------------------------------------------------------------------
// Conversion Helpers
// ---------------------------------------------------------------------------

/// Converts a Synapse [`Type`] to its Rust representation.
fn type_to_rust(ty: &Type) -> String {
    match ty {
        Type::Int => "i64".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Str => "String".to_owned(),
        Type::List(inner) => format!("List<{}>", type_to_rust(inner)),
    }
}

/// Converts a Synapse type name string to a Rust type string.
fn synapse_type_to_rust(ty: &str) -> &str {
    match ty {
        "Int" => "i64",
        "Bool" => "bool",
        _ => "String",
    }
}

/// Converts a default value to a Rust literal for the given type.
fn default_to_rust(ty: &str, val: &str) -> String {
    match ty {
        "Int" => format!("{val}_i64"),
        "Bool" => val.to_owned(),
        _ => format!("\"{val}\".to_owned()"),
    }
}

/// Builds a parse expression that converts a string to the given type.
fn parse_expr_for_type(ty: &str, expr: &str) -> String {
    match ty {
        "Int" => format!(
            "{expr}.parse::<i64>().unwrap_or_else(|_| {{ eprintln!(\"invalid integer\"); std::process::exit(1); }})"
        ),
        "Bool" => format!(
            "{expr}.parse::<bool>().unwrap_or_else(|_| {{ eprintln!(\"invalid boolean\"); std::process::exit(1); }})"
        ),
        _ => format!("{expr}.to_owned()"),
    }
}

/// Converts a [`BinOp`] to its Rust operator string.
fn binop_to_rust(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

/// Collects variable names that are in Cons-tail position (bound as `Box<List<_>>`).
fn collect_box_vars(pattern: &Pattern) -> Vec<String> {
    match pattern {
        Pattern::Cons(_, tail, _) => {
            let mut vars = Vec::new();
            if let Pattern::Identifier(name, _) = tail.as_ref() {
                vars.push(name.clone());
            }
            vars.extend(collect_box_vars(tail));
            vars
        },
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_simple_function() {
        let rust = compile("function f() -> Int\n  returns 42\n");
        assert!(rust.contains("fn f() -> i64"), "function signature:\n{rust}");
        assert!(rust.contains("42_i64"), "literal:\n{rust}");
    }

    #[test]
    fn emit_factorial() {
        let rust = compile(
            "function factorial(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * factorial(n - 1)\n",
        );
        assert!(rust.contains("fn factorial(n: i64) -> i64"), "signature:\n{rust}");
        assert!(rust.contains("match n"), "match:\n{rust}");
    }

    #[test]
    fn emit_cons_nil() {
        let rust = compile("function f() -> List<Int>\n  returns Cons(1, Nil)\n");
        assert!(rust.contains("enum List<T>"), "list type:\n{rust}");
        assert!(rust.contains("List::Cons("), "cons:\n{rust}");
        assert!(rust.contains("List::Nil"), "nil:\n{rust}");
    }

    #[test]
    fn emit_main_wrapper() {
        let rust = compile("function main() -> Int\n  returns 42\n");
        assert!(rust.contains("fn synapse_main() -> i64"), "renamed main:\n{rust}");
        assert!(rust.contains("fn main()"), "wrapper:\n{rust}");
        assert!(rust.contains("synapse_main()"), "wrapper calls synapse_main:\n{rust}");
    }

    #[test]
    fn emit_value_binding() {
        let rust = compile("function f() -> Int\n  value x = 10\n  returns x\n");
        assert!(rust.contains("let x = 10_i64"), "let binding:\n{rust}");
    }

    #[test]
    fn emit_list_pattern() {
        let source = "function len(List<Int> xs) -> Int\n  returns match xs\n    when Nil -> 0\n    when Cons(_, rest) -> 1 + len(rest)\n";
        let rust = compile(source);
        assert!(rust.contains("List::Cons(_, rest)"), "cons pattern:\n{rust}");
        assert!(rust.contains("let rest = *rest;"), "box deref:\n{rust}");
    }

    #[test]
    fn emit_app_main_bool_flag() {
        let app = AppMeta {
            flags: vec![AppFlag {
                long_name: "verbose".to_owned(),
                default: None,
                ty: None,
            }],
            positionals: vec![AppPositional {
                binding: "file".to_owned(),
                ty: "String".to_owned(),
            }],
            ..AppMeta::default()
        };
        let rust = compile_with_app("function main(Bool verbose, String file) -> Int\n  returns 42\n", &app);
        assert!(rust.contains("let mut verbose = false;"), "bool default:\n{rust}");
        assert!(
            rust.contains("\"--verbose\" => verbose = true"),
            "bool flag match:\n{rust}"
        );
        assert!(
            rust.contains("missing required argument: file"),
            "positional validation:\n{rust}"
        );
        assert!(rust.contains("synapse_main(verbose, file)"), "synapse call:\n{rust}");
    }

    #[test]
    fn emit_app_main_typed_flag_with_default() {
        let app = AppMeta {
            flags: vec![AppFlag {
                long_name: "port".to_owned(),
                default: Some("8080".to_owned()),
                ty: Some("Int".to_owned()),
            }],
            ..AppMeta::default()
        };
        let rust = compile_with_app("function main(Int port) -> Int\n  returns port\n", &app);
        assert!(rust.contains("let mut port: i64 = 8080_i64;"), "typed default:\n{rust}");
        assert!(rust.contains("\"--port\" =>"), "typed flag match:\n{rust}");
    }

    #[test]
    fn emit_app_main_required_flag() {
        let app = AppMeta {
            flags: vec![AppFlag {
                long_name: "name".to_owned(),
                default: None,
                ty: Some("String".to_owned()),
            }],
            ..AppMeta::default()
        };
        let rust = compile_with_app("function main(String name) -> Int\n  returns 0\n", &app);
        assert!(
            rust.contains("let mut name: Option<String> = None;"),
            "required flag as Option:\n{rust}"
        );
        assert!(
            rust.contains("missing required flag: --name"),
            "required validation:\n{rust}"
        );
    }

    #[test]
    fn emit_app_main_env_var() {
        let app = AppMeta {
            env_vars: vec![AppEnvVar {
                binding: "api_key".to_owned(),
                default: None,
                ty: "String".to_owned(),
                var_name: "API_KEY".to_owned(),
            }],
            ..AppMeta::default()
        };
        let rust = compile_with_app("function main(String api_key) -> Int\n  returns 0\n", &app);
        assert!(rust.contains("std::env::var(\"API_KEY\")"), "env var read:\n{rust}");
        assert!(
            rust.contains("missing required environment variable: API_KEY"),
            "env required:\n{rust}"
        );
    }

    #[test]
    fn emit_app_main_env_var_with_default() {
        let app = AppMeta {
            env_vars: vec![AppEnvVar {
                binding: "timeout".to_owned(),
                default: Some("30".to_owned()),
                ty: "Int".to_owned(),
                var_name: "TIMEOUT".to_owned(),
            }],
            ..AppMeta::default()
        };
        let rust = compile_with_app("function main(Int timeout) -> Int\n  returns timeout\n", &app);
        assert!(rust.contains("std::env::var(\"TIMEOUT\")"), "env var read:\n{rust}");
        assert!(rust.contains("30_i64"), "env default:\n{rust}");
    }

    #[test]
    fn emit_app_main_verb() {
        let app = AppMeta {
            verb: Some("action".to_owned()),
            ..AppMeta::default()
        };
        let rust = compile_with_app("function main(String action) -> Int\n  returns 0\n", &app);
        assert!(
            rust.contains("missing required argument: action"),
            "verb validation:\n{rust}"
        );
        assert!(
            rust.contains("let action = positionals.remove(0)"),
            "verb extraction:\n{rust}"
        );
    }

    #[test]
    fn emit_main_with_string_arg() {
        let rust = compile("function main(String city) -> Int\n  returns 42\n");
        assert!(rust.contains("std::env::args()"), "collects CLI args:\n{rust}");
        assert!(rust.contains("args.len() < 1"), "checks arg count:\n{rust}");
        assert!(
            rust.contains("usage: <program> <city>"),
            "usage message includes param name:\n{rust}"
        );
        assert!(
            rust.contains("let city = args[0].clone()"),
            "string conversion:\n{rust}"
        );
        assert!(
            rust.contains("synapse_main(city)"),
            "passes arg to synapse_main:\n{rust}"
        );
    }

    #[test]
    fn emit_main_with_int_arg() {
        let rust = compile("function main(Int n) -> Int\n  returns n\n");
        assert!(
            rust.contains("args[0].parse::<i64>().expect(\"invalid integer\")"),
            "int parse:\n{rust}"
        );
        assert!(rust.contains("synapse_main(n)"), "passes arg:\n{rust}");
    }

    #[test]
    fn emit_main_with_multiple_args() {
        let rust = compile("function main(String name, Int count) -> Int\n  returns count\n");
        assert!(rust.contains("args.len() < 2"), "checks for 2 args:\n{rust}");
        assert!(
            rust.contains("usage: <program> <name> <count>"),
            "usage with both params:\n{rust}"
        );
        assert!(rust.contains("let name = args[0].clone()"), "first arg string:\n{rust}");
        assert!(
            rust.contains("let count = args[1].parse::<i64>()"),
            "second arg int:\n{rust}"
        );
        assert!(rust.contains("synapse_main(name, count)"), "call with both:\n{rust}");
    }

    #[test]
    fn emit_main_no_args_unchanged() {
        let rust = compile("function main() -> Int\n  returns 42\n");
        assert!(
            !rust.contains("std::env::args()"),
            "no arg parsing when no params:\n{rust}"
        );
        assert!(
            rust.contains("let result = synapse_main();"),
            "direct call with no args:\n{rust}"
        );
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Full pipeline: lex -> parse -> check -> emit.
    fn compile(source: &str) -> String {
        let tokens = crate::lexer::lex(source).unwrap();
        let ast = crate::parser::parse(&tokens).unwrap();
        let typed = crate::checker::check(&ast).unwrap();
        emit(&typed)
    }

    /// Full pipeline with application metadata: lex -> parse -> check -> emit_with_application.
    fn compile_with_app(source: &str, app: &AppMeta) -> String {
        let tokens = crate::lexer::lex(source).unwrap();
        let ast = crate::parser::parse(&tokens).unwrap();
        let typed = crate::checker::check(&ast).unwrap();
        emit_with_application(&typed, app)
    }
}
