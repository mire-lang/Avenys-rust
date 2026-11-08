// M11: map remove + insert mix.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 5000 {
        m.insert(format!("key{}", i), i * 2 + 3);
        i += 1;
    }
    let mut i: i64 = 0;
    while i < 2500 {
        m.remove(&format!("key{}", i));
        i += 1;
    }
    let mut i: i64 = 5000;
    while i < 7500 {
        m.insert(format!("key{}", i), i * 2 + 3);
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 7500 {
        acc = (acc + m.get(&format!("key{}", i)).copied().unwrap_or(0)) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}
