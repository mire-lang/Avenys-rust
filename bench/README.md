# Mire benchmark suite: against Rust and against C

Each kernel exists three times — in Mire, in Rust, and in C — performing the
same calculation. The C leg exists so that "equivalent" has a meaning: C has no
language runtime, so a kernel that needs runtime machinery to reach C's numbers
is measuring the runtime, not the language. `verify` compares their output before `measure` is allowed to
report timings. This is a focused language/runtime comparison, not a claim
that every application will have the same performance profile.

## Commands

```sh
./run.sh verify
./run.sh measure
./run.sh measure C2_allocacion
./run.sh curve C2_allocacion
./run.sh startup
```

`verify` is a required precondition for measurements. If any kernel differs,
`measure` exits without producing benchmark numbers.

The script uses `MIRE_BIN` when set; otherwise it selects the compiler built in
this repository, falling back to `~/.local/bin/mire` only if that binary is not
available. `MIRE_LIB_DIR` selects the Mire library directory. `MIRE_BENCH_WORK`
sets the isolated work directory (default: `/tmp/mire-bench`).

## Kernels

| Kernel | Workload |
|---|---|
| `alloc_pressure` | Eight million short-lived vectors, each pushed eight times |
| `binary_search` | Repeated binary search over an ordered vector |
| `break_continue` | Branch-heavy loop control |
| `list_indexed_read` | Repeated indexed reads from a vector |
| `many_locals` | Live local variables and register pressure |
| `matrix_rowmajor` | Row-major matrix traversal |
| `method_dispatch` | Struct method dispatch |
| `mixed_int_float` | Interleaved integer and floating-point operations |
| `nested_loops` | Two nested loops |
| `numeric_format` | Formatting integers into strings |
| `recursion_ackermann` | Recursive Ackermann workload |
| `sort_introsort` | Insertion sort of a freshly filled vector |
| `string_impl` | Method delegation through an implementation |
| `string_scan` | Repeated substring search |
| `swap_exchange` | Rotation of eight variables |
| `trait_dispatch` | Trait dispatch across implementations |
| `vec_growth` | Repeated vector growth and reallocation |
| `vec_mixed_reads` | Interleaved vector reads |
| `vec_partition` | In-place vector partitioning |
| `vec_pop_drain` | Repeated pop/drain operations |
| `while_counter` | Counter-based while loop |

`noop` is reserved for measuring process startup and is not part of the
21-kernel comparison.

## Latest measurement

Each kernel exists three times: in Mire, in Rust, and in C. The C leg is the
reference for what "equivalent" means — C has no language runtime, so those
kernels call the C library directly (`malloc`, `realloc`, `free`) with no arena,
no reference counting, and no allocator of the compiler's own. Where a Mire
version would need runtime machinery to reach the same numbers, that gap is a
finding to record, not an excuse to rewrite the kernel until it passes.

Measured November 5, 2026, with Rust 1.99.0 (`-C opt-level=3`), C
(`cc -O3 -march=native`), and the Avenys 4.4.14 release (`-O3`), best of seven
runs. Output verification passed for all 21 kernels across all available legs
immediately beforehand. Values are machine-specific; runs are not pinned to a
CPU, and sub-25 ms results (`*`) are dominated by process startup.

| Kernel | Workload | Rust -O3 | C -O3 | Mire -O3 | Mire/C | Mire/Rust |
|---|---|---:|---:|---:|---:|---:|
| `alloc_pressure` | hard | 26 ms | 59 ms | 150 ms | 2.54x | 5.77x |
| `binary_search` | medium | 19 ms | 24 ms | 48 ms | 2.00x | 2.53x* |
| `break_continue` | medium | 29 ms | 19 ms | 25 ms | 1.32x | 0.86x |
| `list_indexed_read` | easy | 14 ms | 14 ms | 14 ms | 1.00x | 1.00x* |
| `many_locals` | medium | 43 ms | 37 ms | 44 ms | 1.19x | 1.02x |
| `matrix_rowmajor` | medium | 22 ms | 5 ms | 23 ms | 4.60x | 1.05x* |
| `method_dispatch` | trivial | 128 ms | 125 ms | 130 ms | 1.04x | 1.02x |
| `mixed_int_float` | medium | 38 ms | 34 ms | 38 ms | 1.12x | 1.00x |
| `nested_loops` | medium | 50 ms | 54 ms | 53 ms | 0.98x | 1.06x |
| `numeric_format` | medium | 57 ms | 280 ms | 274 ms | 0.98x | 4.81x |
| `recursion_ackermann` | hard | 36 ms | 35 ms | 31 ms | 0.89x | 0.86x |
| `sort_introsort` | medium | 7 ms | 52 ms | 102 ms | 1.96x | 14.57x* |
| `string_impl` | hard | 57 ms | 53 ms | 57 ms | 1.08x | 1.00x |
| `string_scan` | medium | 18 ms | 9 ms | 17 ms | 1.89x | 0.94x* |
| `swap_exchange` | easy | 34 ms | 32 ms | 35 ms | 1.09x | 1.03x |
| `trait_dispatch` | medium | 58 ms | 62 ms | 61 ms | 0.98x | 1.05x |
| `vec_growth` | - | 54 ms | 75 ms | 69 ms | 0.92x | 1.28x |
| `vec_mixed_reads` | - | 43 ms | 39 ms | 40 ms | 1.03x | 0.93x |
| `vec_partition` | medium | 11 ms | 5 ms | 15 ms | 3.00x | 1.36x* |
| `vec_pop_drain` | - | 46 ms | 44 ms | 55 ms | 1.25x | 1.20x |
| `while_counter` | trivial | 47 ms | 38 ms | 43 ms | 1.13x | 0.91x |

### What the C leg shows

**Most kernels are at or near native baselines.** Fourteen of the 21 are within
1.3x of C, and several (`nested_loops`, `numeric_format`,
`recursion_ackermann`, `trait_dispatch`, `vec_growth`) are at parity or faster.
`numeric_format` is 4.81x slower than Rust but slightly faster than the C leg
because `rt_i64_to_string` no longer goes through `snprintf`.

**Sorting is now compiled by LLVM instead of tcc.** `sort_introsort` fell from
292 ms to about 100 ms after `vec::sort` moved into Mire and the hot loop walks the
buffer through a raw pointer; tcc could not optimize the nested loop in the C
runtime. It is still 14.57x Rust because the Rust leg uses the standard
library's sort while the Mire, C and Rust benchmark legs deliberately use the
same insertion sort.

**The former O(n²) string scan is gone.** `string_scan` fell from 3677 ms to
17 ms once `str::index::from` let the kernel resume the search without
`substr`-ing the remaining input on every match. It is now below the noise
floor and at parity with Rust.

**Remaining gaps are concentrated.** `sort_introsort` (Rust's stdlib sort),
`alloc_pressure` (2.54x C), `binary_search` and `matrix_rowmajor` (C
autovectorizes the reference while Mire matches Rust) are the clearest
follow-ups. `matrix_rowmajor` is 4.60x C but 1.05x Rust, which points at the C
reference's vectorization rather than at Mire codegen.

The former 3.12x C2 result is withdrawn: it was affected by a vector-push
ownership defect and is not comparable. Always run `verify` before publishing
measurements; use paired runs when attributing a change to a code revision.
