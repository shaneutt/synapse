# Synapse Intent Language Reference

## Overview

The intent layer sits above the Synapse language. It
provides a structured way to describe program logic
that expands into valid `.synapse` code via built-in
templates or LLM generation.

```console
.intent -> [expand] -> .synapse -> [cortex] -> .rs -> [rustc] -> binary
```

## Applications

The highest level. Describes a runnable program with
CLI arguments, environment variables, and a purpose.

```yaml
application wordcount:
  args:
    flag: --verbose
    positional: file String
  environment:
    - String locale from LANG default en_US
  intent: read the file, count words, print the count
```

### Args

| Kind | Syntax | Runtime |
| ------ | -------- | --------- |
| Verb | `verb: action` | First positional, string |
| Boolean flag | `flag: --verbose` | Absent = false |
| Typed flag | `flag: --port Int default 8080` | Required if no default |
| Positional | `positional: file String` | Required, ordered |

### Environment

```console
- <Type> <binding> from <ENV_VAR> [default <value>]
```

Missing required env vars exit with an error.

### Output

All applications produce stdout, stderr, and an exit
code. No `output` field is needed.

## Modules

A module groups related capabilities and pipelines.

```yaml
module math:
  capability factorial:
    input: Int n
    output: Int
    intent: compute factorial using recursion

  pipeline double_then_sum:
    double_all(xs) -> list_sum(doubled)
```

## Capabilities

A capability declares a single unit of work with typed
inputs, an output, and an intent phrase.

```yaml
capability <name>:
  input: <Type> <param> [, <Type> <param>]*
  output: <Type>
  intent: <verb phrase>
```

The intent phrase is matched against built-in templates
or expanded by an LLM into `.synapse` code.

### Built-in Templates

| Keyword in Intent | Pattern |
| ------------------- | --------- |
| `factorial` | Recursive match on 0/n |
| `fibonacci`, `fib` | Double recursive |
| `sum` | Recursive list fold |
| `length`, `count` | Recursive list count |
| `reverse` | Accumulator pattern |
| `map` | Recursive list transform |
| `filter` | Recursive list filter |
| `gcd`, `greatest common` | Euclidean algorithm |
| `power`, `exponent` | Recursive exponentiation |
| `max`, `maximum` | Helper-based list scan |
| `min`, `minimum` | Helper-based list scan |

## Pipelines

A pipeline chains capability calls sequentially.

```console
pipeline process:
  step_one(x) -> step_two(result) -> step_three(final)
```

Each step's output feeds as input to the next. Expands
to a Synapse function with `value` bindings.

## Expansion

```console
axon expand file.intent          # expand to .synapse
axon expand file.intent --no-llm # templates only
```

The expander tries templates first. If no template
matches and `--no-llm` is not set, the `claude` CLI is
called to generate the function body.

## LLM Expansion

For capabilities that do not match any built-in
template, the expander calls the `claude` CLI with a
structured prompt containing:

- The full Synapse language specification
- The capability's name, inputs, output, and intent
- Rules for valid Synapse syntax

The LLM response is validated through the cortex
compiler (lex, parse, type-check) before being
accepted. Invalid output is retried once.

### Application Expansion

Applications are expanded directly to `.synapse` code
in a single LLM call. The prompt includes the full language
spec, built-in signatures, the application's declared
args/env/intent, and complete example programs. The
LLM produces a ready-to-compile `.synapse` program
that is validated through cortex before being accepted.

Recommended LLM: [Claude](https://claude.ai) via the
Anthropic API or `claude` CLI. For local use, Llama 3.1
with grammar-constrained decoding (Outlines or
llama.cpp GBNF).
