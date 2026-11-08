// A3: allocation fragmentation.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut pool: Vec<Vec<i64>> = Vec::with_capacity(1000);
    let mut i: i64 = 0;
    while i < 200_000 {
        if i % 7 == 0 && !pool.is_empty() {
            let idx = (i * 13) % pool.len() as i64;
            pool.swap_remove(idx as usize);
        }
        let sz = 32 + (i % 15) * 16;
        let mut v: Vec<i64> = Vec::with_capacity((sz / 8) as usize);
        let mut j: i64 = 0;
        while j < sz / 8 {
            v.push(i + j);
            j += 1;
        }
        pool.push(v);
        seed = (seed + pool.last().unwrap().len() as i64 + pool.last().unwrap()[0]) % 1_000_003;
        i += 1;
    }
    println!("{}", seed);
}