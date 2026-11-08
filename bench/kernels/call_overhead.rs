// D1: function call overhead.
#[inline(never)]
fn leaf_add(a: i64, b: i64) -> i64 { a + b }
#[inline(never)]
fn leaf_mul(a: i64, b: i64) -> i64 { a * b }

fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut i: i64 = 0;
    while i < 5_000_000 {
        seed = (seed + leaf_add(seed, i)) % 1_000_003;
        seed = (seed + leaf_mul(seed, i)) % 1_000_003;
        i += 1;
    }
    println!("{}", seed);
}