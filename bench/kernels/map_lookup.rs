// B2: hash map lookup benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 4096 {
        m.insert(format!("lookup_key{}", i), i * 5 + 1);
        i += 1;
    }
    let mut acc: i64 = 0;
    // Phase 1: hit
    let mut i: i64 = 0;
    while i < 1_000_000 {
        acc = (acc + m.get(&format!("lookup_key{}", i % 4096)).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    // Phase 2: miss
    let mut i: i64 = 0;
    while i < 1_000_000 {
        acc = (acc + m.get(&format!("missing_key{}", i % 4096)).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}