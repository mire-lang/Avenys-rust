// S6: word count.
fn main() {
    let mut seed = std::hint::black_box(987654321i64);
    let text = "the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog the quick brown fox jumps over the lazy dog";
    let count = text.split(' ').count();
    println!("{}", count);
}
