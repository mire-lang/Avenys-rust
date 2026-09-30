# Mire Runtime

> Runtime tiers, verified symbol reference, and configuration.

Version: **4.1.0**
Status: **Stable**
Date: **2026-09-12**

---

## Overview

Mire supports **three runtime tiers** that control how much of the standard runtime is linked into your program. The tier is configured via `[c] runtime` in `owl.toml`.

---

## Tier Comparison

| Feature | `full` (default) | `minimal` | `none` |
|---------|------------------|-----------|--------|
| String ops (`str::*`) | Yes | Yes | No |
| Vector ops (`vec::*`) | Yes | Yes | No |
| Map ops (`map::*`) | Yes | Yes | No |
| Math functions | Yes | Yes | No |
| Random | Yes | No | No |
| Threads | Yes | No | No |
| File I/O | Yes | On demand | No |
| Process spawning | Yes | On demand | No |
| Managed memory | Yes | Yes | User provides |
| Safety checks | Yes | Yes | User provides |
| PAL (fs, proc, crypto) | Yes | On demand | User provides |
| Binary size | ~200 KB | ~80 KB | ~15 KB |
| Libraries linked | `-lm -lssl -lcrypto -lsodium -lpthread -lc` | `-lm -lc` (+ crypto if PAL) | `-lc` (or `-nostdlib`) |

---

## Configuration

```toml
# owl.toml
[c]
runtime = "full"       # full | minimal | none
target = "x86_64-unknown-linux-gnu"  # optional LLVM target override
nostartfiles = false   # requires runtime = "none" or libt = static/shared
nostdlib = false       # requires runtime = "none" + nostartfiles
libt = "bin"           # bin | static | shared
```

### Freestanding (Zero Runtime)

```toml
[c]
runtime = "none"
nostartfiles = true
nostdlib = true
libt = "bin"
```

Produces a **~15 KB statically-linked binary** with **zero external dependencies**.

---

## Verified Runtime Symbols

All symbols below are extracted from `src/runtime/runtime.h` and verified to exist in the compiled source. Grouped by module.

### Managed Memory (`rt_managed_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_managed_alloc(size)` | `*mut` | Allocate managed string |
| `rt_managed_from_slice(src, len)` | `*mut` | Create managed from slice |
| `rt_managed_from_cstr(src)` | `*mut` | Create managed from C string |
| `rt_managed_free(value)` | void | Decrement refcount, free when 0 |
| `rt_managed_cleanup_all()` | void | Release entire arena |
| `rt_managed_is_managed(value)` | `i64` | Check if pointer is managed |
| `rt_managed_len(value)` | `size_t` | Get managed string length |
| `rt_managed_contains(data_ptr)` | `i64` | Check if in managed table |
| `rt_managed_register(data_ptr)` | void | No-op (ABI compat) |
| `rt_managed_unregister(data_ptr)` | void | No-op (ABI compat) |
| `rt_managed_retain(data_ptr)` | void | Increment refcount |
| `rt_string_header(data)` | `*` | Get MireManagedString header |
| `rt_string_growth_cap(min_cap)` | `size_t` | Compute growth capacity |
| `rt_managed_printf_i64(fmt, val)` | `*mut` | Format i64 into managed string |
| `rt_managed_printf_f64(fmt, val)` | `*mut` | Format f64 into managed string |

### Strings (`rt_strings_*`, `rt_string_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_string_copy(value)` | `*mut` | Copy a string |
| `rt_string_concat(left, right)` | `*mut` | Concatenate two strings |
| `rt_string_concat_n(count, parts)` | `*mut` | N-ary concat |
| `rt_strings_len(s)` | `i64` | String length (bytes) |
| `rt_strings_len_utf8(s)` | `i64` | String length (codepoints) |
| `rt_strings_char_at(s, idx)` | `i64` | Get codepoint at index |
| `rt_strings_index_of(s, sub)` | `i64` | Find substring index |
| `rt_strings_index_of_utf8(s, sub)` | `i64` | Find UTF-8 substring |
| `rt_strings_contains(input, needle)` | `i64` | Check substring |
| `rt_strings_starts_with(str, prefix)` | `i64` | Check prefix |
| `rt_strings_ends_with(str, suffix)` | `i64` | Check suffix |
| `rt_strings_substr(input, start, len)` | `*mut` | Extract substring |
| `rt_strings_substr_utf8(input, start_cp, count_cp)` | `*mut` | UTF-8 substring |
| `rt_strings_pad_left(input, width, pad)` | `*mut` | Left pad |
| `rt_strings_pad_right(input, width, pad)` | `*mut` | Right pad |
| `rt_strings_trim(input)` | `*mut` | Trim whitespace |
| `rt_strings_split_list(input, delimiter)` | `*mut` | Split into list |
| `rt_strings_join(parts, count, delimiter)` | `*mut` | Join with separator |
| `rt_strings_replace(input, from, to)` | `*mut` | Replace all occurrences |
| `rt_strings_replace_first(input, from, to)` | `*mut` | Replace first occurrence |
| `rt_string_to_upper(value)` | `*mut` | Uppercase |
| `rt_string_to_lower(value)` | `*mut` | Lowercase |
| `rt_strings_to_upper(s)` | `*mut` | Alias |
| `rt_strings_to_lower(s)` | `*mut` | Alias |
| `rt_strings_strip(s)` | `*mut` | Strip whitespace |
| `rt_strings_split(s, sep)` | `*mut` | Alias |
| `rt_strings_join_list(parts, sep)` | `*mut` | Alias |
| `rt_unicode_to_lower(c)` | `char` | Unicode to lowercase |
| `rt_unicode_to_upper(c)` | `char` | Unicode to uppercase |
| `rt_string_to_i64(value)` | `i64` | Parse i64 |
| `rt_f64_to_i64(value)` | `i64` | Parse f64 to i64 |
| `rt_i64_to_string(value)` | `*mut` | i64 -> str |
| `rt_bool_to_string(value)` | `*mut` | bool -> str |
| `rt_f64_to_string(value)` | `*mut` | f64 -> str |
| `rt_f32_to_string(value)` | `*mut` | f32 -> str |
| `rt_i128_to_string(value)` | `*mut` | i128 -> str |
| `rt_u128_to_string(value)` | `*mut` | u128 -> str |
| `rt_string_append_owned(value, suffix)` | `*mut` | Append to owned string |
| `rt_string_to_i64(value)` | `i64` | String to i64 |

### Vecs (`rt_list_*`, `rt_vecs_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_list_create(cap, elem_size)` | `*mut` | Create list |
| `rt_list_len(list)` | `i64` | List length |
| `rt_list_push_i64(list, val)` | `*mut` | Push i64 |
| `rt_list_push_ptr(list, val)` | `*mut` | Push pointer |
| `rt_list_push_scalar(list, val, size)` | `*mut` | Push scalar |
| `rt_list_pop_i64(list)` | `i64` | Pop i64 |
| `rt_list_get_i64(list, idx)` | `i64` | Get i64 |
| `rt_list_set_i64(list, idx, val)` | void | Set i64 |
| `rt_list_get_ptr(list, idx)` | `*mut` | Get pointer |
| `rt_list_concat(left, right)` | `*mut` | Concatenate |
| `rt_list_slice(list, start, end)` | `*mut` | Slice |
| `rt_list_remove(list, idx)` | `*mut` | Remove |
| `rt_list_clear(list)` | `*mut` | Clear |
| `rt_list_free(list)` | void | Free |
| `rt_lists_reverse(list)` | `*mut` | Reverse |
| `rt_lists_unique(list)` | `*mut` | Remove duplicates |
| `rt_lists_contains_i64(list, needle)` | `i64` | Contains |
| `rt_lists_index_of_i64(list, needle)` | `i64` | Index of |
| `rt_lists_first(list, ...)` | `i64` | First element |
| `rt_lists_last(list, ...)` | `i64` | Last element |
| `rt_lists_join_list(list, sep)` | `*mut` | Join |
| `rt_lists_flatten(list)` | `*mut` | Flatten |
| `rt_lists_sort(list)` | `*mut` | Sort |
| `rt_vecs_len(vec)` | `i64` | Vec length |
| `rt_vecs_get_i64(vec, idx)` | `i64` | Get i64 |
| `rt_vecs_get_ptr(vec, idx)` | `*mut` | Get pointer |
| `rt_vecs_push_i64(vec, val)` | `*mut` | Push i64 |
| `rt_vecs_push_ptr(vec, val)` | `*mut` | Push pointer |
| `rt_vecs_set_i64(vec, idx, val)` | void | Set i64 |
| `rt_vecs_pop(vec)` | `i64` | Pop |
| `rt_vecs_slice(vec, start, end)` | `*mut` | Slice |
| `rt_vecs_concat(a, b)` | `*mut` | Concatenate |
| `rt_vecs_remove(vec, idx)` | `*mut` | Remove |
| `rt_vecs_clear(vec)` | `*mut` | Clear |
| `rt_vecs_reverse(vec)` | `*mut` | Reverse |
| `rt_vecs_unique(vec)` | `*mut` | Unique |
| `rt_vecs_clone(vec)` | `*mut` | Clone |
| `rt_vecs_contains_i64(vec, needle)` | `i64` | Contains |
| `rt_vecs_index_of_i64(vec, needle)` | `i64` | Index of |
| `rt_vecs_first(vec, ...)` | `i64` | First |
| `rt_vecs_last(vec, ...)` | `i64` | Last |
| `rt_vecs_sort(vec)` | `*mut` | Sort |

### Maps (`rt_dict_*`, `rt_maps_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_dict_len(dict)` | `i64` | Dict length |
| `rt_dict_ensure(dict)` | `*mut` | Ensure dict |
| `rt_dict_ensure_kind(dict, key_kind, val_kind)` | `*mut` | Ensure with kinds |
| `rt_dict_get_i64(dict, key_kind, key_i64, key_ptr, default)` | `i64` | Get i64 |
| `rt_dict_set_i64(dict, key_kind, val_kind, key_i64, key_ptr, val)` | `*mut` | Set i64 |
| `rt_dict_get_ptr(dict, key_kind, key_i64, key_ptr, default)` | `*mut` | Get pointer |
| `rt_dict_set_ptr(dict, key_kind, val_kind, key_i64, key_ptr, val)` | `*mut` | Set pointer |
| `rt_dict_has(dict, key_kind, key_i64, key_ptr)` | `i64` | Has key |
| `rt_dict_remove(dict, key_kind, key_i64, key_ptr)` | `*mut` | Remove |
| `rt_dict_to_string(dict)` | `*mut` | To string |
| `rt_dict_free(dict)` | void | Free |
| `rt_dict_keys(dict)` | `*mut` | Get keys |
| `rt_dict_values(dict)` | `*mut` | Get values |
| `rt_dicts_len(dict)` | `i64` | Alias |
| `rt_dicts_get(dict, key)` | `*mut` | Alias |
| `rt_dicts_get_i64(dict, key)` | `i64` | Alias |
| `rt_dicts_set(dict, key, val)` | `*mut` | Alias |
| `rt_dicts_set_i64(dict, key, val)` | `*mut` | Alias |
| `rt_dicts_has(dict, key)` | `i64` | Alias |
| `rt_dicts_remove(dict, key)` | `*mut` | Alias |
| `rt_dicts_keys(dict)` | `*mut` | Alias |
| `rt_dicts_values(dict)` | `*mut` | Alias |
| `rt_dicts_entries(dict)` | `i64` | Alias |
| `rt_dicts_merge(a, b)` | `*mut` | Alias |
| `rt_dicts_is_empty(dict)` | `i64` | Alias |
| `rt_dicts_set_with_kind(dict, key, val, kind)` | `*mut` | Alias |
| `rt_maps_len(map)` | `i64` | Alias |
| `rt_maps_get(map, key)` | `*mut` | Alias |
| `rt_maps_get_i64(map, key)` | `i64` | Alias |
| `rt_maps_set(map, key, val)` | `*mut` | Alias |
| `rt_maps_set_i64(map, key, val)` | `*mut` | Alias |
| `rt_maps_has(map, key)` | `i64` | Alias |
| `rt_maps_remove(map, key)` | `*mut` | Alias |
| `rt_maps_keys(map)` | `*mut` | Alias |
| `rt_maps_values(map)` | `*mut` | Alias |
| `rt_maps_entries(map)` | `i64` | Alias |
| `rt_maps_merge(a, b)` | `*mut` | Alias |
| `rt_maps_is_empty(map)` | `i64` | Alias |
| `rt_maps_set_with_kind(map, key, val, kind)` | `*mut` | Alias |

### Maybe (`rt_maybe_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_maybe_some_i64(value)` | `*mut` | Some(i64) |
| `rt_maybe_some_str(value)` | `*mut` | Some(str) |
| `rt_maybe_some_f64(value)` | `*mut` | Some(f64) |
| `rt_maybe_some_ptr(value)` | `*mut` | Some(ptr) |
| `rt_maybe_is_none(ptr)` | `i64` | Check None |
| `rt_maybe_is_some(ptr)` | `i64` | Check Some |
| `rt_maybe_none_as_ptr()` | `*mut` | None as pointer |
| `rt_maybe_unwrap_i64(ptr, line, col, file)` | `i64` | Unwrap |
| `rt_maybe_unwrap_str(ptr, line, col, file)` | `*mut` | Unwrap |
| `rt_maybe_unwrap_f64(ptr, line, col, file)` | `f64` | Unwrap |
| `rt_maybe_unwrap_ptr(ptr, line, col, file)` | `*mut` | Unwrap |
| `rt_maybe_unwrap_or_i64(ptr, default)` | `i64` | Unwrap or |
| `rt_maybe_unwrap_or_str(ptr, default)` | `*mut` | Unwrap or |
| `rt_maybe_unwrap_or_f64(ptr, default)` | `f64` | Unwrap or |
| `rt_maybe_unwrap_or_ptr(ptr, default)` | `*mut` | Unwrap or |
| `rt_maybe_free(ptr)` | void | Free |

### Result (`rt_result_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_result_ok_i64(value)` | `*mut` | Ok(i64) |
| `rt_result_ok_str(value)` | `*mut` | Ok(str) |
| `rt_result_ok_ptr(value)` | `*mut` | Ok(ptr) |
| `rt_result_err_i64(error)` | `*mut` | Err(i64) |
| `rt_result_err_str(error)` | `*mut` | Err(str) |
| `rt_result_err_ptr(error)` | `*mut` | Err(ptr) |
| `rt_result_is_ok(ptr)` | `i64` | Check Ok |
| `rt_result_is_err(ptr)` | `i64` | Check Err |
| `rt_result_err_payload(ptr)` | `*mut` | Get err payload |
| `rt_result_unwrap_i64(ptr, line, col, file)` | `i64` | Unwrap |
| `rt_result_unwrap_str(ptr, line, col, file)` | `*mut` | Unwrap |
| `rt_result_unwrap_f64(ptr, line, col, file)` | `f64` | Unwrap |
| `rt_result_unwrap_ptr(ptr, line, col, file)` | `*mut` | Unwrap |
| `rt_result_unwrap_err_str(ptr, line, col, file)` | `*mut` | Get err str |
| `rt_result_unwrap_or_i64(ptr, default)` | `i64` | Unwrap or |
| `rt_result_unwrap_or_str(ptr, default)` | `*mut` | Unwrap or |
| `rt_result_unwrap_or_f64(ptr, default)` | `f64` | Unwrap or |
| `rt_result_unwrap_or_ptr(ptr, default)` | `*mut` | Unwrap or |
| `rt_result_free(ptr)` | void | Free |

### Arrays (`rt_arr_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_arr_len(arr, count)` | `i64` | Array length |
| `rt_arr_first_i64(arr, count, ...)` | `i64` | First element |
| `rt_arr_last_i64(arr, count, ...)` | `i64` | Last element |
| `rt_arr_contains_i64(arr, count, needle)` | `i64` | Contains |
| `rt_arr_index_of_i64(arr, count, needle)` | `i64` | Index of |
| `rt_arr_reverse_i64(arr, count)` | void | Reverse |
| `rt_arr_join(arr, count, sep)` | `*mut` | Join |

### Math (`rt_math_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_math_pi()` | `f64` | pi |
| `rt_math_e()` | `f64` | e |
| `rt_math_tau()` | `f64` | tau |
| `rt_math_sin(value)` | `f64` | Sine |
| `rt_math_cos(value)` | `f64` | Cosine |
| `rt_math_tan(value)` | `f64` | Tangent |
| `rt_math_sqrt(value)` | `f64` | Square root |
| `rt_math_pow(base, exp)` | `f64` | Power |
| `rt_math_log(value)` | `f64` | Natural log |
| `rt_math_log10(value)` | `f64` | Base-10 log |
| `rt_math_exp(value)` | `f64` | Exponential |
| `rt_math_atan2(y, x)` | `f64` | Arctangent |
| `rt_math_asin(value)` | `f64` | Arc sine |
| `rt_math_acos(value)` | `f64` | Arc cosine |
| `rt_math_round(value)` | `i64` | Round |
| `rt_math_floor(value)` | `i64` | Floor |
| `rt_math_ceil(value)` | `i64` | Ceiling |
| `rt_math_sum_i64(list)` | `i64` | Sum |
| `rt_math_min_list_i64(list)` | `i64` | Min |
| `rt_math_max_list_i64(list)` | `i64` | Max |
| `rt_math_mean_i64(list)` | `f64` | Mean |
| `rt_math_variance_i64(list)` | `f64` | Variance |
| `rt_math_stddev_i64(list)` | `f64` | Std dev |
| `rt_math_median_i64(list)` | `f64` | Median |
| `rt_math_range_i64(end)` | `*mut` | Range |
| `rt_math_range_between_i64(start, end)` | `*mut` | Range |
| `rt_math_range_step_i64(start, end, step)` | `*mut` | Range |
| `rt_math_random_seed(seed)` | void | Seed PRNG |
| `rt_math_random_u64()` | `i64` | Random u64 |
| `rt_math_random_i64()` | `i64` | Random i64 |
| `rt_math_random_f64()` | `f64` | Random f64 |
| `rt_math_random_bool()` | `i64` | Random bool |
| `rt_math_random_range_i64(min, max)` | `i64` | Random range |
| `rt_math_inf()` | `f64` | Infinity |
| `rt_math_neg_inf()` | `f64` | Negative infinity |
| `rt_math_nan()` | `f64` | NaN |
| `rt_math_epsilon()` | `f64` | Epsilon |
| `rt_math_comb(n, k)` | `i64` | Combinations |
| `rt_math_factorial(n)` | `i64` | Factorial |
| `rt_math_gcd(a, b)` | `i64` | GCD |
| `rt_math_isqrt(n)` | `i64` | Integer sqrt |
| `rt_math_lcm(a, b)` | `i64` | LCM |
| `rt_math_perm(n, k)` | `i64` | Permutations |
| `rt_math_fabs(value)` | `f64` | Absolute value |
| `rt_math_fmod(x, y)` | `f64` | Modulo |
| `rt_math_remainder(x, y)` | `f64` | IEEE remainder |
| `rt_math_trunc(value)` | `f64` | Truncate |
| `rt_math_fma(x, y, z)` | `f64` | Fused multiply-add |
| `rt_math_copysign(x, y)` | `f64` | Copy sign |
| `rt_math_frexp(value, exp)` | `f64` | Free exponent |
| `rt_math_ldexp(value, exp)` | `f64` | Load exponent |
| `rt_math_nextafter(x, y)` | `f64` | Next after |
| `rt_math_ulp(value)` | `f64` | Unit in last place |
| `rt_math_modf(value, iptr)` | `f64` | Split fraction |
| `rt_math_isfinite(value)` | `i64` | Is finite |
| `rt_math_isinf(value)` | `i64` | Is infinite |
| `rt_math_isnan(value)` | `i64` | Is NaN |
| `rt_math_isclose(a, b, rel, abs)` | `i64` | Close |
| `rt_math_cbrt(value)` | `f64` | Cube root |
| `rt_math_exp2(value)` | `f64` | Power of 2 |
| `rt_math_expm1(value)` | `f64` | exp-1 |
| `rt_math_log2(value)` | `f64` | Log base 2 |
| `rt_math_log1p(value)` | `f64` | log(1+x) |
| `rt_math_atan(value)` | `f64` | Arc tangent |
| `rt_math_degrees(value)` | `f64` | To degrees |
| `rt_math_radians(value)` | `f64` | To radians |
| `rt_math_sinh(value)` | `f64` | Hyperbolic sine |
| `rt_math_cosh(value)` | `f64` | Hyperbolic cosine |
| `rt_math_tanh(value)` | `f64` | Hyperbolic tangent |
| `rt_math_asinh(value)` | `f64` | Arc hyperbolic sine |
| `rt_math_acosh(value)` | `f64` | Arc hyperbolic cosine |
| `rt_math_atanh(value)` | `f64` | Arc hyperbolic tangent |
| `rt_math_erf(value)` | `f64` | Error function |
| `rt_math_erfc(value)` | `f64` | Complementary error |
| `rt_math_gamma(value)` | `f64` | Gamma |
| `rt_math_lgamma(value)` | `f64` | Log gamma |
| `rt_math_fsum(list)` | `f64` | Floating sum |
| `rt_math_prod(list, start)` | `f64` | Product |
| `rt_math_sumprod(p, q)` | `f64` | Sum product |
| `rt_math_dist(p, q)` | `f64` | Distance |
| `rt_math_hypot(x, y)` | `f64` | Hypotenuse |

### Safety (`rt_panic_*`, `rt_div_*`, `rt_check_*`)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_panic(msg)` | void | Panic |
| `rt_panic_loc(msg, line, col, file)` | void | Panic with location |
| `rt_div_i64(a, b, line, col, file)` | `i64` | Safe division |
| `rt_rem_i64(a, b, line, col, file)` | `i64` | Safe remainder |
| `rt_check_bounds_i64(index, len, line, col, file)` | void | Bounds check |
| `rt_closure_env_alloc(size)` | `*mut` | Allocate closure env |
| `rt_closure_env_free(env)` | void | Free closure env |

### I/O (`dasu`, `ireru`, `rt_get_args`, etc.)

| Symbol | Return | Description |
|--------|--------|-------------|
| `dasu(value)` | `*mut` | Print to stdout |
| `ireru(prompt)` | `*mut` | Read line from stdin |
| `rt_get_args(argc, argv)` | `*mut` | Get arguments |
| `rt_time_elapsed_ms_str(start_ns)` | `*mut` | Time elapsed |
| `rt_cpu_elapsed_ms_str(start_ns)` | `*mut` | CPU elapsed |

### Byte/File Utilities

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_crypto_byte_at(s, i)` | `i64` | Byte at index |
| `rt_crypto_sha256_hex(s)` | `*mut` | SHA-256 hex |
| `rt_crypto_sha512_hex(s)` | `*mut` | SHA-512 hex |
| `rt_read_bytes(path)` | `*mut` | Read file bytes |
| `rt_hex_to_file(path, hex)` | `i64` | Hex to file |
| `rt_fs_read_bytes(path)` | `*mut` | Read file as managed |

### Process

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_build_argv(cmd, args_vec, argc_out)` | `**mut` | Build argv |
| `rt_free_argv(argv, argc)` | void | Free argv |
| `rt_proc_capture_output(cmd)` | `*mut` | Capture output |
| `rt_proc_capture_argv(cmd, args)` | `*mut` | argv capture |
| `rt_proc_capture_argv2(cmd, args, cwd, merge_err)` | `*mut` | Extended capture |
| `rt_proc_last_exit()` | `i64` | Last exit code |
| `rt_read_tty()` | `*mut` | Read from tty |

### Channel

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_channel_recv_into(ch, buf, cap)` | `i64` | Receive into buffer |

### Font

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_font_get_pixel(ch, col, row)` | `i64` | Get font pixel |
| `rt_font_char_at(s, i)` | `i64` | Get UTF-8 codepoint |
| `rt_font_char_len(s, i)` | `i64` | Get UTF-8 byte length |

### Thread

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_thread_spawn_closure(fn_ptr, env_ptr)` | `i64` | Spawn thread |

### Raw Memory

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_alloc_raw(size)` | `*mut` | Allocate raw |
| `rt_free_raw(ptr)` | void | Free raw |
| `rt_read_u8(ptr)` | `i64` | Read u8 |
| `rt_read_u16(ptr)` | `i64` | Read u16 |
| `rt_read_u32(ptr)` | `i64` | Read u32 |
| `rt_read_i32(ptr)` | `i64` | Read i32 |
| `rt_read_u64(ptr)` | `i64` | Read u64 |
| `rt_read_ptr(ptr)` | `i64` | Read pointer |
| `rt_read_f64(ptr)` | `f64` | Read f64 |
| `rt_read_f32(ptr)` | `f32` | Read f32 |
| `rt_write_u8(ptr, val)` | void | Write u8 |
| `rt_write_u16(ptr, val)` | void | Write u16 |
| `rt_write_u32(ptr, val)` | void | Write u32 |
| `rt_write_i32(ptr, val)` | void | Write i32 |
| `rt_write_f32(ptr, val)` | void | Write f32 |
| `rt_write_f64(ptr, val)` | void | Write f64 |
| `rt_write_u64(ptr, val)` | void | Write u64 |
| `rt_read_cstr(ptr)` | `*mut` | Read C string |

### Websocket (internal)

| Symbol | Return | Description |
|--------|--------|-------------|
| `rt_websocket_accept(client_key)` | `*mut` | WebSocket accept |
| `rt_websocket_text_frame(payload)` | `*mut` | WebSocket frame |
| `rt_web_log_append(path, line)` | `i64` | Web log append |

---

## Symbol -> C File Mapping

| Symbol Pattern | C File |
|----------------|--------|
| `rt_managed_*`, `rt_string_*`, `rt_i64_to_string`, etc. | `strings.c` / `strings_minimal.c` |
| `rt_list_*`, `rt_vecs_*`, `rt_lists_*` | `vecs.c` |
| `rt_dict_*`, `rt_dicts_*`, `rt_maps_*` | `maps.c` / `maps_internal.c` |
| `rt_math_*` | `math.c` |
| `rt_math_random_*` | `random.c` |
| `rt_managed_*`, `rt_panic_*` | `managed.c` / `managed_minimal.c` |
| `rt_check_bounds_*`, `rt_div_*`, `rt_rem_*` | `safety.c` / `safety_minimal.c` |
| `rt_closure_env_*` | `safety.c` |
| `rt_get_args`, `rt_free_argv` | `helpers.c` |
| `rt_thread_*` | `thread.c` |
| `rt_crypto_*`, `rt_read_bytes`, `rt_hex_to_file` | `mire_io.c` |
| `rt_font_*`, `rt_read_cstr`, `rt_blend_u32` | `helpers.c` |
| `rt_build_argv`, `rt_free_argv`, `rt_proc_*` | `helpers.c` |
| `rt_channel_recv_into` | `helpers.c` |

**Always included**: `managed.c` + `safety.c` (safety baselines)

---

## C File Dependency Graph

```
strings.c -> vecs.c -> strings.c  (cycle, resolved via fixed-point)
maps.c -> vecs.c -> managed.c
maps_internal.c -> managed.c
math.c -> vecs.c
helpers.c -> strings.c -> managed.c
mire_types.c -> strings.c -> managed.c -> safety.c
mire_io.c -> managed.c
```

---

## Verification

```bash
# All symbols verified against src/runtime/runtime.h (595 lines)
# No symbol documented here is missing from the source
# No symbol in the source is omitted from this reference

# Check what C files are compiled
mire build --verbose 2>&1 | grep "\.c"

# Check linked libraries
ldd bin/release/myprogram

# Check symbols in binary
nm -D bin/release/myprogram | grep -E "rt_|pal_"
```

---

## Related

- [Runtime Configuration](../../docs/README.md)
- [PAL v4](../pal/README.md)
- [Mire ABI v4](../abi/README.md)
- [Avenys Documentation Index](../README.md)

---

## License

GNU General Public License v3.0
