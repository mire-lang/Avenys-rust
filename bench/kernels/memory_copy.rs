// A4: memory copy benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut i: i64 = 0;
    while i < 100_000 {
        let sz = 64 + (i % 16384) * 64;
        let mut src: Vec<i64> = Vec::with_capacity((sz / 8) as usize);
        let mut j: i64 = 0;
        while j < sz / 8 {
            src.push(i + j);
            j += 1;
        }
        let mut dst = vec![0i64; (sz / 8) as usize];
        dst.copy_from_slice(&src);
        seed = (seed + dst.len() as i64 + dst[0]) % 1_000_003;
        i += 1;
    }
    println!("{}", seed);
}