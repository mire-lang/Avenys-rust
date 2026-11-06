// H3: integer hash throughput.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut acc: u64 = 0;
    let mut i: i64 = 0;
    while i < 20_000_000 {
        let z = (i as u64).wrapping_mul(2654435761).wrapping_add(1);
        let h = z * 33 + z * 7;
        acc = acc + (h % 1000003);
        i += 1;
    }
    println!("{}", acc);
}
