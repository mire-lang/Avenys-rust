// C2: string search benchmark.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let unit = "abcdefghijklmnopqrstuvwxyz0123456789 ";
    let mut haystack = String::with_capacity(unit.len() * 2000);
    for _ in 0..2000 { haystack.push_str(unit); }
    let patterns = [
        "pattern0", "pattern1", "pattern2", "pattern3",
        "pattern4", "pattern5", "pattern6", "pattern7",
        "xyz01", "mnopq", "notfound", "0123"
    ];
    let mut round: i64 = 0;
    while round < 100 {
        let mut hits: i64 = 0;
        for &pat in &patterns {
            let mut pos: usize = 0;
            while let Some(found) = haystack[pos..].find(pat) {
                hits += 1;
                pos += found + 1;
            }
        }
        seed = (seed + hits) % 1_000_003;
        round += 1;
    }
    println!("{}", seed);
}