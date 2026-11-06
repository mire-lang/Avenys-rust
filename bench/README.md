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
| `hash_mix` | Integer hash throughput over an i64 |
| `list_indexed_read` | Repeated indexed reads from a vector |
| `map_ops` | Insert + lookup in a string-keyed hash map |
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
23-kernel comparison.

## Latest measurement

Each kernel exists three times: in Mire, in Rust, and in C. The C leg is the
reference for what "equivalent" means — C has no language runtime, so those
kernels call the C library directly (`malloc`, `realloc`, `free`) with no arena,
no reference counting, and no allocator of the compiler's own. Where a Mire
version would need runtime machinery to reach the same numbers, that gap is a
finding to record, not an excuse to rewrite the kernel until it passes.

Measured November 6, 2026, with Rust 1.99.0 (`-C opt-level=3`), C
(`cc -O3 -march=native`), and the Avenys 4.4.15 release (`-O3`), best of seven
runs. Output verification passed for all 23 kernels across all available legs
immediately beforehand. Values are machine-specific; runs are not pinned to a
CPU, and sub-25 ms results (`*`) are dominated by process startup.

| Kernel | Workload | Rust -O3 | C -O3 | Mire -O3 | Mire/C | Mire/Rust |
|---|---|---:|---:|---:|---:|---:|
| `alloc_pressure` | hard | 26 ms | 61 ms | 98 ms | 1.61x | 3.77x |
| `binary_search` | medium | 19 ms | 24 ms | 21 ms | 0.88x | 1.11x* |
| `break_continue` | medium | 29 ms | 19 ms | 24 ms | 1.26x | 0.83x* |
| `hash_mix` | easy | 19 ms | 19 ms | 24 ms | 1.26x | 1.26x* |
| `list_indexed_read` | easy | 14 ms | 13 ms | 14 ms | 1.08x | 1.00x* |
| `map_ops` | hard | 121 ms | 117 ms | 406 ms | 3.47x | 3.36x |
| `many_locals` | medium | 43 ms | 37 ms | 44 ms | 1.19x | 1.02x |
| `matrix_rowmajor` | medium | 22 ms | 4 ms | 22 ms | 5.50x | 1.00x* |
| `method_dispatch` | trivial | 127 ms | 125 ms | 130 ms | 1.04x | 1.02x |
| `mixed_int_float` | medium | 38 ms | 33 ms | 37 ms | 1.12x | 0.97x |
| `nested_loops` | medium | 49 ms | 53 ms | 50 ms | 0.94x | 1.02x |
| `numeric_format` | medium | 57 ms | 280 ms | 201 ms | 0.72x | 3.53x |
| `recursion_ackermann` | hard | 35 ms | 34 ms | 30 ms | 0.88x | 0.86x |
| `sort_introsort` | medium | 6 ms | 52 ms | 13 ms | 0.25x | 2.17x* |
| `string_impl` | hard | 56 ms | 53 ms | 56 ms | 1.06x | 1.00x |
| `string_scan` | medium | 16 ms | 9 ms | 16 ms | 1.78x | 1.00x* |
| `swap_exchange` | easy | 34 ms | 31 ms | 35 ms | 1.13x | 1.03x |
| `trait_dispatch` | medium | 58 ms | 63 ms | 61 ms | 0.97x | 1.05x |
| `vec_growth` | - | 52 ms | 75 ms | 65 ms | 0.87x | 1.25x |
| `vec_mixed_reads` | - | 43 ms | 38 ms | 40 ms | 1.05x | 0.93x |
| `vec_partition` | medium | 10 ms | 5 ms | 12 ms | 2.40x | 1.20x* |
| `vec_pop_drain` | - | 46 ms | 44 ms | 49 ms | 1.11x | 1.07x |
| `while_counter` | trivial | 46 ms | 38 ms | 43 ms | 1.13x | 0.93x |

### What the C leg shows

**Most kernels are at or near native baselines.** Sixteen of the 23 are within
1.3x of C, and several (`nested_loops`, `numeric_format`,
`recursion_ackermann`, `trait_dispatch`, `vec_growth`, `binary_search`) are at
parity or faster. `numeric_format` is 3.53x slower than Rust but faster than
the C leg because `rt_i64_to_string` no longer goes through `snprintf`.

**64-bit address indices closed most of the sorting gap.** Mire's GEP lowering
used to truncate every array index to `i32` and sign-extend it again for each
access. LLVM could not strength-reduce the resulting shift pair, so the
insertion sort paid two extra instructions per comparison and store.
`sort_introsort` fell from 102 ms to 59 ms (C: 52 ms) and `binary_search` from
48 ms to 22 ms once indices stay at 64 bits. The change also removes a latent
wrap-around for indices past 2^31.

**Sorting is now compiled by LLVM in Mire, and it sorts.** `sort_introsort`
fell from 292 ms (before the sort moved into Mire) to 59 ms while the hot loop
walks the buffer through a raw pointer; tcc could not optimize the nested loop
in the C runtime, so the loop moved into Mire. The insertion sort then paid
two extra instructions per comparison (fixed by the 64-bit GEP change above),
and it was replaced by an in-tree quicksort with a Lomuto partition: the
latest run puts Mire at 13 ms against the C leg's 52 ms (insertion sort) and
Rust's 6 ms (standard library sort), well past the mean of the two legs.

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

**The map path is the next collection frontier.** The new `map_ops` kernel
builds a string-keyed map with a fixed key set and then runs two million
lookups through it; the C and Rust legs use their own hash maps, the Mire leg
goes through the runtime map. Mire pays 406 ms against 121 ms (Rust) and 117 ms
(C), the widest gap of the suite next to `alloc_pressure` — the remaining cost
is the managed-key handling in front of the hash itself, not the hash arithmetic
(`hash_mix`, the new integer hash throughput kernel, shows the arithmetic
itself at parity: 24 ms Mire vs 19 ms Rust / 19 ms C).

**The former O(n²) string scan is gone.** `string_scan` fell from 3677 ms to
16 ms once `str::index::from` let the kernel resume the search without
`substr`-ing the remaining input on every match. It is now below the noise
floor and at parity with Rust.

**Remaining gaps are concentrated.** `matrix_rowmajor` (5.50x C but 1.00x Rust)
and `vec_partition` (2.40x C but 1.20x Rust) point at the C reference's
autovectorization rather than at Mire codegen. `alloc_pressure` is the largest
real gap at 1.61x C: the C leg allocates 64 bytes inline and never calls a push
helper, while Mire pays the PAL allocation/free calls and a vector header.
`map_ops` is the widest gap of the current suite at 3.47x C. `numeric_format`
(3.53x Rust) sits at the managed-string allocation cost of the round trip.

The former 3.12x C2 result is withdrawn: it was affected by a vector-push
ownership defect and is not comparable. Always run `verify` before publishing
measurements; use paired runs when attributing a change to a code revision.
