// D3: cache locality benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let size: usize = 1_000_000;
    let data: Vec<i64> = (0..size).map(|i| i as i64 * 3 + 7).collect();
    // Phase 1: sequential access
    for i in 0..size {
        seed = (seed + data[i]) % 1_000_003;
    }
    // Phase 2: strided access
    for i in (0..size).step_by(256) {
        seed = (seed + data[i]) % 1_000_003;
    }
    // Phase 3: random access (LCG)
    let mut idx: usize = 12345;
    for _ in 0..size {
        idx = (idx * 1103515245 + 12345) % 1_000_000;
        seed = (seed + data[idx]) % 1_000_003;
    }
    println!("{}", seed);
}