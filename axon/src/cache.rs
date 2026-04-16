use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::project::ProjectConfig;

/// Cache directory name, created next to `synapse.toml`.
const CACHE_DIR: &str = ".synapse-cache";

/// Manifest filename inside the cache directory.
const MANIFEST_FILE: &str = "manifest.toml";

/// Subdirectory for cached intent expansions.
const EXPANDED_DIR: &str = "expanded";

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Persistent build manifest tracking source hashes and outputs.
///
/// Stored as TOML in `.synapse-cache/manifest.toml`.
///
/// ```
/// # use axon::cache::CacheManifest;
/// let m = CacheManifest::default();
/// assert!(m.files.is_empty());
/// ```
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CacheManifest {
    /// SHA-256 hex digest of each source file, keyed by relative path.
    #[serde(default)]
    pub files: BTreeMap<String, String>,

    /// SHA-256 hex digest of each `.intent` source file.
    #[serde(default)]
    pub intent: BTreeMap<String, String>,

    /// SHA-256 hex digest of the emitted Rust source.
    #[serde(default)]
    pub output: OutputHashes,
}

/// Hashes for the compiler output artifacts.
///
/// ```
/// # use axon::cache::OutputHashes;
/// let o = OutputHashes::default();
/// assert!(o.rust.is_empty());
/// ```
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct OutputHashes {
    /// SHA-256 of the emitted `.rs` file.
    #[serde(default)]
    pub rust: String,

    /// SHA-256 of the compiled binary.
    #[serde(default)]
    pub binary: String,

    /// Path to the compiled binary.
    #[serde(default)]
    pub binary_path: String,
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// Computes the SHA-256 hex digest of a file's contents.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
///
/// ```no_run
/// # use axon::cache::hash_file;
/// let digest = hash_file(std::path::Path::new("Cargo.toml")).unwrap();
/// assert_eq!(digest.len(), 64);
/// ```
pub fn hash_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    Ok(hex_sha256(&bytes))
}

/// Computes the SHA-256 hex digest of a string.
///
/// ```
/// # use axon::cache::hash_string;
/// let d = hash_string("hello");
/// assert_eq!(d.len(), 64);
/// ```
pub fn hash_string(content: &str) -> String {
    hex_sha256(content.as_bytes())
}

/// Raw SHA-256 to lowercase hex.
fn hex_sha256(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

// ---------------------------------------------------------------------------
// Manifest I/O
// ---------------------------------------------------------------------------

/// Returns the cache directory path for a project.
fn cache_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(CACHE_DIR)
}

/// Returns the manifest file path for a project.
fn manifest_path(project_dir: &Path) -> PathBuf {
    cache_dir(project_dir).join(MANIFEST_FILE)
}

/// Loads the cache manifest, returning `None` if it does not exist.
///
/// ```no_run
/// # use axon::cache::load_manifest;
/// let m = load_manifest(std::path::Path::new("."));
/// ```
pub fn load_manifest(dir: &Path) -> Option<CacheManifest> {
    let path = manifest_path(dir);
    let text = fs::read_to_string(&path).ok()?;
    toml::from_str(&text).ok()
}

/// Writes the cache manifest to disk, creating the cache directory if
/// needed.
///
/// # Errors
///
/// Returns an error if the directory or file cannot be written.
///
/// ```no_run
/// # use axon::cache::{save_manifest, CacheManifest};
/// save_manifest(std::path::Path::new("."), &CacheManifest::default()).unwrap();
/// ```
pub fn save_manifest(dir: &Path, manifest: &CacheManifest) -> Result<(), Box<dyn std::error::Error>> {
    let cd = cache_dir(dir);
    fs::create_dir_all(&cd)?;
    let text = toml::to_string_pretty(manifest)?;
    fs::write(manifest_path(dir), text)?;
    Ok(())
}

/// Removes the entire cache directory.
///
/// # Errors
///
/// Returns an error if the directory cannot be removed.
pub fn clear_cache(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let cd = cache_dir(dir);
    if cd.exists() {
        fs::remove_dir_all(&cd)?;
        tracing::info!(path = %cd.display(), "cleared cache");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Up-to-date check
// ---------------------------------------------------------------------------

/// Returns `true` when all source hashes match the manifest and the
/// binary exists on disk.
///
/// ```no_run
/// # use axon::cache::is_up_to_date;
/// # use axon::project::ProjectConfig;
/// // Checks file hashes against the stored manifest.
/// let config: ProjectConfig = toml::from_str(
///     r#"
/// [project]
/// name = "demo"
/// version = "0.1.0"
/// [build]
/// entry = "src/main.synapse"
/// "#,
/// )
/// .unwrap();
/// let fresh = is_up_to_date(std::path::Path::new("."), &config);
/// ```
pub fn is_up_to_date(dir: &Path, config: &ProjectConfig) -> bool {
    let Some(manifest) = load_manifest(dir) else {
        return false;
    };

    let binary = PathBuf::from(&manifest.output.binary_path);
    if !binary.exists() {
        tracing::debug!("binary missing, rebuild required");
        return false;
    }

    let entry_path = dir.join(&config.build.entry);
    let entry_key = config.build.entry.clone();

    if config.build.entry.ends_with(".intent") {
        match manifest.intent.get(&entry_key) {
            Some(cached_hash) => match hash_file(&entry_path) {
                Ok(h) if &h == cached_hash => {},
                _ => {
                    tracing::debug!(file = %entry_key, "intent hash changed");
                    return false;
                },
            },
            None => return false,
        }
    } else {
        match manifest.files.get(&entry_key) {
            Some(cached_hash) => match hash_file(&entry_path) {
                Ok(h) if &h == cached_hash => {},
                _ => {
                    tracing::debug!(file = %entry_key, "source hash changed");
                    return false;
                },
            },
            None => return false,
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Intent expansion cache
// ---------------------------------------------------------------------------

/// Returns the path where a cached intent expansion is stored.
fn expanded_path(dir: &Path, intent_entry: &str) -> PathBuf {
    let stem = Path::new(intent_entry)
        .file_stem()
        .map_or_else(|| "unknown".to_owned(), |s| s.to_string_lossy().into_owned());
    cache_dir(dir).join(EXPANDED_DIR).join(format!("{stem}.synapse"))
}

/// Loads a cached intent expansion if the intent source hash matches.
///
/// Returns `Some(synapse_source)` when the cached expansion is still
/// valid, `None` otherwise.
pub fn load_cached_expansion(dir: &Path, intent_entry: &str, current_hash: &str) -> Option<String> {
    let manifest = load_manifest(dir)?;
    let cached_hash = manifest.intent.get(intent_entry)?;
    if cached_hash != current_hash {
        return None;
    }
    let path = expanded_path(dir, intent_entry);
    fs::read_to_string(&path).ok()
}

/// Stores an expanded `.synapse` source for a given intent entry.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn save_expansion(dir: &Path, intent_entry: &str, synapse_source: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = expanded_path(dir, intent_entry);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, synapse_source)?;
    tracing::debug!(path = %path.display(), "cached intent expansion");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn hash_string_deterministic() {
        let a = hash_string("hello world");
        let b = hash_string("hello world");
        assert_eq!(a, b, "same input must produce same hash");
        assert_eq!(a.len(), 64, "SHA-256 hex digest must be 64 chars");
    }

    #[test]
    fn hash_string_differs_for_different_input() {
        let a = hash_string("alpha");
        let b = hash_string("bravo");
        assert_ne!(a, b, "different inputs must produce different hashes");
    }

    #[test]
    fn hash_string_known_value() {
        let digest = hash_string("hello");
        assert_eq!(
            digest, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "must match the well-known SHA-256 of 'hello'"
        );
    }

    #[test]
    fn hash_file_matches_hash_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.txt");
        fs::write(&path, "test content").unwrap();
        let file_hash = hash_file(&path).unwrap();
        let str_hash = hash_string("test content");
        assert_eq!(file_hash, str_hash, "hash_file and hash_string must agree");
    }

    #[test]
    fn manifest_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = CacheManifest::default();
        manifest
            .files
            .insert("src/main.synapse".to_owned(), "abc123".to_owned());
        manifest
            .intent
            .insert("src/main.intent".to_owned(), "def456".to_owned());
        manifest.output.rust = "rusthash".to_owned();
        manifest.output.binary = "binhash".to_owned();
        manifest.output.binary_path = "target/demo".to_owned();

        save_manifest(dir.path(), &manifest).unwrap();
        let loaded = load_manifest(dir.path()).expect("manifest should load");

        assert_eq!(
            loaded.files.get("src/main.synapse").unwrap(),
            "abc123",
            "file hash roundtrip"
        );
        assert_eq!(
            loaded.intent.get("src/main.intent").unwrap(),
            "def456",
            "intent hash roundtrip"
        );
        assert_eq!(loaded.output.rust, "rusthash", "output rust hash roundtrip");
        assert_eq!(loaded.output.binary, "binhash", "output binary hash roundtrip");
        assert_eq!(loaded.output.binary_path, "target/demo", "binary path roundtrip");
    }

    #[test]
    fn load_manifest_missing_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_manifest(dir.path()).is_none(), "missing manifest must return None");
    }

    #[test]
    fn clear_cache_removes_directory() {
        let dir = tempfile::tempdir().unwrap();
        let cd = dir.path().join(CACHE_DIR);
        fs::create_dir_all(&cd).unwrap();
        fs::write(cd.join("junk.txt"), "data").unwrap();
        clear_cache(dir.path()).unwrap();
        assert!(!cd.exists(), "cache directory must be removed");
    }

    #[test]
    fn clear_cache_noop_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        clear_cache(dir.path()).unwrap();
    }

    #[test]
    fn is_up_to_date_false_without_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let config: ProjectConfig = toml::from_str(
            r#"
            [project]
            name = "test"
            version = "0.1.0"
            [build]
            entry = "src/main.synapse"
            "#,
        )
        .unwrap();
        assert!(!is_up_to_date(dir.path(), &config), "must be stale without manifest");
    }

    #[test]
    fn is_up_to_date_true_when_hashes_match() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let entry = src_dir.join("main.synapse");
        fs::write(&entry, "function main() -> Int\n  returns 0\n").unwrap();

        let binary = dir.path().join("test");
        fs::write(&binary, "fake binary").unwrap();

        let mut manifest = CacheManifest::default();
        manifest
            .files
            .insert("src/main.synapse".to_owned(), hash_file(&entry).unwrap());
        manifest.output.binary_path = binary.to_string_lossy().into_owned();

        save_manifest(dir.path(), &manifest).unwrap();

        let config: ProjectConfig = toml::from_str(
            r#"
            [project]
            name = "test"
            version = "0.1.0"
            [build]
            entry = "src/main.synapse"
            "#,
        )
        .unwrap();
        assert!(
            is_up_to_date(dir.path(), &config),
            "must be up-to-date when hashes match"
        );
    }

    #[test]
    fn is_up_to_date_false_when_hash_differs() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let entry = src_dir.join("main.synapse");
        fs::write(&entry, "function main() -> Int\n  returns 0\n").unwrap();

        let binary = dir.path().join("test");
        fs::write(&binary, "fake binary").unwrap();

        let mut manifest = CacheManifest::default();
        manifest
            .files
            .insert("src/main.synapse".to_owned(), "stale_hash".to_owned());
        manifest.output.binary_path = binary.to_string_lossy().into_owned();

        save_manifest(dir.path(), &manifest).unwrap();

        let config: ProjectConfig = toml::from_str(
            r#"
            [project]
            name = "test"
            version = "0.1.0"
            [build]
            entry = "src/main.synapse"
            "#,
        )
        .unwrap();
        assert!(!is_up_to_date(dir.path(), &config), "must be stale when hash differs");
    }

    #[test]
    fn is_up_to_date_false_when_binary_missing() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let entry = src_dir.join("main.synapse");
        fs::write(&entry, "function main() -> Int\n  returns 0\n").unwrap();

        let mut manifest = CacheManifest::default();
        manifest
            .files
            .insert("src/main.synapse".to_owned(), hash_file(&entry).unwrap());
        manifest.output.binary_path = dir.path().join("nonexistent").to_string_lossy().into_owned();

        save_manifest(dir.path(), &manifest).unwrap();

        let config: ProjectConfig = toml::from_str(
            r#"
            [project]
            name = "test"
            version = "0.1.0"
            [build]
            entry = "src/main.synapse"
            "#,
        )
        .unwrap();
        assert!(
            !is_up_to_date(dir.path(), &config),
            "must be stale when binary is missing"
        );
    }

    #[test]
    fn expansion_cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let intent_entry = "src/main.intent";
        let intent_hash = hash_string("intent source");
        let synapse_src = "function main() -> Int\n  returns 42\n";

        let mut manifest = CacheManifest::default();
        manifest.intent.insert(intent_entry.to_owned(), intent_hash.clone());
        save_manifest(dir.path(), &manifest).unwrap();
        save_expansion(dir.path(), intent_entry, synapse_src).unwrap();

        let loaded = load_cached_expansion(dir.path(), intent_entry, &intent_hash);
        assert_eq!(loaded.as_deref(), Some(synapse_src), "cached expansion must round-trip");
    }

    #[test]
    fn expansion_cache_miss_on_changed_hash() {
        let dir = tempfile::tempdir().unwrap();
        let intent_entry = "src/main.intent";
        let old_hash = hash_string("old intent");
        let new_hash = hash_string("new intent");
        let synapse_src = "function main() -> Int\n  returns 42\n";

        let mut manifest = CacheManifest::default();
        manifest.intent.insert(intent_entry.to_owned(), old_hash);
        save_manifest(dir.path(), &manifest).unwrap();
        save_expansion(dir.path(), intent_entry, synapse_src).unwrap();

        let loaded = load_cached_expansion(dir.path(), intent_entry, &new_hash);
        assert!(loaded.is_none(), "must miss when intent hash changed");
    }

    #[test]
    fn is_up_to_date_intent_entry() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let entry = src_dir.join("main.intent");
        fs::write(&entry, "intent content").unwrap();

        let binary = dir.path().join("test");
        fs::write(&binary, "fake binary").unwrap();

        let mut manifest = CacheManifest::default();
        manifest
            .intent
            .insert("src/main.intent".to_owned(), hash_file(&entry).unwrap());
        manifest.output.binary_path = binary.to_string_lossy().into_owned();
        save_manifest(dir.path(), &manifest).unwrap();

        let config: ProjectConfig = toml::from_str(
            r#"
            [project]
            name = "test"
            version = "0.1.0"
            [build]
            entry = "src/main.intent"
            "#,
        )
        .unwrap();
        assert!(
            is_up_to_date(dir.path(), &config),
            "must be up-to-date for intent entry"
        );
    }
}
