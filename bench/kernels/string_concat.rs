// C1: string concatenation benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    // Scenario 1: without reserved capacity
    let mut i: i64 = 0;
    while i < 50_000 {
        let mut s = String::new();
        let mut j: i64 = 0;
        while j < 10 {
            s.push_str(&format!("chunk{}", j));
            j += 1;
        }
        seed = (seed + s.len() as i64) % 1_000_003;
        i += 1;
    }
    // Scenario 2: with reserved capacity
    let mut base = String::with_capacity(2048);
    for _ in 0..100 {
        base.push_str("preallocated_base_string_");
    }
    let base_len = base.len();
    let mut i: i64 = 0;
    while i < 50_000 {
        let mut s = String::with_capacity(base_len + 16);
        s.push_str(&base);
        s.push_str(&format!("suffix{}", i % 10));
        seed = (seed + s.len() as i64) % 1_000_003;
        i += 1;
    }
    println!("{}", seed);
}