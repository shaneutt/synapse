use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use cortex::emitter::{AppEnvVar, AppFlag, AppMeta, AppPositional};
use intent::ast::Application;

use crate::{cache, project::ProjectConfig};

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
/// };
/// assert!(r.app_meta.is_none());
/// ```
pub struct ResolvedEntry {
    /// The `.synapse` source code (possibly expanded from `.intent`).
    pub synapse_source: String,
    /// Application metadata when the entry is an `.intent` application.
    pub app_meta: Option<AppMeta>,
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
    fs::create_dir_all(dir.join("target/rust"))?;
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
/// - `target/rust/<stem>.rs`: emitted Rust
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

    let tokens = cortex::lexer::lex(&resolved.synapse_source)?;
    let ast = cortex::parser::parse(&tokens)?;
    let typed = cortex::checker::check(&ast)?;
    let rust = match &resolved.app_meta {
        Some(meta) => cortex::emitter::emit_with_application(&typed, meta),
        None => cortex::emitter::emit(&typed),
    };

    let rust_path = dir.join(format!("target/rust/{stem}.rs"));
    fs::write(&rust_path, &rust)?;
    eprintln!("  rust -> target/rust/{stem}.rs");

    let output = binary_path(dir, &config.project.name);
    let status = Command::new("rustc").arg(&rust_path).arg("-o").arg(&output).status()?;

    if !status.success() {
        return Err("rustc compilation failed".into());
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
/// alongside the expanded source.
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

        let app_meta = extract_app_meta(&source)?;

        if let Some(cached) = cache::load_cached_expansion(dir, &config.build.entry, &current_hash) {
            tracing::info!("reusing cached intent expansion");
            return Ok(ResolvedEntry {
                synapse_source: cached,
                app_meta,
            });
        }

        let synapse = expand_intent_source(&source, use_llm)?;
        cache::save_expansion(dir, &config.build.entry, &synapse)?;

        let mut manifest = cache::load_manifest(dir).unwrap_or_default();
        manifest.intent.insert(config.build.entry.clone(), current_hash);
        cache::save_manifest(dir, &manifest)?;

        Ok(ResolvedEntry {
            synapse_source: synapse,
            app_meta,
        })
    } else {
        Ok(ResolvedEntry {
            synapse_source: source,
            app_meta: None,
        })
    }
}

// ---------------------------------------------------------------------------
// AppMeta Conversion
// ---------------------------------------------------------------------------

/// Extracts [`AppMeta`] from an intent source if it contains an
/// `application` block.
///
/// Lexes and parses the intent source, then converts the first
/// application's args and environment definitions into the cortex
/// emitter's [`AppMeta`] type.
///
/// [`AppMeta`]: cortex::emitter::AppMeta
fn extract_app_meta(source: &str) -> Result<Option<AppMeta>, Box<dyn std::error::Error>> {
    let tokens = intent::lexer::lex(source)?;
    let program = intent::parser::parse(&tokens)?;
    match program.applications.first() {
        Some(app) => {
            tracing::info!(name = %app.name, "found application block");
            Ok(Some(to_app_meta(app)))
        },
        None => Ok(None),
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
///     environment: vec![EnvVar {
///         binding: "key".to_owned(),
///         default: None,
///         ty: "String".to_owned(),
///         var_name: "API_KEY".to_owned(),
///     }],
///     intent: "do something".to_owned(),
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
