// M9: map iteration.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    while i < 10000 {
        m.insert(format!("key{}", i), i * 5 + 11);
        i += 1;
    }
    let mut acc: i64 = 0;
    for (k, v) in &m {
        acc = (acc + v) % 1_000_003;
    }
    for k in m.keys() {
        acc = (acc + k.len() as i64) % 1_000_003;
    }
    println!("{}", acc);
}
