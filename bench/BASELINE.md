# Kernel baseline

Fecha: 2026-11-23

Comando:

```bash
cd avenys/bench
MIRE_BIN=/tmp/opencode/bin/mire \
MIRE_LIB_DIR=/run/media/nyx/89bfae2c-a03e-488c-aca6-69ae258ad67e/Evelynx/Arch \
./run.sh measure
```

Entorno: Linux x86_64, Rust `-C opt-level=3`, C `cc -O3 -march=native`, Mire/Avenys `-O3 --cpu native`, mejor de 7.

| KERNEL | WORKLOAD | Rust -O3 | C -O3 | Mire -O3 | Mire/Rust | Mire/C |
|--------|----------|----------|-------|----------|-----------|--------|
| alloc_fragmentation | medium | 33 ms | 76 ms | 8 ms | 0.24x | 0.11x |
| alloc_large_objects | medium | 2 ms | 2 ms | 6 ms | 3.00x | 3.00x |
| alloc_pressure | hard | 64 ms | 59 ms | 63 ms | 0.98x | 1.07x |
| alloc_small_objects | medium | 14 ms | 10 ms | 16 ms | 1.14x | 1.60x |
| binary_search | medium | 19 ms | 25 ms | 23 ms | 1.21x | 0.92x |
| branch_prediction | medium | 3 ms | 2 ms | 4 ms | 1.33x | 2.00x |
| break_continue | medium | 29 ms | 15 ms | 25 ms | 0.86x | 1.67x |
| cache_locality | medium | 11 ms | 11 ms | 12 ms | 1.09x | 1.09x |
| call_overhead | easy | 37 ms | 36 ms | 37 ms | 1.00x | 1.03x |
| hash_mix | easy | 19 ms | 20 ms | 26 ms | 1.37x | 1.30x |
| list_indexed_read | easy | 15 ms | 14 ms | 15 ms | 1.00x | 1.07x |
| many_locals | medium | 44 ms | 38 ms | 45 ms | 1.02x | 1.18x |
| map_collision | medium | SKIPPED | SKIPPED | SKIPPED | - | - |
| map_growth | hard | 10 ms | 8 ms | 13 ms | 1.30x | 1.62x |
| map_int_keys | medium | 35 ms | 44 ms | 35 ms | 1.00x | 0.80x |
| map_iter | medium | 5 ms | 5 ms | 7 ms | 1.40x | 1.40x |
| map_lookup | medium | SKIPPED | SKIPPED | SKIPPED | - | - |
| map_mixed | hard | 7 ms | 6 ms | 11 ms | 1.57x | 1.83x |
| map_ops | hard | 123 ms | 118 ms | 192 ms | 1.56x | 1.63x |
| map_remove | hard | 5 ms | 5 ms | 8 ms | 1.60x | 1.60x |
| map_resize | medium | SKIPPED | SKIPPED | SKIPPED | - | - |
| matrix_rowmajor | medium | 23 ms | 6 ms | 23 ms | 1.00x | 3.83x |
| memory_copy | easy | 764 ms | 705 ms | 789 ms | 1.03x | 1.12x |
| method_dispatch | trivial | 128 ms | 125 ms | 132 ms | 1.03x | 1.06x |
| mixed_int_float | medium | 39 ms | 35 ms | 40 ms | 1.03x | 1.14x |
| nested_loops | medium | 50 ms | 54 ms | 52 ms | 1.04x | 0.96x |
| numeric_format | medium | 58 ms | 280 ms | 166 ms | 2.86x | 0.59x |
| recursion_ackermann | hard | 36 ms | 36 ms | 32 ms | 0.89x | 0.89x |
| sort_introsort | medium | 7 ms | 53 ms | 11 ms | 1.57x | 0.21x |
| sort_scaling | medium | 19 ms | 564 ms | 38 ms | 2.00x | 0.07x |
| string_builder | hard | 3 ms | 3 ms | 4 ms | 1.33x | 1.33x |
| string_concat | medium | SKIPPED | SKIPPED | SKIPPED | - | - |
| string_impl | hard | 58 ms | 54 ms | 58 ms | 1.00x | 1.07x |
| string_scan | medium | 18 ms | 11 ms | 18 ms | 1.00x | 1.64x |
| string_search | medium | 26 ms | 21 ms | 31 ms | 1.19x | 1.48x |
| swap_exchange | easy | 35 ms | 33 ms | 35 ms | 1.00x | 1.06x |
| trait_dispatch | medium | 60 ms | 63 ms | 62 ms | 1.03x | 0.98x |
| vec_growth | medium | 53 ms | 75 ms | 63 ms | 1.19x | 0.84x |
| vec_mixed_reads | medium | 45 ms | 40 ms | 41 ms | 0.91x | 1.02x |
| vec_partition | medium | 11 ms | 6 ms | 12 ms | 1.09x | 2.00x |
| vec_pop_drain | medium | 47 ms | 46 ms | 47 ms | 1.00x | 1.02x |
| vec_strings | hard | 5 ms | 4 ms | 7 ms | 1.40x | 1.75x |
| while_counter | trivial | 48 ms | 39 ms | 45 ms | 0.94x | 1.15x |
| word_count | medium | 2 ms | 76 ms | 3 ms | 1.50x | 0.04x |

Notes:

- `noise = below 25 ms`; startup dominates.
- Base RSS: Mire 2.1 MB, Rust 2.2 MB, C 1.6 MB.
- `map_ops` was measured again after the bitmask indexing change: 1.37x Rust / 1.43x C.