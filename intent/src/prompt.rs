use crate::ast::{Application, Capability};

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
/// the application's args/env/intent, and complete `.synapse`
/// examples so the LLM produces a ready-to-compile program.
///
/// [`Application`]: crate::ast::Application
pub fn build_application_prompt(app: &Application) -> String {
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

    format!(
        r#"You are generating a complete program in Synapse, a purely functional language.

{BUILTIN_SIGNATURES}

SYNAPSE LANGUAGE REFERENCE:
{LANGUAGE_SPEC}

APPLICATION TO IMPLEMENT:
  Name: {name}
  Main signature: {main_sig}
  Intent: {intent}

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

EXAMPLE 1 (pure computation):

function factorial(Int n) -> Int
  returns match n
    when 0 -> 1
    otherwise -> n * factorial(n - 1)

function main() -> Int
  returns factorial(10)

EXAMPLE 2 (IO with built-ins, taking a CLI arg):

function fetch_weather(String city) -> String
  value url = concat("https://wttr.in/", concat(city, "?format=3"))
  returns http_get(url)

function main(String city) -> Int
  value report = fetch_weather(city)
  value _ = print(report)
  returns 0

Output ONLY valid Synapse code. No markdown, no backticks, no explanation, no comments.
Start directly with "function"."#,
        name = app.name,
        intent = app.intent,
    )
}
