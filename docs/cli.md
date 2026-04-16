# CLI Reference

Synapse has two CLI tools: **cortex** (the compiler)
and **axon** (the build tool). Axon is the primary
interface for project workflows; cortex handles
single-file compilation.

## Cortex

The Synapse compiler. Operates on individual
`.synapse` files.

### `cortex check <file>`

Parse and type-check a `.synapse` file without
emitting code. Prints `ok` on success or a diagnostic
on failure.

```console
cortex check examples/synapse/factorial.synapse
```

### `cortex emit <file>`

Run the full compiler pipeline and print the emitted
Rust source to stdout. Useful for inspecting
generated code.

```console
cortex emit examples/synapse/factorial.synapse
```

### `cortex compile <file> [-o <output>]`

Compile a `.synapse` file to a native binary. Emits
Rust source, then invokes `rustc`.

| Flag | Default | Description |
|------|---------|-------------|
| `-o`, `--output` | `output` | Path for the compiled binary |

```console
cortex compile examples/synapse/factorial.synapse -o factorial
./factorial
```

## Axon

The Synapse build tool. Manages projects defined by a
`synapse.toml` file. Run axon from the project root
(the directory containing `synapse.toml`).

### `axon new <name>`

Scaffold a new Synapse project. Creates a directory
with `synapse.toml` and `src/main.synapse`.

```console
axon new my-project
cd my-project
```

Generated structure:

```
my-project/
  synapse.toml
  src/
    main.synapse
```

### `axon build [--no-llm] [--force]`

Compile the project to a binary. The output binary is
placed in the project root, named after the project.

| Flag | Description |
|------|-------------|
| `--no-llm` | Disable LLM fallback for intent expansion (templates only) |
| `--force` | Force a full rebuild, ignoring the cache |

```console
axon build
axon build --no-llm
axon build --force
```

### `axon check [--no-llm]`

Type-check the project without compiling. Expands
`.intent` files if the entry file is `.intent`.

| Flag | Description |
|------|-------------|
| `--no-llm` | Disable LLM fallback for intent expansion |

```console
axon check
```

### `axon run [--no-llm] [--force]`

Build and run the project in one step. Equivalent to
`axon build` followed by executing the binary.

| Flag | Description |
|------|-------------|
| `--no-llm` | Disable LLM fallback for intent expansion |
| `--force` | Force a full rebuild, ignoring the cache |

```console
axon run
```

### `axon expand <file> [--no-llm]`

Expand a `.intent` file to `.synapse` source and print
it to stdout. Does not compile the result.

| Flag | Description |
|------|-------------|
| `--no-llm` | Disable LLM fallback (templates only) |

```console
axon expand examples/intent/math.intent
axon expand examples/intent/math.intent --no-llm
```

To compile the expanded output:

```console
axon expand src/main.intent > main.synapse
cortex compile main.synapse -o main
```

## Build Cache

Axon maintains a build cache in `.synapse-cache/`
inside the project directory. The cache stores:

- SHA-256 hashes of source files and output artifacts
  in `manifest.toml`
- Cached intent expansions in `expanded/`

A build is skipped when the source hash matches the
manifest and the binary exists on disk. Use
`--force` to bypass the cache.
