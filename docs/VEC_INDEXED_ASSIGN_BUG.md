# Bug: corruption of `vec[i64]` by indexed assignment

**Status:** reproduced on Avenys / Mire v3.15.0 / v3.16.0.
**Severity:** high — affects any indexed write on dynamically-sized vectors
(`vec[T]`). Subsequent reads return wrong or shifted values; the vector is
silently corrupted.

## Symptoms

A benchmark that creates a `vec[i64]`, fills it, and then writes by index
using the native indexed-assignment syntax loses the written value and
corrupts later reads. There is no panic and no compile error: the result is
simply incorrect.

## Minimal reproducible case

```mire
module main
load kioto::strings
load kioto::strings::from
load kioto::lists

pub fn main: () {
 set v = [] :vec[i64] mut
 set i = 0 :i64 mut
 while i < 10 {
 set v = lists::push(v 1)
 set i = i + 1
 }
 set v at 4 = 42 # native indexed assignment
 set a0 = lists::get(v 0)
 set a4 = lists::get(v 4)
 set a9 = lists::get(v 9)
 use dasu("expected: 1 42 1")
 use dasu("read: " + strings::from::i64(a0) + " "
 + strings::from::i64(a4) + " " + strings::from::i64(a9))
}
```

**Observed output:** `read: 1 1 1` (the 42 was lost; the vector is corrupted).
**Expected output:** `read: 1 42 1`.

## Workaround (works)

Use `lists::set(vec, idx, val)` from kioto instead of the native indexed
assignment. This function routes to `rt_lists_set_i64` in the C runtime and
writes the correct slot without corrupting the vector.

```mire
 lists::set(v 4 42) # correct: writes slot 4
```

With `lists::set` the output is `read: 1 42 1` (correct).

## Scope

- **Affected:** dynamically-sized `vec[T]` with indexed assignment
  (`set vec at idx = val`).
- **Not affected:** `arr[T N]` (fixed-size array) — its indexed assignment
  (`set self.data at idx = v`) works correctly.
- **Not affected:** indexed read (`lists::get`) or `lists::push`.
- The bug is in the codegen/ABI of indexed assignment over the dynamic
  vector header (the written slot is not resolved against the correct base
  pointer, or `len`/capacity becomes desynchronized).

## Relation to the stress suite

The benchmark suite (`Arch/stress`) avoids this bug by using `lists::get` for
reads and accumulating results in scalars; the few benchmarks that needed
indexed writes were rewritten to avoid the native syntax. That is why the
current suite does not trigger it, but the bug is still present in the runtime
and must be documented for any code that writes to `vec[]` by index.
