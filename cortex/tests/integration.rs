//! Integration tests for the cortex compiler pipeline.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

static COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Test Utilities
// ---------------------------------------------------------------------------

/// Returns the workspace root (one level up from cortex's CARGO_MANIFEST_DIR).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_owned()
}

/// Compiles a `.synapse` file to a binary via cortex, runs it,
/// and returns stdout.
fn compile_and_run(rel_path: &str) -> String {
    let source_path = workspace_root().join(rel_path);
    let source =
        fs::read_to_string(&source_path).unwrap_or_else(|e| panic!("cannot read {}: {e}", source_path.display()));

    let tokens = cortex::lexer::lex(&source).unwrap_or_else(|e| panic!("lex error: {e}"));
    let ast = cortex::parser::parse(&tokens).unwrap_or_else(|e| panic!("parse error: {e}"));
    let typed = cortex::checker::check(&ast).unwrap_or_else(|e| panic!("type error: {e}"));
    let rust = cortex::emitter::emit(&typed);

    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_rs = std::env::temp_dir().join(format!("synapse_test_{id}.rs"));
    let tmp_bin = std::env::temp_dir().join(format!("synapse_test_{id}"));
    fs::write(&tmp_rs, &rust).unwrap();

    let status = Command::new("rustc")
        .arg(&tmp_rs)
        .arg("-o")
        .arg(&tmp_bin)
        .status()
        .expect("failed to run rustc");
    assert!(status.success(), "rustc failed for {rel_path}.\nEmitted Rust:\n{rust}");

    let output = Command::new(&tmp_bin).output().expect("failed to run binary");
    assert!(output.status.success(), "binary failed for {rel_path}");

    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn factorial() {
    let result = compile_and_run("tests/programs/factorial.synapse");
    assert_eq!(result, "3628800", "10! = 3628800");
}

#[test]
fn fibonacci() {
    let result = compile_and_run("tests/programs/fibonacci.synapse");
    assert_eq!(result, "55", "fib(10) = 55");
}

#[test]
fn list_length() {
    let result = compile_and_run("tests/programs/list_length.synapse");
    assert_eq!(result, "3", "length([1,2,3]) = 3");
}

#[test]
fn list_sum() {
    let result = compile_and_run("tests/programs/list_sum.synapse");
    assert_eq!(result, "15", "sum([1,2,3,4,5]) = 15");
}

#[test]
fn cortex_cli_check() {
    let bin = Path::new(env!("CARGO_BIN_EXE_cortex"));
    let prog = workspace_root().join("tests/programs/factorial.synapse");
    let status = Command::new(bin)
        .args(["check", &prog.to_string_lossy()])
        .status()
        .expect("failed to run cortex");
    assert!(status.success(), "cortex check should pass");
}

#[test]
fn cortex_cli_emit() {
    let bin = Path::new(env!("CARGO_BIN_EXE_cortex"));
    let prog = workspace_root().join("tests/programs/factorial.synapse");
    let output = Command::new(bin)
        .args(["emit", &prog.to_string_lossy()])
        .output()
        .expect("failed to run cortex");
    assert!(output.status.success(), "cortex emit should pass");
    let rust = String::from_utf8(output.stdout).unwrap();
    assert!(rust.contains("fn factorial"), "should contain function");
}
