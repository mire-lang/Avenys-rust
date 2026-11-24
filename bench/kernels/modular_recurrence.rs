fn main() {
    let mut state = OPACUE(987654321i64);
    let mut i = 0i64;
    while i < 12_000_000 {
        state = (state * 48_271 + i + 1) % 2_147_483_647;
        i += 1;
    }
    println!("{}", state);
}
