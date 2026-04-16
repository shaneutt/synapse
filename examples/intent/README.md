# Intent Examples

`.intent` files describe program logic at a higher level
than `.synapse`. The intent layer expands structured
capability descriptions into valid Synapse code, either
via built-in templates (deterministic) or via
grammar-constrained LLM expansion (for novel patterns).

## Examples

| File | Capabilities | Pipelines | Description |
|------|-------------|-----------|-------------|
| `math.intent` | factorial, fibonacci | none | Basic recursive arithmetic |
| `lists.intent` | sum, length, reverse | none | Core list operations |
| `algorithms.intent` | gcd, power | none | Classic algorithms |
| `statistics.intent` | sum, length, max, min | none | Aggregate list statistics |
| `number_theory.intent` | gcd, power, factorial, fibonacci | none | Combined numeric algorithms |
| `list_processing.intent` | double, filter, reverse, length | double_then_count | List transforms with pipeline |
| `data_pipeline.intent` | sum, length, double, filter, reverse, max, min | filter_double_sum, filter_then_reverse | Multi-stage data processing |

## Expanding an Intent File

Build axon first:

```console
cargo build --release
```

Expand to `.synapse` source:

```console
./target/release/axon expand examples/intent/data_pipeline.intent
```

This prints the generated Synapse code to stdout. Redirect
to a file to compile it:

```console
./target/release/axon expand examples/intent/math.intent > math.synapse
./target/release/cortex compile math.synapse -o math
./math
```

## Template Expansion (Deterministic)

The expander recognizes 11 common patterns by matching
keywords in the `intent:` phrase:

| Keyword in Intent | Template |
|-------------------|----------|
| `factorial` | Recursive `match 0 -> 1, n -> n * f(n-1)` |
| `fibonacci`, `fib` | Double recursive with base cases 0, 1 |
| `sum` | Recursive list fold adding elements |
| `length`, `count` | Recursive list fold counting elements |
| `reverse` | Accumulator-based list reversal |
| `map` | Recursive list map (doubles each element) |
| `filter` | Recursive list filter (keeps positives) |
| `gcd`, `greatest common` | Euclidean algorithm with modulo |
| `power`, `exponent` | Recursive exponentiation |
| `max`, `maximum` | Helper-based list scan for largest |
| `min`, `minimum` | Helper-based list scan for smallest |

Template expansion is fully deterministic: same input
always produces the same output.

## LLM Expansion (For Novel Patterns)

When an intent phrase does not match any built-in
template, an LLM can generate the function body. This
requires grammar-constrained decoding so the output is
guaranteed to be syntactically valid `.synapse`.

**Recommended LLM:** [Claude](https://claude.ai)
(Sonnet or Opus) via the Anthropic API. Claude produces
high-quality code with strong instruction following,
which pairs well with the structured prompt format. For
local/offline use, Llama 3.1 (8B or 70B) with
[Outlines](https://github.com/dottxt-ai/outlines) or
[llama.cpp](https://github.com/ggml-org/llama.cpp) GBNF
grammar constraints works well.

LLM expansion is implemented via the `claude` CLI. When
no template matches and `--no-llm` is not set, the
expander calls Claude to generate the function body.
The LLM output is validated through cortex before being
accepted, with one automatic retry on failure.

## Syntax Reference

```
module <name>:
  capability <name>:
    input: <Type> <param> [, <Type> <param>]*
    output: <Type>
    intent: <verb phrase describing what to compute>

  pipeline <name>:
    <step>(<args>) -> <step>(<args>) [-> ...]
```

Types: `Int`, `Bool`, `String`, `List<Int>`,
`List<Bool>`, `List<String>`

See [`docs/intent.md`](../../docs/intent.md) for the
full intent specification and
[`docs/language.md`](../../docs/language.md) for the
Synapse language reference.
