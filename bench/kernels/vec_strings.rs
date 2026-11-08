// V11: vec of strings.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let mut v: Vec<String> = Vec::new();
    let mut i: i64 = 0;
    while i < 20000 {
        v.push(format!("item{}", i));
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut i: i64 = 0;
    while i < 10000 {
        if let Some(s) = v.pop() {
            acc = (acc + s.len() as i64) % 1_000_003;
        }
        i += 1;
    }
    for s in &v {
        acc = (acc + s.len() as i64) % 1_000_003;
    }
    println!("{}", acc);
}
