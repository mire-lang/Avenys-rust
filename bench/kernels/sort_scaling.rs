// Sorting scale kernel. Large enough to measure the runtime sort beyond startup.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut round: i64 = 0;
    while round < 100 {
        let mut values: Vec<i64> = Vec::with_capacity(10000);
        let mut index: i64 = 0;
        while index < 10000 {
            values.push((index * 2654435761i64 + round) % 1000003);
            index += 1;
        }
        values.sort();
        seed = (seed + values[0] + values[9999]) % 1000003;
        round += 1;
    }
    println!("{}", seed);
}
