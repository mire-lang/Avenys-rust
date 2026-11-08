// D2: branch prediction benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let data: Vec<i64> = (0..10000).map(|i| i * 7 + 13).collect();
    // Phase 1: predictable branch
    for i in 0..10000 {
        let x = data[i];
        if i < 5000 {
            seed = (seed + x) % 1_000_003;
        } else {
            seed = (seed + x * 2) % 1_000_003;
        }
    }
    // Phase 2: pseudo-random branch
    for i in 0..10000 {
        let x = data[i];
        if x % 3 == 0 {
            seed = (seed + x) % 1_000_003;
        } else {
            seed = (seed + x * 3) % 1_000_003;
        }
    }
    println!("{}", seed);
}