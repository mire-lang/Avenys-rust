// M12: map mixed workload.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut m: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut i: i64 = 0;
    let mut acc: i64 = 0;
    while i < 50000 {
        let k = format!("k{}", i % 1000);
        if let Some(v) = m.get(&k) {
            let v = *v;
            if i % 7 == 0 {
                m.remove(&k);
            } else {
                acc = (acc + v) % 1_000_003;
            }
        } else {
            m.insert(k, i * 7 + 11);
        }
        i += 1;
    }
    println!("{}", acc);
}
