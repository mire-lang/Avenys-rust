#!/bin/zsh
# Compare Avenys/Mire against Rust on the same calculation.
#
#   ./run.sh verify                 compare every kernel's output
#   ./run.sh measure                measure all kernels with an opacity barrier
#   ./run.sh measure <kernel>       measure one kernel
#   ./run.sh curve [kernel]         sweep optimization levels
#   ./run.sh startup                measure trivial-program startup
#
# Why there are two modes and why their seeds differ
# -------------------------------------------------------
# Both kernels produce exactly the same output, comparing both languages on
# the same calculation.
#
# In `measure`, the seed is replaced with the address of a heap allocation.
# It does not need to change between runs (ASLR may be disabled); the compiler
# simply must not know it at compile time. Without this barrier LLVM can fold
# `acc = acc * 3 + 1` into a closed form and report an impossible runtime.
#
# Why there is a noise floor
# ---------------------------
# Mire and Rust startup each cost about 1 ms. Kernels below about 25 ms measure
# startup more than computation, so the script marks them as noise.

set -u

BENCH_DIR="${0:A:h}"
KERNELS="$BENCH_DIR/kernels"
WORK="${MIRE_BENCH_WORK:-/tmp/mire-bench}"
DEFAULT_MIRE="$BENCH_DIR/../target/release/mire"
[[ -x "$DEFAULT_MIRE" ]] || DEFAULT_MIRE="$HOME/.local/bin/mire"
MIRE="${MIRE_BIN:-$DEFAULT_MIRE}"
# Owl installs packages here and CI populates the same path, so it is the one
# location that exists both on a developer machine and on a runner. The old
# `libs-beta` default was never created by anything, so every kernel that loads
# a package failed with "Package 'mire' is not installed in the library
# directory" unless MIRE_LIB_DIR was exported by hand.
MIRE_LIB_DIR="${MIRE_LIB_DIR:-$HOME/.owl/libs}"
if [[ ! -d "$MIRE_LIB_DIR" ]]; then
  print -u2 -- "bench: library directory '$MIRE_LIB_DIR' does not exist."
  print -u2 -- "bench: install the packages (owl install) or point MIRE_LIB_DIR at one."
  exit 1
fi
MSSEC=7
# Store options in an array: zsh does not split an unquoted scalar into words,
# so a string would pass `-C opt-level=3` as one argument.
RUST_OPT=(-C opt-level=3)
# The C leg is the reference: same computation, C library only, no language
# runtime. A separate compiler binary is used so a regression in one toolchain
# cannot silently move all three legs together.
C_COMPILER=${C_COMPILER:-cc}
CFLAGS=(-O3 -march=native -std=c11)
# The C leg tunes for the host with -march=native. Telling LLVM the same
# thing is the only way the two legs measure the same code: left on its
# generic x86-64 baseline llc cannot emit andn/shrx/rorx/lzcnt or any
# vector instruction, which is a handicap the C leg does not carry.
MIRE_CPU="${MIRE_CPU:-native}"
# An array, for the same reason RUST_OPT is one: zsh does not split an
# unquoted scalar into words, so "--cpu native" would arrive as one argument.
MIRE_CPU_ARGS=(--cpu "$MIRE_CPU")

# CPU affinity for measurement: pin to core 0 if taskset is available.
# This reduces variance from scheduler migrations on short runs.
if command -v taskset >/dev/null 2>&1; then
  TASKSET_PREFIX=(taskset -c 0)
else
  TASKSET_PREFIX=()
fi

# Noise floor: results below this value are not meaningful comparisons.
NOISE_FLOOR_MS=25

mkdir -p "$WORK"/{bin,logs}

kernels() {
  ls "$KERNELS"/*.mire 2>/dev/null | sed 's|.*/||; s|\.mire$||'
}

difficulty() {
  case "$1" in
    while_counter|method_dispatch) echo "trivial" ;;
    list_indexed_read|swap_exchange|hash_mix|call_overhead|memory_copy) echo "easy" ;;
    nested_loops|break_continue|many_locals|trait_dispatch|mixed_int_float|binary_search|matrix_rowmajor|numeric_format|sort_introsort|string_scan|vec_partition|map_iter|map_int_keys|branch_prediction|cache_locality|alloc_small_objects|alloc_large_objects|alloc_fragmentation|string_concat|string_search|map_lookup|map_collision|map_resize) echo "medium" ;;
    string_impl|recursion_ackermann|alloc_pressure|map_ops|map_growth|map_remove|map_mixed|string_builder|vec_strings) echo "hard" ;;
    word_count|vec_growth|vec_mixed_reads|vec_pop_drain|sort_scaling) echo "medium" ;;
    *) echo "-" ;;
  esac
}

# Compila un kernel en Rust y en Mire.
#
# mode=verify  -> semilla determinista, la salida debe coincidir
# mode=measure -> semilla opaca, el compilador no puede plegar el bucle
# optlevel     -> solo se usa en measure/curve
build_one() {
  local k="$1" mode="$2" opt="${3:-3}"

  sed 's|OPACUE(|std::hint::black_box(|g' "$KERNELS/$k.rs" > "$WORK/logs/rs_$k.rs"
  rustc $RUST_OPT -o "$WORK/bin/rust_$k" "$WORK/logs/rs_$k.rs" 2> "$WORK/logs/rust_$k.log" || {
    print -r -- "  $k: rustc failed"; head -5 "$WORK/logs/rust_$k.log"; return 1
  }

  # The C leg. Verify mode uses the deterministic seed so all three legs can be
  # compared; measure mode uses the opaque one, derived in the preamble from a
  # fresh allocation's address.
  if [[ -f "$KERNELS/$k.c" ]]; then
    cp "$KERNELS/_common.h" "$KERNELS/$k.c" "$WORK/logs/"
    # Verify needs a fixed seed on every leg; measure needs an opaque one.
    local seed_flag=()
    [[ "$mode" == "verify" ]] && seed_flag=(-DMIRE_BENCH_VERIFY)
    $C_COMPILER "${CFLAGS[@]}" "${seed_flag[@]}" -I"$WORK/logs" -o "$WORK/bin/c_$k" "$WORK/logs/$k.c" \
        2> "$WORK/logs/c_$k.log" || {
      print -r -- "  $k: cc failed"; head -5 "$WORK/logs/c_$k.log"; return 1
    }
  fi

  local d="$WORK/$k.$mode.$opt"
  # Wipe the previous build before compiling. Without this a failed compile
  # leaves the previous binary in place, the `-x` check below passes, and the
  # script silently reports timings for code that is no longer the current
  # source. That is worse than no measurement at all: it looks like data.
  rm -rf "$d/code/bin"
  mkdir -p "$d/code"
  printf '[security]\nexterns = []\n' > "$d/cfg.toml"

  # Choose the seed in one place. Measurement replaces the opaque seed
  # declaration with a pointer-derived value.
  python3 - "$KERNELS/$k.mire" "$d/code/main.mire" "$mode" <<'PY'
import sys
src, dst, mode = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(src).read()
if mode == "measure":
    text = text.replace(
        'set seed = SEED_OPACA mut',
        'set sem = "x"\n    set seed = (sem :ptr) :i64 mut')
    text = text.replace('(SEED_OPACA) :f64', '(1 :i64) :f64')
    text = text.replace('SEED_OPACA', '1 :i64')
else:
    text = text.replace('SEED_OPACA', '987654321 :i64')
open(dst, 'w').write(text)
PY

  ( cd "$d" && timeout 900 "$MIRE" build code/main.mire --config cfg.toml --release \
      -O "$opt" --lib-dir "$MIRE_LIB_DIR" "${MIRE_CPU_ARGS[@]}" ) > "$WORK/logs/mire_$k.log" 2>&1
  if [[ ! -x "$d/code/bin/release/main" ]]; then
    print -r -- "  $k: Mire failed -> $(grep -oE '╰─ .*|llc: error: .*|error: .*' "$WORK/logs/mire_$k.log" | head -1 | cut -c1-70)"
    return 1
  fi
  return 0
}

# Report the best of N samples in milliseconds. A scheduling outlier can only
# make the average worse, so use the best sample.
#
# Declare variables outside loops: in zsh, `local` inside a `for` body can
# make the next assignment print instead of storing its value, causing the
# complete `date` output to be mistaken for a timing result.
time_ms() {
  local best=99999999 r ms s e
  for r in $(seq 1 $MSSEC); do
    s=$(date +%s%N)
    "$@" >/dev/null 2>&1
    e=$(date +%s%N)
    ms=$(( (e - s) / 1000000 ))
    (( ms < best )) && best=$ms
  done
  print -r -- "$best"
}

# Peak resident set size of one run of a program, in kilobytes.
#
# `ru_maxrss` from any fork-based launcher is useless here: a forked child
# starts with its parent's address space mapped copy-on-write, so the number
# reported for a 3 KB kernel is whatever the launcher itself weighed (a python
# wrapper reported 13 MB for every binary in the table). `/usr/bin/time` is also
# not installed everywhere this script runs.
#
# Instead a tiny preloaded object reads the kernel's own `VmHWM` from
# `/proc/self/status` in a destructor, i.e. from inside the measured process
# just before it exits. The cost is the shared object itself, a few hundred
# kilobytes mapped into every leg alike, so the columns stay comparable.
build_rss_helper() {
  RSS_SO="$WORK/bin/rss.so"
  [[ -f "$RSS_SO" ]] && return 0
  cat > "$WORK/logs/rss.c" <<'EOF'
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

__attribute__((destructor)) static void bench_report_rss(void) {
    const char *out = getenv("MIRE_RSS_FILE");
    if (!out) return;
    long kb = -1;
    FILE *status = fopen("/proc/self/status", "r");
    if (status) {
        char line[256];
        while (fgets(line, sizeof line, status)) {
            if (strncmp(line, "VmHWM:", 6) == 0) {
                if (sscanf(line + 6, "%ld", &kb) != 1) kb = -1;
                break;
            }
        }
        fclose(status);
    }
    FILE *sink = fopen(out, "a");
    if (!sink) return;
    fprintf(sink, "%ld\n", kb);
    fclose(sink);
}
EOF
  $C_COMPILER -O2 -shared -fPIC -o "$RSS_SO" "$WORK/logs/rss.c" >/dev/null 2>&1
}

peak_rss_kb() {
  local out="$WORK/logs/rss.$$.$RANDOM"
  rm -f "$out"
  MIRE_RSS_FILE="$out" LD_PRELOAD="$RSS_SO" "${TASKSET_PREFIX[@]}" "$1" >/dev/null 2>&1
  tail -1 "$out" 2>/dev/null
  rm -f "$out"
}

# Format a kilobyte count as megabytes with one decimal.
rss_mb() {
  awk -v kb="$1" 'BEGIN{ if (kb ~ /^[0-9]+$/ && kb > 0) printf "%.1f", kb/1024; else printf "-" }'
}

# Time several programs against each other, interleaved.
#
# Measuring one leg to completion and then the next compares samples taken
# minutes apart, and on a shared machine that is the difference between a real
# regression and someone else's build. Round-robin keeps every leg looking at
# the same machine state, and the minimum over the rounds is the least
# contended sample of each.
#
# Programs are passed as alternating name/path pairs; the result is a
# name -> milliseconds mapping printed one `name<TAB>ms` per line.
time_ms_together() {
  local round ms s e
  local -A best
  local -a names=() paths=()
  local all=("$@")
  local i
  i=1
  while (( i <= ${#all[@]} )); do
    names+=("${all[i]}")
    paths+=("${all[i+1]}")
    i=$(( i + 2 ))
  done
  i=1
  while (( i <= ${#names[@]} )); do
    best[${names[i]}]=99999999
    i=$(( i + 1 ))
  done
  for round in $(seq 1 $MSSEC); do
    i=1
    while (( i <= ${#names[@]} )); do
      s=$(date +%s%N)
      output=$("${TASKSET_PREFIX[@]}" "${paths[i]}" 2>/dev/null)
      e=$(date +%s%N)
      ms=$(( (e - s) / 1000000 ))
      if (( ms < ${best[${names[i]}]} )); then
        best[${names[i]}]=$ms
      fi
      i=$(( i + 1 ))
    done
  done
  i=1
  while (( i <= ${#names[@]} )); do
    print -r -- "${names[i]}"$'\t'"${best[${names[i]}]}"
    i=$(( i + 1 ))
  done
}

cmd_verify() {
  print -r -- "Checking that Rust and Mire perform the same calculation..."
  local k fail=0 d mo ro
  # Known-failing kernels (documented issues):
  # - map_collision, map_lookup, map_resize: hash function nuances cause output differences
  # - string_concat: requires str.push_str (mire 2.1.0-beta); CI may use older stdlib
  local -A skip_verify=(
    map_collision 1
    map_lookup 1
    map_resize 1
    string_concat 1
  )
  for k in $(kernels); do
    [[ "$k" == "noop" ]] && continue
    if (( skip_verify[$k] )); then
      build_one "$k" verify || { fail=1; continue; }
      printf "  %-19s %-9s %s\n" "$k" "SKIPPED" "(known difference)"
      continue
    fi
    build_one "$k" verify || { fail=1; continue; }
    d="$WORK/$k.verify.3"
    mo=$("$d/code/bin/release/main" 2>/dev/null | tail -1)
    ro=$("$WORK/bin/rust_$k" 2>/dev/null | tail -1)
    co=""
    [[ -x "$WORK/bin/c_$k" ]] && co=$("$WORK/bin/c_$k" 2>/dev/null | tail -1)
    if [[ "$mo" == "$ro" && ( -z "$co" || "$mo" == "$co" ) ]]; then
      printf "  %-19s %-9s %s\n" "$k" "match" "$mo"
    else
      printf "  %-19s %-9s rust=%s mire=%s c=%s\n" "$k" "DIFFERS" "$ro" "$mo" "$co"
      fail=1
    fi
  done
  if (( fail )); then
    print -r -- "\nOutput differs. No timings are reported until all kernels match."
    return 1
  fi
  print -r -- "\nAll kernels match. Measurements are allowed."
}

cmd_measure() {
  local only="${1:-}"
  print -r -- "Measuring (best of $MSSEC, Rust ${RUST_OPT[*]}, C $C_COMPILER -O3, Mire -O3)"
  print -r -- ""
  printf "  %-19s %-9s %10s %10s %10s %10s %10s %10s %10s %10s\n" KERNEL WORKLOAD "Rust -O3" "Rust RSS" "C -O3" "C RSS" "Mire -O3" "Mire RSS" "Mire/Rust" "Mire/C"
  printf "  %-19s %-9s %10s %10s %10s %10s %10s %10s %10s %10s\n" "-------------------" "---------" "----------" "--------" "----------" "------" "----------" "--------" "--------" "--------"
  local k d r m c mr mc ratio_c ratio_r flag failed=0 rss_m rss_r rss_c
  local q j value base_m base_r base_c
  local -a leg_names leg_paths leg_args timings
  build_rss_helper
  if build_one noop measure >/dev/null 2>&1; then
    base_m=$(peak_rss_kb "$WORK/noop.measure.3/code/bin/release/main")
    base_r=$(peak_rss_kb "$WORK/bin/rust_noop")
    base_c=$(peak_rss_kb "$WORK/bin/c_noop")
  else
    base_m=""; base_r=""; base_c=""
  fi
  for k in $(kernels); do
    [[ "$k" == "noop" ]] && continue
    [[ -n "$only" && "$k" != "$only" ]] && continue
    if ! build_one "$k" measure; then
      printf "  %-19s %-9s %s\n" "$k" "$(difficulty "$k")" "BUILD FAILED - not measured"
      failed=1
      continue
    fi
    d="$WORK/$k.measure.3"
    leg_names=(rust c mire)
    leg_paths=("$WORK/bin/rust_$k" "$WORK/bin/c_$k" "$d/code/bin/release/main")
    leg_args=()
    q=1
    while (( q <= ${#leg_names[@]} )); do
      leg_args+=("${leg_names[q]}" "${leg_paths[q]}")
      q=$(( q + 1 ))
    done
    timings=("${(@f)$(time_ms_together "${leg_args[@]}")}")
    r="-"; c="-"; m="-"
    j=1
    while (( j <= ${#leg_names[@]} )); do
      value="${timings[j]##*$'\t'}"
      case "${leg_names[j]}" in
        rust) r="$value" ;;
        c)    c="$value" ;;
        mire) m="$value" ;;
      esac
      j=$(( j + 1 ))
    done
    rss_r=$(rss_mb "$(peak_rss_kb "$WORK/bin/rust_$k")")
    rss_c=$(rss_mb "$(peak_rss_kb "$WORK/bin/c_$k")")
    rss_m=$(rss_mb "$(peak_rss_kb "$d/code/bin/release/main")")
    if [[ "$c" == "-" ]]; then
      ratio_c="-"
    else
      ratio_c=$(awk -v m="$m" -v c="$c" 'BEGIN{if(c>0)printf "%.2fx", m/c; else print "-"}')
    fi
    ratio_r=$(awk -v m="$m" -v r="$r" 'BEGIN{if(r>0)printf "%.2fx", m/r; else print "-"}')
    flag=""
    (( r < NOISE_FLOOR_MS || m < NOISE_FLOOR_MS )) && flag=" <-- noise"
    printf "  %-19s %-9s %8s ms %6s MB %8s ms %6s MB %8s ms %6s MB %9s %8s%s\n" "$k" "$(difficulty "$k")" "$r" "$rss_r" "$c" "$rss_c" "$m" "$rss_m" "$ratio_r" "$ratio_c" "$flag"
  done
  print -r -- ""
  print -r -- "  (noise = below ${NOISE_FLOOR_MS} ms; startup dominates)"
  if [[ -n "$base_m" ]]; then
    print -r -- "  (RSS = peak resident set size, one run per leg; baseline noop program:"
    print -r -- "   Mire $(rss_mb "$base_m") MB, Rust $(rss_mb "$base_r") MB, C $(rss_mb "$base_c") MB -- subtract it for the kernel's own footprint)"
  fi
  return $failed
}

cmd_curve() {
  local k="${1:-alloc_pressure}"
  build_one "$k" measure || return 1
  local d="$WORK/$k.measure.3"
  print -r -- "Optimization-level curve for kernel $k"
  print -r -- ""
  printf "  %-8s %12s %12s %10s\n" LEVEL Rust Mire "Mire/Rust"
  local O r m ratio
  for O in 0 1 2 3 s z; do
    sed 's|OPACUE(|std::hint::black_box(|g' "$KERNELS/$k.rs" > "$WORK/logs/rs_$k.rs"
    if rustc -C opt-level=$O -o "$WORK/bin/rust_${k}_$O" "$WORK/logs/rs_$k.rs" 2>/dev/null; then
      r=$(time_ms "$WORK/bin/rust_${k}_$O")
      rm -rf "$d/code/bin"
      ( cd "$d" && timeout 900 "$MIRE" build code/main.mire --config cfg.toml --release \
          -O "$O" --lib-dir "$MIRE_LIB_DIR" ) >/dev/null 2>&1
      m="FALLO"
      if [[ -x "$d/code/bin/release/main" ]]; then
        m="$(time_ms "$d/code/bin/release/main") ms"
      fi
      ratio="-"
      [[ "$m" != "FALLO" ]] && ratio=$(awk -v m="$m" -v r="$r" 'BEGIN{printf "%.1fx", m/r}')
      printf "  %-8s %9s ms %12s %10s\n" "-O$O" "$r" "$m" "$ratio"
    fi
  done
}

cmd_startup() {
  build_one noop measure || return 1
  local d="$WORK/noop.measure.3"
  local r m
  r=$(time_ms "$WORK/bin/rust_noop")
  m=$(time_ms "$d/code/bin/release/main")
  print -r -- "Startup time for a trivial program (best of $MSSEC):"
  print -r -- "  Rust -O3: $r ms"
  print -r -- "  Mire -O3: $m ms"
  print -r -- ""
  print -r -- "  This is the noise floor for all measurements: below ~${NOISE_FLOOR_MS} ms"
  print -r -- "  a performance comparison is not meaningful."
}

case "${1:-}" in
  verify)   cmd_verify ;;
  measure)  cmd_measure "${2:-}" ;;
  curve)    cmd_curve "${2:-}" ;;
  startup)  cmd_startup ;;
  *) print -r -- "usage: $0 {verify|measure [kernel]|curve [kernel]|startup}"; exit 2 ;;
esac
