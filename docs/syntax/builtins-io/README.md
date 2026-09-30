// Built-in I/O Primitives

## dasu (Print)

```mire
// Basic print
dasu("Hello World")

// With interpolation
set name = "Alice"
dasu("Hello {name}!")

// Multiple values
dasu("x={x}, y={y}, sum={x+y}")

// No newline by default
dasu("Line 1")
dasu("Line 2")  // Same line: "Line 1Line 2"
```

- `dasu(msg :str)` - prints string
- Interpolation: `{var}` substitutes variable
- `{expr}` NOT supported (assign to variable first)
- No automatic newline
- **Flushes after every call**, so output is not held back when stdout is a pipe
  or a file rather than a terminal. A `dasu` interleaved with a write to stderr
  reaches the shared destination in call order.

## ireru (Read Line)

```mire
set input = ireru("Enter name: ")
dasu("You entered: {input}")
```

- `ireru(prompt :str) :str` - prints prompt, reads stdin line
- Returns line without trailing newline
- Blocks until Enter pressed
- **The prompt is flushed after it is written**, so it appears before the read
  starts rather than when the buffer happens to fill

## proc::run::output (Capture Command)

```mire
set result = proc::run::output("echo" ["hello"])
dasu(result)  // "hello\n"
```

- `proc::run::output(cmd, args) :str`
- argv-safe, NO shell
- Returns stdout as string
- Stderr not captured (use `output_cwd` for merge)

## proc::run::spawn (Async Process)

```mire
set proc = proc::run::spawn("sleep" ["10"])
proc::wait(proc)
```

- `proc::run::spawn(cmd, args) :Process`
- Non-blocking spawn
- Use `proc::wait` to synchronize

## proc::run::shell (Explicit Shell)

```mire
set result = proc::run::shell("echo hello | grep hello")
```

- `proc::run::shell(cmd :str) :str`
- **Explicit shell escape hatch** - name signals intent
- Uses `/bin/sh -c`
- Use sparingly (security)

## proc::run::output_cwd (With Working Dir)

```mire
set result = proc::run::output_cwd("git" ["status"] "/path/to/repo" true)
```

- `proc::run::output_cwd(cmd, args, cwd, merge_err) :str`
- `cwd` - working directory (optional)
- `merge_err` - merge stderr into stdout

## proc::run::last_exit (Exit Code)

```mire
proc::run::output("false" [])
set code = proc::run::last_exit()  // 1
```

- `proc::run::last_exit() :i64`
- Returns exit code of last `proc::run::*` call
- Only valid immediately after

## proc::run::read_line (TTY Input)

```mire
set answer = proc::run::read_line()  // "y" or "n"
```

- `proc::run::read_line() :str`
- Reads from `/dev/tty` directly
- Returns `"y"` in non-interactive contexts

## Channel I/O

```mire
// Create channel
set ch = async::channel::create()

// Send
async::channel::send(ch, "message")

// Receive
set msg = async::channel::recv(ch, buf, cap)

// Close
async::channel::close(ch)
```

## File I/O (via fs module)

```mire
load kioto::fs

// Read entire file
set content = fs::read("file.txt")

// Write file
fs::write("file.txt", "content")

// Append
fs::append("file.txt", "more")

// Binary
set bytes = fs::read_bytes("file.bin")
fs::write_bytes("file.bin", bytes)
```

## Stdio Channels

```mire
load kioto::proc

set proc = proc::create("cat" [])
set stdin = proc::stdin(proc)
set stdout = proc::stdout(proc)
set stderr = proc::stderr(proc)

// Write to stdin
proc::write(stdin, "input")

// Read from stdout
set output = proc::read(stdout)
```

## Printing Patterns

```mire
// Simple
dasu("Done")

// With values
dasu("Result: {result}")

// Multiple lines (manual)
dasu("Line 1\nLine 2")

// Formatted numbers
dasu("Value: " + strings::from_i64(x))
dasu("Value: " + strings::from_f64(y))
dasu("Flag: " + strings::from_bool(flag))
```

## Key Points

1. **`dasu` is the only print primitive** - no `println`
2. **No auto-newline** - add `\n` manually
3. **Interpolation only** - `{var}`, not `{expr}`
4. **`ireru` for input** - blocking line read
5. **proc::run::output for commands** - argv-safe, no shell
6. **Shell is explicit** - `proc::run::shell` name signals danger