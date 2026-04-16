#![deny(unsafe_code)]
//! Cortex CLI: the Synapse compiler binary.

use std::{fs, path::PathBuf, process::Command};

use clap::{Parser, Subcommand};

// ---------------------------------------------------------------------------
// CLI Types
// ---------------------------------------------------------------------------

/// The Synapse compiler.
#[derive(Parser)]
#[command(name = "cortex", version, about = "Synapse compiler")]
struct Cli {
    /// The subcommand to execute.
    #[command(subcommand)]
    command: Cmd,
}

/// Cortex subcommands.
#[derive(Subcommand)]
enum Cmd {
    /// Parse and type-check a .synapse file.
    Check {
        /// Path to the `.synapse` file.
        file: PathBuf,
    },
    /// Emit Rust source to stdout.
    Emit {
        /// Path to the `.synapse` file.
        file: PathBuf,
    },
    /// Compile a .synapse file to a binary.
    Compile {
        /// Path to the `.synapse` file.
        file: PathBuf,
        /// Output binary path.
        #[arg(short, long, default_value = "output")]
        output: PathBuf,
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
        Cmd::Check { file } => {
            let source = fs::read_to_string(&file)?;
            let tokens = cortex::lexer::lex(&source)?;
            let ast = cortex::parser::parse(&tokens)?;
            cortex::checker::check(&ast)?;
            eprintln!("ok");
            Ok(())
        },
        Cmd::Emit { file } => {
            let source = fs::read_to_string(&file)?;
            let rust = compile_to_rust(&source)?;
            print!("{rust}");
            Ok(())
        },
        Cmd::Compile { file, output } => {
            let source = fs::read_to_string(&file)?;
            let rust = compile_to_rust(&source)?;

            let tmp = std::env::temp_dir().join("synapse_output.rs");
            fs::write(&tmp, &rust)?;

            let status = Command::new("rustc").arg(&tmp).arg("-o").arg(&output).status()?;

            if !status.success() {
                return Err("rustc compilation failed".into());
            }
            Ok(())
        },
    }
}

// ---------------------------------------------------------------------------
// Compilation Pipeline
// ---------------------------------------------------------------------------

/// Full pipeline: source -> Rust code.
fn compile_to_rust(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    let tokens = cortex::lexer::lex(source)?;
    let ast = cortex::parser::parse(&tokens)?;
    let typed = cortex::checker::check(&ast)?;
    Ok(cortex::emitter::emit(&typed))
}
