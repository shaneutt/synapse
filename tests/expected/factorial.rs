fn factorial(n: i64) -> i64 {
    match n {
        0_i64 => 1_i64,
        _ => n * factorial(n - 1_i64),
    }
}

fn synapse_main() -> i64 {
    factorial(10_i64)
}

fn main() {
    let result = synapse_main();
    println!("{result}");
}
