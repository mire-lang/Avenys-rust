// Testing

## Test Declaration

```mire
@[test]
pub fn test_add: () :bool {
    set result = add(2, 3)
    return result == 5
}

@[test]
@[section("math")]
pub fn test_mul: () :bool {
    return mul(4, 5) == 20
}

@[test]
@[ignore]
pub fn test_pending: () :bool {
    return false  // Not run
}
```

- `@[test]` attribute marks test function
- Must return `bool` (`true` = pass, `false` = fail)
- `@[section("name")]` groups tests
- `@[ignore]` skips test

## Test Structure

```mire
// tests/my_test.mire
load mylib::math

@[test]
pub fn test_add: () :bool {
    return math::add(2, 3) == 5
}

@[test]
pub fn test_sub: () :bool {
    return math::sub(10, 4) == 6
}
```

- Tests in `tests/` directory
- Can `load` dependencies
- Cannot `load` package's own `code/`

## Running Tests

```bash
// Via owl (recommended)
owl test

// With options
owl test --verbose       // Show per-test results
owl test --no-run        // Compile only
owl test -j 8            // Parallel (8 jobs)

// Via mire (compiler dev)
mire test
mire test tests/
mire test tests/math.mire
```

## Test Output

```
Running tests...
  [PASS] test_add
  [PASS] test_mul
  [SKIP] test_pending
  [FAIL] test_div

Ok: 4 - Passed: 3 - Failed: 1 - Filtered Out: 0
```

- `[PASS]` / `[FAIL]` / `[SKIP]`
- Summary at end
- `--verbose` shows per-file results

## Assertions

```mire
@[test]
pub fn test_assertions: () :bool {
    // Manual check
    if add(2, 3) != 5 {
        return false
    }
    return true
}
```

- No built-in assert in test body (use `if` + `return false`)
- `assert!` macro available but panics on failure
- `dbg!` useful for debugging test values

## Test Discovery

- All `@[test]` functions in `tests/*.mire`
- Also in `code/*.mire` (if present)
- Filename doesn't matter
- Functions can be in any module

## Parallel Execution

```bash
owl test -j 8   // 8 parallel jobs
```

- Each test file compiled separately
- Runs in parallel up to `-j N`
- Cache shared (WAL-safe since v3.24.25)

## Filtering

```bash
// Not directly supported in CLI
// Use sections:
@[test][section("slow")]
pub fn test_heavy: () :bool { ... }

// Run specific section (not yet implemented)
// owl test --section math
```

## Best Practices

1. **One assertion per test** - easier debugging
2. **Descriptive names** - `test_add_positive_numbers`
3. **Use sections** - group related tests
4. **Ignore flaky tests** - `@[ignore]` with issue reference
5. **Test edge cases** - zero, empty, max values
6. **Test error paths** - invalid input, missing files