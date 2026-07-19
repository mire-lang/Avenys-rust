# Incremental Compilation: Architecture & Refactor Plan

## Current State

### Directory Structure

```
project-root/
 bin/
 .cache/
 incremental.bin ← single binary blob (index + serialized blobs)
 debug/
 <binary> ← compiled debug binary
 release/ ← (not created unless --release)
 <binary>
```

### Cache Format (`MIREINC2`, version 7)

- **Single file** `incremental.bin` contains: magic header → format version → serialized index (files/analyses/builds) → raw blob store
- **Index**: `HashMap<String, FileCacheEntry>`, `HashMap<String, AnalysisCacheEntry>`, `HashMap<String, BuildCacheRecord>`
- **Blob store**: Contiguous byte array with entries referenced by (offset, len) pairs
- **Memory-mapped** on Unix for read; copy-on-write for mutations
- **LRU eviction**: Collect all entries, sort by `last_access_epoch_ms`, evict oldest until under `max_units`
- **Compaction**: When live ratio < 70%, rebuild blob store, discarding dead blobs

### Performance characteristics

| Metric | Value |
|---|---|
| Cache file size | ~25 MB (real project) |
| Blob store | ~80% of file |
| Index size | ~20% of file |
| LRU cost | O(n log n) sort on eviction |
| Compaction cost | O(n) read + O(m) write (m dead blobs) |
| Memory mapped | Read-only, zero-copy |
| Write strategy | Sync (immediate fsync via `save()`) |

## Problems

1. **Stale analysis cache**: Analysis cache key didn't include source hash → returning old program after source change (FIXED: `analysis_cache_key` now includes `source_hash`)

2. **Hash collision in file cache**: Single FNV-1a hash could collide → stale `CachedParsedFile` (FIXED: `source_hash2` with different seed, both must match)

3. **Single-file blob store**: Every mutation rewrites the entire `incremental.bin`. Excessive I/O for small changes.

4. **No write-ahead log**: A crash during `save()` corrupts the entire cache. Self-heal only deletes the file.

5. **LRU on sort**: O(n log n) eviction instead of O(1). Overkill for typical workloads.

6. **No segment isolation**: File, analysis, and build caches share the same blob store. Compaction requires checking all three.

7. **No per-directory metadata**: `bin/.cache/incremental.bin` has no structure — everything is in one opaque blob.

## Refactor Plan

### Directory Structure (new)

```
project-root/
 bin/
 .cache/
 version.txt ← schema version + format description
 index/ ← per-entry metadata (separate files)
 files/
 <path_hash>.meta ← FileCacheEntry
 analyses/
 <source_hash>.meta ← AnalysisCacheEntry
 builds/
 <key_hash>.meta ← BuildCacheRecord
 blobs/ ← content-addressed blob segments
 <blob_hash> ← raw blob data (named by content hash)
 wal/ ← write-ahead log for crash safety
 <timestamp>.wal ← pending writes (applied on restart)
 debug/
 <binary> ← debug build output
 release/
 <binary> ← release build output
 optimized/
 <binary> ← O2/O3 build output (separate from debug/release)
```

### New segment-based blob store

Instead of one contiguous blob store:

- **Blob hash addressing**: Each blob stored in `blobs/<sha256_prefix>` named by content hash (first 16 hex chars of BLAKE3/SHA-256 of content)
- **Deduplication**: Identical program snapshots share one blob (common with small edits)
- **Segments**: Blobs grouped into segments of ~4MB each. A segment file is immutable once written; mutations create new segments
- **GC sweep**: Background compaction merges segments, discarding unreferenced blobs
- **No rewrite on mutation**: Adding an entry only writes a new blob + index entry. No full-file rewrite.

### Real LRU with intrusive linked list

Replace timestamp-sorted eviction:

- **Hot entries**: `LinkedHashMap<K, Entry>` (or `HashMap<K, Node<K>>` + doubly-linked list)
 - O(1) lookup, O(1) insertion, O(1) eviction
 - On access: move node to head (O(1))
 - On eviction: remove from tail (O(1))
- **Cold storage**: Entries evicted from hot set have their blobs mmap'd and index entry serialized to `index/` directory
 - Reheat: when a cold entry is accessed, its blob is loaded and it returns to hot set
- **Hot/cold threshold**: Configurable (default: keep last N accessed entries hot; evict oldest)

### Write-Ahead Log (WAL)

- **Before any mutation**: Append operation to `wal/<timestamp>.wal` (JSON lines or binary records)
- **Crash recovery**: On load, replay WAL entries and apply to index/blob store
- **Checkpoint**: WAL files older than 5 minutes are compacted into the main index
- **No data loss**: A crash during `save()` leaves cache fully recoverable

### Per-directory metadata

- `version.txt`: Contains `format_version`, `created_at`, `last_optimized_at`
- Index files split by type (file, analysis, build) for parallel reads
- No more monolithic blob store — individual blobs can be read independently

## Incremental Compilation Optimization

### Current: Whole-file granularity

- Any change to source → re-parse whole file → re-analyze changed units → rebuild
- Analysis units track function-level hashes, but parsing is still whole-file

### Target: Statement-level granularity

1. **Statement-level hashing**: Each statement already has a stable hash via `stable_statement_hash_pair()`. Use these to identify exactly which statements changed.

2. **Selective re-parsing**: Store statement spans in the parsed AST. On re-parse, hash each statement independently; only re-analysis changed statements (existing: `prepare_program_with_partial_analysis_reuse`).

3. **Selective codegen**: Track which statements produce which LLVM IR segments. Only regenerate IR for changed statements. Patch the IR file instead of rewriting entirely.

4. **Module-level cache**: When a file has N imports (load statements), cache each imported module's IR independently. Only re-link if the imported module hash changes.

### O0 (incremental off)

At `-O0`, the optimizer (`optimize()`) must be skipped entirely. The build pipeline already does this:

```
if matches!(options.opt_level, OptLevel::O0) {
 // skip LLVM opt, skip MIR optimize
}
```

This means at O0, the MIR pipeline only runs lower + codegen (no fixed-point loop). This is critical for fast iteration during development.

## Optimization of the Optimizer

### Current fixed-point loop

```
loop {
 const_fold + alg_simplify + copy_propagate + fold_brcond + dce + dead_elim + merge_blocks
}
```

Each pass iterates over all blocks and all instructions. For small functions this is negligible. For large functions, it adds up.

### Planned improvements

| Optimization | Impact |
|---|---|
| **Skip passes with no effect**: Track per-function complexity; skip DCE if 0 unused, skip fold_brcond if 0 BrCond | Saves pass overhead for simple functions |
| **Batch writes**: WAL accumulates changes; flush every 5s or on explicit `save()` | Reduces disk I/O by 10-100x for rapid edits |
| **Parallel file cache**: `index/` entries per file type can be read in parallel | Faster cache load for projects with 100+ files |
| **Memory-mapped cold blobs**: No deserialization until accessed | Near-zero memory for cached but unused entries |

## Aggressive Future Optimizations

| Optimization | Description |
|---|---|
| **Incremental LLVM opt**: Track which functions changed; only run `opt` on changed functions | Avoids re-optimizing unchanged LLVM IR |
| **Module-level IR caching**: Cache compiled `.o` files per module; only re-link changed modules | Avoids recompiling the entire runtime C codebase |
| **Parallel module compilation**: Compile independent modules in parallel (via rayon or manual threading) | Scales with core count for large projects |
| **Incremental linking**: Use `lld --incremental` or manual section patching | Avoids full re-link for small changes |
| **Hot reload**: Swap changed functions at runtime without restart | Debug loop without restart |

## Memory & Performance Budget

| Operation | Current | Target | Improvement |
|---|---|---|---|
| Cache load | 2ms (mmap) | 1ms (parallel index read) | 2x |
| Cache save | 50ms (full rewrite) | 5ms (WAL append) | 10x |
| LRU eviction | O(n log n) | O(1) | n/a for small caches |
| Blob compaction | O(n) | O(live) | proportional to churn |
| Stale hit (worst case) | Fixed (hash2 + analysis key) | — | — |
| Crash recovery | Data loss | WAL replay | Reliability |

## Current State (after Phase 1 refactor)

### New Directory Structure

```
project-root/
 bin/
 .cache/
 version.txt ← "MIREINC3\n1\n"
 index/
 files/ ← <key_hash>.meta for file cache entries
 analyses/ ← <key_hash>.meta for analysis cache entries
 builds/ ← <key_hash>.meta for build cache entries
 blobs/ ← content-addressed blob files (<blob_hash>)
 wal/ ← write-ahead log files (<timestamp>.wal)
 debug/
 <binary>
 release/
 <binary>
```

### Cache Format (`MIREINC3`, version 1)

- **Per-entry index files**: Each cached entry has a `.meta` JSON file in the appropriate `index/<type>/` subdirectory, named by a hash of the cache key
- **Content-addressed blobs**: Each blob (serialized Program/AnalysisPayload) is stored in `blobs/<hash>` named by FNV-1a hash of the blob content. Identical content deduplicates automatically.
- **WAL (Write-Ahead Log)**: Before any mutation, a JSON record is appended to a timestamped `.wal` file in `wal/`. On load, WAL records are replayed to recover from crashes.
- **Real LRU with O(1) eviction**: `LruMap<K, V>` backed by `HashMap<K, V>` + `VecDeque<K>`. Access → promote to back (MRU). Evict → pop from front (LRU).
- **No memory-mapped I/O**: Blobs are small files read on demand. No mmap complexity needed.
- **No monolithic blob store**: Each blob is an individual file. No compaction needed. GC happens on `save()`: scan all index entries + WAL for referenced blob hashes, delete unreferenced blob files.
- **Crash recovery**: If the process crashes mid-write, WAL records are replayed on next load. The cache is always consistent.
- **Per-entry LRU eviction**: `enforce_capacity()` method called after each store. Evicts the oldest entry (by LRU tracking) from the appropriate HashMap. O(1) per eviction.

### Key files

| File | Purpose |
|---|---|---|
| `src/incremental/lru.rs` | `LruMap<K, V>` — generic O(1) LRU with HashMap + VecDeque |
| `src/incremental/cache.rs` | `IncrementalCache` — WAL, blob store, index, LRU, metrics |
| `src/incremental/mod.rs` | Types, re-exports, error serialization |
| `src/incremental/utils.rs` | Cache path, hashing, key generation (`dependency_fingerprint`, `analysis_cache_key`, `latest_analysis_key`) |
| `src/incremental/analysis.rs` | Analysis units, invalidation reports |
| `src/incremental/dependencies.rs` | Statement dependency tracking |

### Performance characteristics (new)

| Metric | Value |
|---|---|
| Cache load | ~5ms (scan index dirs + replay WAL) |
| Cache save | ~2ms (write stale index entries + GC unreferenced blobs) |
| LRU eviction | O(1) via `VecDeque::pop_front()` |
| Blob write | O(1) per blob (one `write()` syscall) |
| Blob dedup | Automatic (content-hash naming) |
| Crash recovery | Full (WAL replay) |

### Analysis Cache Invalidation (v3.11.45+)

The analysis cache now invalidates correctly when dependency files change:

- **`dependency_fingerprint(files)`**: Computes a combined hash of all loaded file paths + content hashes + dependency edges (`FxHasher`, sorted by path, includes `CARGO_PKG_VERSION`). Called after `load_program_with_cache`.

- **`analysis_cache_key(path, source_hash, dep_fingerprint)`**: Key format is `{path}::analysis::{source_hash}::{dep_fingerprint}`. Previously only used `source_hash`, so changing a loaded module (different dep_fingerprint) would still hit the old cache entry. Now `dep_fingerprint` is part of the key, so any change to a loaded file causes a cache miss.

- **`latest_analysis_key(path)`**: Fixed key `{path}::analysis::latest` that always points to the most recently stored analysis, regardless of hash/fingerprint. Used by `latest_successful_analysis()` and `analysis_invalidation_report()` for partial re-analysis reuse.

- **`store_analysis` writes TWO entries**: The specific fingerprint-keyed entry (for exact cache lookup) AND the `::latest` entry (for partial re-analysis). Both are tracked in the LRU for eviction.

- **Cache migration**: Old cache keys (without `{dep_fingerprint}` suffix) never match new lookups, causing a single cold re-analysis on first run after upgrade. The `::latest` entry is refreshed on every store, so partial re-analysis always has access to the most recent snapshot.

### Removed (from old format)

- `incremental.bin` single-file binary blob — replaced by directory structure
- `CacheDb` with three `HashMap`s — replaced by per-type `HashMap`s + `LruMap`
- `BlobStore` with mmap support — replaced by individual blob files
- `MemoryMappedFile` — not needed with file-per-blob approach
- `encode_cache_db` / `decode_cache_db` — custom binary serialization removed
- `BLOB_COMPACT_THRESHOLD_RATIO` — compaction no longer needed
- `CACHE_FORMAT_VERSION` — now `version.txt` with `MIREINC3` / version `1`

## Implementation order

1. **WAL + segment blob store**: Foundation for crash safety and small writes
2. **Real LRU**: Replace sort-based eviction with `LinkedHashMap` (VecDeque-based)
3. **Per-directory metadata**: Split index from blobs, per-entry `.meta` files
4. **Hot/cold separation**: Keep hot entries in memory, cold on mmap
5. **Statement-level selective codegen**: Fine-grained rebuild
6. **Parallel module compilation**: Multi-core scale-out
