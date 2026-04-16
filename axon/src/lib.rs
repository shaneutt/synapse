#![deny(unsafe_code)]
//! Axon: the Synapse build tool library.
//!
//! Provides project configuration, incremental build caching,
//! and the build pipeline that drives cortex.

/// Build pipeline: compiles Synapse projects to binaries.
pub mod build;
/// Incremental build cache with SHA-256 hash tracking.
pub mod cache;
/// Capability API resolution for LLM prompts.
pub mod capabilities;
/// Project configuration loaded from `synapse.toml`.
pub mod project;
