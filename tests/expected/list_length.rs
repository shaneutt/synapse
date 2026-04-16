#[derive(Debug, Clone, PartialEq)]
enum List<T> {
    Cons(T, Box<List<T>>),
    Nil,
}

impl<T: std::fmt::Display> std::fmt::Display for List<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[")?;
        let mut current = self;
        let mut first = true;
        loop {
            match current {
                List::Cons(head, tail) => {
                    if !first { write!(f, ", ")?; }
                    write!(f, "{head}")?;
                    first = false;
                    current = tail;
                }
                List::Nil => break,
            }
        }
        write!(f, "]")
    }
}

fn length(xs: List<i64>) -> i64 {
    match xs {
        List::Nil => 0_i64,
        List::Cons(_, rest) => {
            let rest = *rest;
            1_i64 + length(rest)
        },
    }
}

fn synapse_main() -> i64 {
    length(List::Cons(1_i64, Box::new(List::Cons(2_i64, Box::new(List::Cons(3_i64, Box::new(List::Nil)))))))
}

fn main() {
    let result = synapse_main();
    println!("{result}");
}
