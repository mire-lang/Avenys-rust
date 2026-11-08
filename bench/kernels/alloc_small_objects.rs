// A1: small object allocation pressure.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut i: i64 = 0;
    while i < 1_000_000 {
        let sz = 16 + (i % 7) * 8;
        let mut v: Vec<i64> = Vec::with_capacity((sz / 8) as usize);
        let mut j: i64 = 0;
        while j < sz / 8 {
            v.push(i + j);
            j += 1;
        }
        seed = (seed + v.len() as i64 + v[0]) % 1_000_003;
        i += 1;
    }
    println!("{}", seed);
}