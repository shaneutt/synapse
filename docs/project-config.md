# Project Configuration

Synapse projects are configured with a `synapse.toml`
file in the project root. Axon reads this file to
locate the entry point and name the output binary.

## Format

```toml
[project]
name = "my-program"
version = "0.1.0"

[build]
entry = "src/main.synapse"
```

## Sections

### `[project]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Project name; also used as the output binary name |
| `version` | string | yes | Semantic version |

### `[build]`

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `entry` | string | yes | Path to the entry file, relative to the project root |

The entry file can be either `.synapse` or `.intent`.
When the entry is a `.intent` file, axon expands it to
`.synapse` before compilation.

## Examples

A standard Synapse project:

```toml
[project]
name = "factorial"
version = "1.0.0"

[build]
entry = "src/main.synapse"
```

An intent-driven project (expanded before compilation):

```toml
[project]
name = "math-tools"
version = "0.1.0"

[build]
entry = "src/main.intent"
```

## Scaffolding

`axon new <name>` generates a `synapse.toml` with
sensible defaults:

```console
axon new hello
```

Produces:

```toml
[project]
name = "hello"
version = "0.1.0"

[build]
entry = "src/main.synapse"
```
