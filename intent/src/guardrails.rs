use crate::{
    ast::{CapabilityDef, CapabilityKind},
    error::IntentError,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// A suggestion for a missing capability declaration.
///
/// Produced when the LLM generates code that imports something
/// not declared in the application's capabilities.
///
/// ```
/// # use intent::guardrails::CapabilitySuggestion;
/// let s = CapabilitySuggestion {
///     undeclared_import: "serde_json".to_owned(),
///     suggested_capability: "serde_json: rust crate serde_json".to_owned(),
/// };
/// assert!(s.suggested_capability.contains("rust crate"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitySuggestion {
    /// The import statement the LLM used without authorization.
    pub undeclared_import: String,
    /// The capability declaration the user should add.
    pub suggested_capability: String,
}

/// Validates that every `import` in `synapse_source` maps to a
/// declared capability.
///
/// Parses the source through cortex's lexer and parser, extracts
/// all [`Import`] statements, and checks each one against the
/// provided capabilities. Returns `Ok(())` when all imports are
/// covered, or `Err` with a list of suggestions for undeclared
/// imports.
///
/// # Errors
///
/// Returns a list of [`CapabilitySuggestion`]s when the source
/// contains imports that do not map to any declared capability.
///
/// ```
/// # use intent::guardrails::validate_imports;
/// # use intent::ast::{CapabilityDef, CapabilityKind};
/// let caps = vec![CapabilityDef {
///     name: "builtins".to_owned(),
///     kind: CapabilityKind::Import { path: None },
/// }];
/// let src = "import builtins\nfunction f() -> Int\n  returns 42\n";
/// assert!(validate_imports(src, &caps).is_ok());
/// ```
///
/// [`Import`]: cortex::ast::Import
pub fn validate_imports(synapse_source: &str, capabilities: &[CapabilityDef]) -> Result<(), Vec<CapabilitySuggestion>> {
    let Ok(tokens) = cortex::lexer::lex(synapse_source) else {
        return Ok(());
    };
    let Ok(program) = cortex::parser::parse(&tokens) else {
        return Ok(());
    };

    let suggestions: Vec<CapabilitySuggestion> = program
        .imports
        .iter()
        .filter_map(|imp| check_import(imp, capabilities))
        .collect();

    if suggestions.is_empty() {
        Ok(())
    } else {
        Err(suggestions)
    }
}

/// Converts a list of [`CapabilitySuggestion`]s into a single
/// user-friendly [`IntentError::UndeclaredImport`] for the first
/// undeclared import, or a combined error message if there are
/// multiple.
///
/// ```
/// # use intent::guardrails::{CapabilitySuggestion, suggestions_to_errors};
/// let suggestions = vec![CapabilitySuggestion {
///     undeclared_import: "builtins".to_owned(),
///     suggested_capability: "builtins: import".to_owned(),
/// }];
/// let errors = suggestions_to_errors(&suggestions);
/// assert_eq!(errors.len(), 1);
/// ```
pub fn suggestions_to_errors(suggestions: &[CapabilitySuggestion]) -> Vec<IntentError> {
    suggestions
        .iter()
        .map(|s| IntentError::UndeclaredImport {
            import: s.undeclared_import.clone(),
            suggestion: s.suggested_capability.clone(),
        })
        .collect()
}

/// Formats a list of [`CapabilitySuggestion`]s into a single
/// human-readable error message.
///
/// ```
/// # use intent::guardrails::{CapabilitySuggestion, format_suggestions};
/// let suggestions = vec![CapabilitySuggestion {
///     undeclared_import: "import builtins".to_owned(),
///     suggested_capability: "builtins: import".to_owned(),
/// }];
/// let msg = format_suggestions(&suggestions);
/// assert!(msg.contains("undeclared imports"));
/// ```
pub fn format_suggestions(suggestions: &[CapabilitySuggestion]) -> String {
    let mut msg = "generated code uses undeclared imports:\n".to_owned();
    for s in suggestions {
        msg.push_str(&format!(
            "  - '{}': add '{}' to capabilities\n",
            s.undeclared_import, s.suggested_capability
        ));
    }
    msg
}

// ---------------------------------------------------------------------------
// Private Implementation
// ---------------------------------------------------------------------------

/// Checks a single import against capabilities. Returns `Some`
/// suggestion if the import is undeclared.
fn check_import(import: &cortex::ast::Import, capabilities: &[CapabilityDef]) -> Option<CapabilitySuggestion> {
    match import {
        cortex::ast::Import::Builtins => {
            let covered = capabilities
                .iter()
                .any(|c| matches!(c.kind, CapabilityKind::Import { .. }) && c.name == "builtins");
            if covered {
                None
            } else {
                Some(CapabilitySuggestion {
                    undeclared_import: "import builtins".to_owned(),
                    suggested_capability: "builtins: import".to_owned(),
                })
            }
        },
        cortex::ast::Import::SynapseModule(name) => {
            let covered = capabilities.iter().any(|c| match &c.kind {
                CapabilityKind::Import { .. } | CapabilityKind::NewModule => c.name == *name,
                _ => false,
            });
            if covered {
                None
            } else {
                Some(CapabilitySuggestion {
                    undeclared_import: format!("import {name}"),
                    suggested_capability: format!("{name}: new module"),
                })
            }
        },
        cortex::ast::Import::RustCrate(name) => {
            let covered = capabilities.iter().any(|c| {
                matches!(
                    &c.kind,
                    CapabilityKind::ImportRustCrate { spec }
                        if spec.name == *name
                )
            });
            if covered {
                None
            } else {
                Some(CapabilitySuggestion {
                    undeclared_import: format!("import rust {name}"),
                    suggested_capability: format!("{name}: import rust crate {name}"),
                })
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CapabilityDef, CapabilityKind, RustCrateSpec};

    #[test]
    fn all_imports_matching_capabilities() {
        let caps = vec![
            CapabilityDef {
                name: "builtins".to_owned(),
                kind: CapabilityKind::Import { path: None },
            },
            CapabilityDef {
                name: "math".to_owned(),
                kind: CapabilityKind::NewModule,
            },
            CapabilityDef {
                name: "serde_json".to_owned(),
                kind: CapabilityKind::ImportRustCrate {
                    spec: RustCrateSpec {
                        name: "serde_json".to_owned(),
                        version: Some("1.0.140".to_owned()),
                        path: None,
                        git: None,
                    },
                },
            },
        ];
        let src = "\
import builtins
import math
import rust serde_json
function f() -> Int
  returns 42
";
        assert!(validate_imports(src, &caps).is_ok(), "all imports are declared");
    }

    #[test]
    fn undeclared_builtins() {
        let caps: Vec<CapabilityDef> = vec![];
        let src = "\
import builtins
function f() -> Int
  returns 42
";
        let suggestions = validate_imports(src, &caps).unwrap_err();
        assert_eq!(suggestions.len(), 1, "one undeclared import");
        assert_eq!(
            suggestions[0].undeclared_import, "import builtins",
            "identifies builtins"
        );
        assert_eq!(
            suggestions[0].suggested_capability, "builtins: import",
            "suggests import"
        );
    }

    #[test]
    fn undeclared_rust_crate() {
        let caps: Vec<CapabilityDef> = vec![];
        let src = "\
import rust serde_json
function f() -> Int
  returns 42
";
        let suggestions = validate_imports(src, &caps).unwrap_err();
        assert_eq!(suggestions.len(), 1, "one undeclared import");
        assert_eq!(
            suggestions[0].undeclared_import, "import rust serde_json",
            "identifies rust crate"
        );
        assert_eq!(
            suggestions[0].suggested_capability, "serde_json: import rust crate serde_json",
            "suggests import rust crate"
        );
    }

    #[test]
    fn undeclared_synapse_module() {
        let caps: Vec<CapabilityDef> = vec![];
        let src = "\
import bar
function f() -> Int
  returns 42
";
        let suggestions = validate_imports(src, &caps).unwrap_err();
        assert_eq!(suggestions.len(), 1, "one undeclared import");
        assert_eq!(
            suggestions[0].undeclared_import, "import bar",
            "identifies synapse module"
        );
        assert_eq!(
            suggestions[0].suggested_capability, "bar: new module",
            "suggests new module"
        );
    }

    #[test]
    fn import_capability_covers_synapse_module() {
        let caps = vec![CapabilityDef {
            name: "utils".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }];
        let src = "\
import utils
function f() -> Int
  returns 42
";
        assert!(
            validate_imports(src, &caps).is_ok(),
            "import capability covers synapse module import"
        );
    }

    #[test]
    fn multiple_undeclared_imports() {
        let caps: Vec<CapabilityDef> = vec![];
        let src = "\
import builtins
import rust tokio
function f() -> Int
  returns 42
";
        let suggestions = validate_imports(src, &caps).unwrap_err();
        assert_eq!(suggestions.len(), 2, "two undeclared imports");
    }

    #[test]
    fn no_imports_always_ok() {
        let caps: Vec<CapabilityDef> = vec![];
        let src = "\
function f() -> Int
  returns 42
";
        assert!(validate_imports(src, &caps).is_ok(), "no imports is always valid");
    }

    #[test]
    fn format_suggestions_message() {
        let suggestions = vec![
            CapabilitySuggestion {
                undeclared_import: "import builtins".to_owned(),
                suggested_capability: "builtins: import".to_owned(),
            },
            CapabilitySuggestion {
                undeclared_import: "import rust serde_json".to_owned(),
                suggested_capability: "serde_json: import rust crate 1.0.140".to_owned(),
            },
        ];
        let msg = format_suggestions(&suggestions);
        assert!(msg.contains("undeclared imports"), "header present: {msg}");
        assert!(msg.contains("import builtins"), "first import listed: {msg}");
        assert!(msg.contains("import rust serde_json"), "second import listed: {msg}");
    }

    #[test]
    fn suggestions_to_errors_converts_all() {
        let suggestions = vec![
            CapabilitySuggestion {
                undeclared_import: "import builtins".to_owned(),
                suggested_capability: "builtins: import".to_owned(),
            },
            CapabilitySuggestion {
                undeclared_import: "import rust foo".to_owned(),
                suggested_capability: "foo: import rust crate foo".to_owned(),
            },
        ];
        let errors = suggestions_to_errors(&suggestions);
        assert_eq!(errors.len(), 2, "one error per suggestion");
        assert!(
            matches!(
                &errors[0],
                IntentError::UndeclaredImport { import, .. }
                    if import == "import builtins"
            ),
            "first error matches: {:?}",
            errors[0]
        );
    }

    #[test]
    fn invalid_source_returns_ok() {
        let caps: Vec<CapabilityDef> = vec![];
        let src = "this is not valid synapse";
        assert!(
            validate_imports(src, &caps).is_ok(),
            "unparseable source is not an import error"
        );
    }
}
