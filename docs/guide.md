# Synapse Weather App: Step-by-Step Guide

This guide walks through building a weather app from a
four-line `.intent` file to a running binary, showing
every intermediate artifact.

## Step 1: The Application Intent

The entire program, described in four lines:

```
application weather:
  args:
    positional: city String
  intent: fetch today's weather report for the given city from wttr.in and print it to stdout
```

This declares:
- A program called `weather`
- One CLI argument: a city name (String)
- What it does: fetch weather and print it

No functions, no modules, no implementation.

## Step 2: Expand to Synapse

```console
axon expand examples/intent/applications/app_weather.intent
```

A single LLM call expands the application directly
into valid `.synapse` code. The prompt includes the
full language spec, built-in function signatures, and
the application's declared args and intent.

Output:

```synapse
function build_url(String city) -> String
  value base = concat("https://wttr.in/", city)
  returns concat(base, "?format=3")

function fetch_weather(String city) -> String
  value url = concat("https://wttr.in/", city)
  value body = http_get(url)
  returns body

function main(String city) -> Int
  value result = fetch_weather(city)
  value _ = print(result)
  returns 0
```

Three functions with correct types:
- `build_url`: pure string work using `concat`
- `fetch_weather`: calls `http_get` (returns String)
- `main`: wires it together, calls `print` (returns
  Int exit code)

## Step 3: Emit Rust

```console
cortex emit weather.synapse
```

Cortex transpiles the `.synapse` to valid Rust:

```rust
fn __builtin_http_get(url: String) -> String {
    let output = std::process::Command::new("curl")
        .args(["-s", &url])
        .output()
        .expect("failed to run curl");
    String::from_utf8(output.stdout)
        .unwrap_or_default()
}

fn build_url(city: String) -> String {
    let base = format!("{}{}",
        "https://wttr.in/".to_owned(), city);
    format!("{}{}", base, "?format=3".to_owned())
}

fn fetch_weather(city: String) -> String {
    let url = format!("{}{}",
        "https://wttr.in/".to_owned(), city);
    let body = __builtin_http_get(url);
    body
}

fn synapse_main(city: String) -> i64 {
    let result = fetch_weather(city);
    let _ = { println!("{}", result); 0_i64 };
    0_i64
}

fn main() {
    let args: Vec<String> =
        std::env::args().skip(1).collect();
    if args.len() < 1 {
        eprintln!("usage: <program> <city>");
        std::process::exit(1);
    }
    let city = args[0].clone();
    let result = synapse_main(city);
    println!("{result}");
}
```

Key details:
- `http_get` emits a `curl -s` subprocess call
- `concat` emits `format!("{}{}", ...)`
- `print` emits `println!`
- Cortex infers from `main(String city)` that `city`
  is a positional CLI arg and generates arg parsing
- The Synapse `main` is renamed to `synapse_main`;
  the Rust `main` handles args and calls it

## Step 4: Compile

```console
cortex compile weather.synapse -o weather
```

This writes the emitted Rust to a temp file and
invokes `rustc` on it, producing the `weather`
binary.

## Step 5: Run

```console
./weather Seattle
```

Output:

```
seattle: ⛅  +53°F
```

The binary parses "Seattle" from the command line,
builds the wttr.in URL, fetches the weather via curl,
and prints the result.

## Using `axon build` (Project Mode)

For a project-based workflow, create a directory with
`synapse.toml`:

```toml
[project]
name = "weather"
version = "0.1.0"

[build]
entry = "src/main.intent"
```

Place the `.intent` file at `src/main.intent`, then:

```console
axon build
axon run Seattle
```

## Where Files End Up

With `axon build`, every intermediate artifact is
written to `target/`:

```
weather-project/
  synapse.toml
  src/
    main.intent               # your source
  target/
    synapse/
      main.synapse            # expanded .synapse
    rust/
      main.rs                 # emitted Rust
    bin/
      weather                 # compiled binary
  .synapse-cache/
    manifest.toml             # hashes for incremental builds
```

Each intermediate is inspectable:
- `target/synapse/main.synapse`: the generated
  Synapse functions
- `target/rust/main.rs`: the emitted Rust with arg
  parsing, `http_get` implementation, etc.
- `target/bin/weather`: the final executable

## The Full Pipeline

```
app.intent
  -> [claude: expand] -> target/synapse/main.synapse
  -> [cortex: emit] -> target/rust/main.rs
  -> [rustc: compile] -> target/bin/weather
```

Three transformations. Every step deterministic except
the LLM call, which is cached (same input produces
same output on cache hit, skipping the LLM entirely).
