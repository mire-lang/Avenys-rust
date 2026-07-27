# PAL Design — Mire v4

The PAL is not a library of functions. The PAL is a resource model of the Host.

## Goals

The PAL exists to fulfill exactly these objectives:

1. **Isolate Kioto from the OS.** Kioto never knows which Host it runs on.
2. **Define a stable contract.** The contract between Mire and any Host is documented in the PAL ABI.
3. **Expose only fundamental Host primitives.** If it can be correctly composed from other PAL primitives, it belongs in Kioto.
4. **Maintain security by default.** Safe operations are the default. Insecure operations are explicit and opt-in.
5. **Allow multiple Hosts without modifying Kioto.** Kioto only depends on the PAL ABI, not on any Host implementation.
6. **Minimize abstraction cost.** The PAL maps directly to Host capabilities without extra layers, allocations, or reinterpretation.
7. **Enable internal evolution without breaking the ABI.** The design may grow; the contract does not.

## Host

A Host is any execution environment that provides OS-level capabilities. A Host can be Linux, Windows, FreeBSD, WASI, an RTOS, a hypervisor, a custom kernel, or a simulator for testing.

Linux is a Host. Windows is a Host. WASI is a Host. They are implementations of the same Host abstraction, not special cases.

## Four Fundamental Concepts

### Host

The execution environment. Defines what capabilities exist and what primitives the PAL can use. The PAL does not describe the Host; the Host exists independently and the PAL exposes its capabilities through a stable interface.

### Resource

A manageable entity provided by the Host. Every resource follows a lifecycle: Acquire → Use → Release. Resources are the root concept of the PAL model. They exist on their own; capabilities provide the authority to access them.

### Capability

Authorization granted by the Host to operate on a resource. A Capability is not a handle. A Capability authorizes; a Handle identifies an open instance. Resources exist independently of both: a File Resource exists whether or not anyone holds a Capability or Handle for it.

Example: a Root Capability grants the authority to open files under a specific root. The Root Capability itself is not the file. It is the authority to interact with files within that boundary.

### Handle

An opaque identifier for a specific opened instance of a Resource. Handles are the only way Kioto interacts with Resources. They never expose internal implementation details (file descriptors, socket identifiers, pointer values). The PAL defines the handle type; only the Host Adapter knows how it is implemented internally.

### Operation

A primitive action on a Resource or stateless service. Operations are the functions exposed by the PAL ABI. Each operation is a fundamental capability that cannot be correctly composed from other PAL primitives.

## Resource Categories

### Stateful Resources

Resources with a lifecycle: Acquire → Use → Release. Only the owner can use or release a Stateful Resource. Ownership transfers explicitly (via Transfer or Clone).

| Resource | Description |
|----------|-------------|
| File | A file opened under a Root |
| Directory | A directory opened for iteration |
| Process | A spawned execution context |
| Socket | A connected or bound network endpoint |
| Listener | A socket listening for incoming connections |
| Channel | An async message queue |
| Secret | Cryptographic key material (private) |

### Stateless Services

Services that query Host state without a lifecycle. They have no Acquire or Release. They return values directly.

| Service | Description |
|---------|-------------|
| Time | Current time queries |
| CPU | CPU information queries |
| Memory | Memory information queries |
| Random | Random bytes generation |

Stateless services never own resources. They only query Host state.

## Relationship Model

```
Host → Resource → Capability → Handle → Operation
```

A Host provides Resources. A Capability grants authority to access a Resource. A Handle identifies an opened instance of a Resource (granted by a Capability). An Operation executes on a Handle.

The Capability does not create the Resource. It authorizes access to one.

## Orthogonal Capabilities

Resources expose capabilities that are orthogonal dimensions, not type hierarchies. A File is Readable + Writable + Seekable. A Socket is Readable + Writable. A Pipe is Readable + Writable. A Listener is Acceptable.

This design scales: when new capabilities are needed (e.g., for mmap, locks, async I/O), they are added as new orthogonal capabilities on existing resources, not as new types or functions.

## Resource Protocol

Every Stateful Resource follows this protocol:

```
Acquire
  ↓
Use
  ├── Clone (optional, creates a second owner)
  ├── Transfer (optional, changes the single owner)
  └── ...
  ↓
Release
```

Clone and Transfer are Operations that occur during the Use phase. They are not separate lifecycle stages. A Resource is never in a "Clone state" — it is always in Use, with possible ownership branching via Clone or Transfer.

### Ownership Rules

Every resource has exactly one owner unless explicitly cloned. Resources are move-only unless explicitly cloned. Clone creates a new independent owner. Transfer changes the sole owner.

### Minimalism Rule

A PAL operation is valid only if at least one Host cannot implement it correctly outside the PAL. If Kioto can compose an operation from existing PAL primitives, that operation belongs in Kioto, not in the PAL.

This rule applies automatically: every proposed PAL function must pass the composition test. If the answer is "Kioto can do it with existing primitives," it is rejected.

## PAL vs Kioto Boundary

### PAL owns (primitives only)

Operations that require Host privileges and cannot be correctly composed from other PAL primitives:

- open (open a file under a Root)
- read (read bytes from a Resource)
- write (write bytes to a Resource)
- spawn (execute a process with argv)
- allocate (PAL-owned memory)
- secure_erase (zeroize sensitive memory)
- sign (cryptographic signing with a Secret)
- verify (cryptographic verification with a PublicKey)
- create_secret (acquire a cryptographic key handle)
- export_public_key (derive public key from a Secret)
- fill_random (fill buffer with Host entropy)
- channel_create (create a message channel)
- channel_send (send on a channel)
- channel_recv (receive from a channel)
- time_now (query current time)
- cpu_count (query CPU count)
- mem_total (query total memory)
- mem_available (query available memory)
- mem_process (query current process memory)

### Kioto owns (compositions)

Operations that can be correctly implemented by composing PAL primitives:

- copy (open + read + write + close)
- move (rename at the Kioto filesystem level)
- walk (iterate directory entries + open + read + close)
- read_text (read bytes + decode UTF-8)
- read_all (read until EOF into buffer)
- write_json (serialize + write)
- hash (read + hash computation)
- any string processing
- any data structure operations
- HTTP parsing/generation
- WebSocket framing

## Faithful Mapping

When a Host provides an equivalent primitive, the PAL maps to it directly. The PAL does not reinterpret, wrap, or add semantics.

When a Host does not provide an equivalent primitive, the Host Adapter provides the minimal faithful implementation without changing PAL semantics. The ABI contract defines what must be true; the Host Adapter ensures it, even if the Host natively provides a different mechanism.

Examples of what the PAL must never do:

- Retry automatically
- Follow symlinks implicitly
- Expand environment variables
- Interpret shell syntax
- Convert text encodings
- Transform line endings
- Change permissions
- Cache results silently

The PAL does exactly what the Host does.

## Errors

Two categories of operations:

### Pure Operations (no error infrastructure)

Operations that have no failure mode or whose failure is a normal condition:

```
time_now() → int64
cpu_count() → int64
mem_total() → int64
mem_available() → int64
mem_process() → int64
```

These return values directly. No error state, no error queries.

### Resource Operations (thread-local error state)

Operations that may fail return an invalid handle or zero value on failure and set the thread's error state. The thread must query the error state after a failure:

```
pal_last_error() → pal_error_code_t
pal_strerror(pal_error_code_t) → const char*
pal_clear_error() → void
```

Kioto maps pal_error_code_t directly to typed error values (e.g., PermissionDenied). No string parsing, no reinterpretation.

### ABI Compatibility Rules

These invariant rules define what can and cannot change without breaking ABI compatibility. They are the contract for ABI evolution.

**Stable invariants (never change):**

- close() always invalidates the resource handle
- clone() always creates a new independent owner
- transfer() always unambiguously changes the sole owner
- Root capabilities never expand authority
- read() never modifies ownership of the resource
- handle types never shrink in size
- enum values are never removed or reordered

**Compatible changes (ABI-compatible):**

- Adding new operations
- Adding new resource types
- Adding new capability types
- Adding new flags (with reserved bits in existing flags)
- Adding new algorithm identifiers to crypto registry
- Expanding struct fields (with reserved padding)
- Adding new stateless service queries

**Incompatible changes (require ABI renegotiation):**

- Removing or renaming operations
- Changing ownership semantics
- Changing handle sizes or layouts
- Changing the meaning of close()
- Changing error code semantics
- Modifying resource protocol guarantees

## Design Principles

### Principle 0: Everything is a Resource

Every meaningful OS entity is a Resource with a lifecycle. Resources are the root concept of the PAL model. Capabilities grant authority over Resources. Handles identify opened instances. Operations execute on Handles.

### Principle 1: Capability-Based Security

Authority comes from the Host, never from the user. A Capability is a grant from the Host. Handles are identifiers granted by Capabilities. Resources exist independently of both. The PAL never fabricates capabilities from userspace input. A path string does not grant authority — a Capability does.

### Principle 2: Minimalism

The PAL contains only primitives that cannot be correctly composed from other PAL primitives. Every function must justify its existence by answering: "Why can't Kioto implement this?" If there is no answer, it does not belong in the PAL.

### Principle 3: Faithful Mapping

The PAL must not invent semantics. If the Host provides a primitive, PAL maps to it directly. If the Host does not, the Host Adapter provides the minimal faithful implementation without changing PAL semantics. No reinterpretation, no added behavior, no hidden transformations.

### Principle 4: Determinism

The PAL never hides Host side effects. It makes no automatic retries, follows no symlinks, interprets no variables, expands no shell syntax, converts no encodings, transforms no line endings, changes no permissions, and does not attempt to be "intelligent." It does exactly what the Host does.

### Principle 5: Ownership Integrity

Every resource has exactly one owner unless explicitly cloned. Resources are move-only unless explicitly cloned. Ownership transfers are explicit and unambiguous. This guarantees no accidental aliasing and no resource leaks.

### Principle 6: Stateless Services Are Not Resources

Time, Random, CPU, and Memory queries do not have lifecycles, ownership, or handles. They are direct queries of Host state. They do not require allocate/release patterns. Conflating stateless queries with stateful resources adds unnecessary complexity.

### Principle 7: PAL vs Kioto Separation

The PAL only contains primitives requiring Host privilege. Kioto composes primitives into higher-level abstractions. Copy, walk, JSON, string processing, and data structures belong in Kioto. The PAL never becomes a second standard library.

### Principle 8: Separation of Concerns

Design (PAL-DESIGN.md) defines the model. ABI (PAL-ABI.md) defines the contract. Implementation (PAL-CORE.md + Host Adapter) defines the realization. The design does not dictate implementation details. The ABI does not leak implementation. The implementation does not redefine the design.
