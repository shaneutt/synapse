use std::path::Path;

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Top-level project configuration from `synapse.toml`.
///
/// ```
/// # use axon::project::ProjectConfig;
/// let toml_str = r#"
/// [project]
/// name = "demo"
/// version = "0.1.0"
///
/// [build]
/// entry = "src/main.synapse"
/// "#;
/// let config: ProjectConfig = toml::from_str(toml_str).unwrap();
/// assert_eq!(config.project.name, "demo");
/// ```
#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    /// Project identity and version.
    pub project: ProjectMeta,
    /// Build settings.
    pub build: BuildConfig,
}

// ---------------------------------------------------------------------------
// Configuration Sections
// ---------------------------------------------------------------------------

/// Project metadata.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ProjectMeta {
    /// Project name.
    pub name: String,
    /// Semantic version.
    pub version: String,
}

/// Build configuration.
#[derive(Debug, Deserialize)]
pub struct BuildConfig {
    /// Entry source file path relative to the project root.
    pub entry: String,
}

// ---------------------------------------------------------------------------
// Config Loading
// ---------------------------------------------------------------------------

/// Reads and parses `synapse.toml` from the given directory.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
pub fn load_config(dir: &Path) -> Result<ProjectConfig, Box<dyn std::error::Error>> {
    let path = dir.join("synapse.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let config: ProjectConfig = toml::from_str(&text).map_err(|e| format!("invalid synapse.toml: {e}"))?;
    Ok(config)
}
