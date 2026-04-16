# Architecture

## Overview

Synapse has three crates in a Cargo workspace:

```console
cortex/    compiler library + CLI binary
intent/    intent language library
axon/      build tool binary
```

Cortex is the compiler (like `rustc`). Axon is the
build tool (like `cargo`). Intent is the expansion
layer that converts `.intent` files into `.synapse`
code.

## Compilation Pipeline

A `.synapse` file passes through four stages:

```console
source -> [lex] -> tokens -> [parse] -> AST
  -> [check] -> typed AST -> [emit] -> Rust source
```

When the entry file is `.intent`, a pre-stage expands
it first:

```console
.intent -> [lex] -> [parse] -> [expand] -> .synapse
  -> cortex pipeline -> .rs -> [rustc] -> binary
```

## Intent (Expansion Layer)

The intent crate has its own lex/parse pipeline for
`.intent` files, plus expansion logic:

| Module | Purpose |
| -------- | --------- |
| `lexer` | Tokenizes `.intent` source |
| `parser` | Parses tokens into `IntentProgram` |
| `ast` | Intent AST types (Application, Module, Capability, Pipeline) |
| `validator` | Structural validation (duplicates, missing intents, type refs) |
| `templates` | 11 built-in code generation templates |
| `expander` | Orchestrates template + LLM expansion |
| `llm` | Calls `claude` CLI for novel patterns |
| `prompt` | Builds structured prompts for the LLM |
| `error` | `IntentError` variants |

Expansion order:

1. Applications are expanded directly to `.synapse`
   in a single LLM call (validated through cortex)
2. Modules are validated, then each capability is
   matched against templates
3. Unmatched capabilities fall back to LLM (if enabled)
4. Pipelines are expanded into chained function calls
5. Module results are verified through cortex (lex,
   parse, type-check)

## Cortex (Compiler)

The compiler library exports four public modules,
each responsible for one pipeline stage:

| Module | Entry point | Input | Output |
| -------- | ------------- | ------- | -------- |
| `lexer` | `lex(&str)` | source text | `Vec<Token>` |
| `parser` | `parse(&[Token])` | token stream | `Program` (untyped AST) |
| `checker` | `check(&Program)` | untyped AST | `TypedProgram` |
| `emitter` | `emit(&TypedProgram)` | typed AST | Rust source (`String`) |

Supporting modules:

| Module | Purpose |
| -------- | --------- |
| `token` | `Token`, `TokenKind`, `Span` types |
| `ast` | Untyped AST node types |
| `typed_ast` | Typed AST node types |
| `error` | `LexError`, `ParseError`, `TypeError` |

The cortex binary (`main.rs`) is a thin CLI wrapper
that reads a file, calls the library functions, and
optionally invokes `rustc`.

## Axon (Build Tool)

Axon drives the full build process:

| Module | Purpose |
| -------- | --------- |
| `project` | Parses `synapse.toml` configuration |
| `build` | Build orchestration (intent expansion, cortex pipeline, rustc invocation) |
| `cache` | SHA-256 based build cache with manifest tracking |

Build flow:

1. Load `synapse.toml`
2. Check cache (skip if up-to-date)
3. Resolve entry file (expand `.intent` if needed)
4. Run cortex pipeline (lex, parse, check, emit)
5. Write emitted Rust to a temp file
6. Invoke `rustc` to produce the binary
7. Update the cache manifest

## Crate Dependencies

```console
axon  -->  cortex  (compiler library)
axon  -->  intent  (intent expansion)
intent -->  cortex  (validates expanded code)
```

Axon depends on both cortex and intent. Intent depends
on cortex to validate expanded `.synapse` code through
the full compiler pipeline.
