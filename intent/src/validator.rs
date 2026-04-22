use std::collections::HashSet;

use crate::{ast::IntentProgram, error::IntentError};

/// Validates an [`IntentProgram`] for structural correctness.
///
/// Checks:
/// 1. No duplicate type, module, or capability names.
/// 2. Pipeline steps reference defined capabilities.
/// 3. All type references resolve to defined or built-in types.
/// 4. Every capability has an intent phrase.
/// 5. Application capability declarations are unique.
/// 6. Application property references resolve to declared capabilities.
/// 7. Application descriptions are non-empty.
/// 8. Applications have at least one property (when structured).
///
/// # Errors
///
/// Returns all detected errors in a [`Vec`].
///
/// ```
/// # use intent::{lexer::lex, parser::parse, validator::validate};
/// let tokens = lex(
///     "module m:\n  capability f:\n    input: Int n\n    output: Int\n    intent: do something\n",
/// )
/// .unwrap();
/// let prog = parse(&tokens).unwrap();
/// let errors = validate(&prog);
/// assert!(errors.is_empty());
/// ```
///
/// [`IntentProgram`]: crate::ast::IntentProgram
pub fn validate(program: &IntentProgram) -> Vec<IntentError> {
    tracing::debug!("validating intent program");
    let mut errors = Vec::new();

    check_duplicate_types(program, &mut errors);
    check_duplicate_modules(program, &mut errors);
    check_duplicate_capabilities(program, &mut errors);
    check_pipeline_references(program, &mut errors);
    check_type_references(program, &mut errors);
    check_missing_intents(program, &mut errors);
    check_application_capabilities(program, &mut errors);

    errors
}

// ---------------------------------------------------------------------------
// Validation Rules
// ---------------------------------------------------------------------------

/// Reports duplicate type names.
fn check_duplicate_types(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    let mut seen = HashSet::new();
    for td in &program.types {
        if !seen.insert(&td.name) {
            tracing::warn!(name = %td.name, "duplicate type name");
            errors.push(IntentError::DuplicateName {
                kind: "type".to_owned(),
                name: td.name.clone(),
            });
        }
    }
}

/// Reports duplicate module names.
fn check_duplicate_modules(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    let mut seen = HashSet::new();
    for m in &program.modules {
        if !seen.insert(&m.name) {
            tracing::warn!(name = %m.name, "duplicate module name");
            errors.push(IntentError::DuplicateName {
                kind: "module".to_owned(),
                name: m.name.clone(),
            });
        }
    }
}

/// Reports duplicate capability names within each module.
fn check_duplicate_capabilities(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    for m in &program.modules {
        let mut seen = HashSet::new();
        for cap in &m.capabilities {
            if !seen.insert(&cap.name) {
                tracing::warn!(
                    module = %m.name,
                    capability = %cap.name,
                    "duplicate capability name"
                );
                errors.push(IntentError::DuplicateName {
                    kind: "capability".to_owned(),
                    name: cap.name.clone(),
                });
            }
        }
    }
}

/// Checks that pipeline steps reference defined capabilities.
fn check_pipeline_references(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    for m in &program.modules {
        let cap_names: HashSet<&str> = m.capabilities.iter().map(|c| c.name.as_str()).collect();

        for pipe in &m.pipelines {
            for step in &pipe.steps {
                if !cap_names.contains(step.capability.as_str()) {
                    tracing::warn!(
                        pipeline = %pipe.name,
                        step = %step.capability,
                        "undefined capability reference in pipeline"
                    );
                    errors.push(IntentError::UndefinedCapability {
                        pipeline: pipe.name.clone(),
                        capability: step.capability.clone(),
                    });
                }
            }
        }
    }
}

/// Checks that all type references resolve to built-in or defined types.
fn check_type_references(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    let builtin: HashSet<&str> = ["Int", "Bool", "String", "List"].iter().copied().collect();

    let user_types: HashSet<&str> = program.types.iter().map(|t| t.name.as_str()).collect();

    for td in &program.types {
        for field in &td.fields {
            if !check_all_type_parts(&field.ty, &builtin, &user_types) {
                tracing::warn!(ty = %field.ty, "unresolved type");
                errors.push(IntentError::UnresolvedType { name: field.ty.clone() });
            }
        }
    }

    for m in &program.modules {
        for cap in &m.capabilities {
            for param in &cap.inputs {
                if !check_all_type_parts(&param.ty, &builtin, &user_types) {
                    tracing::warn!(ty = %param.ty, "unresolved type");
                    errors.push(IntentError::UnresolvedType { name: param.ty.clone() });
                }
            }
            if let Some(ref out_ty) = cap.output {
                if !check_all_type_parts(out_ty, &builtin, &user_types) {
                    tracing::warn!(ty = %out_ty, "unresolved type");
                    errors.push(IntentError::UnresolvedType { name: out_ty.clone() });
                }
            }
        }
    }
}

/// Checks that every capability has a non-empty intent phrase.
fn check_missing_intents(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    for m in &program.modules {
        for cap in &m.capabilities {
            if cap.intent.trim().is_empty() {
                tracing::warn!(
                    capability = %cap.name,
                    "missing intent phrase"
                );
                errors.push(IntentError::MissingIntent { name: cap.name.clone() });
            }
        }
    }
}

/// Validates application capability declarations and structured intent.
fn check_application_capabilities(program: &IntentProgram, errors: &mut Vec<IntentError>) {
    for app in &program.applications {
        let mut cap_names = HashSet::new();
        for cap in &app.capabilities {
            if !cap_names.insert(&cap.name) {
                tracing::warn!(name = %cap.name, "duplicate capability name in application");
                errors.push(IntentError::DuplicateCapability { name: cap.name.clone() });
            }
        }

        if !app.capabilities.is_empty() {
            if app.intent.description.trim().is_empty() {
                tracing::warn!(app = %app.name, "empty description in structured intent");
                errors.push(IntentError::EmptyDescription);
            }

            if app.intent.properties.is_empty() {
                tracing::warn!(app = %app.name, "no properties in structured intent");
                errors.push(IntentError::NoProperties);
            }

            for prop in &app.intent.properties {
                if !cap_names.contains(&prop.capability) {
                    tracing::warn!(
                        property = %prop.action,
                        capability = %prop.capability,
                        "property references undeclared capability"
                    );
                    errors.push(IntentError::UndefinedCapabilityRef {
                        property: prop.action.clone(),
                        capability: prop.capability.clone(),
                    });
                }
            }

            let referenced: HashSet<&str> = app.intent.properties.iter().map(|p| p.capability.as_str()).collect();
            for cap in &app.capabilities {
                if !referenced.contains(cap.name.as_str()) {
                    tracing::warn!(name = %cap.name, "declared capability not referenced by any property");
                    errors.push(IntentError::UnusedCapability { name: cap.name.clone() });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Type Reference Utilities
// ---------------------------------------------------------------------------

/// Extracts the base type name from a possibly-generic type string.
fn extract_base_type(ty: &str) -> &str {
    ty.split('<').next().unwrap_or(ty)
}

/// Checks all type components in a possibly-generic type string.
fn check_all_type_parts(ty: &str, builtin: &HashSet<&str>, user_types: &HashSet<&str>) -> bool {
    let base = extract_base_type(ty);
    if !builtin.contains(base) && !user_types.contains(base) {
        return false;
    }
    if let Some(rest) = ty
        .strip_prefix(base)
        .and_then(|s| s.strip_prefix('<'))
        .and_then(|s| s.strip_suffix('>'))
    {
        check_all_type_parts(rest, builtin, user_types)
    } else {
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer::lex, parser::parse};

    #[test]
    fn valid_program() {
        let prog = parse_intent(
            "module m:\n  capability f:\n    input: Int n\n    output: Int\n    intent: compute factorial\n",
        );
        let errors = validate(&prog);
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
    }

    #[test]
    fn duplicate_type_names() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![
                crate::ast::TypeDef {
                    name: "Pair".to_owned(),
                    fields: vec![],
                },
                crate::ast::TypeDef {
                    name: "Pair".to_owned(),
                    fields: vec![],
                },
            ],
            modules: vec![],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 1, "one duplicate type error");
        assert!(
            matches!(
                &errors[0],
                IntentError::DuplicateName { kind, name }
                if kind == "type" && name == "Pair"
            ),
            "expected DuplicateName for Pair, got {:?}",
            errors[0]
        );
    }

    #[test]
    fn duplicate_module_names() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![
                crate::ast::Module {
                    name: "m".to_owned(),
                    capabilities: vec![],
                    pipelines: vec![],
                },
                crate::ast::Module {
                    name: "m".to_owned(),
                    capabilities: vec![],
                    pipelines: vec![],
                },
            ],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 1, "one duplicate module error");
        assert!(
            matches!(
                &errors[0],
                IntentError::DuplicateName { kind, .. }
                if kind == "module"
            ),
            "expected DuplicateName for module, got {:?}",
            errors[0]
        );
    }

    #[test]
    fn duplicate_capability_names() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![
                    crate::ast::Capability {
                        name: "f".to_owned(),
                        inputs: vec![],
                        intent: "do something".to_owned(),
                        output: None,
                    },
                    crate::ast::Capability {
                        name: "f".to_owned(),
                        inputs: vec![],
                        intent: "do another thing".to_owned(),
                        output: None,
                    },
                ],
                pipelines: vec![],
            }],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 1, "one duplicate capability error");
    }

    #[test]
    fn undefined_pipeline_capability() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![crate::ast::Capability {
                    name: "a".to_owned(),
                    inputs: vec![],
                    intent: "do a".to_owned(),
                    output: None,
                }],
                pipelines: vec![crate::ast::Pipeline {
                    name: "p".to_owned(),
                    steps: vec![crate::ast::PipelineStep {
                        args: vec![],
                        capability: "nonexistent".to_owned(),
                    }],
                }],
            }],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 1, "one undefined capability error");
        assert!(
            matches!(
                &errors[0],
                IntentError::UndefinedCapability { pipeline, capability }
                if pipeline == "p" && capability == "nonexistent"
            ),
            "expected UndefinedCapability, got {:?}",
            errors[0]
        );
    }

    #[test]
    fn unresolved_type_reference() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![crate::ast::Capability {
                    name: "f".to_owned(),
                    inputs: vec![crate::ast::Param {
                        name: "x".to_owned(),
                        ty: "Unknown".to_owned(),
                    }],
                    intent: "do something".to_owned(),
                    output: None,
                }],
                pipelines: vec![],
            }],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 1, "one unresolved type error");
        assert!(
            matches!(
                &errors[0],
                IntentError::UnresolvedType { name }
                if name == "Unknown"
            ),
            "expected UnresolvedType, got {:?}",
            errors[0]
        );
    }

    #[test]
    fn missing_intent_phrase() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![crate::ast::Capability {
                    name: "f".to_owned(),
                    inputs: vec![],
                    intent: String::new(),
                    output: None,
                }],
                pipelines: vec![],
            }],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 1, "one missing intent error");
        assert!(
            matches!(
                &errors[0],
                IntentError::MissingIntent { name }
                if name == "f"
            ),
            "expected MissingIntent, got {:?}",
            errors[0]
        );
    }

    #[test]
    fn list_type_resolves() {
        let prog = parse_intent(
            "module m:\n  capability s:\n    input: List<Int> xs\n    output: Int\n    intent: sum elements\n",
        );
        let errors = validate(&prog);
        assert!(errors.is_empty(), "List<Int> should resolve, got {errors:?}");
    }

    #[test]
    fn user_defined_type_resolves() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![crate::ast::TypeDef {
                name: "Pair".to_owned(),
                fields: vec![crate::ast::Field {
                    name: "x".to_owned(),
                    ty: "Int".to_owned(),
                }],
            }],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![crate::ast::Capability {
                    name: "f".to_owned(),
                    inputs: vec![crate::ast::Param {
                        name: "p".to_owned(),
                        ty: "Pair".to_owned(),
                    }],
                    intent: "do something".to_owned(),
                    output: Some("Int".to_owned()),
                }],
                pipelines: vec![],
            }],
        };
        let errors = validate(&prog);
        assert!(errors.is_empty(), "Pair should resolve, got {errors:?}");
    }

    #[test]
    fn multiple_errors() {
        let prog = IntentProgram {
            applications: vec![],
            types: vec![],
            modules: vec![crate::ast::Module {
                name: "m".to_owned(),
                capabilities: vec![crate::ast::Capability {
                    name: "f".to_owned(),
                    inputs: vec![crate::ast::Param {
                        name: "x".to_owned(),
                        ty: "Unknown".to_owned(),
                    }],
                    intent: String::new(),
                    output: None,
                }],
                pipelines: vec![],
            }],
        };
        let errors = validate(&prog);
        assert_eq!(errors.len(), 2, "unresolved type + missing intent");
    }

    #[test]
    fn duplicate_app_capability_names() {
        let prog = IntentProgram {
            applications: vec![crate::ast::Application {
                name: "app".to_owned(),
                args: crate::ast::ArgsDef::default(),
                capabilities: vec![
                    crate::ast::CapabilityDef {
                        name: "builtins".to_owned(),
                        kind: crate::ast::CapabilityKind::Import { path: None },
                    },
                    crate::ast::CapabilityDef {
                        name: "builtins".to_owned(),
                        kind: crate::ast::CapabilityKind::Import { path: None },
                    },
                ],
                environment: vec![],
                intent: crate::ast::StructuredIntent {
                    description: "test".to_owned(),
                    properties: vec![crate::ast::Property {
                        capability: "builtins".to_owned(),
                        action: "print".to_owned(),
                    }],
                },
            }],
            types: vec![],
            modules: vec![],
        };
        let errors = validate(&prog);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, IntentError::DuplicateCapability { .. })),
            "expected DuplicateCapability, got {errors:?}"
        );
    }

    #[test]
    fn undefined_capability_ref_in_property() {
        let prog = IntentProgram {
            applications: vec![crate::ast::Application {
                name: "app".to_owned(),
                args: crate::ast::ArgsDef::default(),
                capabilities: vec![crate::ast::CapabilityDef {
                    name: "builtins".to_owned(),
                    kind: crate::ast::CapabilityKind::Import { path: None },
                }],
                environment: vec![],
                intent: crate::ast::StructuredIntent {
                    description: "test".to_owned(),
                    properties: vec![crate::ast::Property {
                        capability: "nonexistent".to_owned(),
                        action: "do something".to_owned(),
                    }],
                },
            }],
            types: vec![],
            modules: vec![],
        };
        let errors = validate(&prog);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                IntentError::UndefinedCapabilityRef { capability, .. }
                if capability == "nonexistent"
            )),
            "expected UndefinedCapabilityRef, got {errors:?}"
        );
    }

    #[test]
    fn empty_description_error() {
        let prog = IntentProgram {
            applications: vec![crate::ast::Application {
                name: "app".to_owned(),
                args: crate::ast::ArgsDef::default(),
                capabilities: vec![crate::ast::CapabilityDef {
                    name: "builtins".to_owned(),
                    kind: crate::ast::CapabilityKind::Import { path: None },
                }],
                environment: vec![],
                intent: crate::ast::StructuredIntent {
                    description: String::new(),
                    properties: vec![crate::ast::Property {
                        capability: "builtins".to_owned(),
                        action: "print".to_owned(),
                    }],
                },
            }],
            types: vec![],
            modules: vec![],
        };
        let errors = validate(&prog);
        assert!(
            errors.iter().any(|e| matches!(e, IntentError::EmptyDescription)),
            "expected EmptyDescription, got {errors:?}"
        );
    }

    #[test]
    fn no_properties_error() {
        let prog = IntentProgram {
            applications: vec![crate::ast::Application {
                name: "app".to_owned(),
                args: crate::ast::ArgsDef::default(),
                capabilities: vec![crate::ast::CapabilityDef {
                    name: "builtins".to_owned(),
                    kind: crate::ast::CapabilityKind::Import { path: None },
                }],
                environment: vec![],
                intent: crate::ast::StructuredIntent {
                    description: "test".to_owned(),
                    properties: vec![],
                },
            }],
            types: vec![],
            modules: vec![],
        };
        let errors = validate(&prog);
        assert!(
            errors.iter().any(|e| matches!(e, IntentError::NoProperties)),
            "expected NoProperties, got {errors:?}"
        );
    }

    #[test]
    fn unused_capability_warning() {
        let prog = IntentProgram {
            applications: vec![crate::ast::Application {
                name: "app".to_owned(),
                args: crate::ast::ArgsDef::default(),
                capabilities: vec![
                    crate::ast::CapabilityDef {
                        name: "builtins".to_owned(),
                        kind: crate::ast::CapabilityKind::Import { path: None },
                    },
                    crate::ast::CapabilityDef {
                        name: "unused".to_owned(),
                        kind: crate::ast::CapabilityKind::NewModule,
                    },
                ],
                environment: vec![],
                intent: crate::ast::StructuredIntent {
                    description: "test".to_owned(),
                    properties: vec![crate::ast::Property {
                        capability: "builtins".to_owned(),
                        action: "print".to_owned(),
                    }],
                },
            }],
            types: vec![],
            modules: vec![],
        };
        let errors = validate(&prog);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                IntentError::UnusedCapability { name }
                if name == "unused"
            )),
            "expected UnusedCapability for 'unused', got {errors:?}"
        );
    }

    #[test]
    fn valid_application_with_capabilities() {
        let prog = parse_intent(
            "\
application weather:
  capabilities:
    builtins: import
  intent:
    description: fetch weather
    properties:
      - uses builtins to print output
",
        );
        let errors = validate(&prog);
        assert!(errors.is_empty(), "expected no errors, got {errors:?}");
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Lexes, parses an intent source into an IntentProgram.
    fn parse_intent(source: &str) -> IntentProgram {
        let tokens = lex(source).unwrap();
        parse(&tokens).unwrap()
    }
}
