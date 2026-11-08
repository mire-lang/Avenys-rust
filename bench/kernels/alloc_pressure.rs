// C2: allocation pressure.
//
// The barrier on the vector's data pointer is the counterpart of the C leg's
// `bench_alloc_barrier`: without it LLVM proves that only `len` and `v[0]` are
// observed, folds both out of the push loop, and then deletes the allocation
// itself. The reported time then belongs to a program that never calls
// `malloc`, which is not the workload this kernel exists to measure. The C leg
// carries the same barrier for the same reason.
fn alloc_barrier<T>(v: &mut Vec<T>) {
    std::hint::black_box(v.as_mut_ptr());
}

fn main() {
    let mut acc = OPACUE(987654321i64);
    let mut i: i64 = 0;
    while i < 8_000_000 {
        let mut v: Vec<i64> = Vec::with_capacity(8);
        for j in 0..8i64 { v.push(j + i); }
        alloc_barrier(&mut v);
        acc = (acc + v.len() as i64 + v[0]) % 1_000_003;
        i += 1;
    }
    println!("{}", acc);
}
