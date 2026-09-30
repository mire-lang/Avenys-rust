// Memory Ownership

## Overview

Mire uses a **compile-time ownership model** integrated with an **arena allocator** at runtime. The type system tracks ownership; the runtime manages memory.

## Owned Values (Move Semantics)

```mire
set s1 = "hello" :str
set s2 = s1          // MOVE: s1 is now invalid
// use s1             // Error: use after move

pub fn take(s :str) { ... }
take(s2)             // MOVE into function
// use s2             // Error: moved
```

- Passing by value = **move** (ownership transfers)
- After move, source binding is invalid
- Borrow checker reports "Use after move" (MSS Error)

## Borrowed References (Borrow Semantics)

```mire
set s = "hello" :str
set r = &s           // Shared borrow (&str)
set r2 = &s          // Multiple shared borrows OK

pub fn read(r :&str) { ... }
read(r)              // Borrow passed
// s still usable after
```

- `&T` = shared borrow (read-only)
- Multiple shared borrows allowed simultaneously
- Original owner remains accessible

## Mutable Borrows

```mire
set s = "hello" :str mut
set r = &mut s       // Exclusive borrow (&mut str)

pub fn modify(r :&mut str) { ... }
modify(r)
// s accessible after borrow ends
```

- `&mut T` = exclusive borrow (read-write)
- **Only one mutable borrow at a time**
- No shared borrows while mutable borrow exists
- No mutable borrows while shared borrows exist

## Borrow Checker Rules

| Scenario | Allowed? |
|----------|----------|
| Multiple `&T` | Yes |
| One `&mut T` | Yes |
| `&mut T` + any `&T` | No |
| Two `&mut T` | No |

## Function Parameter Modes

```mire
// By value (move) - consumes ownership
pub fn consume(s :str) { ... }

// By shared reference - read only
pub fn read(r :&str) { ... }

// By mutable reference - can modify
pub fn write(r :&mut str) { ... }
```

**Design Rule**: Prefer `&str` if function doesn't destroy/consume the value.

## Return Values

```mire
pub fn create: () :str {
    return "new string"  // Returns owned value
}

pub fn get_ref: (s :&str) :&str {
    return s  // Returns borrowed reference (tied to input lifetime)
}
```

- Returning owned = move to caller
- Returning reference = borrow tied to parameter (enforced by compiler)

## Runtime Memory Management

##// Arena Allocator

- Single 1GB `mmap` + 9 size-class free lists (16..4096 bytes)
- All strings/structs allocated from arena
- On `refs == 0`: block returns to free list
- `rt_managed_cleanup_all` uses `munmap` at exit

##// Refcounting

- Each managed string has `refs` counter
- `rt_managed_retain` increments, `rt_managed_free` decrements
- When `refs` reaches 0: block returned to free list
- Container storage (`vec`, `map`) retains elements

##// Drop (MIR Level)

```mir
MirOp::Drop(value)  // Explicit drop inserted by borrowck/lowering
```

- Inserted on: reassignment, scope exit, return
- Codegen per tier:
  - `Full`: no-op (arena handles)
  - `Minimal`: `rt_managed_free`
  - `None`: `free`