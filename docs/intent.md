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
CLI arguments, capabilities, environment variables,
and a structured intent.

```yaml
application wordcount:
  args:
    flag: --verbose
    positional: file String
  capabilities:
    builtins: import
  environment:
    - String locale from LANG default en_US
  intent:
    description: read the file, count words, print the count
    properties:
      - uses builtins to print the word count
```

### Args

| Kind | Syntax | Runtime |
| ------ | -------- | --------- |
| Verb | `verb: action` | First positional, string |
| Boolean flag | `flag: --verbose` | Absent = false |
| Typed flag | `flag: --port Int default 8080` | Required if no default |
| Positional | `positional: file String` | Required, ordered |

### Capabilities

Every dependency an application uses must be declared.

| Declaration | Meaning |
| ----------- | ------- |
| `import` | Resolved by name: builtins, `.synapse` or `.rs` in `src/` |
| `import <path>` | Explicit path to `.synapse` or `.rs` file |
| `import rust crate [version] [path <p>] [git <url>]` | Cargo dependency |
| `new module` | LLM generates a new `.synapse` module |
| `new crate` | LLM generates a multi-module crate |

### Structured Intent

The `intent:` block contains a `description` and
`properties` that reference declared capabilities.

```yaml
intent:
  description: fetch weather for a city and print it
  properties:
    - uses builtins to print output to stdout
    - uses http to fetch data from wttr.in
```

Validation rules:
1. Description must be non-empty.
2. At least one property required.
3. Every `uses <name>` must reference a declared
   capability.
4. Every declared capability should be referenced by
   at least one property (warning if unused).
5. No duplicate capability names.

For backward compatibility, `intent: <free text>` on
a single line is still accepted for applications
without capabilities.

### Importing Synapse Modules

Use `import <path>` to declare a pre-written `.synapse`
file as a dependency, or bare `import` to resolve by
name (looks in `src/`). The build tool compiles it
through cortex, extracts its public API, and makes it
available to the LLM and type checker.

```yaml
application math_demo:
  capabilities:
    builtins: import
    math: import lib/math.synapse
  intent:
    description: compute factorial of 10 and print it
    properties:
      - uses math to compute the factorial
      - uses builtins to print the result
```

The path is relative to the project directory. The
module name (here `math`) becomes the qualified prefix
in generated code: `math.factorial(10)`.

The module file must use `pub` on any functions the
application needs to call:

```synapse
pub function factorial(Int n) -> Int
  returns match n
    when 0 -> 1
    otherwise -> n * factorial(n - 1)
```

Private functions (without `pub`) are internal to the
module and invisible to importers.

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
