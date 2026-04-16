use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use cortex::{
    emitter::{AppEnvVar, AppFlag, AppMeta, AppPositional, CrateSpec},
    module::ModuleMap,
};
use intent::ast::{Application, CapabilityKind};

use crate::{cache, capabilities, project::ProjectConfig};

// ---------------------------------------------------------------------------
// ResolvedEntry
// ---------------------------------------------------------------------------

/// The result of resolving an entry file: expanded `.synapse` source
/// plus optional application metadata.
///
/// ```
/// # use axon::build::ResolvedEntry;
/// let r = ResolvedEntry {
///     synapse_source: String::new(),
///     app_meta: None,
///     rust_crates: vec![],
///     capabilities: vec![],
///     generated_modules: std::collections::HashMap::new(),
/// };
/// assert!(r.app_meta.is_none());
/// ```
pub struct ResolvedEntry {
    /// The `.synapse` source code (possibly expanded from `.intent`).
    pub synapse_source: String,
    /// Application metadata when the entry is an `.intent` application.
    pub app_meta: Option<AppMeta>,
    /// Rust crate dependency specs extracted from capabilities.
    pub rust_crates: Vec<CrateSpec>,
    /// Declared capabilities from the application block.
    pub capabilities: Vec<intent::ast::CapabilityDef>,
    /// Generated `.synapse` modules from `new module` capabilities,
    /// keyed by module name.
    pub generated_modules: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Target Paths
// ---------------------------------------------------------------------------

/// Derives the stem from the entry filename (e.g. `src/main.intent` -> `main`).
fn entry_stem(config: &ProjectConfig) -> String {
    Path::new(&config.build.entry)
        .file_stem()
        .map_or_else(|| "main".to_owned(), |s| s.to_string_lossy().into_owned())
}

/// Returns the path to the compiled binary under `target/bin/`.
///
/// ```
/// # use axon::build::binary_path;
/// let p = binary_path(std::path::Path::new("/tmp/proj"), "demo");
/// assert_eq!(p, std::path::Path::new("/tmp/proj/target/bin/demo"));
/// ```
pub fn binary_path(dir: &Path, project_name: &str) -> PathBuf {
    dir.join("target/bin").join(project_name)
}

/// Creates the target subdirectories for build artifacts.
fn create_target_dirs(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(dir.join("target/synapse"))?;
    fs::create_dir_all(dir.join("target/rust/src"))?;
    fs::create_dir_all(dir.join("target/bin"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

/// Compiles a Synapse project to a binary.
///
/// All intermediate artifacts are written to `target/`:
/// - `target/synapse/<stem>.synapse`: expanded `.synapse` source
/// - `target/rust/src/<name>.rs`: emitted Rust for each module
/// - `target/rust/src/main.rs`: emitted Rust for the entry file
/// - `target/bin/<project-name>`: compiled binary
///
/// When `force` is true, the cache is cleared before building.
/// Otherwise, the build is skipped if all hashes match and the
/// binary exists.
///
/// # Errors
///
/// Returns an error if any compilation step fails.
#[allow(clippy::print_stderr)]
pub fn build(dir: &Path, config: &ProjectConfig, use_llm: bool, force: bool) -> Result<(), Box<dyn std::error::Error>> {
    if force {
        cache::clear_cache(dir)?;
    } else if cache::is_up_to_date(dir, config) {
        eprintln!("up to date");
        return Ok(());
    }

    create_target_dirs(dir)?;

    let stem = entry_stem(config);
    let is_intent = config.build.entry.ends_with(".intent");

    let resolved = resolve_entry(dir, config, use_llm)?;

    if is_intent {
        let synapse_path = dir.join(format!("target/synapse/{stem}.synapse"));
        fs::write(&synapse_path, &resolved.synapse_source)?;
        eprintln!("  synapse -> target/synapse/{stem}.synapse");
    }

    let entry_dir = dir.join(&config.build.entry);
    let src_dir = entry_dir.parent().unwrap_or(dir);

    let mut module_apis = compile_modules(src_dir, &stem, dir)?;
    compile_capability_modules(dir, &resolved.capabilities, &mut module_apis)?;
    compile_generated_modules(dir, &resolved.generated_modules, &mut module_apis)?;

    let tokens = cortex::lexer::lex(&resolved.synapse_source)?;
    let ast = cortex::parser::parse(&tokens)?;
    let typed = cortex::checker::check_with_modules(&ast, &module_apis)?;
    let rust = match &resolved.app_meta {
        Some(meta) => cortex::emitter::emit_with_application(&typed, meta),
        None => cortex::emitter::emit(&typed),
    };

    let has_modules = !module_apis.is_empty();
    let has_crate_deps = !resolved.rust_crates.is_empty();
    let use_cargo = has_crate_deps;

    if use_cargo || has_modules {
        let main_rs_path = dir.join("target/rust/src/main.rs");
        fs::write(&main_rs_path, &rust)?;
        eprintln!("  rust -> target/rust/src/main.rs");
    } else {
        let rust_path = dir.join(format!("target/rust/{stem}.rs"));
        fs::write(&rust_path, &rust)?;
        eprintln!("  rust -> target/rust/{stem}.rs");
    }

    let output = binary_path(dir, &config.project.name);

    if use_cargo {
        compile_with_cargo(dir, &config.project.name, &resolved.rust_crates, &output)?;
    } else if has_modules {
        let rustc_main = dir.join("target/rust/src/main.rs");
        let status = Command::new("rustc").arg(&rustc_main).arg("-o").arg(&output).status()?;

        if !status.success() {
            return Err("rustc compilation failed".into());
        }
    } else {
        let rust_path = dir.join(format!("target/rust/{stem}.rs"));
        let status = Command::new("rustc").arg(&rust_path).arg("-o").arg(&output).status()?;

        if !status.success() {
            return Err("rustc compilation failed".into());
        }
    }

    let mut manifest = cache::load_manifest(dir).unwrap_or_default();
    let entry_key = config.build.entry.clone();
    let entry_path = dir.join(&config.build.entry);

    if is_intent {
        manifest.intent.insert(entry_key, cache::hash_file(&entry_path)?);
    } else {
        manifest.files.insert(entry_key, cache::hash_file(&entry_path)?);
    }

    manifest.output.rust = cache::hash_string(&rust);
    manifest.output.binary = cache::hash_file(&output)?;
    manifest.output.binary_path = output.to_string_lossy().into_owned();
    cache::save_manifest(dir, &manifest)?;

    eprintln!("  built: target/bin/{}", config.project.name);
    Ok(())
}

// ---------------------------------------------------------------------------
// Multi-File Compilation
// ---------------------------------------------------------------------------

/// Compiles all non-entry `.synapse` modules in a directory.
///
/// Each module is compiled through the full cortex pipeline, its
/// public API is extracted, and its emitted Rust is written to
/// `target/rust/src/<name>.rs`.
///
/// Returns a [`ModuleMap`] for use by the entry file's type checker.
///
/// [`ModuleMap`]: cortex::module::ModuleMap
fn compile_modules(
    src_dir: &Path,
    entry_stem: &str,
    project_dir: &Path,
) -> Result<ModuleMap, Box<dyn std::error::Error>> {
    let all_modules = cortex::module::discover_modules(src_dir);

    let non_entry: Vec<_> = all_modules.into_iter().filter(|(name, _)| name != entry_stem).collect();

    if non_entry.is_empty() {
        return Ok(HashMap::new());
    }

    tracing::info!(count = non_entry.len(), "compiling synapse modules");

    let mut module_apis = ModuleMap::new();

    for (name, path) in &non_entry {
        tracing::info!(module = %name, path = %path.display(), "compiling module");
        let source = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;

        let tokens = cortex::lexer::lex(&source)?;
        let ast = cortex::parser::parse(&tokens)?;
        let typed = cortex::checker::check_with_modules(&ast, &module_apis)?;

        let api = cortex::module::extract_api(name, &typed);
        let rust = cortex::emitter::emit(&typed);

        let module_rs = project_dir.join(format!("target/rust/src/{name}.rs"));
        fs::write(&module_rs, &rust)?;
        #[allow(clippy::print_stderr)]
        {
            eprintln!("  rust -> target/rust/src/{name}.rs");
        }

        module_apis.insert(name.clone(), api);
    }

    Ok(module_apis)
}

// ---------------------------------------------------------------------------
// Capability Module Compilation
// ---------------------------------------------------------------------------

/// Compiles `Import` capabilities with `.synapse` paths, emits their Rust to
/// `target/rust/src/<name>.rs`, and returns their APIs merged into
/// the given [`ModuleMap`].
///
/// Each module is compiled through the full cortex pipeline. Its public
/// API is extracted and its emitted Rust is written to the target
/// directory so `rustc`/`cargo` can find it as `mod <name>;`.
///
/// [`ModuleMap`]: cortex::module::ModuleMap
fn compile_capability_modules(
    project_dir: &Path,
    capabilities: &[intent::ast::CapabilityDef],
    existing: &mut ModuleMap,
) -> Result<(), Box<dyn std::error::Error>> {
    let synapse_caps: Vec<_> = capabilities
        .iter()
        .filter_map(|cap| {
            if let CapabilityKind::Import { path: Some(path) } = &cap.kind {
                if path.ends_with(".synapse") {
                    return Some((cap.name.clone(), path.clone()));
                }
            }
            None
        })
        .collect();

    if synapse_caps.is_empty() {
        return Ok(());
    }

    tracing::info!(
        count = synapse_caps.len(),
        "compiling existing synapse module capabilities"
    );

    for (name, rel_path) in &synapse_caps {
        if existing.contains_key(name) {
            tracing::debug!(module = %name, "module already compiled, skipping");
            continue;
        }

        let full_path = project_dir.join(rel_path);
        tracing::info!(module = %name, path = %full_path.display(), "compiling capability module");

        let source = fs::read_to_string(&full_path)
            .map_err(|e| format!("cannot read synapse module '{}': {e}", full_path.display()))?;

        let tokens = cortex::lexer::lex(&source)?;
        let ast = cortex::parser::parse(&tokens)?;
        let typed = cortex::checker::check_with_modules(&ast, existing)?;

        let api = cortex::module::extract_api(name, &typed);
        let rust = cortex::emitter::emit(&typed);

        let module_rs = project_dir.join(format!("target/rust/src/{name}.rs"));
        fs::write(&module_rs, &rust)?;
        #[allow(clippy::print_stderr)]
        {
            eprintln!("  rust -> target/rust/src/{name}.rs");
        }

        existing.insert(name.clone(), api);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Generated Module Compilation
// ---------------------------------------------------------------------------

/// Compiles LLM-generated `.synapse` modules from `new module` capabilities.
///
/// Each module source is written to `target/synapse/<name>.synapse`,
/// compiled through the full cortex pipeline, and its emitted Rust
/// is written to `target/rust/src/<name>.rs`. The module's public
/// API is added to the [`ModuleMap`].
///
/// [`ModuleMap`]: cortex::module::ModuleMap
#[allow(clippy::print_stderr)]
fn compile_generated_modules(
    project_dir: &Path,
    modules: &HashMap<String, String>,
    existing: &mut ModuleMap,
) -> Result<(), Box<dyn std::error::Error>> {
    if modules.is_empty() {
        return Ok(());
    }

    tracing::info!(count = modules.len(), "compiling generated new-module capabilities");

    for (name, source) in modules {
        if existing.contains_key(name) {
            tracing::debug!(module = %name, "generated module already compiled, skipping");
            continue;
        }

        let synapse_path = project_dir.join(format!("target/synapse/{name}.synapse"));
        fs::write(&synapse_path, source)?;
        eprintln!("  synapse -> target/synapse/{name}.synapse");

        tracing::info!(module = %name, "compiling generated module");

        let tokens = cortex::lexer::lex(source)?;
        let ast = cortex::parser::parse(&tokens)?;
        let typed = cortex::checker::check_with_modules(&ast, existing)?;

        let api = cortex::module::extract_api(name, &typed);
        let rust = cortex::emitter::emit(&typed);

        let module_rs = project_dir.join(format!("target/rust/src/{name}.rs"));
        fs::write(&module_rs, &rust)?;
        eprintln!("  rust -> target/rust/src/{name}.rs");

        existing.insert(name.clone(), api);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Cargo Build Pipeline
// ---------------------------------------------------------------------------

/// Compiles the project using `cargo build` when Rust crate dependencies
/// are present.
///
/// Generates a `Cargo.toml` in `target/rust/`, fetches remote
/// dependencies, runs `cargo build --release`, and copies the resulting
/// binary to the output path.
#[allow(clippy::print_stderr)]
fn compile_with_cargo(
    dir: &Path,
    project_name: &str,
    rust_crates: &[CrateSpec],
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let cargo_dir = dir.join("target/rust");

    let cargo_toml = cortex::emitter::generate_cargo_toml(project_name, rust_crates);
    let cargo_toml_path = cargo_dir.join("Cargo.toml");
    fs::write(&cargo_toml_path, &cargo_toml)?;
    eprintln!("  cargo -> target/rust/Cargo.toml");

    fetch_remote_crates(&cargo_dir, rust_crates)?;

    tracing::info!("running cargo build --release");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .current_dir(&cargo_dir)
        .status()?;

    if !status.success() {
        return Err("cargo build failed".into());
    }

    let cargo_bin = cargo_dir.join("target/release").join(project_name);
    if cargo_bin.exists() {
        fs::copy(&cargo_bin, output)?;
        tracing::info!(
            from = %cargo_bin.display(),
            to = %output.display(),
            "copied cargo binary"
        );
    }

    Ok(())
}

/// Runs `cargo fetch` in the given directory to download remote crate
/// dependencies.
///
/// Skips the fetch if all crate specs are local path dependencies
/// (no version or git fields).
fn fetch_remote_crates(cargo_dir: &Path, rust_crates: &[CrateSpec]) -> Result<(), Box<dyn std::error::Error>> {
    let has_remote = rust_crates.iter().any(|s| s.version.is_some() || s.git.is_some());

    if !has_remote {
        tracing::debug!("no remote crates to fetch");
        return Ok(());
    }

    tracing::info!("running cargo fetch for remote dependencies");
    let status = Command::new("cargo").arg("fetch").current_dir(cargo_dir).status()?;

    if !status.success() {
        return Err("cargo fetch failed".into());
    }

    Ok(())
}

/// Extracts [`CrateSpec`] entries from application capabilities.
///
/// Filters capabilities to `ImportRustCrate` variants and converts each
/// [`RustCrateSpec`] to a [`CrateSpec`].
///
/// ```
/// # use intent::ast::*;
/// # use axon::build::extract_rust_crate_specs;
/// let caps = vec![
///     CapabilityDef {
///         name: "builtins".to_owned(),
///         kind: CapabilityKind::Import { path: None },
///     },
///     CapabilityDef {
///         name: "serde_json".to_owned(),
///         kind: CapabilityKind::ImportRustCrate {
///             spec: RustCrateSpec {
///                 name: "serde_json".to_owned(),
///                 version: Some("1.0.140".to_owned()),
///                 path: None,
///                 git: None,
///             },
///         },
///     },
/// ];
/// let specs = extract_rust_crate_specs(&caps);
/// assert_eq!(specs.len(), 1);
/// assert_eq!(specs[0].name, "serde_json");
/// ```
///
/// [`CrateSpec`]: cortex::emitter::CrateSpec
/// [`RustCrateSpec`]: intent::ast::RustCrateSpec
pub fn extract_rust_crate_specs(capabilities: &[intent::ast::CapabilityDef]) -> Vec<CrateSpec> {
    capabilities
        .iter()
        .filter_map(|cap| {
            if let CapabilityKind::ImportRustCrate { spec } = &cap.kind {
                Some(CrateSpec {
                    name: spec.name.clone(),
                    version: spec.version.clone(),
                    path: spec.path.clone(),
                    git: spec.git.clone(),
                })
            } else {
                None
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------

/// Type-checks the entry file without emitting Rust.
///
/// Supports both `.synapse` and `.intent` entry files.
///
/// # Errors
///
/// Returns an error if type-checking fails.
#[allow(clippy::print_stderr)]
pub fn check_only(dir: &Path, config: &ProjectConfig, use_llm: bool) -> Result<(), Box<dyn std::error::Error>> {
    let resolved = resolve_entry(dir, config, use_llm)?;
    cortex::compile_check(&resolved.synapse_source)?;
    eprintln!("ok");
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry Resolution
// ---------------------------------------------------------------------------

/// Resolves the entry source, expanding `.intent` files to `.synapse`.
///
/// For intent files, checks the expansion cache first. If the intent
/// source hasn't changed, the cached `.synapse` is reused without
/// calling the LLM or template expander.
///
/// When the intent file contains an `application` block, the
/// [`Application`] metadata is converted to [`AppMeta`] and returned
/// alongside the expanded source. Capability APIs are resolved and
/// passed to the LLM prompt.
///
/// [`Application`]: intent::ast::Application
/// [`AppMeta`]: cortex::emitter::AppMeta
fn resolve_entry(
    dir: &Path,
    config: &ProjectConfig,
    use_llm: bool,
) -> Result<ResolvedEntry, Box<dyn std::error::Error>> {
    let path = dir.join(&config.build.entry);
    let source = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;

    if config.build.entry.ends_with(".intent") {
        tracing::info!(entry = %config.build.entry, "expanding intent file");
        let current_hash = cache::hash_string(&source);

        let info = extract_app_meta_and_crates(&source)?;

        if let Some(cached) = cache::load_cached_expansion(dir, &config.build.entry, &current_hash) {
            tracing::info!("reusing cached intent expansion");
            return Ok(ResolvedEntry {
                synapse_source: cached,
                app_meta: info.app_meta,
                rust_crates: info.rust_crates,
                capabilities: info.capabilities,
                generated_modules: HashMap::new(),
            });
        }

        let expanded = expand_intent_source_with_apis(dir, &source, use_llm)?;
        cache::save_expansion(dir, &config.build.entry, &expanded.main_synapse)?;

        let mut manifest = cache::load_manifest(dir).unwrap_or_default();
        manifest.intent.insert(config.build.entry.clone(), current_hash);
        cache::save_manifest(dir, &manifest)?;

        Ok(ResolvedEntry {
            synapse_source: expanded.main_synapse,
            app_meta: info.app_meta,
            rust_crates: info.rust_crates,
            capabilities: info.capabilities,
            generated_modules: expanded.modules,
        })
    } else {
        Ok(ResolvedEntry {
            synapse_source: source,
            app_meta: None,
            rust_crates: vec![],
            capabilities: vec![],
            generated_modules: HashMap::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// AppMeta Conversion
// ---------------------------------------------------------------------------

/// Extracted metadata from an intent source's application block.
struct ExtractedAppInfo {
    /// Application metadata (args, flags, env vars).
    app_meta: Option<AppMeta>,
    /// Rust crate dependency specs.
    rust_crates: Vec<CrateSpec>,
    /// Declared capability definitions.
    capabilities: Vec<intent::ast::CapabilityDef>,
}

/// Extracts [`AppMeta`], [`CrateSpec`] entries, and capability definitions
/// from an intent source.
///
/// Lexes and parses the intent source, then converts the first
/// application's args, environment definitions, and capability
/// declarations into their cortex emitter counterparts.
///
/// [`AppMeta`]: cortex::emitter::AppMeta
/// [`CrateSpec`]: cortex::emitter::CrateSpec
fn extract_app_meta_and_crates(source: &str) -> Result<ExtractedAppInfo, Box<dyn std::error::Error>> {
    let tokens = intent::lexer::lex(source)?;
    let program = intent::parser::parse(&tokens)?;
    match program.applications.first() {
        Some(app) => {
            tracing::info!(name = %app.name, "found application block");
            let rust_crates = extract_rust_crate_specs(&app.capabilities);
            Ok(ExtractedAppInfo {
                app_meta: Some(to_app_meta(app)),
                rust_crates,
                capabilities: app.capabilities.clone(),
            })
        },
        None => Ok(ExtractedAppInfo {
            app_meta: None,
            rust_crates: vec![],
            capabilities: vec![],
        }),
    }
}

/// Converts an intent [`Application`] to a cortex [`AppMeta`].
///
/// Maps each [`FlagDef`], [`PositionalDef`], and [`EnvVar`] from the
/// intent AST to its cortex emitter counterpart.
///
/// ```
/// # use intent::ast::*;
/// # use axon::build::to_app_meta;
/// let app = Application {
///     name: "demo".to_owned(),
///     args: ArgsDef {
///         verb: Some("action".to_owned()),
///         flags: vec![FlagDef {
///             long_name: "verbose".to_owned(),
///             default: None,
///             ty: None,
///         }],
///         positionals: vec![PositionalDef {
///             binding: "file".to_owned(),
///             ty: "String".to_owned(),
///         }],
///     },
///     capabilities: vec![],
///     environment: vec![EnvVar {
///         binding: "key".to_owned(),
///         default: None,
///         ty: "String".to_owned(),
///         var_name: "API_KEY".to_owned(),
///     }],
///     intent: StructuredIntent {
///         description: "do something".to_owned(),
///         properties: vec![],
///     },
/// };
/// let meta = to_app_meta(&app);
/// assert_eq!(meta.verb.as_deref(), Some("action"));
/// assert_eq!(meta.flags.len(), 1);
/// assert_eq!(meta.positionals.len(), 1);
/// assert_eq!(meta.env_vars.len(), 1);
/// ```
///
/// [`Application`]: intent::ast::Application
/// [`AppMeta`]: cortex::emitter::AppMeta
/// [`FlagDef`]: intent::ast::FlagDef
/// [`PositionalDef`]: intent::ast::PositionalDef
/// [`EnvVar`]: intent::ast::EnvVar
pub fn to_app_meta(app: &Application) -> AppMeta {
    AppMeta {
        verb: app.args.verb.clone(),
        flags: app
            .args
            .flags
            .iter()
            .map(|f| AppFlag {
                long_name: f.long_name.clone(),
                default: f.default.clone(),
                ty: f.ty.clone(),
            })
            .collect(),
        positionals: app
            .args
            .positionals
            .iter()
            .map(|p| AppPositional {
                binding: p.binding.clone(),
                ty: p.ty.clone(),
            })
            .collect(),
        env_vars: app
            .environment
            .iter()
            .map(|e| AppEnvVar {
                binding: e.binding.clone(),
                default: e.default.clone(),
                ty: e.ty.clone(),
                var_name: e.var_name.clone(),
            })
            .collect(),
    }
}

/// Expands an intent source string to valid `.synapse` source.
///
/// # Errors
///
/// Returns an error if lexing, parsing, or expansion fails.
pub fn expand_intent_source(source: &str, use_llm: bool) -> Result<String, Box<dyn std::error::Error>> {
    let tokens = intent::lexer::lex(source)?;
    let program = intent::parser::parse(&tokens)?;
    let synapse = if use_llm {
        intent::expander::expand_with_llm(&program)?
    } else {
        intent::expander::expand(&program)?
    };
    Ok(synapse)
}

/// Expands an intent source string with resolved capability APIs.
///
/// Parses the intent, resolves capability APIs for any application
/// block, and passes them to the expander so the LLM prompt
/// includes exact function signatures.
///
/// Returns an [`ExpandedApplication`] containing the main `.synapse`
/// source and any generated module sources.
///
/// # Errors
///
/// Returns an error if lexing, parsing, API resolution, or
/// expansion fails.
///
/// [`ExpandedApplication`]: intent::llm::ExpandedApplication
fn expand_intent_source_with_apis(
    dir: &Path,
    source: &str,
    use_llm: bool,
) -> Result<intent::llm::ExpandedApplication, Box<dyn std::error::Error>> {
    let tokens = intent::lexer::lex(source)?;
    let program = intent::parser::parse(&tokens)?;

    if let Some(app) = program.applications.first() {
        if !app.capabilities.is_empty() {
            let apis = capabilities::resolve_capability_apis(dir, &app.capabilities)?;
            tracing::info!(count = apis.len(), "resolved capability APIs for application prompt");
            let expanded = intent::expander::expand_with_llm_and_apis_full(&program, &apis)?;
            return Ok(expanded);
        }
    }

    let synapse = if use_llm {
        intent::expander::expand_with_llm(&program)?
    } else {
        intent::expander::expand(&program)?
    };
    Ok(intent::llm::ExpandedApplication {
        main_synapse: synapse,
        modules: HashMap::new(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use intent::ast::{CapabilityDef, CapabilityKind, RustCrateSpec};

    use super::*;

    #[test]
    fn extract_rust_crate_specs_filters_correctly() {
        let caps = vec![
            CapabilityDef {
                name: "builtins".to_owned(),
                kind: CapabilityKind::Import { path: None },
            },
            CapabilityDef {
                name: "json".to_owned(),
                kind: CapabilityKind::ImportRustCrate {
                    spec: RustCrateSpec {
                        name: "serde_json".to_owned(),
                        version: Some("1.0.140".to_owned()),
                        path: None,
                        git: None,
                    },
                },
            },
            CapabilityDef {
                name: "weather".to_owned(),
                kind: CapabilityKind::NewModule,
            },
        ];
        let specs = extract_rust_crate_specs(&caps);
        assert_eq!(specs.len(), 1, "only RustCrate capability extracted");
        assert_eq!(specs[0].name, "serde_json");
        assert_eq!(specs[0].version.as_deref(), Some("1.0.140"), "version is preserved");
    }

    #[test]
    fn extract_rust_crate_specs_empty_when_no_crates() {
        let caps = vec![
            CapabilityDef {
                name: "builtins".to_owned(),
                kind: CapabilityKind::Import { path: None },
            },
            CapabilityDef {
                name: "helper".to_owned(),
                kind: CapabilityKind::Import {
                    path: Some("helper.rs".to_owned()),
                },
            },
        ];
        let specs = extract_rust_crate_specs(&caps);
        assert!(specs.is_empty(), "no rust crate specs from non-crate capabilities");
    }

    #[test]
    fn extract_rust_crate_specs_multiple_crates() {
        let caps = vec![
            CapabilityDef {
                name: "json".to_owned(),
                kind: CapabilityKind::ImportRustCrate {
                    spec: RustCrateSpec {
                        name: "serde_json".to_owned(),
                        version: Some("1.0.140".to_owned()),
                        path: None,
                        git: None,
                    },
                },
            },
            CapabilityDef {
                name: "local".to_owned(),
                kind: CapabilityKind::ImportRustCrate {
                    spec: RustCrateSpec {
                        name: "mylib".to_owned(),
                        version: None,
                        path: Some("../mylib".to_owned()),
                        git: None,
                    },
                },
            },
        ];
        let specs = extract_rust_crate_specs(&caps);
        assert_eq!(specs.len(), 2, "both crate specs extracted");
        assert_eq!(specs[0].name, "serde_json");
        assert_eq!(specs[1].name, "mylib");
        assert_eq!(specs[1].path.as_deref(), Some("../mylib"), "path is preserved");
    }

    #[test]
    fn extract_rust_crate_specs_preserves_git() {
        let caps = vec![CapabilityDef {
            name: "foo".to_owned(),
            kind: CapabilityKind::ImportRustCrate {
                spec: RustCrateSpec {
                    name: "foo".to_owned(),
                    version: None,
                    path: None,
                    git: Some("https://github.com/x/foo".to_owned()),
                },
            },
        }];
        let specs = extract_rust_crate_specs(&caps);
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].git.as_deref(),
            Some("https://github.com/x/foo"),
            "git url is preserved"
        );
    }

    #[test]
    fn compile_capability_modules_emits_rust_and_api() {
        let dir = tempfile::tempdir().unwrap();
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_src).unwrap();

        let synapse_path = dir.path().join("lib/math.synapse");
        fs::create_dir_all(synapse_path.parent().unwrap()).unwrap();
        fs::write(
            &synapse_path,
            "pub function factorial(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * factorial(n - 1)\n",
        )
        .unwrap();

        let caps = vec![CapabilityDef {
            name: "math".to_owned(),
            kind: CapabilityKind::Import {
                path: Some("lib/math.synapse".to_owned()),
            },
        }];

        let mut module_apis = HashMap::new();
        compile_capability_modules(dir.path(), &caps, &mut module_apis).unwrap();

        assert_eq!(module_apis.len(), 1, "one module API extracted");
        let api = &module_apis["math"];
        assert_eq!(api.functions.len(), 1, "one pub function");
        assert_eq!(api.functions[0].name, "factorial");
        assert_eq!(api.functions[0].params.len(), 1, "factorial takes one param");
        assert_eq!(
            api.functions[0].return_type,
            cortex::ast::Type::Int,
            "factorial returns Int"
        );

        let emitted_rs = dir.path().join("target/rust/src/math.rs");
        assert!(emitted_rs.exists(), "emitted Rust file must exist");
        let rust_content = fs::read_to_string(&emitted_rs).unwrap();
        assert!(
            rust_content.contains("fn factorial(n: i64) -> i64"),
            "emitted Rust contains factorial function: {rust_content}"
        );
    }

    #[test]
    fn compile_capability_modules_skips_non_synapse() {
        let dir = tempfile::tempdir().unwrap();
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_src).unwrap();

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

        let mut module_apis = HashMap::new();
        compile_capability_modules(dir.path(), &caps, &mut module_apis).unwrap();
        assert!(
            module_apis.is_empty(),
            "no modules compiled for non-synapse capabilities"
        );
    }

    #[test]
    fn compile_capability_modules_skips_already_compiled() {
        let dir = tempfile::tempdir().unwrap();
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_src).unwrap();

        let synapse_path = dir.path().join("math.synapse");
        fs::write(
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

        let mut module_apis = HashMap::new();
        module_apis.insert(
            "math".to_owned(),
            cortex::module::ModuleApi {
                name: "math".to_owned(),
                functions: vec![],
            },
        );

        compile_capability_modules(dir.path(), &caps, &mut module_apis).unwrap();
        assert!(
            module_apis["math"].functions.is_empty(),
            "pre-existing API was not overwritten"
        );
    }

    #[test]
    fn compile_capability_modules_multiple_modules() {
        let dir = tempfile::tempdir().unwrap();
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_src).unwrap();

        fs::write(
            dir.path().join("math.synapse"),
            "pub function add(Int a, Int b) -> Int\n  returns a + b\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("utils.synapse"),
            "pub function negate(Int x) -> Int\n  returns 0 - x\n",
        )
        .unwrap();

        let caps = vec![
            CapabilityDef {
                name: "math".to_owned(),
                kind: CapabilityKind::Import {
                    path: Some("math.synapse".to_owned()),
                },
            },
            CapabilityDef {
                name: "utils".to_owned(),
                kind: CapabilityKind::Import {
                    path: Some("utils.synapse".to_owned()),
                },
            },
        ];

        let mut module_apis = HashMap::new();
        compile_capability_modules(dir.path(), &caps, &mut module_apis).unwrap();

        assert_eq!(module_apis.len(), 2, "both modules compiled");
        assert_eq!(module_apis["math"].functions[0].name, "add");
        assert_eq!(module_apis["utils"].functions[0].name, "negate");

        assert!(dir.path().join("target/rust/src/math.rs").exists(), "math.rs emitted");
        assert!(dir.path().join("target/rust/src/utils.rs").exists(), "utils.rs emitted");
    }

    #[test]
    fn compile_generated_modules_writes_and_extracts_api() {
        let dir = tempfile::tempdir().unwrap();
        let target_synapse = dir.path().join("target/synapse");
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_synapse).unwrap();
        fs::create_dir_all(&target_src).unwrap();

        let mut modules = HashMap::new();
        modules.insert(
            "calculator".to_owned(),
            "pub function factorial(Int n) -> Int\n  returns match n\n    when 0 -> 1\n    otherwise -> n * factorial(n - 1)\n".to_owned(),
        );

        let mut module_apis = HashMap::new();
        compile_generated_modules(dir.path(), &modules, &mut module_apis).unwrap();

        assert_eq!(module_apis.len(), 1, "one module API extracted");
        let api = &module_apis["calculator"];
        assert_eq!(api.functions.len(), 1, "one pub function");
        assert_eq!(api.functions[0].name, "factorial");

        assert!(
            dir.path().join("target/synapse/calculator.synapse").exists(),
            "synapse source written"
        );
        assert!(
            dir.path().join("target/rust/src/calculator.rs").exists(),
            "rust source emitted"
        );
    }

    #[test]
    fn compile_generated_modules_empty_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let modules: HashMap<String, String> = HashMap::new();
        let mut module_apis = HashMap::new();
        compile_generated_modules(dir.path(), &modules, &mut module_apis).unwrap();
        assert!(module_apis.is_empty(), "no modules compiled from empty map");
    }

    #[test]
    fn compile_generated_modules_skips_existing() {
        let dir = tempfile::tempdir().unwrap();
        let target_synapse = dir.path().join("target/synapse");
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_synapse).unwrap();
        fs::create_dir_all(&target_src).unwrap();

        let mut modules = HashMap::new();
        modules.insert(
            "calc".to_owned(),
            "pub function add(Int a, Int b) -> Int\n  returns a + b\n".to_owned(),
        );

        let mut module_apis = HashMap::new();
        module_apis.insert(
            "calc".to_owned(),
            cortex::module::ModuleApi {
                name: "calc".to_owned(),
                functions: vec![],
            },
        );

        compile_generated_modules(dir.path(), &modules, &mut module_apis).unwrap();
        assert!(
            module_apis["calc"].functions.is_empty(),
            "pre-existing API was not overwritten"
        );
    }

    #[test]
    fn compile_generated_modules_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let target_synapse = dir.path().join("target/synapse");
        let target_src = dir.path().join("target/rust/src");
        fs::create_dir_all(&target_synapse).unwrap();
        fs::create_dir_all(&target_src).unwrap();

        let mut modules = HashMap::new();
        modules.insert(
            "math".to_owned(),
            "pub function double(Int x) -> Int\n  returns x * 2\n".to_owned(),
        );
        modules.insert(
            "utils".to_owned(),
            "pub function negate(Int x) -> Int\n  returns 0 - x\n".to_owned(),
        );

        let mut module_apis = HashMap::new();
        compile_generated_modules(dir.path(), &modules, &mut module_apis).unwrap();

        assert_eq!(module_apis.len(), 2, "both modules compiled");
        assert!(dir.path().join("target/rust/src/math.rs").exists(), "math.rs emitted");
        assert!(dir.path().join("target/rust/src/utils.rs").exists(), "utils.rs emitted");
    }
}
