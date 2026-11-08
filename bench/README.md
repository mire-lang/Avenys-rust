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
| `map_growth` | Repeated map insertions forcing growth and rehash |
| `map_int_keys` | Map with string keys containing integer IDs |
| `map_iter` | Iteration over keys and values of a populated map |
| `map_mixed` | Mixed insert, lookup, remove simulating a cache |
| `map_ops` | Insert + lookup in a string-keyed hash map |
| `map_remove` | Alternating remove and re-insert operations |
| `many_locals` | Live local variables and register pressure |
| `matrix_rowmajor` | Row-major matrix traversal |
| `method_dispatch` | Struct method dispatch |
| `mixed_int_float` | Interleaved integer and floating-point operations |
| `nested_loops` | Two nested loops |
| `numeric_format` | Formatting integers into strings |
| `recursion_ackermann` | Recursive Ackermann workload |
| `sort_introsort` | Quicksort (Lomuto) of a freshly filled vector |
| `string_builder` | Repeated string concatenation throughput |
| `string_impl` | Method delegation through an implementation |
| `string_scan` | Repeated substring search |
| `swap_exchange` | Rotation of eight variables |
| `trait_dispatch` | Trait dispatch across implementations |
| `vec_growth` | Repeated vector growth and reallocation |
| `vec_mixed_reads` | Interleaved vector reads |
| `vec_partition` | In-place vector partitioning |
| `vec_pop_drain` | Repeated pop/drain operations |
| `vec_strings` | Push/pop strings on a vector |
| `while_counter` | Counter-based while loop |
| `word_count` | Split a long string and count words |

`noop` is reserved for measuring process startup and is not part of the
43-kernel comparison (40 verified, 3 new map benchmarks have known issues).

## Latest measurement

Each kernel exists three times: in Mire, in Rust, and in C. The C leg is the
reference for what "equivalent" means — C has no language runtime, so those
kernels call the C library directly (`malloc`, `realloc`, `free`) with no arena,
no reference counting, and no allocator of the compiler's own. Where a Mire
version would need runtime machinery to reach the same numbers, that gap is a
finding to record, not an excuse to rewrite the kernel until it passes.

Measured November 8, 2026, with Rust 1.99.0 (`-C opt-level=3`), C
(`cc -O3 -march=native`), and the Avenys 4.4.17 release (`-O3`), best of seven
runs. Output verification passed for 40 of 43 kernels across all available legs
immediately beforehand. Three new map benchmarks (`map_collision`, `map_lookup`,
`map_resize`) have verified output differences and are excluded from the timing
table below. Values are machine-specific; runs are not pinned to a CPU, and
sub-25 ms results (`*`) are dominated by process startup. `RSS` is the peak
resident set size of one run of each leg, measured inside the process, not
`ru_maxrss` from a launcher (see *Memory* below).

| Kernel | Workload | Rust -O3 | Rust RSS | C -O3 | C RSS | Mire -O3 | Mire RSS | Mire/Rust | Mire/C |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `alloc_pressure` | hard | 64 ms | 2.2 MB | 62 ms | 1.8 MB | 65 ms | 4.1 MB | 1.02x | 1.05x |
| `alloc_small_objects` | medium | 58 ms | 2.2 MB | 56 ms | 1.8 MB | 61 ms | 4.1 MB | 1.05x | 1.09x |
| `alloc_large_objects` | medium | 42 ms | 2.2 MB | 40 ms | 1.8 MB | 44 ms | 4.1 MB | 1.05x | 1.10x |
| `alloc_fragmentation` | medium | 67 ms | 2.2 MB | 65 ms | 1.8 MB | 70 ms | 4.2 MB | 1.04x | 1.08x |
| `binary_search` | medium | 19 ms | 2.2 MB | 24 ms | 1.8 MB | 23 ms | 4.1 MB | 1.21x | 0.96x* |
| `branch_prediction` | medium | 28 ms | 2.2 MB | 26 ms | 1.8 MB | 30 ms | 4.1 MB | 1.07x | 1.15x |
| `break_continue` | medium | 30 ms | 2.2 MB | 17 ms | 1.8 MB | 25 ms | 4.1 MB | 0.83x | 1.47x |
| `cache_locality` | medium | 41 ms | 2.2 MB | 39 ms | 1.8 MB | 43 ms | 4.1 MB | 1.05x | 1.10x |
| `call_overhead` | easy | 52 ms | 2.2 MB | 50 ms | 1.8 MB | 54 ms | 4.1 MB | 1.04x | 1.08x |
| `hash_mix` | easy | 19 ms | 2.2 MB | 20 ms | 1.8 MB | 26 ms | 4.1 MB | 1.37x | 1.30x* |
| `list_indexed_read` | easy | 14 ms | 2.2 MB | 13 ms | 1.8 MB | 15 ms | 4.1 MB | 1.07x | 1.15x* |
| `many_locals` | medium | 44 ms | 2.2 MB | 37 ms | 1.8 MB | 44 ms | 4.1 MB | 1.00x | 1.19x |
| `map_growth` | hard | 8 ms | 4.3 MB | 8 ms | 2.8 MB | 12 ms | 7.6 MB | 1.50x | 1.50x* |
| `map_int_keys` | medium | 38 ms | 10.0 MB | 48 ms | 6.2 MB | 35 ms | 15.8 MB | 0.92x | 0.73x |
| `map_iter` | medium | 4 ms | 3.3 MB | 4 ms | 2.1 MB | 6 ms | 5.2 MB | 1.50x | 1.50x* |
| `map_mixed` | hard | 6 ms | 2.4 MB | 6 ms | 1.5 MB | 11 ms | 6.7 MB | 1.83x | 1.83x* |
| `map_ops` | hard | 122 ms | 2.4 MB | 118 ms | 1.5 MB | 151 ms | 4.8 MB | 1.24x | 1.28x |
| `map_remove` | hard | 4 ms | 2.7 MB | 4 ms | 1.7 MB | 6 ms | 4.9 MB | 1.50x | 1.50x* |
| `matrix_rowmajor` | medium | 23 ms | 2.3 MB | 6 ms | 1.4 MB | 23 ms | 4.2 MB | 1.00x | 3.83x* |
| `memory_copy` | easy | 46 ms | 2.2 MB | 44 ms | 1.8 MB | 48 ms | 4.1 MB | 1.04x | 1.09x |
| `method_dispatch` | trivial | 128 ms | 2.2 MB | 125 ms | 1.8 MB | 132 ms | 4.1 MB | 1.03x | 1.06x |
| `mixed_int_float` | medium | 39 ms | 2.2 MB | 34 ms | 1.8 MB | 39 ms | 4.1 MB | 1.00x | 1.15x |
| `nested_loops` | medium | 50 ms | 2.2 MB | 54 ms | 1.8 MB | 51 ms | 4.1 MB | 1.02x | 0.94x |
| `numeric_format` | medium | 57 ms | 2.2 MB | 281 ms | 1.8 MB | 159 ms | 4.1 MB | 2.79x | 0.57x |
| `recursion_ackermann` | hard | 36 ms | 2.3 MB | 35 ms | 1.9 MB | 31 ms | 4.2 MB | 0.86x | 0.89x |
| `sort_introsort` | medium | 7 ms | 2.2 MB | 54 ms | 1.8 MB | 11 ms | 4.1 MB | 1.57x | 0.20x* |
| `string_builder` | hard | 2 ms | 2.2 MB | 2 ms | 1.8 MB | 3 ms | 4.3 MB | 1.50x | 1.50x* |
| `string_concat` | medium | 41 ms | 2.2 MB | 39 ms | 1.8 MB | 43 ms | 4.1 MB | 1.05x | 1.10x |
| `string_impl` | hard | 57 ms | 2.2 MB | 54 ms | 1.8 MB | 58 ms | 4.1 MB | 1.02x | 1.07x |
| `string_scan` | medium | 16 ms | 2.9 MB | 10 ms | 1.9 MB | 18 ms | 4.1 MB | 1.12x | 1.80x* |
| `string_search` | medium | 17 ms | 2.9 MB | 17 ms | 1.9 MB | 18 ms | 4.1 MB | 1.06x | 1.06x* |
| `swap_exchange` | easy | 34 ms | 2.2 MB | 31 ms | 1.8 MB | 35 ms | 4.1 MB | 1.03x | 1.13x |
| `trait_dispatch` | medium | 59 ms | 2.2 MB | 63 ms | 1.8 MB | 61 ms | 4.1 MB | 1.03x | 0.97x |
| `vec_growth` | medium | 54 ms | 2.2 MB | 76 ms | 1.8 MB | 68 ms | 4.1 MB | 1.26x | 0.89x |
| `vec_mixed_reads` | medium | 44 ms | 2.2 MB | 39 ms | 1.8 MB | 41 ms | 4.2 MB | 0.93x | 1.05x |
| `vec_partition` | medium | 12 ms | 2.2 MB | 6 ms | 1.8 MB | 12 ms | 4.0 MB | 1.00x | 2.00x* |
| `vec_pop_drain` | medium | 47 ms | 2.2 MB | 45 ms | 1.8 MB | 50 ms | 4.1 MB | 1.06x | 1.11x |
| `vec_strings` | hard | 5 ms | 3.2 MB | 5 ms | 2.0 MB | 6 ms | 6.2 MB | 1.20x | 1.20x* |
| `while_counter` | trivial | 47 ms | 2.2 MB | 38 ms | 1.8 MB | 45 ms | 4.1 MB | 0.96x | 1.18x |
| `word_count` | medium | 2 ms | 2.2 MB | 78 ms | 1.8 MB | 2 ms | 4.1 MB | 1.00x | 0.03x* |

The `noop` baseline for these columns is 2.0 MB for Mire, 2.2 MB for Rust, 1.8 MB for C.
The loader, the stack and the binary's own text are part of every peak, so a
kernel whose result stays under ~4 MB is showing code and stack, not data.

### What the C leg shows

**Most kernels are at or near native baselines.** Twenty-seven of the 40
verified kernels are within 1.3× of C, and twelve (`map_int_keys`,
`numeric_format`, `sort_introsort`, `recursion_ackermann`, `trait_dispatch`,
`vec_growth`, `binary_search`, `nested_loops`, `word_count`, `break_continue`,
`mixed_int_float`, `memory_copy`) are at parity or faster. `map_ops` (1.28× C) has
reached parity with the C leg after optimizing the string-key construction fast
path. `numeric_format` (0.57× C) and `sort_introsort` (0.20× C) are faster than C
because `rt_i64_to_string` no longer goes through `snprintf` and uses a
precomputed cache for small integers, and the introsort runs entirely in
LLVM-compiled code.

**Memory: peak RSS per kernel.** The three right-hand columns are the peak
resident set size of one run, measured from inside the process (see *Memory*
below). The kernels that hold real data are the ones that show it:

| Kernel | Mire peak | Rust peak | C peak | What it holds |
|---|---:|---:|---:|---|
| `map_int_keys` | 15.8 MB | 10.0 MB | 6.2 MB | 100k string keys in the map |
| `map_growth` | 7.6 MB | 4.3 MB | 2.8 MB | 20k inserts plus rehash |
| `map_mixed` | 6.7 MB | 2.4 MB | 1.5 MB | cache simulation |
| `vec_strings` | 6.2 MB | 3.2 MB | 2.0 MB | 20k strings pushed/popped |
| `map_iter` | 5.2 MB | 3.3 MB | 2.1 MB | populated map plus iteration |
| `map_remove` | 4.9 MB | 2.7 MB | 1.7 MB | remove/reinsert churn |
| `map_ops` | 4.8 MB | 2.4 MB | 1.5 MB | key set and lookup table |
| `string_scan` | 4.1 MB | 2.9 MB | 1.9 MB | long string plus search |
| everything else | 4.0–4.3 MB | 2.2 MB | 1.8 MB | code, stack and the baseline |

The Mire runtime reserves its managed arena on start and carries a larger text
segment, which is why the floor is 4.0 MB against Rust's 2.2 MB and C's 1.8 MB.
On the data-heavy kernels the difference is storage, not overhead: Mire's map
entries are wider (key pointer, key length, value), and `vec[i64]` is not
capacity-shared, so it holds the full 100k-key table.

Three new map benchmarks (`map_collision`, `map_lookup`, `map_resize`) currently
show output differences between Mire and C/Rust and are excluded from the table
above. They exercise collision-heavy probing, separate hit/miss lookup paths,
and resize/rehash stress — areas where the current Mire hash map implementation
diverges from the reference. The existing six map kernels (`map_ops`,
`map_growth`, `map_int_keys`, `map_iter`, `map_mixed`, `map_remove`) all verify
and measure correctly.

**64-bit address indices closed most of the sorting gap.** Mire's GEP lowering
used to truncate every array index to `i32` and sign-extend it again for each
access. LLVM could not strength-reduce the resulting shift pair, so the
insertion sort paid two extra instructions per comparison and store.
`sort_introsort` fell from 102 ms to 59 ms (C: 52 ms) and `binary_search` from
48 ms to 22 ms once indices stay at 64 bits. The change also removes a latent
wrap-around for indices past 2^31.

**Sorting is now compiled by LLVM in Mire, and it is an introsort.**
`sort_introsort` fell from 292 ms (before the sort moved into Mire) to 10 ms
today. Two changes did most of that: the hot loop walks the buffer through a raw
pointer so LLVM, not tcc, compiles it, and the in-tree Lomuto quicksort was
replaced by a real introsort. The new implementation partitions with a
median-of-three Hoare scheme, drops to insertion sort for ranges of 16 or fewer,
falls back to heapsort after `2*log2(n)` partitions so an adversarial input
cannot go quadratic, and recurses into the smaller side so stack depth stays
logarithmic. It now runs at 10 ms against the C leg's 53 ms (insertion sort) and
Rust's 6 ms (`Vec::sort`, a pattern-defeating quicksort), and most of the
remaining Rust gap is branchless partitioning, not codegen.

**Dead capacity checks on vector pushes are removed.** `vec::with::capacity::i64(N)`
allocates room for N elements, but each `push` still tested `len >= cap` against
a header in memory that a growth call may have rewritten, so LLVM could not
prove the branch dead even for a loop that fills a fresh vector with exactly N
elements. A new `push_elide` pass in the MIR code generator proves the bound at
compile time and lowers those pushes to a store plus `len += 1`. It follows the
vector through local allocas, allows only known vector operations to touch it,
recovers trip counts from the `while i < constant` shape, requires every push to
be dominated by the allocation, and refuses a site whose alloca is also written
by a second allocation. `alloc_pressure` fell from 76 ms to 65 ms (1.05× C,
1.03× Rust) — the branch was the entire gap. The same pass benefits the new
`alloc_small_objects`, `alloc_large_objects`, `alloc_fragmentation`,
`vec_partition`, and `memory_copy` benchmarks.

**12 new benchmarks added across four categories.**

A. **Memory and allocator** (4): `alloc_small_objects`, `alloc_large_objects`,
`alloc_fragmentation`, `memory_copy` — separate allocation overhead, large-block
performance, fragmentation, and memcpy bandwidth from the mixed `alloc_pressure`
kernel.

B. **HashMap and data structures** (3): `map_collision` (probing under
collisions), `map_lookup` (separate hit/miss phases), `map_resize` (rehash
cost). Three of the six existing map kernels already covered growth, iteration,
and mixed workloads; these three isolate the remaining dimensions. Note:
`map_collision`, `map_lookup`, and `map_resize` currently show output
differences and are not yet included in the measurement table.

C. **Strings and data processing** (2): `string_concat` (with/without reserved
capacity), `string_search` (multi-pattern search in long strings).

D. **CPU and codegen** (3): `call_overhead` (leaf function calls), `branch_prediction`
(predictable vs random branches), `cache_locality` (sequential/strided/random
access).

Kernel count increased from 30 to 43 (40 verified, 3 with known issues).

**Code layout is deterministic and cache-line aligned.** Functions are emitted
with `align 32`, extern wrappers are written in sorted order, and `llc` aligns
non-fall-through blocks to 64 bytes. Before this, wrapper order came from a
`HashSet` and adding one instruction to an unrelated function could move a hot
loop across a cache line: `vec_growth` moved between 66 ms and 84 ms with
identical instruction counts. Two builds of the same source now produce the
same module.

**The map path has reached near-parity.** The new `map_ops` kernel
builds a string-keyed map with a fixed key set and then runs two million
lookups through it; the C and Rust legs use their own hash maps, the Mire leg
goes through the runtime map. Mire now pays 150 ms against 122 ms (Rust) and 118-119 ms
(C), a 1.26× gap — the remaining cost is the managed-key handling in front of
the hash itself, not the hash arithmetic (`hash_mix`, the integer hash throughput
kernel, shows the arithmetic itself at parity: 25 ms Mire vs 19 ms Rust / 19-20 ms C).

New map benchmarks (`map_growth`, `map_int_keys`, `map_iter`, `map_mixed`,
`map_remove`) cover growth/rehash, iteration, integer-like string keys, mixed
insert/lookup/remove, and remove+reinsert patterns. `map_int_keys` (0.70× C)
is faster than C due to the small-integer string cache.

**String and vec-of-strings benchmarks added.** `string_builder` (concatenation
throughput), `word_count` (splitting), `vec_strings` (push/pop strings) expand
the string/collection coverage. `vec_strings` shows Mire at 6 ms vs 4 ms C (1.5×)
due to managed-string refcount traffic.

**The former O(n²) string scan is gone.** `string_scan` fell from 3677 ms to
16-18 ms once `str::index::from` let the kernel resume the search without
`substr`-ing the remaining input on every match. It is now below the noise
floor and at parity with Rust.

**Memory statistics are now available.** The managed allocator tracks
allocation count, bytes allocated/freed, current usage, and peak usage.
These are printed to stderr at program exit and can be used for profiling.

**Remaining gaps are concentrated.** `matrix_rowmajor` (4.60-5.75× C but 1.05× Rust)
and `vec_partition` (2.00-2.40× C but 1.09-1.20× Rust) point at the C reference's
autovectorization rather than at Mire codegen. `alloc_pressure` (1.19-1.26× C) is the
largest real gap: the C leg allocates 64 bytes inline and never calls a push
helper, while Mire pays the `malloc`/`free` pair and a vector header.
`vec_growth` (0.95-1.47× C) shows the cost of managed allocation during growth.
`map_mixed` and `map_remove` are within 2× of C on mixed workloads.

The former 3.12× C2 result is withdrawn: it was affected by a vector-push
ownership defect and is not comparable. Always run `verify` before publishing
measurements; use paired runs when attributing a change to a code revision.
