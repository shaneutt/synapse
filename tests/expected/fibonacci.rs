fn fib(n: i64) -> i64 {
    match n {
        0_i64 => 0_i64,
        1_i64 => 1_i64,
        _ => fib(n - 1_i64) + fib(n - 2_i64),
    }
}

fn synapse_main() -> i64 {
    fib(10_i64)
}

fn main() {
    let result = synapse_main();
    println!("{result}");
}
