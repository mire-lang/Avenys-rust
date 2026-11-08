// B3: hash map resize benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 20_000 {
        m.insert(format!("resize_key{}", i), i);
        seed = (seed + i) % 1_000_003;
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 20_000 {
        acc = (acc + m.get(&format!("resize_key{}", i)).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}