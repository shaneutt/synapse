use crate::ast::Capability;

/// Attempts to match a capability's intent phrase to a known template
/// and generate the corresponding `.synapse` function body.
///
/// Returns `None` if no template matches the intent phrase.
///
/// ```
/// # use intent::ast::*;
/// # use intent::templates::expand_capability;
/// let cap = Capability {
///     name: "factorial".to_owned(),
///     inputs: vec![Param {
///         name: "n".to_owned(),
///         ty: "Int".to_owned(),
///     }],
///     intent: "compute factorial using recursion".to_owned(),
///     output: Some("Int".to_owned()),
/// };
/// let code = expand_capability(&cap).unwrap();
/// assert!(code.contains("function factorial"));
/// ```
pub fn expand_capability(cap: &Capability) -> Option<String> {
    let intent = cap.intent.to_lowercase();
    tracing::debug!(
        name = %cap.name,
        intent = %intent,
        "matching intent to template"
    );

    if matches_factorial(&intent) {
        return Some(emit_factorial(cap));
    }
    if matches_fibonacci(&intent) {
        return Some(emit_fibonacci(cap));
    }
    if matches_sum(&intent) {
        return Some(emit_sum(cap));
    }
    if matches_length(&intent) {
        return Some(emit_length(cap));
    }
    if matches_reverse(&intent) {
        return Some(emit_reverse(cap));
    }
    if matches_map(&intent) {
        return Some(emit_map(cap));
    }
    if matches_filter(&intent) {
        return Some(emit_filter(cap));
    }
    if matches_gcd(&intent) {
        return Some(emit_gcd(cap));
    }
    if matches_power(&intent) {
        return Some(emit_power(cap));
    }
    if matches_max(&intent) {
        return Some(emit_max(cap));
    }
    if matches_min(&intent) {
        return Some(emit_min(cap));
    }

    tracing::info!(name = %cap.name, "no template matched");
    None
}

// ---------------------------------------------------------------------------
// Intent Matchers
// ---------------------------------------------------------------------------

/// Matches "factorial" intents.
fn matches_factorial(intent: &str) -> bool {
    intent.contains("factorial")
}

/// Matches "fibonacci" intents.
fn matches_fibonacci(intent: &str) -> bool {
    intent.contains("fibonacci") || intent.contains("fib")
}

/// Matches "sum" intents.
fn matches_sum(intent: &str) -> bool {
    intent.contains("sum")
}

/// Matches "length" intents.
fn matches_length(intent: &str) -> bool {
    intent.contains("length") || intent.contains("count")
}

/// Matches "reverse" intents.
fn matches_reverse(intent: &str) -> bool {
    intent.contains("reverse")
}

/// Matches "map" intents.
fn matches_map(intent: &str) -> bool {
    intent.contains("map")
}

/// Matches "filter" intents.
fn matches_filter(intent: &str) -> bool {
    intent.contains("filter")
}

/// Matches "gcd" or "greatest common divisor" intents.
fn matches_gcd(intent: &str) -> bool {
    intent.contains("gcd") || intent.contains("greatest common")
}

/// Matches "power" or "exponent" intents.
fn matches_power(intent: &str) -> bool {
    intent.contains("power") || intent.contains("exponent")
}

/// Matches "maximum" or "max" intents.
fn matches_max(intent: &str) -> bool {
    (intent.contains("max") || intent.contains("maximum")) && !intent.contains("min")
}

/// Matches "minimum" or "min" intents.
fn matches_min(intent: &str) -> bool {
    intent.contains("min") || intent.contains("minimum")
}

// ---------------------------------------------------------------------------
// Template Emitters
// ---------------------------------------------------------------------------

/// Emits a recursive factorial function.
fn emit_factorial(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let ret = output_type(cap);
    format!(
        "function {name}({ret} {param}) -> {ret}\n  \
         returns match {param}\n    \
         when 0 -> 1\n    \
         otherwise -> {param} * {name}({param} - 1)\n"
    )
}

/// Emits a double-recursive fibonacci function.
fn emit_fibonacci(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let ret = output_type(cap);
    format!(
        "function {name}({ret} {param}) -> {ret}\n  \
         returns match {param}\n    \
         when 0 -> 0\n    \
         when 1 -> 1\n    \
         otherwise -> {name}({param} - 1) + {name}({param} - 2)\n"
    )
}

/// Emits a recursive list sum function.
fn emit_sum(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    let ret = output_type(cap);
    format!(
        "function {name}({input_ty} {param}) -> {ret}\n  \
         returns match {param}\n    \
         when Nil -> 0\n    \
         when Cons(x, rest) -> x + {name}(rest)\n"
    )
}

/// Emits a recursive list length function.
fn emit_length(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    let ret = output_type(cap);
    format!(
        "function {name}({input_ty} {param}) -> {ret}\n  \
         returns match {param}\n    \
         when Nil -> 0\n    \
         when Cons(_, rest) -> 1 + {name}(rest)\n"
    )
}

/// Emits a reverse function with an accumulator helper.
fn emit_reverse(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    let helper = format!("{name}_helper");
    format!(
        "function {helper}({input_ty} {param}, \
         {input_ty} acc) -> {input_ty}\n  \
         returns match {param}\n    \
         when Nil -> acc\n    \
         when Cons(x, rest) -> {helper}(rest, Cons(x, acc))\n\
         \n\
         function {name}({input_ty} {param}) -> {input_ty}\n  \
         returns {helper}({param}, Nil)\n"
    )
}

/// Emits a recursive list map function.
///
/// The map applies `x * 2` as a default transform since intent
/// phrases like "double" or "map" do not specify the operation.
fn emit_map(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    format!(
        "function {name}({input_ty} {param}) -> {input_ty}\n  \
         returns match {param}\n    \
         when Nil -> Nil\n    \
         when Cons(x, rest) -> Cons(x * 2, {name}(rest))\n"
    )
}

/// Emits a recursive list filter function.
///
/// Filters elements where `x > 0` as a default predicate.
fn emit_filter(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    format!(
        "function {name}({input_ty} {param}) -> {input_ty}\n  \
         returns match {param}\n    \
         when Nil -> Nil\n    \
         when Cons(x, rest) -> match x > 0\n      \
         when true -> Cons(x, {name}(rest))\n      \
         otherwise -> {name}(rest)\n"
    )
}

/// Emits a GCD function using the Euclidean algorithm.
fn emit_gcd(cap: &Capability) -> String {
    let name = &cap.name;
    let (p1, p2) = two_param_names(cap);
    let ret = output_type(cap);
    format!(
        "function {name}({ret} {p1}, {ret} {p2}) -> {ret}\n  \
         returns match {p2} == 0\n    \
         when true -> {p1}\n    \
         otherwise -> {name}({p2}, {p1} % {p2})\n"
    )
}

/// Emits a recursive exponentiation function.
fn emit_power(cap: &Capability) -> String {
    let name = &cap.name;
    let (base, exp) = two_param_names(cap);
    let ret = output_type(cap);
    format!(
        "function {name}({ret} {base}, {ret} {exp}) -> {ret}\n  \
         returns match {exp} == 0\n    \
         when true -> 1\n    \
         otherwise -> {base} * {name}({base}, {exp} - 1)\n"
    )
}

/// Emits a recursive list maximum function.
fn emit_max(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    let helper = format!("{name}_helper");
    format!(
        "function {helper}({input_ty} {param}, \
         Int best) -> Int\n  \
         returns match {param}\n    \
         when Nil -> best\n    \
         when Cons(x, rest) -> match x > best\n      \
         when true -> {helper}(rest, x)\n      \
         otherwise -> {helper}(rest, best)\n\
         \n\
         function {name}({input_ty} {param}) -> Int\n  \
         returns match {param}\n    \
         when Nil -> 0\n    \
         when Cons(x, rest) -> {helper}(rest, x)\n"
    )
}

/// Emits a recursive list minimum function.
fn emit_min(cap: &Capability) -> String {
    let name = &cap.name;
    let param = first_param_name(cap);
    let input_ty = first_param_type(cap);
    let helper = format!("{name}_helper");
    format!(
        "function {helper}({input_ty} {param}, \
         Int best) -> Int\n  \
         returns match {param}\n    \
         when Nil -> best\n    \
         when Cons(x, rest) -> match x > best\n      \
         when true -> {helper}(rest, best)\n      \
         otherwise -> {helper}(rest, x)\n\
         \n\
         function {name}({input_ty} {param}) -> Int\n  \
         returns match {param}\n    \
         when Nil -> 0\n    \
         when Cons(x, rest) -> {helper}(rest, x)\n"
    )
}

// ---------------------------------------------------------------------------
// Parameter Extraction Utilities
// ---------------------------------------------------------------------------

/// Returns the name of the first input parameter, or `"n"` as default.
fn first_param_name(cap: &Capability) -> &str {
    cap.inputs.first().map_or("n", |p| p.name.as_str())
}

/// Returns the type of the first input parameter, or `"Int"` as default.
fn first_param_type(cap: &Capability) -> &str {
    cap.inputs.first().map_or("Int", |p| p.ty.as_str())
}

/// Returns the output type, or `"Int"` as default.
fn output_type(cap: &Capability) -> &str {
    cap.output.as_deref().unwrap_or("Int")
}

/// Returns the names of the first two parameters.
fn two_param_names(cap: &Capability) -> (&str, &str) {
    let a = cap.inputs.first().map_or("a", |p| p.name.as_str());
    let b = cap.inputs.get(1).map_or("b", |p| p.name.as_str());
    (a, b)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Capability, Param};

    #[test]
    fn factorial_template() {
        let cap = make_cap(
            "factorial",
            vec![("Int", "n")],
            "Int",
            "compute factorial using recursion",
        );
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("function factorial(Int n) -> Int"), "signature: {code}");
        assert!(code.contains("n * factorial(n - 1)"), "recursive call: {code}");
    }

    #[test]
    fn fibonacci_template() {
        let cap = make_cap("fibonacci", vec![("Int", "n")], "Int", "compute nth fibonacci number");
        let code = expand_capability(&cap).unwrap();
        assert!(
            code.contains("fibonacci(n - 1) + fibonacci(n - 2)"),
            "double recursion: {code}"
        );
    }

    #[test]
    fn sum_template() {
        let cap = make_cap(
            "list_sum",
            vec![("List<Int>", "xs")],
            "Int",
            "sum all elements using recursion",
        );
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("x + list_sum(rest)"), "recursive sum: {code}");
    }

    #[test]
    fn length_template() {
        let cap = make_cap(
            "list_length",
            vec![("List<Int>", "xs")],
            "Int",
            "compute length of list",
        );
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("1 + list_length(rest)"), "recursive length: {code}");
    }

    #[test]
    fn reverse_template() {
        let cap = make_cap("reverse", vec![("List<Int>", "xs")], "List<Int>", "reverse list");
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("reverse_helper"), "uses helper: {code}");
        assert!(code.contains("Cons(x, acc)"), "accumulator pattern: {code}");
    }

    #[test]
    fn map_template() {
        let cap = make_cap(
            "double_all",
            vec![("List<Int>", "xs")],
            "List<Int>",
            "map double over list",
        );
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("Cons(x * 2, double_all(rest))"), "map transform: {code}");
    }

    #[test]
    fn filter_template() {
        let cap = make_cap(
            "positives",
            vec![("List<Int>", "xs")],
            "List<Int>",
            "filter positives from list",
        );
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("x > 0"), "filter predicate: {code}");
    }

    #[test]
    fn gcd_template() {
        let cap = make_cap("gcd", vec![("Int", "a"), ("Int", "b")], "Int", "compute gcd");
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("gcd(b, a % b)"), "euclidean: {code}");
    }

    #[test]
    fn power_template() {
        let cap = make_cap(
            "power",
            vec![("Int", "base"), ("Int", "exp")],
            "Int",
            "compute power/exponent",
        );
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("base * power(base, exp - 1)"), "recursive power: {code}");
    }

    #[test]
    fn max_template() {
        let cap = make_cap("find_max", vec![("List<Int>", "xs")], "Int", "find maximum in list");
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("find_max_helper"), "uses helper: {code}");
    }

    #[test]
    fn min_template() {
        let cap = make_cap("find_min", vec![("List<Int>", "xs")], "Int", "find minimum in list");
        let code = expand_capability(&cap).unwrap();
        assert!(code.contains("find_min_helper"), "uses helper: {code}");
    }

    #[test]
    fn no_match_returns_none() {
        let cap = make_cap("custom", vec![("Int", "x")], "Int", "do something completely novel");
        assert!(expand_capability(&cap).is_none(), "should not match any template");
    }

    // ---------------------------------------------------------------------------
    // Test Utilities
    // ---------------------------------------------------------------------------

    /// Builds a capability for testing.
    fn make_cap(name: &str, inputs: Vec<(&str, &str)>, output: &str, intent: &str) -> Capability {
        Capability {
            name: name.to_owned(),
            inputs: inputs
                .into_iter()
                .map(|(ty, n)| Param {
                    name: n.to_owned(),
                    ty: ty.to_owned(),
                })
                .collect(),
            intent: intent.to_owned(),
            output: Some(output.to_owned()),
        }
    }
}
