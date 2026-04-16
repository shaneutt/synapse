//! Integration tests for the intent expansion pipeline.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Test Utilities
// ---------------------------------------------------------------------------

/// Returns the workspace root.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_owned()
}

/// Reads an intent file, expands it, and verifies the result compiles.
fn expand_and_verify(rel_path: &str) -> String {
    let source_path = workspace_root().join(rel_path);
    let source =
        std::fs::read_to_string(&source_path).unwrap_or_else(|e| panic!("cannot read {}: {e}", source_path.display()));

    let tokens = intent::lexer::lex(&source).unwrap_or_else(|e| panic!("lex error: {e}"));
    let program = intent::parser::parse(&tokens).unwrap_or_else(|e| panic!("parse error: {e}"));
    let synapse = intent::expander::expand(&program).unwrap_or_else(|e| panic!("expand error: {e}"));

    let cortex_tokens =
        cortex::lexer::lex(&synapse).unwrap_or_else(|e| panic!("cortex lex error: {e}\nCode:\n{synapse}"));
    let ast =
        cortex::parser::parse(&cortex_tokens).unwrap_or_else(|e| panic!("cortex parse error: {e}\nCode:\n{synapse}"));
    cortex::checker::check(&ast).unwrap_or_else(|e| panic!("cortex type error: {e}\nCode:\n{synapse}"));

    synapse
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn math_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/math.intent");
    assert!(synapse.contains("function factorial"), "has factorial: {synapse}");
    assert!(synapse.contains("function fibonacci"), "has fibonacci: {synapse}");
}

#[test]
fn lists_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/lists.intent");
    assert!(synapse.contains("function list_sum"), "has list_sum: {synapse}");
    assert!(synapse.contains("function list_length"), "has list_length: {synapse}");
    assert!(synapse.contains("function reverse"), "has reverse: {synapse}");
}

#[test]
fn algorithms_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/algorithms.intent");
    assert!(synapse.contains("function gcd"), "has gcd: {synapse}");
    assert!(synapse.contains("function power"), "has power: {synapse}");
}

#[test]
fn statistics_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/statistics.intent");
    assert!(synapse.contains("function list_sum"), "has sum: {synapse}");
    assert!(synapse.contains("function list_length"), "has length: {synapse}");
    assert!(synapse.contains("function find_max"), "has max: {synapse}");
    assert!(synapse.contains("function find_min"), "has min: {synapse}");
}

#[test]
fn list_processing_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/list_processing.intent");
    assert!(synapse.contains("function double_all"), "has double_all: {synapse}");
    assert!(
        synapse.contains("function keep_positive"),
        "has keep_positive: {synapse}"
    );
    assert!(synapse.contains("function reverse"), "has reverse: {synapse}");
    assert!(
        synapse.contains("function double_then_count"),
        "has pipeline: {synapse}"
    );
}

#[test]
fn number_theory_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/number_theory.intent");
    assert!(synapse.contains("function gcd"), "has gcd: {synapse}");
    assert!(synapse.contains("function power"), "has power: {synapse}");
    assert!(synapse.contains("function factorial"), "has factorial: {synapse}");
    assert!(synapse.contains("function fibonacci"), "has fibonacci: {synapse}");
}

#[test]
fn data_pipeline_intent_expands() {
    let synapse = expand_and_verify("examples/intent/modules/data_pipeline.intent");
    assert!(synapse.contains("function list_sum"), "has sum: {synapse}");
    assert!(synapse.contains("function double_all"), "has double: {synapse}");
    assert!(synapse.contains("function keep_positive"), "has filter: {synapse}");
    assert!(synapse.contains("function reverse"), "has reverse: {synapse}");
    assert!(synapse.contains("function find_max"), "has max: {synapse}");
    assert!(synapse.contains("function find_min"), "has min: {synapse}");
    assert!(
        synapse.contains("function filter_double_sum"),
        "has pipeline 1: {synapse}"
    );
    assert!(
        synapse.contains("function filter_then_reverse"),
        "has pipeline 2: {synapse}"
    );
}

#[test]
fn deterministic_expansion() {
    let synapse1 = expand_and_verify("examples/intent/modules/math.intent");
    let synapse2 = expand_and_verify("examples/intent/modules/math.intent");
    assert_eq!(synapse1, synapse2, "expansion must be deterministic");
}

#[test]
fn factorial_output_matches_known_pattern() {
    let synapse = expand_and_verify("examples/intent/modules/math.intent");
    assert!(synapse.contains("when 0 -> 1"), "factorial base case: {synapse}");
    assert!(
        synapse.contains("n * factorial(n - 1)"),
        "factorial recursive: {synapse}"
    );
}

#[test]
fn reverse_uses_accumulator() {
    let synapse = expand_and_verify("examples/intent/modules/lists.intent");
    assert!(synapse.contains("reverse_helper"), "reverse uses helper: {synapse}");
    assert!(synapse.contains("Cons(x, acc)"), "reverse accumulator: {synapse}");
}

#[test]
fn pipeline_chains_steps() {
    let synapse = expand_and_verify("examples/intent/modules/data_pipeline.intent");
    assert!(
        synapse.contains("value cleaned = keep_positive(xs)"),
        "pipeline step 1: {synapse}"
    );
    assert!(
        synapse.contains("returns list_sum(doubled)"),
        "pipeline final step: {synapse}"
    );
}
