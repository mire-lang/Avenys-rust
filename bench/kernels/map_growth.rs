// M8: map growth and rehashing.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 20000 {
        m.insert(format!("key{}", i), i * 3 + 7);
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 20000 {
        acc = (acc + m.get(&format!("key{}", i)).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}
