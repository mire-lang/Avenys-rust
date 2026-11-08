// B1: hash map collision benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 2048 {
        m.insert(format!("key_collision_{}", i), i * 3);
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 2_000_000 {
        acc = (acc + m.get(&format!("key_collision_{}", i % 2048)).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}