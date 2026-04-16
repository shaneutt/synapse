#![deny(unsafe_code)]
//! Axon CLI: the Synapse build tool binary.

// These modules are also `pub` in lib.rs for external consumers.
// The binary re-declares them privately; suppress unreachable_pub
// because the items legitimately need to be `pub` for the library.
/// Build pipeline.
#[allow(unreachable_pub)]
mod build;
/// Build cache.
#[allow(unreachable_pub)]
mod cache;
/// Project configuration.
#[allow(unreachable_pub)]
mod project;

use std::{fs, path::PathBuf, process::Command};

use clap::{Parser, Subcommand};

// ---------------------------------------------------------------------------
// CLI Types
// ---------------------------------------------------------------------------

/// The Synapse build tool.
#[derive(Parser)]
#[command(name = "axon", version, about = "Synapse build tool")]
struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Cmd,
}

/// Axon subcommands.
#[derive(Subcommand)]
enum Cmd {
    /// Compile the project to a binary.
    Build {
        /// Disable LLM fallback for intent expansion.
        #[arg(long)]
        no_llm: bool,
        /// Force a full rebuild, ignoring the cache.
        #[arg(long)]
        force: bool,
    },
    /// Type-check without compiling.
    Check {
        /// Disable LLM fallback for intent expansion.
        #[arg(long)]
        no_llm: bool,
    },
    /// Expand a `.intent` file to `.synapse` source.
    Expand {
        /// Path to the `.intent` file.
        file: PathBuf,
        /// Disable LLM fallback (template-only).
        #[arg(long)]
        no_llm: bool,
    },
    /// Scaffold a new Synapse project.
    New {
        /// The project name.
        name: String,
    },
    /// Build and run the project.
    Run {
        /// Disable LLM fallback for intent expansion.
        #[arg(long)]
        no_llm: bool,
        /// Force a full rebuild, ignoring the cache.
        #[arg(long)]
        force: bool,
    },
}

// ---------------------------------------------------------------------------
// Entrypoint
// ---------------------------------------------------------------------------

#[allow(clippy::print_stderr)]
fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli.command) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Subcommand Dispatch
// ---------------------------------------------------------------------------

/// Dispatches to the appropriate subcommand.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn run(cmd: Cmd) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        Cmd::Build { no_llm, force } => {
            let dir = std::env::current_dir()?;
            let config = project::load_config(&dir)?;
            build::build(&dir, &config, !no_llm, force)
        },
        Cmd::Check { no_llm } => {
            let dir = std::env::current_dir()?;
            let config = project::load_config(&dir)?;
            build::check_only(&dir, &config, !no_llm)
        },
        Cmd::Expand { file, no_llm } => {
            let source = fs::read_to_string(&file)?;
            let synapse = build::expand_intent_source(&source, !no_llm)?;
            print!("{synapse}");
            Ok(())
        },
        Cmd::New { name } => scaffold(&name),
        Cmd::Run { no_llm, force } => {
            let dir = std::env::current_dir()?;
            let config = project::load_config(&dir)?;
            build::build(&dir, &config, !no_llm, force)?;

            let bin = build::binary_path(&dir, &config.project.name);
            let status = Command::new(&bin).status()?;
            std::process::exit(status.code().unwrap_or(1));
        },
    }
}

// ---------------------------------------------------------------------------
// Project Scaffolding
// ---------------------------------------------------------------------------

/// Creates a new Synapse project directory.
#[allow(clippy::print_stderr)]
fn scaffold(name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(name);
    fs::create_dir_all(root.join("src"))?;

    fs::write(
        root.join("synapse.toml"),
        format!(
            "[project]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n\
             [build]\nentry = \"src/main.synapse\"\n"
        ),
    )?;

    fs::write(root.join("src/main.synapse"), "function main() -> Int\n  returns 0\n")?;

    eprintln!("created project: {name}");
    Ok(())
}
