use std::mem::discriminant;

use crate::{
    ast::{
        Application, ArgsDef, Capability, EnvVar, Field, FlagDef, IntentProgram, Module, Param, Pipeline, PipelineStep,
        PositionalDef, TypeDef,
    },
    error::IntentError,
    token::{Token, TokenKind},
};

/// Parses an intent token stream into an [`IntentProgram`].
///
/// # Errors
///
/// Returns [`IntentError`] if the token stream does not match the grammar.
///
/// ```
/// # use intent::{lexer::lex, parser::parse};
/// let tokens = lex("module m:\n  capability f:\n    input: Int n\n    output: Int\n    intent: compute factorial\n").unwrap();
/// let program = parse(&tokens).unwrap();
/// assert_eq!(program.modules.len(), 1);
/// ```
///
/// [`IntentProgram`]: crate::ast::IntentProgram
/// [`IntentError`]: crate::error::IntentError
pub fn parse(tokens: &[Token]) -> Result<IntentProgram, IntentError> {
    tracing::debug!(count = tokens.len(), "parsing intent tokens");
    Parser::new(tokens).parse_program()
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Recursive-descent parser state for intent files.
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

    /// Parses a complete intent program.
    fn parse_program(&mut self) -> Result<IntentProgram, IntentError> {
        let mut applications = Vec::new();
        let mut types = Vec::new();
        let mut modules = Vec::new();

        while !self.at_eof() {
            self.skip_newlines();
            if self.at_eof() {
                break;
            }

            match &self.peek().kind {
                TokenKind::Application => {
                    applications.push(self.parse_application()?);
                },
                TokenKind::Types => {
                    types.extend(self.parse_types_section()?);
                },
                TokenKind::Module => {
                    modules.push(self.parse_module()?);
                },
                _ => {
                    return Err(self.unexpected("'application', 'types', or 'module'"));
                },
            }
        }

        Ok(IntentProgram {
            applications,
            types,
            modules,
        })
    }

    // ---------------------------------------------------------------------------
    // Application Section
    // ---------------------------------------------------------------------------

    /// Parses an `application name:` block with args, environment, and intent.
    fn parse_application(&mut self) -> Result<Application, IntentError> {
        self.expect(&TokenKind::Application)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut args = ArgsDef::default();
        let mut environment = Vec::new();
        let mut intent = String::new();

        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }

            match &self.peek().kind {
                TokenKind::Args => {
                    args = self.parse_args_section()?;
                },
                TokenKind::Environment => {
                    environment = self.parse_environment_section()?;
                },
                TokenKind::Intent => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    intent = self.expect_free_text()?;
                    self.expect(&TokenKind::Newline)?;
                },
                _ => {
                    return Err(self.unexpected("'args', 'environment', or 'intent'"));
                },
            }
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(Application {
            name,
            args,
            environment,
            intent,
        })
    }

    /// Parses the `args:` section with verb, flag, and positional declarations.
    fn parse_args_section(&mut self) -> Result<ArgsDef, IntentError> {
        self.expect(&TokenKind::Args)?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut def = ArgsDef::default();

        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }

            match &self.peek().kind {
                TokenKind::Verb => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    let (binding, _) = self.expect_identifier()?;
                    def.verb = Some(binding);
                    self.expect(&TokenKind::Newline)?;
                },
                TokenKind::Flag => {
                    def.flags.push(self.parse_flag_def()?);
                },
                TokenKind::Positional => {
                    def.positionals.push(self.parse_positional_def()?);
                },
                _ => {
                    return Err(self.unexpected("'verb', 'flag', or 'positional'"));
                },
            }
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(def)
    }

    /// Parses a flag definition: `flag: --name [Type [default value]]`.
    fn parse_flag_def(&mut self) -> Result<FlagDef, IntentError> {
        self.expect(&TokenKind::Flag)?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::DashDash)?;
        let (long_name, _) = self.expect_identifier()?;

        let mut ty = None;
        let mut default = None;

        if !self.at(&TokenKind::Newline) && self.at_identifier() {
            let (type_name, _) = self.expect_identifier()?;
            if type_name == "default" {
                ty = None;
                default = Some(self.consume_default_value()?);
            } else {
                ty = Some(type_name);
                if self.at(&TokenKind::Default) {
                    self.advance();
                    default = Some(self.consume_default_value()?);
                }
            }
        }

        self.expect(&TokenKind::Newline)?;
        Ok(FlagDef { long_name, default, ty })
    }

    /// Parses a positional definition: `positional: binding Type`.
    fn parse_positional_def(&mut self) -> Result<PositionalDef, IntentError> {
        self.expect(&TokenKind::Positional)?;
        self.expect(&TokenKind::Colon)?;
        let (binding, _) = self.expect_identifier()?;
        let ty = self.parse_type_ref()?;
        self.expect(&TokenKind::Newline)?;
        Ok(PositionalDef { binding, ty })
    }

    /// Parses the `environment:` section with env var declarations.
    fn parse_environment_section(&mut self) -> Result<Vec<EnvVar>, IntentError> {
        self.expect(&TokenKind::Environment)?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut vars = Vec::new();

        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }
            vars.push(self.parse_env_var()?);
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(vars)
    }

    /// Parses `- Type binding from VAR_NAME [default value]`.
    fn parse_env_var(&mut self) -> Result<EnvVar, IntentError> {
        self.expect(&TokenKind::Dash)?;
        let ty = self.parse_type_ref()?;
        let (binding, _) = self.expect_identifier()?;
        self.expect(&TokenKind::From)?;
        let (var_name, _) = self.expect_identifier()?;

        let mut default = None;
        if self.at(&TokenKind::Default) {
            self.advance();
            default = Some(self.consume_default_value()?);
        }

        self.expect(&TokenKind::Newline)?;
        Ok(EnvVar {
            binding,
            default,
            ty,
            var_name,
        })
    }

    /// Consumes an identifier or integer literal as a default value string.
    fn consume_default_value(&mut self) -> Result<String, IntentError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::Identifier(val) | TokenKind::FreeText(val) => Ok(val),
            _ => Err(IntentError::Unexpected {
                line: tok.span.line,
                column: tok.span.column,
                expected: "default value".to_owned(),
                found: tok.kind.to_string(),
            }),
        }
    }

    // ---------------------------------------------------------------------------
    // Types Section
    // ---------------------------------------------------------------------------

    /// Parses the `types:` section with indented type definitions.
    fn parse_types_section(&mut self) -> Result<Vec<TypeDef>, IntentError> {
        self.expect(&TokenKind::Types)?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut types = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }
            types.push(self.parse_type_def()?);
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(types)
    }

    /// Parses a single type definition (name + indented fields).
    fn parse_type_def(&mut self) -> Result<TypeDef, IntentError> {
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut fields = Vec::new();
        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }
            fields.push(self.parse_field()?);
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(TypeDef { name, fields })
    }

    /// Parses a field line: `- Type name`.
    fn parse_field(&mut self) -> Result<Field, IntentError> {
        self.expect(&TokenKind::Dash)?;
        let ty = self.parse_type_ref()?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Newline)?;
        Ok(Field { name, ty })
    }

    // ---------------------------------------------------------------------------
    // Module Section
    // ---------------------------------------------------------------------------

    /// Parses a `module name:` section with indented capabilities and pipelines.
    fn parse_module(&mut self) -> Result<Module, IntentError> {
        self.expect(&TokenKind::Module)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut capabilities = Vec::new();
        let mut pipelines = Vec::new();

        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }

            match &self.peek().kind {
                TokenKind::Capability => {
                    capabilities.push(self.parse_capability()?);
                },
                TokenKind::Pipeline => {
                    pipelines.push(self.parse_pipeline()?);
                },
                _ => {
                    return Err(self.unexpected("'capability' or 'pipeline'"));
                },
            }
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(Module {
            name,
            capabilities,
            pipelines,
        })
    }

    // ---------------------------------------------------------------------------
    // Capability
    // ---------------------------------------------------------------------------

    /// Parses a `capability name:` block with input, output, and intent.
    fn parse_capability(&mut self) -> Result<Capability, IntentError> {
        self.expect(&TokenKind::Capability)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let mut inputs = Vec::new();
        let mut output = None;
        let mut intent = String::new();

        while !self.at(&TokenKind::Dedent) && !self.at_eof() {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }

            match &self.peek().kind {
                TokenKind::Input => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    inputs = self.parse_param_list()?;
                    self.expect(&TokenKind::Newline)?;
                },
                TokenKind::Output => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    output = Some(self.parse_type_ref()?);
                    self.expect(&TokenKind::Newline)?;
                },
                TokenKind::Intent => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    intent = self.expect_free_text()?;
                    self.expect(&TokenKind::Newline)?;
                },
                _ => {
                    return Err(self.unexpected("'input', 'output', or 'intent'"));
                },
            }
        }

        self.expect(&TokenKind::Dedent)?;
        Ok(Capability {
            name,
            inputs,
            intent,
            output,
        })
    }

    /// Parses a comma-separated parameter list (e.g. `Int n, Bool flag`).
    fn parse_param_list(&mut self) -> Result<Vec<Param>, IntentError> {
        let mut params = Vec::new();
        let ty = self.parse_type_ref()?;
        let (name, _) = self.expect_identifier()?;
        params.push(Param { name, ty });

        while self.at(&TokenKind::Comma) {
            self.advance();
            let ty = self.parse_type_ref()?;
            let (name, _) = self.expect_identifier()?;
            params.push(Param { name, ty });
        }

        Ok(params)
    }

    // ---------------------------------------------------------------------------
    // Pipeline
    // ---------------------------------------------------------------------------

    /// Parses a `pipeline name:` block with chained steps.
    fn parse_pipeline(&mut self) -> Result<Pipeline, IntentError> {
        self.expect(&TokenKind::Pipeline)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        self.expect(&TokenKind::Newline)?;
        self.expect(&TokenKind::Indent)?;

        let steps = self.parse_pipeline_steps()?;

        self.expect(&TokenKind::Dedent)?;
        Ok(Pipeline { name, steps })
    }

    /// Parses `step_a(x) -> step_b(y) -> ...` on one or more lines.
    fn parse_pipeline_steps(&mut self) -> Result<Vec<PipelineStep>, IntentError> {
        let mut steps = Vec::new();

        loop {
            self.skip_newlines();
            if self.at(&TokenKind::Dedent) || self.at_eof() {
                break;
            }

            let step = self.parse_pipeline_step()?;
            steps.push(step);

            if self.at(&TokenKind::Arrow) {
                self.advance();
            } else if self.at(&TokenKind::Newline) {
                self.advance();
                if !self.at(&TokenKind::Dedent) && !self.at_eof() && self.at(&TokenKind::Indent) {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(steps)
    }

    /// Parses a single pipeline step: `name(arg1, arg2)`.
    fn parse_pipeline_step(&mut self) -> Result<PipelineStep, IntentError> {
        let (capability, _) = self.expect_identifier()?;
        self.expect(&TokenKind::OpenParen)?;

        let mut args = Vec::new();
        if !self.at(&TokenKind::CloseParen) {
            let (arg, _) = self.expect_identifier()?;
            args.push(arg);
            while self.at(&TokenKind::Comma) {
                self.advance();
                let (arg, _) = self.expect_identifier()?;
                args.push(arg);
            }
        }

        self.expect(&TokenKind::CloseParen)?;
        Ok(PipelineStep { args, capability })
    }

    // ---------------------------------------------------------------------------
    // Type References
    // ---------------------------------------------------------------------------

    /// Parses a type reference like `Int`, `String`, or `List<Int>`.
    fn parse_type_ref(&mut self) -> Result<String, IntentError> {
        let (name, _) = self.expect_identifier()?;
        if self.at(&TokenKind::LessThan) {
            self.advance();
            let inner = self.parse_type_ref()?;
            self.expect(&TokenKind::GreaterThan)?;
            Ok(format!("{name}<{inner}>"))
        } else {
            Ok(name)
        }
    }

    // ---------------------------------------------------------------------------
    // Token Helpers
    // ---------------------------------------------------------------------------

    /// Returns the current token without consuming it.
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
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

    /// Returns `true` if the current token is an [`Identifier`].
    ///
    /// [`Identifier`]: TokenKind::Identifier
    fn at_identifier(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Identifier(_))
    }

    /// Skips consecutive newline tokens.
    fn skip_newlines(&mut self) {
        while self.at(&TokenKind::Newline) {
            self.advance();
        }
    }

    /// Consumes the current token if it matches `kind`, or returns an error.
    fn expect(&mut self, kind: &TokenKind) -> Result<Token, IntentError> {
        let tok = self.advance();
        if discriminant(&tok.kind) == discriminant(kind) {
            Ok(tok)
        } else {
            Err(IntentError::Unexpected {
                line: tok.span.line,
                column: tok.span.column,
                expected: kind.describe().to_owned(),
                found: tok.kind.to_string(),
            })
        }
    }

    /// Consumes an identifier token and returns its name and span line.
    fn expect_identifier(&mut self) -> Result<(String, u32), IntentError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::Identifier(name) => Ok((name, tok.span.line)),
            _ => Err(IntentError::Unexpected {
                line: tok.span.line,
                column: tok.span.column,
                expected: "identifier".to_owned(),
                found: tok.kind.to_string(),
            }),
        }
    }

    /// Consumes a [`FreeText`] token and returns its contents.
    ///
    /// [`FreeText`]: TokenKind::FreeText
    fn expect_free_text(&mut self) -> Result<String, IntentError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::FreeText(text) => Ok(text),
            _ => Err(IntentError::Unexpected {
                line: tok.span.line,
                column: tok.span.column,
                expected: "intent phrase".to_owned(),
                found: tok.kind.to_string(),
            }),
        }
    }

    /// Builds an error for the current token.
    fn unexpected(&self, expected: &str) -> IntentError {
        let tok = self.peek();
        IntentError::Unexpected {
            line: tok.span.line,
            column: tok.span.column,
            expected: expected.to_owned(),
            found: tok.kind.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

    #[test]
    fn parse_empty_module() {
        let prog = parse_ok(
            "module math:\n  capability f:\n    input: Int n\n    output: Int\n    intent: compute factorial\n",
        );
        assert_eq!(prog.modules.len(), 1, "one module");
        assert_eq!(prog.modules[0].name, "math");
        assert_eq!(prog.modules[0].capabilities.len(), 1, "one capability");
    }

    #[test]
    fn parse_types_section() {
        let prog = parse_ok("types:\n  Pair:\n    - Int first\n    - Int second\n");
        assert_eq!(prog.types.len(), 1, "one type");
        assert_eq!(prog.types[0].name, "Pair");
        assert_eq!(prog.types[0].fields.len(), 2, "two fields");
        assert_eq!(prog.types[0].fields[0].ty, "Int");
        assert_eq!(prog.types[0].fields[0].name, "first");
    }

    #[test]
    fn parse_capability_fields() {
        let prog = parse_ok(
            "module m:\n  capability fib:\n    input: Int n\n    output: Int\n    intent: compute nth fibonacci number\n",
        );
        let cap = &prog.modules[0].capabilities[0];
        assert_eq!(cap.name, "fib");
        assert_eq!(cap.inputs.len(), 1, "one input param");
        assert_eq!(cap.inputs[0].ty, "Int");
        assert_eq!(cap.inputs[0].name, "n");
        assert_eq!(cap.output.as_deref(), Some("Int"));
        assert_eq!(cap.intent, "compute nth fibonacci number");
    }

    #[test]
    fn parse_multiple_inputs() {
        let prog =
            parse_ok("module m:\n  capability g:\n    input: Int a, Int b\n    output: Int\n    intent: compute gcd\n");
        let cap = &prog.modules[0].capabilities[0];
        assert_eq!(cap.inputs.len(), 2, "two input params");
        assert_eq!(cap.inputs[0].name, "a");
        assert_eq!(cap.inputs[1].name, "b");
    }

    #[test]
    fn parse_generic_type() {
        let prog = parse_ok(
            "module m:\n  capability s:\n    input: List<Int> xs\n    output: Int\n    intent: sum all elements\n",
        );
        let cap = &prog.modules[0].capabilities[0];
        assert_eq!(cap.inputs[0].ty, "List<Int>");
    }

    #[test]
    fn parse_pipeline() {
        let prog = parse_ok(
            "module m:\n  capability a:\n    input: Int x\n    output: Int\n    intent: identity\n  pipeline p:\n    a(x) -> a(result)\n",
        );
        assert_eq!(prog.modules[0].pipelines.len(), 1, "one pipeline");
        let pipe = &prog.modules[0].pipelines[0];
        assert_eq!(pipe.name, "p");
        assert_eq!(pipe.steps.len(), 2, "two steps");
        assert_eq!(pipe.steps[0].capability, "a");
        assert_eq!(pipe.steps[0].args, vec!["x"]);
        assert_eq!(pipe.steps[1].capability, "a");
        assert_eq!(pipe.steps[1].args, vec!["result"]);
    }

    #[test]
    fn parse_full_program() {
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

  capability fibonacci:
    input: Int n
    output: Int
    intent: compute nth fibonacci number
";
        let prog = parse_ok(source);
        assert_eq!(prog.types.len(), 1, "one type");
        assert_eq!(prog.modules.len(), 1, "one module");
        assert_eq!(prog.modules[0].capabilities.len(), 2, "two capabilities");
    }

    #[test]
    fn parse_module_only() {
        let prog =
            parse_ok("module m:\n  capability f:\n    input: Int n\n    output: Int\n    intent: compute something\n");
        assert!(prog.types.is_empty(), "no types section");
        assert_eq!(prog.modules.len(), 1, "one module");
    }

    #[test]
    fn error_missing_colon() {
        let result = parse_err("module m\n");
        assert!(
            matches!(result, IntentError::Unexpected { .. }),
            "expected Unexpected, got {result:?}"
        );
    }

    #[test]
    fn error_unexpected_top_level() {
        let result = parse_err("capability c:\n");
        assert!(
            matches!(result, IntentError::Unexpected { .. }),
            "expected Unexpected, got {result:?}"
        );
    }

    #[test]
    fn parse_minimal_application() {
        let prog = parse_ok("application hello:\n  intent: print hello world to stdout\n");
        assert_eq!(prog.applications.len(), 1, "one application");
        let app = &prog.applications[0];
        assert_eq!(app.name, "hello");
        assert_eq!(app.intent, "print hello world to stdout");
        assert!(app.args.verb.is_none(), "no verb");
        assert!(app.args.flags.is_empty(), "no flags");
        assert!(app.args.positionals.is_empty(), "no positionals");
        assert!(app.environment.is_empty(), "no env vars");
    }

    #[test]
    fn parse_application_with_bool_flag() {
        let source = "\
application wordcount:
  args:
    flag: --verbose
  intent: count words
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.args.flags.len(), 1, "one flag");
        assert_eq!(app.args.flags[0].long_name, "verbose");
        assert!(app.args.flags[0].ty.is_none(), "boolean flag has no type");
        assert!(
            app.args.flags[0].default.is_none(),
            "boolean flag has no explicit default"
        );
    }

    #[test]
    fn parse_application_with_typed_flag() {
        let source = "\
application server:
  args:
    flag: --port Int default 8080
  intent: run server
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.args.flags.len(), 1, "one flag");
        assert_eq!(app.args.flags[0].long_name, "port");
        assert_eq!(app.args.flags[0].ty.as_deref(), Some("Int"));
        assert_eq!(app.args.flags[0].default.as_deref(), Some("8080"));
    }

    #[test]
    fn parse_application_with_required_flag() {
        let source = "\
application greeter:
  args:
    flag: --name String
  intent: greet by name
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.args.flags[0].long_name, "name");
        assert_eq!(app.args.flags[0].ty.as_deref(), Some("String"));
        assert!(app.args.flags[0].default.is_none(), "required flag has no default");
    }

    #[test]
    fn parse_application_with_verb() {
        let source = "\
application tool:
  args:
    verb: action
  intent: dispatch based on action
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.args.verb.as_deref(), Some("action"));
    }

    #[test]
    fn parse_application_with_positionals() {
        let source = "\
application convert:
  args:
    positional: file String
    positional: count Int
  intent: process file count times
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.args.positionals.len(), 2, "two positionals");
        assert_eq!(app.args.positionals[0].binding, "file");
        assert_eq!(app.args.positionals[0].ty, "String");
        assert_eq!(app.args.positionals[1].binding, "count");
        assert_eq!(app.args.positionals[1].ty, "Int");
    }

    #[test]
    fn parse_application_with_environment() {
        let source = "\
application client:
  environment:
    - String api_key from API_KEY
    - Int timeout from TIMEOUT default 30
  intent: call the API
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.environment.len(), 2, "two env vars");
        assert_eq!(app.environment[0].ty, "String");
        assert_eq!(app.environment[0].binding, "api_key");
        assert_eq!(app.environment[0].var_name, "API_KEY");
        assert!(app.environment[0].default.is_none(), "required env var");
        assert_eq!(app.environment[1].ty, "Int");
        assert_eq!(app.environment[1].binding, "timeout");
        assert_eq!(app.environment[1].var_name, "TIMEOUT");
        assert_eq!(app.environment[1].default.as_deref(), Some("30"));
    }

    #[test]
    fn parse_full_application() {
        let source = "\
application wordcount:
  args:
    flag: --verbose
    positional: file String
  environment:
    - String locale from LANG default en_US
  intent: read the file, count words, print the count
";
        let prog = parse_ok(source);
        let app = &prog.applications[0];
        assert_eq!(app.name, "wordcount");
        assert_eq!(app.args.flags.len(), 1, "one flag");
        assert_eq!(app.args.positionals.len(), 1, "one positional");
        assert_eq!(app.environment.len(), 1, "one env var");
        assert_eq!(app.intent, "read the file, count words, print the count");
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Lexes and parses source, panicking on error.
    fn parse_ok(source: &str) -> IntentProgram {
        let tokens = lex(source).unwrap();
        parse(&tokens).unwrap()
    }

    /// Lexes and parses source, returning the error.
    fn parse_err(source: &str) -> IntentError {
        let tokens = lex(source).unwrap();
        parse(&tokens).unwrap_err()
    }
}
