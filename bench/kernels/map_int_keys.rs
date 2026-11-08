// M10: map with string keys (integer-like).
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 100000 {
        m.insert(i.to_string(), i * 13 + 17);
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 100000 {
        acc = (acc + m.get(&i.to_string()).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}
