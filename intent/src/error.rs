// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Errors produced during intent file processing.
///
/// Covers lexing, parsing, validation, and expansion.
///
/// ```
/// # use intent::error::IntentError;
/// let err = IntentError::UnexpectedChar {
///     line: 1,
///     column: 5,
///     ch: '@',
/// };
/// assert!(format!("{err}").contains("unexpected character"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntentError {
    /// An unrecognized character was encountered during lexing.
    #[error("{line}:{column}: unexpected character '{ch}'")]
    UnexpectedChar {
        /// 1-based line number.
        line: u32,
        /// 1-based column number.
        column: u32,
        /// The offending character.
        ch: char,
    },

    /// A token was found where a different one was expected.
    #[error("{line}:{column}: expected {expected}, found {found}")]
    Unexpected {
        /// 1-based line number.
        line: u32,
        /// 1-based column number.
        column: u32,
        /// What was expected.
        expected: String,
        /// What was actually found.
        found: String,
    },

    /// A duplicate name was declared (type, module, or capability).
    #[error("duplicate {kind} name '{name}'")]
    DuplicateName {
        /// The kind of duplicate (type, module, capability).
        kind: String,
        /// The duplicate name.
        name: String,
    },

    /// A pipeline step references an undefined capability.
    #[error("pipeline '{pipeline}' step references undefined capability '{capability}'")]
    UndefinedCapability {
        /// The pipeline containing the bad reference.
        pipeline: String,
        /// The undefined capability name.
        capability: String,
    },

    /// A type reference could not be resolved.
    #[error("unresolved type '{name}'")]
    UnresolvedType {
        /// The unresolved type name.
        name: String,
    },

    /// A capability is missing its intent phrase.
    #[error("capability '{name}' has no intent phrase")]
    MissingIntent {
        /// The capability missing its intent.
        name: String,
    },

    /// An intent phrase could not be matched to any template.
    #[error("no template matches intent '{intent}' for capability '{name}'")]
    NoTemplateMatch {
        /// The capability name.
        name: String,
        /// The unmatched intent phrase.
        intent: String,
    },

    /// The expanded `.synapse` code failed cortex compilation.
    #[error("expanded code failed compilation: {message}")]
    CompilationFailed {
        /// The compilation error message.
        message: String,
    },

    /// The `claude` CLI could not be found or executed.
    #[error("LLM unavailable: {message}")]
    LlmUnavailable {
        /// Details about the unavailability.
        message: String,
    },

    /// The `claude` CLI returned a non-zero exit code.
    #[error("LLM expansion failed: {message}")]
    LlmFailed {
        /// The failure details.
        message: String,
    },

    /// The LLM produced output that does not compile as valid Synapse.
    #[error("LLM output for '{name}' is invalid: {message}")]
    LlmOutputInvalid {
        /// The capability or application name.
        name: String,
        /// The validation error message.
        message: String,
    },

    /// A property references an undeclared capability.
    #[error("property '{property}' references undeclared capability '{capability}'")]
    UndefinedCapabilityRef {
        /// The property action text.
        property: String,
        /// The undeclared capability name.
        capability: String,
    },

    /// Structured intent has an empty description.
    #[error("application intent has an empty description")]
    EmptyDescription,

    /// Structured intent has no properties.
    #[error("application intent has no properties")]
    NoProperties,

    /// A duplicate capability name was declared in an application.
    #[error("duplicate capability name '{name}'")]
    DuplicateCapability {
        /// The duplicate capability name.
        name: String,
    },

    /// A declared capability is not referenced by any property.
    #[error("capability '{name}' is declared but not referenced by any property")]
    UnusedCapability {
        /// The unreferenced capability name.
        name: String,
    },

    /// The LLM generated code with undeclared imports.
    #[error(
        "generated code imports '{import}' but no matching capability is declared.\nsuggestion: add '{suggestion}' to capabilities"
    )]
    UndeclaredImport {
        /// The import statement found in the generated code.
        import: String,
        /// The suggested capability declaration.
        suggestion: String,
    },
}
