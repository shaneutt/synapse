# Built-in Functions

Synapse provides three built-in functions for IO and
string operations. These are recognized by the
compiler and emitted as inline Rust code (not as
Synapse function definitions).

## `print`

Print a string to stdout.

| | |
|---|---|
| **Signature** | `(String) -> Int` |
| **Returns** | Always `0` |
| **Emits as** | `println!("{}", arg)` |

```synapse
function main() -> Int
  value _ = print("hello world")
  returns 0
```

The return value is `0` (not the printed string).
Bind with `value _` when the return value is unused.

## `concat`

Concatenate two strings.

| | |
|---|---|
| **Signature** | `(String, String) -> String` |
| **Emits as** | `format!("{}{}", a, b)` |

```synapse
function greet(String name) -> Int
  value msg = concat("hello, ", name)
  value _ = print(msg)
  returns 0
```

## `http_get`

Fetch the body of a URL via HTTP GET.

| | |
|---|---|
| **Signature** | `(String) -> String` |
| **Requires** | `curl` on the system PATH |
| **Emits as** | A helper function that shells out to `curl -s` |

```synapse
function main() -> Int
  value body = http_get("https://example.com")
  value _ = print(body)
  returns 0
```

The `http_get` built-in is a placeholder for the
planned service runtime. It spawns `curl` as a child
process and returns stdout as a string. It does not
handle errors; a failed request returns an empty
string.

## Emission Details

Built-in functions are only emitted into the Rust
output when they are actually called. If your program
never calls `http_get`, no `curl` helper appears in
the generated code.

The emitter detects built-in usage by walking the
typed AST. Each built-in maps to specific Rust code:

| Built-in | Rust emission |
|----------|---------------|
| `print(s)` | `{ println!("{}", s); 0_i64 }` |
| `concat(a, b)` | `format!("{}{}", a, b)` |
| `http_get(url)` | `__builtin_http_get(url)` (helper function) |
