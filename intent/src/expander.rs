use crate::{
    ast::{Capability, IntentProgram, Pipeline, PipelineStep},
    error::IntentError,
    llm, templates, validator,
};

/// Expands an [`IntentProgram`] into valid `.synapse` source code
/// using templates only.
///
/// Validates the program first, then expands each capability using
/// template matching and each pipeline into a chained function call.
/// Returns [`IntentError::NoTemplateMatch`] for capabilities that
/// do not match any built-in template.
///
/// # Errors
///
/// Returns [`IntentError`] if validation fails, no template matches
/// a capability's intent phrase, or the resulting code cannot be
/// compiled by cortex.
///
/// ```
/// # use intent::{lexer::lex, parser::parse, expander::expand};
/// let src = "\
/// module math:
///   capability factorial:
///     input: Int n
///     output: Int
///     intent: compute factorial using recursion
/// ";
/// let tokens = lex(src).unwrap();
/// let program = parse(&tokens).unwrap();
/// let synapse = expand(&program).unwrap();
/// assert!(synapse.contains("function factorial"));
/// ```
///
/// [`IntentProgram`]: crate::ast::IntentProgram
/// [`IntentError`]: crate::error::IntentError
/// [`IntentError::NoTemplateMatch`]: crate::error::IntentError::NoTemplateMatch
pub fn expand(program: &IntentProgram) -> Result<String, IntentError> {
    expand_core(program, false)
}

/// Expands using templates first, falling back to the `claude` CLI
/// for capabilities that do not match any template.
///
/// # Errors
///
/// Returns [`IntentError`] if expansion, validation, or LLM fails.
///
/// ```no_run
/// # use intent::{lexer::lex, parser::parse, expander::expand_with_llm};
/// let src = "module m:\n  capability custom:\n    input: Int n\n    output: Int\n    intent: do something novel\n";
/// let tokens = lex(src).unwrap();
/// let program = parse(&tokens).unwrap();
/// let synapse = expand_with_llm(&program).unwrap();
/// ```
///
/// [`IntentError`]: crate::error::IntentError
pub fn expand_with_llm(program: &IntentProgram) -> Result<String, IntentError> {
    expand_core(program, true)
}

/// Core expansion logic shared by [`expand`] and [`expand_with_llm`].
///
/// Applications are expanded directly to `.synapse` in a single
/// LLM call. Modules go through per-capability template/LLM
/// expansion as before.
fn expand_core(program: &IntentProgram, use_llm: bool) -> Result<String, IntentError> {
    tracing::debug!(use_llm, "expanding intent program");

    if !program.applications.is_empty() && !use_llm {
        return Err(IntentError::LlmUnavailable {
            message: "application expansion requires LLM (remove --no-llm)".to_owned(),
        });
    }

    if !program.applications.is_empty() {
        let app = &program.applications[0];
        tracing::info!(name = %app.name, "expanding application directly to .synapse");
        return llm::expand_application(app);
    }

    let errors = validator::validate(program);
    if !errors.is_empty() {
        return Err(errors.into_iter().next().unwrap());
    }

    let mut output = String::new();
    let mut used_llm = false;
    let mut emitted_fns: std::collections::HashSet<String> = std::collections::HashSet::new();

    for module in &program.modules {
        for cap in &module.capabilities {
            let (code, was_llm) = expand_single_capability(cap, use_llm)?;
            used_llm |= was_llm;
            let deduped = deduplicate_functions(&code, &mut emitted_fns);
            output.push_str(&deduped);
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }
        for pipe in &module.pipelines {
            let code = expand_pipeline(pipe, &module.capabilities);
            let deduped = deduplicate_functions(&code, &mut emitted_fns);
            output.push_str(&deduped);
            if !output.ends_with('\n') {
                output.push('\n');
            }
        }
    }

    if !used_llm {
        verify_compiles(&output)?;
    }

    Ok(output)
}

// ---------------------------------------------------------------------------
// Capability Expansion
// ---------------------------------------------------------------------------

/// Expands a single capability via template, with optional LLM fallback.
/// Returns the code and whether LLM was used.
fn expand_single_capability(cap: &Capability, use_llm: bool) -> Result<(String, bool), IntentError> {
    tracing::info!(
        name = %cap.name,
        intent = %cap.intent,
        "expanding capability"
    );

    if let Some(code) = templates::expand_capability(cap) {
        return Ok((code, false));
    }

    if use_llm {
        tracing::info!(name = %cap.name, "no template match, falling back to LLM");
        let code = llm::expand_with_claude(cap)?;
        return Ok((code, true));
    }

    Err(IntentError::NoTemplateMatch {
        name: cap.name.clone(),
        intent: cap.intent.clone(),
    })
}

// ---------------------------------------------------------------------------
// Pipeline Expansion
// ---------------------------------------------------------------------------

/// Removes functions from `code` whose names are already in `seen`.
/// Adds newly seen function names to `seen`.
fn deduplicate_functions(code: &str, seen: &mut std::collections::HashSet<String>) -> String {
    let mut result = String::new();
    let mut current_fn = String::new();
    let mut current_name: Option<String> = None;

    for line in code.lines() {
        if line.starts_with("function ") {
            if let Some(ref name) = current_name {
                if !seen.contains(name) {
                    seen.insert(name.clone());
                    result.push_str(&current_fn);
                }
            }
            current_fn = format!("{line}\n");
            current_name = line
                .strip_prefix("function ")
                .and_then(|rest| rest.split('(').next())
                .map(str::to_owned);
        } else {
            current_fn.push_str(line);
            current_fn.push('\n');
        }
    }

    if let Some(ref name) = current_name {
        if !seen.contains(name) {
            seen.insert(name.clone());
            result.push_str(&current_fn);
        }
    }

    result
}

/// Expands a pipeline into a Synapse function that chains steps.
///
/// Each step binds its result to `value`, and the final step
/// uses `returns`.
fn expand_pipeline(pipe: &Pipeline, capabilities: &[Capability]) -> String {
    tracing::info!(name = %pipe.name, "expanding pipeline");

    let first_step = pipe.steps.first();
    let last_step = pipe.steps.last();

    let input_params = first_step
        .map(|s| infer_pipeline_input(s, capabilities))
        .unwrap_or_default();

    let return_type = last_step.map_or_else(|| "Int".to_owned(), |s| infer_step_output(s, capabilities));

    let mut body = String::new();

    for (i, step) in pipe.steps.iter().enumerate() {
        let args = step.args.join(", ");
        let call = format!("{}({args})", step.capability);
        let is_last = i == pipe.steps.len() - 1;

        if is_last {
            body.push_str(&format!("  returns {call}\n"));
        } else {
            let binding = pipe
                .steps
                .get(i + 1)
                .and_then(|s| s.args.first())
                .map_or_else(|| format!("v{i}"), Clone::clone);
            body.push_str(&format!("  value {binding} = {call}\n"));
        }
    }

    format!(
        "function {name}({input_params}) -> {return_type}\n{body}",
        name = pipe.name,
    )
}

/// Infers the pipeline input parameter string from the first step.
fn infer_pipeline_input(step: &PipelineStep, capabilities: &[Capability]) -> String {
    let cap = capabilities.iter().find(|c| c.name == step.capability);

    match cap {
        Some(c) => c
            .inputs
            .iter()
            .map(|p| format!("{} {}", p.ty, step.args.first().unwrap_or(&p.name)))
            .collect::<Vec<_>>()
            .join(", "),
        None => step
            .args
            .iter()
            .map(|a| format!("Int {a}"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Infers the output type of a pipeline step from its capability.
fn infer_step_output(step: &PipelineStep, capabilities: &[Capability]) -> String {
    capabilities
        .iter()
        .find(|c| c.name == step.capability)
        .and_then(|c| c.output.clone())
        .unwrap_or_else(|| "Int".to_owned())
}

// ---------------------------------------------------------------------------
// Compilation Verification
// ---------------------------------------------------------------------------

/// Verifies that the expanded `.synapse` source compiles through cortex.
fn verify_compiles(source: &str) -> Result<(), IntentError> {
    tracing::debug!("verifying expanded code compiles");

    cortex::compile_check(source).map_err(|e| IntentError::CompilationFailed { message: e.to_string() })?;

    tracing::info!("expanded code compiles successfully");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::parse};

    #[test]
    fn expand_factorial() {
        let synapse = expand_intent(
            "module m:\n  capability factorial:\n    input: Int n\n    output: Int\n    intent: compute factorial using recursion\n",
        );
        assert!(
            synapse.contains("function factorial(Int n) -> Int"),
            "signature: {synapse}"
        );
        assert!(synapse.contains("factorial(n - 1)"), "recursive call: {synapse}");
    }

    #[test]
    fn expand_fibonacci() {
        let synapse = expand_intent(
            "module m:\n  capability fib:\n    input: Int n\n    output: Int\n    intent: compute fibonacci number\n",
        );
        assert!(
            synapse.contains("fib(n - 1) + fib(n - 2)"),
            "double recursion: {synapse}"
        );
    }

    #[test]
    fn expand_list_sum() {
        let synapse = expand_intent(
            "module m:\n  capability list_sum:\n    input: List<Int> xs\n    output: Int\n    intent: sum all elements using recursion\n",
        );
        assert!(synapse.contains("x + list_sum(rest)"), "recursive sum: {synapse}");
    }

    #[test]
    fn expand_gcd() {
        let synapse = expand_intent(
            "module m:\n  capability gcd:\n    input: Int a, Int b\n    output: Int\n    intent: compute gcd using euclidean algorithm\n",
        );
        assert!(synapse.contains("gcd(b, a % b)"), "euclidean: {synapse}");
    }

    #[test]
    fn expand_power() {
        let synapse = expand_intent(
            "module m:\n  capability power:\n    input: Int base, Int exp\n    output: Int\n    intent: compute power/exponent\n",
        );
        assert!(
            synapse.contains("base * power(base, exp - 1)"),
            "recursive power: {synapse}"
        );
    }

    #[test]
    fn expand_reverse() {
        let synapse = expand_intent(
            "module m:\n  capability reverse:\n    input: List<Int> xs\n    output: List<Int>\n    intent: reverse list\n",
        );
        assert!(synapse.contains("reverse_helper"), "uses helper: {synapse}");
    }

    #[test]
    fn expand_length() {
        let synapse = expand_intent(
            "module m:\n  capability list_length:\n    input: List<Int> xs\n    output: Int\n    intent: compute length of list\n",
        );
        assert!(synapse.contains("1 + list_length(rest)"), "recursive length: {synapse}");
    }

    #[test]
    fn no_template_match_errors() {
        let tokens = lex(
            "module m:\n  capability custom:\n    input: Int x\n    output: Int\n    intent: do something completely novel\n",
        ).unwrap();
        let program = parse(&tokens).unwrap();
        let err = expand(&program).unwrap_err();
        assert!(
            matches!(err, IntentError::NoTemplateMatch { .. }),
            "expected NoTemplateMatch, got {err:?}"
        );
    }

    #[test]
    fn validation_error_propagated() {
        let program = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![Capability {
                    name: "f".to_owned(),
                    inputs: vec![],
                    intent: String::new(),
                    output: None,
                }],
                pipelines: vec![],
            }],
        };
        let err = expand(&program).unwrap_err();
        assert!(
            matches!(err, IntentError::MissingIntent { .. }),
            "expected MissingIntent, got {err:?}"
        );
    }

    #[test]
    fn expanded_code_compiles_through_cortex() {
        let synapse = expand_intent(
            "module math:\n  capability factorial:\n    input: Int n\n    output: Int\n    intent: compute factorial using recursion\n",
        );
        let tokens = cortex::lexer::lex(&synapse).unwrap();
        let ast = cortex::parser::parse(&tokens).unwrap();
        let result = cortex::checker::check(&ast);
        assert!(result.is_ok(), "cortex should accept expanded code: {result:?}");
    }

    #[test]
    fn expand_multiple_capabilities() {
        let source = "\
module math:
  capability factorial:
    input: Int n
    output: Int
    intent: compute factorial using recursion

  capability fibonacci:
    input: Int n
    output: Int
    intent: compute fibonacci number
";
        let synapse = expand_intent(source);
        assert!(synapse.contains("function factorial"), "has factorial: {synapse}");
        assert!(synapse.contains("function fibonacci"), "has fibonacci: {synapse}");
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Lexes, parses, and expands an intent source.
    fn expand_intent(source: &str) -> String {
        let tokens = lex(source).unwrap();
        let program = parse(&tokens).unwrap();
        expand(&program).unwrap()
    }
}
