use std::{collections::HashMap, process::Command};

use cortex::module::ModuleApi;

use crate::{
    ast::{Application, Capability, Property},
    error::IntentError,
    guardrails, prompt,
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The result of expanding an application that may include generated modules.
///
/// Contains the main `.synapse` source and any separately generated
/// module sources keyed by module name.
///
/// ```
/// # use intent::llm::ExpandedApplication;
/// # use std::collections::HashMap;
/// let ea = ExpandedApplication {
///     main_synapse: "function main() -> Int\n  returns 0\n".to_owned(),
///     modules: HashMap::new(),
/// };
/// assert!(ea.modules.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedApplication {
    /// The main `.synapse` source code.
    pub main_synapse: String,
    /// Generated module sources, keyed by module name.
    pub modules: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Expands a single [`Capability`] by calling the `claude` CLI.
///
/// Builds a structured prompt, invokes `claude -p`, validates
/// the response through cortex, and returns the generated
/// Synapse source. Retries once if the first attempt produces
/// invalid code.
///
/// # Errors
///
/// Returns [`IntentError`] if the `claude` CLI is unavailable,
/// fails, or produces invalid output.
///
/// [`Capability`]: crate::ast::Capability
/// [`IntentError`]: crate::error::IntentError
pub fn expand_with_claude(cap: &Capability) -> Result<String, IntentError> {
    match try_expand(cap) {
        Ok(code) => Ok(code),
        Err(IntentError::LlmOutputInvalid { name, message }) => {
            tracing::warn!(name = %name, error = %message, "first LLM attempt invalid, retrying");
            try_expand(cap)
        },
        Err(e) => Err(e),
    }
}

/// Generates a standalone `.synapse` module via the `claude` CLI.
///
/// Builds a module-specific prompt, invokes `claude -p`, validates
/// the response through cortex, and returns the generated Synapse
/// source. Retries once if the first attempt produces invalid code.
///
/// # Errors
///
/// Returns [`IntentError`] if the `claude` CLI is unavailable,
/// fails, or produces invalid output after two attempts.
///
/// [`IntentError`]: crate::error::IntentError
pub fn expand_new_module(
    module_name: &str,
    properties: &[Property],
    other_apis: &HashMap<String, ModuleApi>,
) -> Result<String, IntentError> {
    match try_expand_new_module(module_name, properties, other_apis) {
        Ok(code) => Ok(code),
        Err(IntentError::LlmOutputInvalid { name, message }) => {
            tracing::warn!(name = %name, error = %message, "first module LLM attempt invalid, retrying");
            try_expand_new_module(module_name, properties, other_apis)
        },
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Private Implementation
// ---------------------------------------------------------------------------

/// Single attempt at new module expansion via LLM.
fn try_expand_new_module(
    module_name: &str,
    properties: &[Property],
    other_apis: &HashMap<String, ModuleApi>,
) -> Result<String, IntentError> {
    let prompt_text = prompt::build_new_module_prompt(module_name, properties, other_apis);

    tracing::info!(module = %module_name, "expanding new module via claude CLI");

    let output = Command::new("claude")
        .args(["-p", &prompt_text, "--output-format", "text"])
        .output()
        .map_err(|e| IntentError::LlmUnavailable {
            message: format!("failed to run 'claude': {e}. Is Claude Code installed?"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IntentError::LlmFailed {
            message: format!("claude exited with {}: {stderr}", output.status),
        });
    }

    let response = String::from_utf8_lossy(&output.stdout);
    let code = extract_function(&response);

    tracing::debug!(module = %module_name, len = code.len(), "received module LLM response");

    validate_synapse_by_name(&code, module_name)?;

    tracing::info!(module = %module_name, "new module expanded and validated successfully");
    Ok(code)
}

/// Single attempt at LLM expansion.
fn try_expand(cap: &Capability) -> Result<String, IntentError> {
    let prompt = prompt::build_prompt(cap);

    tracing::info!(name = %cap.name, "expanding capability via claude CLI");

    let output = Command::new("claude")
        .args(["-p", &prompt, "--output-format", "text"])
        .output()
        .map_err(|e| IntentError::LlmUnavailable {
            message: format!("failed to run 'claude': {e}. Is Claude Code installed?"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IntentError::LlmFailed {
            message: format!("claude exited with {}: {stderr}", output.status),
        });
    }

    let response = String::from_utf8_lossy(&output.stdout);
    let code = extract_function(&response);

    tracing::debug!(name = %cap.name, len = code.len(), "received LLM response");

    validate_synapse(&code, cap)?;

    Ok(code)
}

/// Expands an [`Application`] directly into `.synapse` source code
/// by calling the `claude` CLI in a single LLM call.
///
/// `apis` provides the resolved public API for each declared
/// capability so the prompt shows the LLM exactly what functions
/// are available.
///
/// Returns the validated `.synapse` source string. Retries once
/// if the first attempt produces invalid code.
///
/// ```no_run
/// # use std::collections::HashMap;
/// # use intent::ast::*;
/// # use intent::llm::expand_application;
/// let app = Application {
///     name: "demo".to_owned(),
///     args: ArgsDef::default(),
///     capabilities: vec![],
///     environment: vec![],
///     intent: StructuredIntent {
///         description: "print hello world".to_owned(),
///         properties: vec![],
///     },
/// };
/// let synapse = expand_application(&app, &HashMap::new()).unwrap();
/// assert!(synapse.contains("function main"));
/// ```
///
/// # Errors
///
/// Returns [`IntentError`] if the `claude` CLI is unavailable,
/// fails, or produces invalid output.
///
/// [`Application`]: crate::ast::Application
/// [`IntentError`]: crate::error::IntentError
pub fn expand_application(app: &Application, apis: &HashMap<String, ModuleApi>) -> Result<String, IntentError> {
    match try_expand_application(app, apis) {
        Ok(code) => Ok(code),
        Err(IntentError::LlmOutputInvalid { name, message }) => {
            tracing::warn!(name = %name, error = %message, "first application LLM attempt invalid, retrying");
            try_expand_application(app, apis)
        },
        Err(e) => Err(e),
    }
}

/// Single attempt at application expansion.
fn try_expand_application(app: &Application, apis: &HashMap<String, ModuleApi>) -> Result<String, IntentError> {
    let prompt_text = prompt::build_application_prompt(app, apis);

    tracing::info!(name = %app.name, "expanding application directly to .synapse via claude CLI");

    let output = Command::new("claude")
        .args(["-p", &prompt_text, "--output-format", "text"])
        .output()
        .map_err(|e| IntentError::LlmUnavailable {
            message: format!("failed to run 'claude': {e}. Is Claude Code installed?"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IntentError::LlmFailed {
            message: format!("claude exited with {}: {stderr}", output.status),
        });
    }

    let response = String::from_utf8_lossy(&output.stdout);
    let code = extract_function(&response);

    tracing::debug!(name = %app.name, len = code.len(), "received application .synapse code");

    validate_synapse_by_name(&code, &app.name)?;
    validate_imports_against_capabilities(&code, app)?;

    tracing::info!(name = %app.name, "application expanded and validated successfully");

    Ok(code)
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Strips markdown fences if the LLM wrapped the output.
fn extract_function(response: &str) -> String {
    let trimmed = response.trim();

    if !trimmed.starts_with("```") {
        return trimmed.to_owned();
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    let start = 1;
    let end = lines.iter().rposition(|l| l.trim() == "```").unwrap_or(lines.len());

    lines[start..end].join("\n")
}

/// Validates the generated code compiles through the cortex pipeline.
fn validate_synapse(code: &str, cap: &Capability) -> Result<(), IntentError> {
    tracing::debug!(name = %cap.name, "validating LLM output through cortex");
    validate_synapse_by_name(code, &cap.name)
}

/// Validates that every import in the generated code maps to a
/// declared capability.
fn validate_imports_against_capabilities(code: &str, app: &Application) -> Result<(), IntentError> {
    tracing::debug!(name = %app.name, "validating imports against declared capabilities");

    if let Err(suggestions) = guardrails::validate_imports(code, &app.capabilities) {
        let msg = guardrails::format_suggestions(&suggestions);
        return Err(IntentError::LlmOutputInvalid {
            name: app.name.clone(),
            message: msg,
        });
    }

    tracing::info!(name = %app.name, "all imports match declared capabilities");
    Ok(())
}

/// Validates generated code through the cortex pipeline, using a
/// name for error attribution.
fn validate_synapse_by_name(code: &str, name: &str) -> Result<(), IntentError> {
    cortex::compile_check(code).map_err(|e| IntentError::LlmOutputInvalid {
        name: name.to_owned(),
        message: e.to_string(),
    })?;

    tracing::info!(name = %name, "LLM output validated successfully");
    Ok(())
}
