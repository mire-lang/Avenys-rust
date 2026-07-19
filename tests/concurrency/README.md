# Concurrency / Threading Tests

These `.mire` scripts exercise the `thread::spawn` / `thread::join` builtins and closure support.

Run a single test:

```bash
cargo run --bin mire -- run tests/concurrency/spawn_join_return.mire
```

Run all tests:

```bash
for f in tests/concurrency/*.mire; do
 echo "=== $f ==="
 cargo run --bin mire -- run "$f"
done
```

## Test Coverage

- **spawn_join_return.mire** — `thread::spawn(() => 42)` followed by `thread::join` returns the closure's i64 result.
- **closure_captures.mire** — A closure stored in a variable captures an outer `i64` and returns `data + 1` from the spawned thread.
- **multistmt_closure.mire** — Multi-statement closure body `{ ... }` with captures, spawned via `thread::spawn`.
- **multistmt_callable.mire** — Multi-statement closure invoked directly as a callable variable (`g(5)`).
- **thread_pool.mire** — Spawns several deterministic worker threads and sums their returned values.

## Notes

- Warnings about "unused variables" are false positives for variables that are captured by closures or used as call targets; the compiler's dead-code detector currently does not recognise those uses.
- Shared mutable state between threads is not yet supported (no atomics or locks in the language).
