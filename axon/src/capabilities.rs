use std::{collections::HashMap, error::Error, path::Path};

use cortex::{
    ast::Type,
    module::{FunctionSig, ModuleApi},
};
use intent::ast::{CapabilityDef, CapabilityKind};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Resolves public APIs for each declared capability.
///
/// Returns a map from capability name to its [`ModuleApi`]. The API
/// is used in the LLM prompt so the model knows exactly what
/// functions are available from each capability.
///
/// # Errors
///
/// Returns an error if an existing synapse module cannot be compiled
/// or a rust module file cannot be read.
///
/// ```
/// # use std::collections::HashMap;
/// # use intent::ast::{CapabilityDef, CapabilityKind};
/// # use axon::capabilities::resolve_capability_apis;
/// let caps = vec![CapabilityDef {
///     name: "builtins".to_owned(),
///     kind: CapabilityKind::Import { path: None },
/// }];
/// let apis = resolve_capability_apis(std::path::Path::new("."), &caps).unwrap();
/// assert_eq!(apis["builtins"].functions.len(), 3);
/// ```
///
/// [`ModuleApi`]: cortex::module::ModuleApi
pub fn resolve_capability_apis(
    dir: &Path,
    capabilities: &[CapabilityDef],
) -> Result<HashMap<String, ModuleApi>, Box<dyn Error>> {
    tracing::debug!(count = capabilities.len(), "resolving capability APIs");

    let mut apis = HashMap::new();

    for cap in capabilities {
        let api = resolve_single(dir, cap)?;
        tracing::info!(
            name = %cap.name,
            functions = api.functions.len(),
            "resolved capability API"
        );
        apis.insert(cap.name.clone(), api);
    }

    Ok(apis)
}

// ---------------------------------------------------------------------------
// Per-Kind Resolution
// ---------------------------------------------------------------------------

/// Resolves the API for a single capability based on its kind.
fn resolve_single(dir: &Path, cap: &CapabilityDef) -> Result<ModuleApi, Box<dyn Error>> {
    match &cap.kind {
        CapabilityKind::Import { path } => resolve_import(dir, &cap.name, path.as_deref()),
        CapabilityKind::ImportRustCrate { .. } | CapabilityKind::NewModule | CapabilityKind::NewCrate => {
            Ok(empty_api(&cap.name))
        },
    }
}

/// Resolves a bare or path-qualified import by name.
///
/// Resolution order for bare imports (no explicit path):
/// 1. If name is `builtins` -> built-in functions
/// 2. If `src/<name>.synapse` exists -> synapse module
/// 3. If `src/<name>.rs` exists -> rust module
/// 4. Error: cannot resolve
///
/// When an explicit path is provided, the extension determines
/// the type: `.synapse` -> synapse module, `.rs` -> rust module.
fn resolve_import(dir: &Path, name: &str, path: Option<&str>) -> Result<ModuleApi, Box<dyn Error>> {
    if let Some(explicit_path) = path {
        tracing::info!(name, path = explicit_path, "resolving import with explicit path");
        if explicit_path.ends_with(".synapse") {
            return resolve_synapse_module(dir, name, explicit_path);
        } else if explicit_path.ends_with(".rs") {
            return resolve_rust_module(dir, name, explicit_path);
        }
        return Err(format!("cannot determine type for import path '{explicit_path}'").into());
    }

    if name == "builtins" {
        tracing::info!("resolving builtins import");
        return Ok(builtin_api(name));
    }

    let synapse_path = format!("src/{name}.synapse");
    if dir.join(&synapse_path).exists() {
        tracing::info!(name, path = %synapse_path, "resolved import as synapse module");
        return resolve_synapse_module(dir, name, &synapse_path);
    }

    let rs_path = format!("src/{name}.rs");
    if dir.join(&rs_path).exists() {
        tracing::info!(name, path = %rs_path, "resolved import as rust module");
        return resolve_rust_module(dir, name, &rs_path);
    }

    Err(format!("cannot resolve import '{name}': no src/{name}.synapse or src/{name}.rs found").into())
}

/// Returns the built-in synapse API (print, `http_get`, concat).
fn builtin_api(name: &str) -> ModuleApi {
    ModuleApi {
        name: name.to_owned(),
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
    }
}

/// Returns an empty API for capabilities whose API is not yet available.
fn empty_api(name: &str) -> ModuleApi {
    ModuleApi {
        name: name.to_owned(),
        functions: vec![],
    }
}

/// Compiles a `.synapse` file and extracts its public API.
fn resolve_synapse_module(dir: &Path, name: &str, path: &str) -> Result<ModuleApi, Box<dyn Error>> {
    let full_path = dir.join(path);
    tracing::info!(
        module = name,
        path = %full_path.display(),
        "compiling synapse module for API extraction"
    );

    let source = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("cannot read synapse module '{}': {e}", full_path.display()))?;

    let tokens = cortex::lexer::lex(&source)?;
    let ast = cortex::parser::parse(&tokens)?;
    let typed = cortex::checker::check(&ast)?;

    Ok(cortex::module::extract_api(name, &typed))
}

/// Extracts `pub fn` signatures from a `.rs` file using simple
/// line-by-line pattern matching.
fn resolve_rust_module(dir: &Path, name: &str, path: &str) -> Result<ModuleApi, Box<dyn Error>> {
    let full_path = dir.join(path);
    tracing::info!(
        module = name,
        path = %full_path.display(),
        "extracting pub fn signatures from Rust module"
    );

    let source = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("cannot read rust module '{}': {e}", full_path.display()))?;

    let functions = extract_rust_pub_fns(&source);
    Ok(ModuleApi {
        name: name.to_owned(),
        functions,
    })
}

/// Parses `pub fn` signatures from Rust source lines.
///
/// Matches lines of the form:
/// `pub fn name(params) -> ReturnType {`
///
/// Parameter types are kept as opaque strings mapped to
/// [`Type::Str`] since Synapse cannot represent arbitrary
/// Rust types.
///
/// [`Type::Str`]: cortex::ast::Type::Str
fn extract_rust_pub_fns(source: &str) -> Vec<FunctionSig> {
    let mut sigs = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("pub fn ") {
            if let Some(sig) = parse_rust_fn_sig(rest) {
                sigs.push(sig);
            }
        }
    }

    sigs
}

/// Attempts to parse a single Rust function signature after `pub fn `.
///
/// Expects the form `name(params) -> Type` or `name(params)`.
fn parse_rust_fn_sig(s: &str) -> Option<FunctionSig> {
    let paren_open = s.find('(')?;
    let name = s[..paren_open].trim().to_owned();

    let paren_close = s.find(')')?;
    let params_str = &s[paren_open + 1..paren_close];

    let params = parse_rust_params(params_str);

    let after_parens = s[paren_close + 1..].trim();
    let return_type = if let Some(ret) = after_parens.strip_prefix("->") {
        rust_type_to_synapse(ret.trim().trim_end_matches(['{', ' ']))
    } else {
        Type::Int
    };

    Some(FunctionSig {
        name,
        params,
        return_type,
    })
}

/// Parses comma-separated Rust function parameters.
///
/// Each parameter is expected as `name: Type`. Self parameters
/// are skipped.
fn parse_rust_params(s: &str) -> Vec<(String, Type)> {
    if s.trim().is_empty() {
        return Vec::new();
    }

    s.split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.starts_with('&') && part.contains("self") || part == "self" || part == "mut self" {
                return None;
            }

            let colon = part.find(':')?;
            let name = part[..colon].trim().to_owned();
            let ty_str = part[colon + 1..].trim();
            Some((name, rust_type_to_synapse(ty_str)))
        })
        .collect()
}

/// Maps a Rust type string to the closest Synapse [`Type`].
///
/// [`Type`]: cortex::ast::Type
fn rust_type_to_synapse(ty: &str) -> Type {
    let ty = ty.trim().trim_start_matches('&').trim_start_matches("mut ").trim();
    match ty {
        "i64" | "i32" | "usize" | "isize" | "u64" | "u32" => Type::Int,
        "bool" => Type::Bool,
        _ => Type::Str,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_synapse_returns_three_functions() {
        let caps = vec![CapabilityDef {
            name: "builtins".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }];
        let apis = resolve_capability_apis(Path::new("."), &caps).unwrap();
        let api = &apis["builtins"];
        assert_eq!(api.functions.len(), 3, "builtins has 3 functions");

        let names: Vec<&str> = api.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"print"), "has print");
        assert!(names.contains(&"http_get"), "has http_get");
        assert!(names.contains(&"concat"), "has concat");
    }

    #[test]
    fn builtin_print_signature() {
        let api = builtin_api("builtins");
        let print = api.functions.iter().find(|f| f.name == "print").unwrap();
        assert_eq!(print.params.len(), 1, "print takes one param");
        assert_eq!(print.params[0].1, Type::Str, "print param is Str");
        assert_eq!(print.return_type, Type::Int, "print returns Int");
    }

    #[test]
    fn builtin_http_get_signature() {
        let api = builtin_api("builtins");
        let hg = api.functions.iter().find(|f| f.name == "http_get").unwrap();
        assert_eq!(hg.params.len(), 1, "http_get takes one param");
        assert_eq!(hg.params[0].1, Type::Str, "http_get param is Str");
        assert_eq!(hg.return_type, Type::Str, "http_get returns Str");
    }

    #[test]
    fn builtin_concat_signature() {
        let api = builtin_api("builtins");
        let c = api.functions.iter().find(|f| f.name == "concat").unwrap();
        assert_eq!(c.params.len(), 2, "concat takes two params");
        assert_eq!(c.params[0].1, Type::Str, "concat param 0 is Str");
        assert_eq!(c.params[1].1, Type::Str, "concat param 1 is Str");
        assert_eq!(c.return_type, Type::Str, "concat returns Str");
    }

    #[test]
    fn new_module_returns_empty_api() {
        let caps = vec![CapabilityDef {
            name: "weather".to_owned(),
            kind: CapabilityKind::NewModule,
        }];
        let apis = resolve_capability_apis(Path::new("."), &caps).unwrap();
        assert!(apis["weather"].functions.is_empty(), "new module has empty API");
    }

    #[test]
    fn import_rust_crate_returns_empty_api() {
        let caps = vec![CapabilityDef {
            name: "serde_json".to_owned(),
            kind: CapabilityKind::ImportRustCrate {
                spec: intent::ast::RustCrateSpec {
                    name: "serde_json".to_owned(),
                    version: Some("1.0.140".to_owned()),
                    path: None,
                    git: None,
                },
            },
        }];
        let apis = resolve_capability_apis(Path::new("."), &caps).unwrap();
        assert!(
            apis["serde_json"].functions.is_empty(),
            "rust crate has empty API for now"
        );
    }

    #[test]
    fn import_with_path_extracts_synapse_api() {
        let dir = tempfile::tempdir().unwrap();
        let synapse_path = dir.path().join("math.synapse");
        std::fs::write(
            &synapse_path,
            "pub function add(Int a, Int b) -> Int\n  returns a + b\n",
        )
        .unwrap();

        let caps = vec![CapabilityDef {
            name: "math".to_owned(),
            kind: CapabilityKind::Import {
                path: Some("math.synapse".to_owned()),
            },
        }];
        let apis = resolve_capability_apis(dir.path(), &caps).unwrap();
        let api = &apis["math"];
        assert_eq!(api.functions.len(), 1, "one pub function extracted");
        assert_eq!(api.functions[0].name, "add");
        assert_eq!(api.functions[0].params.len(), 2);
        assert_eq!(api.functions[0].return_type, Type::Int);
    }

    #[test]
    fn import_with_path_extracts_rust_pub_fns() {
        let dir = tempfile::tempdir().unwrap();
        let rs_path = dir.path().join("helper.rs");
        std::fs::write(
            &rs_path,
            "pub fn greet(name: &str) -> String {\n    format!(\"hello {name}\")\n}\n\nfn private() -> i32 {\n    42\n}\n\npub fn add(a: i64, b: i64) -> i64 {\n    a + b\n}\n",
        )
        .unwrap();

        let caps = vec![CapabilityDef {
            name: "helper".to_owned(),
            kind: CapabilityKind::Import {
                path: Some("helper.rs".to_owned()),
            },
        }];
        let apis = resolve_capability_apis(dir.path(), &caps).unwrap();
        let api = &apis["helper"];
        assert_eq!(api.functions.len(), 2, "two pub functions extracted");
        assert_eq!(api.functions[0].name, "greet");
        assert_eq!(api.functions[1].name, "add");
    }

    #[test]
    fn extract_rust_pub_fns_handles_no_return() {
        let fns = extract_rust_pub_fns("pub fn init() {\n}\n");
        assert_eq!(fns.len(), 1, "one function found");
        assert_eq!(fns[0].name, "init");
        assert_eq!(fns[0].return_type, Type::Int, "no return type defaults to Int");
    }

    #[test]
    fn extract_rust_pub_fns_skips_self_params() {
        let fns = extract_rust_pub_fns("pub fn method(&self, x: i64) -> bool {\n}\n");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].params.len(), 1, "self param is skipped");
        assert_eq!(fns[0].params[0].0, "x");
        assert_eq!(fns[0].params[0].1, Type::Int);
        assert_eq!(fns[0].return_type, Type::Bool);
    }

    #[test]
    fn rust_type_mapping() {
        assert_eq!(rust_type_to_synapse("i64"), Type::Int);
        assert_eq!(rust_type_to_synapse("i32"), Type::Int);
        assert_eq!(rust_type_to_synapse("bool"), Type::Bool);
        assert_eq!(rust_type_to_synapse("String"), Type::Str);
        assert_eq!(rust_type_to_synapse("&str"), Type::Str);
        assert_eq!(rust_type_to_synapse("Vec<u8>"), Type::Str, "unknown types map to Str");
    }

    #[test]
    fn multiple_capabilities_resolved() {
        let caps = vec![
            CapabilityDef {
                name: "builtins".to_owned(),
                kind: CapabilityKind::Import { path: None },
            },
            CapabilityDef {
                name: "weather".to_owned(),
                kind: CapabilityKind::NewModule,
            },
        ];
        let apis = resolve_capability_apis(Path::new("."), &caps).unwrap();
        assert_eq!(apis.len(), 2, "two capabilities resolved");
        assert_eq!(apis["builtins"].functions.len(), 3);
        assert!(apis["weather"].functions.is_empty());
    }

    #[test]
    fn import_with_path_extracts_multiple_pub_fns() {
        let dir = tempfile::tempdir().unwrap();
        let synapse_path = dir.path().join("math.synapse");
        std::fs::write(
            &synapse_path,
            "pub function factorial(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * factorial(n - 1)\n\npub function double(Int x) -> Int\n  returns x * 2\n\nfunction helper(Int x) -> Int\n  returns x + 1\n",
        )
        .unwrap();

        let caps = vec![CapabilityDef {
            name: "math".to_owned(),
            kind: CapabilityKind::Import {
                path: Some("math.synapse".to_owned()),
            },
        }];
        let apis = resolve_capability_apis(dir.path(), &caps).unwrap();
        let api = &apis["math"];
        assert_eq!(api.functions.len(), 2, "only pub functions extracted, not helper");

        let names: Vec<&str> = api.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"factorial"), "has factorial");
        assert!(names.contains(&"double"), "has double");
    }

    #[test]
    fn bare_import_resolves_builtins() {
        let caps = vec![CapabilityDef {
            name: "builtins".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }];
        let apis = resolve_capability_apis(Path::new("."), &caps).unwrap();
        assert_eq!(apis["builtins"].functions.len(), 3, "builtins resolved by name");
    }

    #[test]
    fn bare_import_resolves_synapse_in_src() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("math.synapse"),
            "pub function add(Int a, Int b) -> Int\n  returns a + b\n",
        )
        .unwrap();

        let caps = vec![CapabilityDef {
            name: "math".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }];
        let apis = resolve_capability_apis(dir.path(), &caps).unwrap();
        let api = &apis["math"];
        assert_eq!(api.functions.len(), 1, "resolved synapse module from src/");
        assert_eq!(api.functions[0].name, "add");
    }

    #[test]
    fn bare_import_resolves_rust_in_src() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(
            src_dir.join("helper.rs"),
            "pub fn greet(name: &str) -> String {\n    format!(\"hello {name}\")\n}\n",
        )
        .unwrap();

        let caps = vec![CapabilityDef {
            name: "helper".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }];
        let apis = resolve_capability_apis(dir.path(), &caps).unwrap();
        let api = &apis["helper"];
        assert_eq!(api.functions.len(), 1, "resolved rust module from src/");
        assert_eq!(api.functions[0].name, "greet");
    }

    #[test]
    fn bare_import_unresolvable_errors() {
        let dir = tempfile::tempdir().unwrap();
        let caps = vec![CapabilityDef {
            name: "nonexistent".to_owned(),
            kind: CapabilityKind::Import { path: None },
        }];
        let err = resolve_capability_apis(dir.path(), &caps).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("cannot resolve import"),
            "should indicate resolution failure: {msg}"
        );
    }
}
