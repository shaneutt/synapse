# Synapse Programming Language

AI-powered functional programming language.

## Overview

Synapse is a functional programming language inspired by
Erlang which transpiles to Rust. It follows a two-layer
pipeline approach which uses Large Language Models (LLMs)
to transpile a high level `intent` language into the functional
`synapse` language.

Synapse is experimental, the purpose is to validate whether
a high-level programming language integrated with AI can be
more viable than having an LLM directly create lower-level code.
The approach is to provide a language that's extremely restrictive
in order to achieve closer to deterministic results from the LLM.

> **Warning**: In its current state Synapse does have significant limitations.
> If you write an intent program that doesn't compile or work properly, please
> put in an issue with as much detail as possible.

with Synapse, the following is a complete program:

```yaml
application weather:
  args:
    positional: city String
  intent: fetch today's weather report for the given city from wttr.in and print it to stdout
```

> **Note**: Right now, a local `claude` command line program is required.
> In future iterations, more LLM sources will be made available. The long
> term plan is to provide a specific, local, open/free model that will
> be packaged with the compiler.

The Synapse build tool `axon` can utilize an LLM on your system to
emit the `.intent` application as a `.synapse` program. Then the Synapse
compiler `cortex` transpiles that to Rust code (there is no AI involved
at this level).

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
