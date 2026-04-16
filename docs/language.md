# Synapse Language Reference

## Overview

Synapse is a purely functional language that transpiles to
Rust. Programs consist of function declarations. The entry
point is a function named `main`.

## Types

| Type | Description |
|------|-------------|
| `Int` | 64-bit signed integer |
| `Bool` | Boolean (`true`, `false`) |
| `String` | UTF-8 string |
| `List<T>` | Cons-list of elements of type `T` |

## Functions

```synapse
function add(Int a, Int b) -> Int
  returns a + b
```

Parameters are typed. Return type follows `->`. The body
is indented and consists of statements.

## Statements

**Value binding:**

```synapse
value x = 42
```

**Returns:**

```synapse
returns expression
```

Every function body must end with `returns`.

## Expressions

**Literals:** `42`, `true`, `false`, `"hello"`

**Identifiers:** `x`, `myVar`

**Arithmetic:** `+`, `-`, `*`, `/`, `%`

**Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`

**Logical:** `&&`, `||`

**Function calls:** `factorial(n - 1)`

**Parenthesized:** `(a + b) * c`

## Match Expressions

```synapse
match n
  when 0 -> 1
  otherwise -> n * factorial(n - 1)
```

Each arm is `when pattern -> expression`. Use `otherwise`
as a catch-all.

## Lists

Construct with `Cons` and `Nil`:

```synapse
Cons(1, Cons(2, Cons(3, Nil)))
```

Destructure in match patterns:

```synapse
match xs
  when Nil -> 0
  when Cons(x, rest) -> x + sum(rest)
```

## Patterns

| Pattern | Matches |
|---------|---------|
| `42` | Integer literal |
| `true` | Boolean literal |
| `"hi"` | String literal |
| `x` | Any value (binds to `x`) |
| `_` | Any value (discarded) |
| `Nil` | Empty list |
| `Cons(h, t)` | Non-empty list |
| `otherwise` | Catch-all (in match arms) |

## Built-in Functions

| Function | Signature | Description |
|----------|-----------|-------------|
| `print` | `(String) -> Int` | Print to stdout, returns 0 |
| `http_get` | `(String) -> String` | Fetch a URL, return the body |
| `concat` | `(String, String) -> String` | Concatenate two strings |

```synapse
value url = concat("https://example.com/", path)
value body = http_get(url)
value _ = print(body)
```

## Indentation

Synapse uses whitespace-significant indentation (spaces
only, no tabs). Function bodies and match arms must be
indented relative to their parent.

## Example

```synapse
function factorial(Int n) -> Int
  returns match n
    when 0 -> 1
    otherwise -> n * factorial(n - 1)

function main() -> Int
  returns factorial(10)
```
