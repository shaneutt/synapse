# Synapse Examples

Hand-written `.synapse` programs demonstrating language
features.

| Program | Demonstrates | Output |
|---------|-------------|--------|
| `factorial.synapse` | Recursion, match, arithmetic | `3628800` |
| `fibonacci.synapse` | Double recursion | `55` |
| `gcd.synapse` | Euclidean algorithm, boolean match | `6` |
| `collatz.synapse` | Nested match, conditionals | `111` |
| `power.synapse` | Exponentiation, two parameters | `1024` |
| `list_reverse.synapse` | Cons lists, accumulator pattern | `[5, 4, 3, 2, 1]` |
| `weather.synapse` | Built-ins, string concat, HTTP | (fetches live data) |
| `weather_demo.synapse` | Same as weather, hardcoded city | (fetches live data) |

## Compiling and Running

Build cortex first:

```console
cargo build --release
```

Compile and run any example:

```console
./target/release/cortex compile examples/synapse/factorial.synapse -o factorial
./factorial
3628800
```

## Viewing Emitted Rust

```console
./target/release/cortex emit examples/synapse/factorial.synapse
```

## Type-checking Only

```console
./target/release/cortex check examples/synapse/factorial.synapse
```

## Language Reference

See [`docs/language.md`](../../docs/language.md) for
full syntax.
