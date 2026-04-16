use std::collections::HashMap;

use cortex::module::ModuleApi;

use crate::ast::{Application, Capability, CapabilityKind, Property};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The Synapse language reference, compiled in from `docs/language.md`.
const LANGUAGE_SPEC: &str = include_str!("../../docs/language.md");

/// Built-in function signatures, repeated prominently in every prompt.
const BUILTIN_SIGNATURES: &str = "\
BUILT-IN FUNCTIONS (these exist and MUST be used correctly):
  print(String) -> Int       : prints a string to stdout, returns 0
  http_get(String) -> String : fetches a URL, returns the response body as String
  concat(String, String) -> String : concatenates two strings, returns String

CRITICAL TYPE RULES:
  - http_get ALWAYS returns String, never Int
  - print ALWAYS returns Int, never String
  - concat ALWAYS returns String
  - A function that calls http_get and returns its result MUST have return type String
  - A function that calls print and returns its result MUST have return type Int
  - main should return Int (0 for success)";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Builds a prompt for the `claude` CLI to expand a single [`Capability`]
/// into valid Synapse code. Includes the full language specification.
///
/// [`Capability`]: crate::ast::Capability
pub fn build_prompt(cap: &Capability) -> String {
    let params = cap
        .inputs
        .iter()
        .map(|p| format!("{} {}", p.ty, p.name))
        .collect::<Vec<_>>()
        .join(", ");
    let output = cap.output.as_deref().unwrap_or("Int");

    format!(
        r#"You are generating code in Synapse, a purely functional language.

{BUILTIN_SIGNATURES}

SYNAPSE LANGUAGE REFERENCE:
{LANGUAGE_SPEC}

RULES:
1. ONLY use Cons/Nil patterns when the scrutinee is a List type
2. ONLY use true/false patterns when the scrutinee is a Bool expression
3. ONLY use integer literal patterns when the scrutinee is an Int
4. All variables must be declared as function parameters or value bindings
5. Every variable used must be defined. Do not reference undefined names.
6. If you need helper functions, define them BEFORE the main function.
7. The function MUST return type {output}. Verify your return expression matches.

GENERATE THIS FUNCTION:
  Name: {name}
  Parameters: {params}
  Return type: {output}
  Behavior: {intent}

Output ONLY valid Synapse code. No markdown, no backticks, no explanation, no comments.
Start directly with "function"."#,
        name = cap.name,
        intent = cap.intent,
    )
}

/// Builds a prompt for the `claude` CLI to expand an [`Application`]
/// directly into valid `.synapse` code in a single LLM call.
///
/// The prompt includes the full language spec, built-in signatures,
/// declared capabilities with their resolved APIs, the structured intent
/// (description + properties), and complete `.synapse` examples.
///
/// When `apis` is non-empty, each capability's known function
/// signatures are listed so the LLM sees exactly what is available.
///
/// [`Application`]: crate::ast::Application
pub fn build_application_prompt(app: &Application, apis: &HashMap<String, ModuleApi>) -> String {
    let mut main_params = Vec::new();

    if let Some(ref verb) = app.args.verb {
        main_params.push(format!("String {verb}"));
    }
    for flag in &app.args.flags {
        let ty = flag.ty.as_deref().unwrap_or("Bool");
        main_params.push(format!("{ty} {}", flag.long_name));
    }
    for pos in &app.args.positionals {
        main_params.push(format!("{} {}", pos.ty, pos.binding));
    }
    for env in &app.environment {
        main_params.push(format!("{} {}", env.ty, env.binding));
    }

    let main_sig = if main_params.is_empty() {
        "function main() -> Int".to_owned()
    } else {
        format!("function main({}) -> Int", main_params.join(", "))
    };

    let capabilities_section = format_capabilities(app, apis);
    let intent_section = format_intent(app);

    format!(
        r#"You are generating a complete program in Synapse, a purely functional language.

{BUILTIN_SIGNATURES}

SYNAPSE LANGUAGE REFERENCE:
{LANGUAGE_SPEC}
{capabilities_section}
APPLICATION TO IMPLEMENT:
  Name: {name}
  Main signature: {main_sig}
{intent_section}
RULES:
1. Output a complete, valid .synapse program (one or more function definitions)
2. The main function MUST have this exact signature: {main_sig}
3. main MUST return Int (0 for success)
4. If you need helper functions, define them BEFORE main
5. ONLY use Cons/Nil patterns when the scrutinee is a List type
6. ONLY use true/false patterns when the scrutinee is a Bool expression
7. ONLY use integer literal patterns when the scrutinee is an Int
8. All variables must be declared as function parameters or value bindings
9. Every variable used must be defined. Do not reference undefined names.
10. Verify return types: http_get returns String, print returns Int, concat returns String
11. You may ONLY use capabilities declared in AVAILABLE CAPABILITIES above
12. When builtins capability is declared, use `import builtins` and call functions as builtins.print(), builtins.http_get(), builtins.concat()

EXAMPLE 1 (pure computation):

function factorial(Int n) -> Int
  returns match n
    when 0 -> 1
    otherwise -> n * factorial(n - 1)

function main() -> Int
  returns factorial(10)

EXAMPLE 2 (IO with built-ins, taking a CLI arg):

import builtins

function fetch_weather(String city) -> String
  value url = builtins.concat("https://wttr.in/", builtins.concat(city, "?format=3"))
  returns builtins.http_get(url)

function main(String city) -> Int
  value report = fetch_weather(city)
  value _ = builtins.print(report)
  returns 0

Output ONLY valid Synapse code. No markdown, no backticks, no explanation, no comments.
Start directly with "import" or "function"."#,
        name = app.name,
    )
}

/// Builds a prompt for the `claude` CLI to generate a standalone
/// `.synapse` module with `pub` functions.
///
/// The prompt includes:
/// - The module name
/// - Properties that reference this module (filtered from `properties`)
/// - APIs of other capabilities the module may reference
/// - The Synapse language spec and built-in signatures
/// - Instructions requiring all functions to be `pub`
///
/// ```
/// # use std::collections::HashMap;
/// # use intent::prompt::build_new_module_prompt;
/// # use intent::ast::Property;
/// let props = vec![Property {
///     capability: "calculator".to_owned(),
///     action: "compute the factorial".to_owned(),
/// }];
/// let prompt = build_new_module_prompt("calculator", &props, &HashMap::new());
/// assert!(prompt.contains("calculator"));
/// assert!(prompt.contains("compute the factorial"));
/// assert!(prompt.contains("pub"));
/// ```
pub fn build_new_module_prompt(
    module_name: &str,
    properties: &[Property],
    other_apis: &HashMap<String, ModuleApi>,
) -> String {
    let relevant: Vec<&Property> = properties.iter().filter(|p| p.capability == module_name).collect();

    let mut purpose = String::new();
    for prop in &relevant {
        purpose.push_str(&format!("  - {}\n", prop.action));
    }
    if purpose.is_empty() {
        purpose.push_str("  - (no specific properties provided)\n");
    }

    let mut apis_section = String::new();
    if !other_apis.is_empty() {
        apis_section.push_str("\nOTHER AVAILABLE MODULE APIs:\n");
        for (name, api) in other_apis {
            if name == module_name {
                continue;
            }
            apis_section.push_str(&format!("  {name}:\n"));
            for f in &api.functions {
                let params = f
                    .params
                    .iter()
                    .map(|(_, ty)| format_synapse_type(ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = format_synapse_type(&f.return_type);
                apis_section.push_str(&format!("    {}({params}) -> {ret}\n", f.name));
            }
        }
        apis_section.push('\n');
    }

    format!(
        r#"You are generating a Synapse MODULE named "{module_name}".

{BUILTIN_SIGNATURES}

SYNAPSE LANGUAGE REFERENCE:
{LANGUAGE_SPEC}
{apis_section}MODULE TO GENERATE:
  Name: {module_name}
  Purpose:
{purpose}
RULES:
1. ALL functions MUST be declared with `pub`
2. Output valid .synapse code only
3. No markdown, no backticks, no explanation, no comments
4. Do NOT define a main function
5. Start directly with "pub function"
6. ONLY use Cons/Nil patterns when the scrutinee is a List type
7. ONLY use true/false patterns when the scrutinee is a Bool expression
8. ONLY use integer literal patterns when the scrutinee is an Int
9. All variables must be declared as function parameters or value bindings
10. Every variable used must be defined. Do not reference undefined names.
11. If you need helper functions, define them with `pub` BEFORE the functions that use them
12. Verify return types: http_get returns String, print returns Int, concat returns String

EXAMPLE MODULE:

pub function factorial(Int n) -> Int
  returns match n
    when 0 -> 1
    otherwise -> n * factorial(n - 1)

pub function double(Int x) -> Int
  returns x * 2

Output ONLY valid Synapse code. Start directly with "pub function"."#,
    )
}

// ---------------------------------------------------------------------------
// Prompt Formatting Utilities
// ---------------------------------------------------------------------------

/// Formats the capabilities section for the application prompt.
///
/// When resolved APIs are available, function signatures are listed
/// under each capability so the LLM knows exactly what to call.
fn format_capabilities(app: &Application, apis: &HashMap<String, ModuleApi>) -> String {
    if app.capabilities.is_empty() {
        return String::new();
    }

    let mut out = String::from("\nAVAILABLE CAPABILITIES:\n");

    for cap in &app.capabilities {
        out.push_str(&format!("\n  {} ({})", cap.name, describe_kind(&cap.kind)));

        match &cap.kind {
            CapabilityKind::Import { .. } if cap.name == "builtins" => {
                out.push_str(":\n");
                format_api_functions(&mut out, apis.get(&cap.name));
                out.push_str(
                    "    Use `import builtins` and call as \
                     builtins.print(), etc.\n",
                );
            },
            CapabilityKind::Import { .. } => {
                out.push_str(":\n");
                format_api_functions(&mut out, apis.get(&cap.name));
                if api_is_empty(apis.get(&cap.name)) {
                    out.push_str(&format!("    Import resolved by name '{name}'\n", name = cap.name,));
                } else {
                    out.push_str(&format!(
                        "    Use `import {name}` and call with \
                         {name}.<function>().\n",
                        name = cap.name,
                    ));
                }
            },
            CapabilityKind::ImportRustCrate { spec } => {
                let mut desc = format!(":\n    Cargo crate {}", spec.name);
                if let Some(ref v) = spec.version {
                    desc.push_str(&format!(" {v}"));
                }
                desc.push('\n');
                out.push_str(&desc);
                if api_is_empty(apis.get(&cap.name)) {
                    out.push_str(
                        "    (API not yet available \
                         - use standard patterns)\n",
                    );
                } else {
                    format_api_functions(&mut out, apis.get(&cap.name));
                }
            },
            CapabilityKind::NewModule => {
                out.push_str(":\n");
                if api_is_empty(apis.get(&cap.name)) {
                    out.push_str(
                        "    YOU MUST GENERATE THIS MODULE.\n\
                         \x20   Define it with `pub` functions \
                         before `main`.\n",
                    );
                } else {
                    format_api_functions(&mut out, apis.get(&cap.name));
                    out.push_str(&format!(
                        "    This module has been generated. Use \
                         `import {name}` and call with \
                         {name}.<function>().\n\
                         \x20   Do NOT redefine these functions. \
                         Only call them.\n",
                        name = cap.name,
                    ));
                }
            },
            CapabilityKind::NewCrate => {
                out.push_str(":\n");
                out.push_str("    YOU MUST GENERATE THIS CRATE.\n");
            },
        }
    }

    out.push('\n');
    out
}

/// Checks whether an optional API has no functions.
fn api_is_empty(api: Option<&ModuleApi>) -> bool {
    api.is_none_or(|a| a.functions.is_empty())
}

/// Appends function signatures from a resolved API to the output.
fn format_api_functions(out: &mut String, api: Option<&ModuleApi>) {
    let Some(api) = api else { return };
    for f in &api.functions {
        let params = f
            .params
            .iter()
            .map(|(_, ty)| format_synapse_type(ty))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = format_synapse_type(&f.return_type);
        out.push_str(&format!("    {}({params}) -> {ret}\n", f.name));
    }
}

/// Formats a cortex [`Type`] as a Synapse type name.
///
/// [`Type`]: cortex::ast::Type
fn format_synapse_type(ty: &cortex::ast::Type) -> String {
    match ty {
        cortex::ast::Type::Int => "Int".to_owned(),
        cortex::ast::Type::Bool => "Bool".to_owned(),
        cortex::ast::Type::Str => "String".to_owned(),
        cortex::ast::Type::List(inner) => {
            format!("List<{}>", format_synapse_type(inner))
        },
    }
}

/// Returns a human-readable description of a [`CapabilityKind`].
///
/// [`CapabilityKind`]: crate::ast::CapabilityKind
fn describe_kind(kind: &CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::Import { .. } => "import",
        CapabilityKind::ImportRustCrate { .. } => "rust crate",
        CapabilityKind::NewModule => "new module",
        CapabilityKind::NewCrate => "new crate",
    }
}

/// Formats the intent section for the application prompt.
fn format_intent(app: &Application) -> String {
    if app.capabilities.is_empty() && app.intent.properties.is_empty() {
        return format!("  Intent: {}\n", app.intent.description);
    }

    let mut out = String::new();
    out.push_str(&format!("  Description: {}\n", app.intent.description));

    if !app.intent.properties.is_empty() {
        out.push_str("  Properties:\n");
        for prop in &app.intent.properties {
            out.push_str(&format!("    - uses {} to {}\n", prop.capability, prop.action));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use cortex::{
        ast::Type,
        module::{FunctionSig, ModuleApi},
    };

    use super::*;
    use crate::ast::{
        ArgsDef, CapabilityDef, CapabilityKind, PositionalDef, Property, RustCrateSpec, StructuredIntent,
    };

    #[test]
    fn prompt_includes_builtin_api_from_resolved_apis() {
        let app = make_app(vec![CapabilityDef {
            name: "builtins".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }]);

        let mut apis = HashMap::new();
        apis.insert(
            "builtins".to_owned(),
            ModuleApi {
                name: "builtins".to_owned(),
                functions: vec![
                    FunctionSig {
                        name: "print".to_owned(),
                        params: vec![("s".to_owned(), Type::Str)],
                        return_type: Type::Int,
                    },
                    FunctionSig {
                        name: "http_get".to_owned(),
                        params: vec![("url".to_owned(), Type::Str)],
                        return_type: Type::Str,
                    },
                    FunctionSig {
                        name: "concat".to_owned(),
                        params: vec![("a".to_owned(), Type::Str), ("b".to_owned(), Type::Str)],
                        return_type: Type::Str,
                    },
                ],
            },
        );

        let prompt = build_application_prompt(&app, &apis);
        assert!(
            prompt.contains("print(String) -> Int"),
            "prompt should contain print signature"
        );
        assert!(
            prompt.contains("http_get(String) -> String"),
            "prompt should contain http_get signature"
        );
        assert!(
            prompt.contains("concat(String, String) -> String"),
            "prompt should contain concat signature"
        );
        assert!(
            prompt.contains("import builtins"),
            "prompt should mention import builtins"
        );
    }

    #[test]
    fn prompt_includes_new_module_generation_directive() {
        let app = make_app(vec![CapabilityDef {
            name: "weather_api".to_owned(),
            kind: CapabilityKind::NewModule,
        }]);

        let prompt = build_application_prompt(&app, &HashMap::new());
        assert!(
            prompt.contains("YOU MUST GENERATE THIS MODULE"),
            "prompt should tell LLM to generate: {prompt}"
        );
        assert!(
            prompt.contains("weather_api (new module)"),
            "prompt should label the capability"
        );
    }

    #[test]
    fn prompt_includes_imported_module_api() {
        let app = make_app(vec![CapabilityDef {
            name: "math".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }]);

        let mut apis = HashMap::new();
        apis.insert(
            "math".to_owned(),
            ModuleApi {
                name: "math".to_owned(),
                functions: vec![FunctionSig {
                    name: "factorial".to_owned(),
                    params: vec![("n".to_owned(), Type::Int)],
                    return_type: Type::Int,
                }],
            },
        );

        let prompt = build_application_prompt(&app, &apis);
        assert!(
            prompt.contains("factorial(Int) -> Int"),
            "prompt should list factorial: {prompt}"
        );
        assert!(prompt.contains("import math"), "prompt should mention import math");
    }

    #[test]
    fn prompt_rust_crate_shows_not_yet_available() {
        let app = make_app(vec![CapabilityDef {
            name: "serde_json".to_owned(),
            kind: CapabilityKind::ImportRustCrate {
                spec: RustCrateSpec {
                    name: "serde_json".to_owned(),
                    version: Some("1.0.140".to_owned()),
                    path: None,
                    git: None,
                },
            },
        }]);

        let prompt = build_application_prompt(&app, &HashMap::new());
        assert!(
            prompt.contains("serde_json 1.0.140"),
            "prompt should show crate name and version"
        );
        assert!(
            prompt.contains("API not yet available"),
            "prompt should note API unavailable"
        );
    }

    #[test]
    fn prompt_with_empty_apis_still_works() {
        let app = make_app(vec![CapabilityDef {
            name: "builtins".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }]);

        let prompt = build_application_prompt(&app, &HashMap::new());
        assert!(
            prompt.contains("AVAILABLE CAPABILITIES"),
            "prompt should have capabilities section"
        );
        assert!(prompt.contains("builtins (import)"), "prompt should label import");
    }

    #[test]
    fn prompt_no_capabilities_omits_section() {
        let app = make_app(vec![]);
        let prompt = build_application_prompt(&app, &HashMap::new());
        assert!(
            !prompt.contains("\nAVAILABLE CAPABILITIES:\n"),
            "no capabilities section header when none declared"
        );
    }

    #[test]
    fn format_synapse_type_list() {
        assert_eq!(format_synapse_type(&Type::List(Box::new(Type::Int))), "List<Int>");
        assert_eq!(
            format_synapse_type(&Type::List(Box::new(Type::List(Box::new(Type::Str))))),
            "List<List<String>>"
        );
    }

    #[test]
    fn prompt_includes_imported_rust_module_functions() {
        let app = make_app(vec![CapabilityDef {
            name: "helper".to_owned(),
            kind: CapabilityKind::Import {
                path: Some("helper.rs".to_owned()),
            },
        }]);

        let mut apis = HashMap::new();
        apis.insert(
            "helper".to_owned(),
            ModuleApi {
                name: "helper".to_owned(),
                functions: vec![FunctionSig {
                    name: "greet".to_owned(),
                    params: vec![("name".to_owned(), Type::Str)],
                    return_type: Type::Str,
                }],
            },
        );

        let prompt = build_application_prompt(&app, &apis);
        assert!(
            prompt.contains("greet(String) -> String"),
            "prompt should list greet: {prompt}"
        );
        assert!(prompt.contains("import helper"), "prompt should mention import helper");
    }

    #[test]
    fn new_module_prompt_includes_module_name() {
        let props = vec![Property {
            capability: "calculator".to_owned(),
            action: "compute the factorial".to_owned(),
        }];
        let prompt = build_new_module_prompt("calculator", &props, &HashMap::new());
        assert!(
            prompt.contains("calculator"),
            "prompt must mention the module name: {prompt}"
        );
    }

    #[test]
    fn new_module_prompt_includes_relevant_properties() {
        let props = vec![
            Property {
                capability: "calculator".to_owned(),
                action: "compute the factorial".to_owned(),
            },
            Property {
                capability: "builtins".to_owned(),
                action: "print the result".to_owned(),
            },
        ];
        let prompt = build_new_module_prompt("calculator", &props, &HashMap::new());
        assert!(
            prompt.contains("compute the factorial"),
            "prompt must include calculator property: {prompt}"
        );
        assert!(
            !prompt.contains("print the result"),
            "prompt must not include unrelated property: {prompt}"
        );
    }

    #[test]
    fn new_module_prompt_includes_other_apis() {
        let props = vec![Property {
            capability: "parser".to_owned(),
            action: "parse input".to_owned(),
        }];
        let mut apis = HashMap::new();
        apis.insert(
            "builtins".to_owned(),
            ModuleApi {
                name: "builtins".to_owned(),
                functions: vec![FunctionSig {
                    name: "print".to_owned(),
                    params: vec![("s".to_owned(), Type::Str)],
                    return_type: Type::Int,
                }],
            },
        );
        let prompt = build_new_module_prompt("parser", &props, &apis);
        assert!(
            prompt.contains("print(String) -> Int"),
            "prompt must list other module's API: {prompt}"
        );
        assert!(
            prompt.contains("builtins:"),
            "prompt must label the other module: {prompt}"
        );
    }

    #[test]
    fn new_module_prompt_requires_pub() {
        let prompt = build_new_module_prompt("calc", &[], &HashMap::new());
        assert!(prompt.contains("pub"), "prompt must require pub functions: {prompt}");
    }

    #[test]
    fn new_module_prompt_forbids_main() {
        let prompt = build_new_module_prompt("calc", &[], &HashMap::new());
        assert!(
            prompt.contains("Do NOT define a main function"),
            "prompt must forbid main: {prompt}"
        );
    }

    #[test]
    fn new_module_prompt_includes_language_spec() {
        let prompt = build_new_module_prompt("calc", &[], &HashMap::new());
        assert!(
            prompt.contains("SYNAPSE LANGUAGE REFERENCE"),
            "prompt must include language spec: {prompt}"
        );
    }

    #[test]
    fn application_prompt_with_generated_module_api() {
        let app = make_app(vec![CapabilityDef {
            name: "calculator".to_owned(),
            kind: CapabilityKind::NewModule,
        }]);

        let mut apis = HashMap::new();
        apis.insert(
            "calculator".to_owned(),
            ModuleApi {
                name: "calculator".to_owned(),
                functions: vec![FunctionSig {
                    name: "factorial".to_owned(),
                    params: vec![("n".to_owned(), Type::Int)],
                    return_type: Type::Int,
                }],
            },
        );

        let prompt = build_application_prompt(&app, &apis);
        assert!(
            prompt.contains("factorial(Int) -> Int"),
            "prompt must show generated module's API: {prompt}"
        );
        assert!(
            prompt.contains("import calculator"),
            "prompt must tell LLM to import the module: {prompt}"
        );
        assert!(
            prompt.contains("Do NOT redefine"),
            "prompt must tell LLM not to redefine: {prompt}"
        );
        assert!(
            !prompt.contains("YOU MUST GENERATE THIS MODULE"),
            "prompt must not ask LLM to generate when API exists: {prompt}"
        );
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Creates a minimal application with given capabilities.
    fn make_app(capabilities: Vec<CapabilityDef>) -> Application {
        let props: Vec<Property> = capabilities
            .iter()
            .map(|c| Property {
                capability: c.name.clone(),
                action: "do something".to_owned(),
            })
            .collect();

        Application {
            name: "test".to_owned(),
            args: ArgsDef {
                verb: None,
                flags: vec![],
                positionals: vec![PositionalDef {
                    binding: "city".to_owned(),
                    ty: "String".to_owned(),
                }],
            },
            capabilities,
            environment: vec![],
            intent: StructuredIntent {
                description: "test application".to_owned(),
                properties: props,
            },
        }
    }
}
