# Mire Test Suite

This directory contains organized tests for the Mire compiler (Avenys).

## Structure

```
tests/
├── level/ # Difficulty-based tests
│ ├── beginner/ # Basic syntax and features (5 tests)
│ ├── intermediate/ # Functions, collections, loops (5 tests)
│ └── advanced/ # Structs, enums, impl (2 tests)
├── type/ # Type-specific tests
│ ├── structs/ # Struct tests (2 tests)
│ ├── enums/ # Enum tests (2 tests)
│ ├── collections/ # Vector, array, map tests (2 tests)
│ └── primitives/ # Basic types (1 test)
├── complex/ # Real-world algorithms and data structures
│ ├── algorithms/ # Sorting, searching, etc.
│ ├── data_structures/ # Structs, enums, stacks
│ └── math/ # Math operations
├── edge/ # Edge cases and stress tests
│ ├── arrays/ # Array indexing tests
│ ├── loops/ # Nested loop tests
│ ├── recursion/ # Recursive function tests
│ └── error_handling/ # Result/Option pattern tests
├── behavior/ # Compiler behavior tests
│ ├── typeck/ # Type checking behavior (2 tests)
│ └── borrowck/ # Ownership/borrow checking (3 tests)
├── modules/ # Module import tests
├── verify/ # Expected output verification
│ └── expected/ # Expected output files
└── smoke.mire # Quick smoke test
```

## Running Tests

```bash
# Run a single test
./target/release/mire test tests/level/beginner/01_hello_world.mire

# Run all tests in a directory (recursively)
./target/release/mire test tests/level/

# Run all integration tests from tests/ (auto-detected)
./target/release/mire test --verbose

# Compile-check only (no execution)
./target/release/mire test --no-run

# Run with timing
./target/release/mire run tests/complex/algorithms/01_sum_loop.mire --ms
```

## Test Strategy

| Layer | Tool | Purpose |
|-------|------|---------|
| Compiler regressions | Rust `#[test]` (cargo test) | Type checking, borrowck, loader, incremental cache |
| Integration tests | `mire test` | End-to-end compilation and execution |
| PAL unit tests | C tests via `cc` crate | Platform abstraction layer correctness |
| Manual smoke | `mire run` | Ad-hoc verification |

## Test Status Summary

| Category | Tests | Status |
|----------|-------|--------|
| level/beginner | 5 | Passing |
| level/intermediate | 5 | Passing |
| level/advanced | 2 | Passing |
| type/structs | 2 | Passing |
| type/enums | 2 | Passing |
| type/collections | 2 | Passing |
| type/primitives | 1 | Passing |
| complex/algorithms | 9 | 7 , 2 |
| complex/data_structures | 14 | 11 , 3 |
| complex/math | 2 | Passing |
| edge/arrays | 4 | Passing |
| edge/loops | 3 | Passing |
| edge/recursion | 1 | Passing |
| edge/error_handling | 1 | Passing |
| behavior/typeck | 2 | Passing |
| behavior/borrowck | 3 | Partial |
| modules | 3 | Passing |

## Known Issues

See `docs/issues.md` for documented issues and limitations.
- **math advanced surface**: core helpers now exist, but further overloads and
 vectorized statistics are still open work.
- **List HOF scope**: `lists.fold/map/filter` are working with inline closures; generic callback values are still not documented as stable surface. Current checked order is `lists.fold(acc, closure, list)`.
- **proc.run/exec args**: process helpers currently preserve the frozen
 two-argument surface, but shell-backed spawn executes the command string and
 does not yet join the `args` list into argv.

## Incremental Compilation

Avenys supports incremental compilation with caching:

- Cache location: `bin/.cache`
- Metrics tracked: file hits/misses, analysis hits/misses, build hits/misses
- Invalidates on import changes

Test incremental compilation:
```bash
./target/release/mire build tests/level/beginner/01_hello_world.mire
./target/release/mire run tests/level/beginner/01_hello_world.mire # Uses cache
```
