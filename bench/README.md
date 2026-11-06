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

Measured November 6, 2026, with Rust 1.99.0 (`-C opt-level=3`), C
(`cc -O3 -march=native`), and the Avenys 4.4.15 release (`-O3`), best of seven
runs. Output verification passed for all 21 kernels across all available legs
immediately beforehand. Values are machine-specific; runs are not pinned to a
CPU, and sub-25 ms results (`*`) are dominated by process startup.

| Kernel | Workload | Rust -O3 | C -O3 | Mire -O3 | Mire/C | Mire/Rust |
|---|---|---:|---:|---:|---:|---:|
| `alloc_pressure` | hard | 26 ms | 59 ms | 98 ms | 1.66x | 3.77x |
| `binary_search` | medium | 19 ms | 24 ms | 22 ms | 0.92x | 1.16x* |
| `break_continue` | medium | 29 ms | 17 ms | 24 ms | 1.41x | 0.83x* |
| `list_indexed_read` | easy | 13 ms | 13 ms | 14 ms | 1.08x | 1.08x* |
| `many_locals` | medium | 44 ms | 37 ms | 44 ms | 1.19x | 1.00x |
| `matrix_rowmajor` | medium | 22 ms | 5 ms | 22 ms | 4.40x | 1.00x* |
| `method_dispatch` | trivial | 127 ms | 125 ms | 131 ms | 1.05x | 1.03x |
| `mixed_int_float` | medium | 37 ms | 33 ms | 38 ms | 1.15x | 1.03x |
| `nested_loops` | medium | 49 ms | 53 ms | 51 ms | 0.96x | 1.04x |
| `numeric_format` | medium | 56 ms | 280 ms | 259 ms | 0.93x | 4.62x |
| `recursion_ackermann` | hard | 35 ms | 34 ms | 30 ms | 0.88x | 0.86x |
| `sort_introsort` | medium | 6 ms | 52 ms | 59 ms | 1.13x | 9.83x* |
| `string_impl` | hard | 57 ms | 53 ms | 57 ms | 1.08x | 1.00x |
| `string_scan` | medium | 16 ms | 9 ms | 17 ms | 1.89x | 1.06x* |
| `swap_exchange` | easy | 33 ms | 32 ms | 35 ms | 1.09x | 1.06x |
| `trait_dispatch` | medium | 58 ms | 63 ms | 61 ms | 0.97x | 1.05x |
| `vec_growth` | - | 53 ms | 75 ms | 67 ms | 0.89x | 1.26x |
| `vec_mixed_reads` | - | 43 ms | 39 ms | 40 ms | 1.03x | 0.93x |
| `vec_partition` | medium | 10 ms | 5 ms | 12 ms | 2.40x | 1.20x* |
| `vec_pop_drain` | - | 46 ms | 44 ms | 49 ms | 1.11x | 1.07x |
| `while_counter` | trivial | 47 ms | 37 ms | 43 ms | 1.16x | 0.91x |

### What the C leg shows

**Most kernels are at or near native baselines.** Sixteen of the 21 are within
1.3x of C, and several (`nested_loops`, `numeric_format`,
`recursion_ackermann`, `trait_dispatch`, `vec_growth`, `binary_search`) are at
parity or faster. `numeric_format` is 4.62x slower than Rust but faster than
the C leg because `rt_i64_to_string` no longer goes through `snprintf`.

**64-bit address indices closed most of the sorting gap.** Mire's GEP lowering
used to truncate every array index to `i32` and sign-extend it again for each
access. LLVM could not strength-reduce the resulting shift pair, so the
insertion sort paid two extra instructions per comparison and store.
`sort_introsort` fell from 102 ms to 59 ms (C: 52 ms) and `binary_search` from
48 ms to 22 ms once indices stay at 64 bits. The change also removes a latent
wrap-around for indices past 2^31.

**Hot vector operations now live in Mire.** `len`, `get`, `set`, `pop`,
`with_capacity` and `binary::search` read the vector header directly instead
of calling the tcc-compiled runtime, so LLVM can inline them. `alloc_pressure`
fell from 150 ms to 98 ms (allocation + eight pushes + free, eight million
times), `binary_search` from 48 ms to 22 ms, and `vec_pop_drain` from 55 ms to
49 ms. `push` stays a thin extern call: the MIR code generator already expands
`rt_vecs_push_i64` into an inline fast path plus a cold growth call, and
forcing a bigger Mire body measured neutral to slightly slower.

**Code layout is deterministic and cache-line aligned.** Functions are emitted
with `align 32`, extern wrappers are written in sorted order, and `llc` aligns
non-fall-through blocks to 64 bytes. Before this, wrapper order came from a
`HashSet` and adding one instruction to an unrelated function could move a hot
loop across a cache line: `vec_growth` moved between 66 ms and 84 ms with
identical instruction counts. Two builds of the same source now produce the
same module.

**Sorting is compiled by LLVM instead of tcc.** `sort_introsort` fell from
292 ms (before the sort moved into Mire) to 59 ms while the hot loop walks the
buffer through a raw pointer; tcc could not optimize the nested loop in the C
runtime. The remaining 9.83x gap to Rust is algorithmic: the Rust leg calls the
standard library's sort while the C and Mire legs deliberately use the same
insertion sort.

**The former O(n²) string scan is gone.** `string_scan` fell from 3677 ms to
17 ms once `str::index::from` let the kernel resume the search without
`substr`-ing the remaining input on every match. It is now below the noise
floor and at parity with Rust.

**Remaining gaps are concentrated.** `matrix_rowmajor` (4.40x C but 1.00x Rust)
and `vec_partition` (2.40x C but 1.20x Rust) point at the C reference's
autovectorization rather than at Mire codegen. `alloc_pressure` is the largest
real gap at 1.66x C: the C leg allocates 64 bytes inline and never calls a push
helper, while Mire pays the PAL allocation/free calls and a vector header.

The former 3.12x C2 result is withdrawn: it was affected by a vector-push
ownership defect and is not comparable. Always run `verify` before publishing
measurements; use paired runs when attributing a change to a code revision.
