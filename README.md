# Synapse Programming Language

AI-powered functional programming language.

## Overview

Synapse is a functional programming language inspired by
Erlang which transpiles to Rust. It follows a two-layer
pipeline approach which uses Large Language Models (LLMs)
to transpile a high level `intent` language into the
functional `synapse` language.

Synapse is experimental; the purpose is to validate
whether a high-level programming language integrated
with AI can be more viable than having an LLM directly
create lower-level code. The approach is to provide a
language that's extremely restrictive in order to
achieve closer to deterministic results from the LLM.

> **Warning**: In its current state Synapse does have
> significant limitations. If you write an intent
> program that doesn't compile or work properly, please
> put in an issue with as much detail as possible.

With Synapse, the following is a complete program:

```
application weather:
  args:
    positional: city String
  capabilities:
    builtins: import
  intent:
    description: fetch today's weather report for the given city from wttr.in and print it to stdout
    properties:
      - uses builtins to fetch weather data via http_get
      - uses builtins to build the URL via concat
      - uses builtins to print the result
```

> **Note**: Right now, a local `claude` command line
> program is required. In future iterations, more LLM
> sources will be made available. The long term plan is
> to provide a specific, local, open/free model that
> will be packaged with the compiler.

The build tool `axon` uses an LLM to expand the
`.intent` application directly into a `.synapse`
program. The compiler `cortex` then transpiles that
to Rust (no AI involved at this level).

Every dependency must be explicitly declared as a
capability. The LLM can only use what's declared;
undeclared imports are rejected with suggestions.

## Quick Start

Build the toolchain from source:

```console
cargo build --workspace --release
```

Create, build, and run a project:

```console
axon new myproject
cd myproject
axon build
axon run
```

## The Synapse Language

```synapse
import builtins

pub function factorial(Int n) -> Int
  returns match n
    when 0 -> 1
    otherwise -> n * factorial(n - 1)

pub function main() -> Int
  value result = factorial(10)
  value _ = builtins.print(result)
  returns 0
```

Types: `Int`, `Bool`, `String`, `List<T>`. Pattern
matching with `match`/`when`/`otherwise`. Cons-lists
with `Cons`/`Nil`. Built-in functions require explicit
`import builtins`.

## Capabilities

Applications declare their dependencies explicitly:

| Kind | Declaration | Meaning |
|------|-------------|---------|
| Import | `import` | Resolved by name (builtins, `.synapse`/`.rs` in `src/`) |
| Import path | `import <path>` | Explicit `.synapse` or `.rs` file |
| Rust crate | `import rust crate [version]` | Cargo dependency |
| New module | `new module` | LLM generates the code |

## Toolchain

| Tool | Role | Analogy |
|------|------|---------|
| **cortex** | Compiler (lex, parse, type-check, emit Rust) | `rustc` |
| **axon** | Build tool (project management, LLM expansion) | `cargo` |

Key commands:

```console
axon build              # compile the project
axon run                # build and run
axon check              # type-check only
axon expand file.intent # expand .intent to .synapse
axon new <name>         # scaffold a new project
```

## Documentation

- [`docs/language.md`](docs/language.md): Synapse
  language reference
- [`docs/intent.md`](docs/intent.md): intent layer
  reference
- [`docs/guide.md`](docs/guide.md): step-by-step
  weather app walkthrough
- [`docs/architecture.md`](docs/architecture.md):
  crate structure and pipeline
- [`docs/builtins.md`](docs/builtins.md): built-in
  function reference

## License

MIT
