// S5: string builder.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut s = String::new();
    let mut i: i64 = 0;
    while i < 300 {
        s.push_str(&i.to_string());
        i += 1;
    }
    println!("{}", s.len());
}
