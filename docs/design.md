# Chimera-BEAM Design

## Summary
`chimera-beam` is a reimplementation of an Erlang language stack and BEAM-like runtime, with Rust as the primary implementation language.

Implementation split from the source table:

| Host language | Share | Role |
| --- | ---: | --- |
| Rust | 55% | VM, schedulers, processes, compiler, distribution, fault tolerance core |
| C++ | 20% | JIT/optimizer or narrow low-level runtime integrations |
| Zig | 15% | Tight VM components and systems-level helpers |
| OCaml | 10% | Compiler frontend and verification prototypes |

Winner: Rust. Reason: BEAM-like work requires schedulers, lightweight processes, message passing, GC strategy, distribution, and fault tolerance machinery, all of which benefit from Rust's safety and systems-level control.

## Goals
- Build an Erlang compiler plus a BEAM-like virtual machine and runtime.
- Preserve the semantic pillars: immutable terms, actor-style concurrency, supervision, soft real-time scheduling, and distribution.
- Keep the runtime architecture explicit enough to evolve independently from the language frontend.

## Non-Goals
- Full drop-in binary compatibility with BEAM on day one.
- A tracing JIT before bytecode execution semantics are stable.
- Distributed clustering across hostile networks in the first release.

## Architecture
1. Frontend
- Rust parser for Erlang syntax, module system, records, pattern matching, guards, and binaries.
- Core Erlang-style typed-lowering stage before bytecode generation.

1. Compiler IR
- Source AST.
- Core functional IR after desugaring.
- VM IR/bytecode with explicit reductions, calls, receives, and exception paths.

1. VM and runtime
- Rust owns process heaps, mailboxes, scheduler run queues, timers, monitors/links, code loading, and node services.
- Per-process or generational GC strategy selected after benchmarking, but isolated behind a process-memory API.

1. Distribution
- Node identity, term serialization, remote spawn/message semantics, and failure detection.
- Start with local runtime first, then same-host node distribution, then remote node support.

1. Acceleration layers
- Zig may own term encoding/decoding helpers, bytecode dispatch experiments, or compact allocator components.
- C++ reserved for optional JIT or optimizer tiers.

## Percentage-to-Subsystem Mapping
- Rust 55%: `compiler/`, `bytecode/`, `vm/`, `scheduler/`, `process/`, `gc/`, `distribution/`, `observability/`.
- C++ 20%: `jit/`, `native-optimizer/`, isolated runtime accelerators.
- Zig 15%: `dispatch/`, `allocator/`, `binary-ops/`, `term-codec/`.
- OCaml 10%: `core-erlang-lab/`, `guard-analysis/`, `frontend-proofs/`.

## Key Design Decisions
- Process isolation is the primary runtime abstraction, not threads.
- Scheduler fairness, reduction counting, and failure semantics are first-class design surfaces.
- Distribution is a phase after single-node correctness, not baked into every first milestone.

## Phases
1. Parser, AST, and Core Erlang lowering.
1. Bytecode format, interpreter loop, process heap, mailboxes, and local scheduling.
1. Links, monitors, supervision, timers, and hot-code loading basics.
1. Distribution protocol and multi-node execution.
1. Optional JIT/optimizer tiers and performance tuning.

## Testing Strategy
- Frontend tests for pattern matching, guards, records, binaries, and modules.
- VM tests for receive semantics, reduction accounting, process spawning, links/monitors, and crash propagation.
- GC stress tests with long-lived processes and binary-heavy workloads.
- Distribution tests for node joins, message ordering, and failure handling.

## Major Risks
- Runtime scope can overwhelm compiler progress if milestones are not sharply staged.
- GC and process heap choices affect almost every subsystem.
- Distribution and hot code loading can introduce architectural churn if started too early.

## Staffing Plan
- Rust carries the product core and most engineering volume.
- Zig and C++ are reserved for measurable runtime hotspots.
- OCaml supports frontend modeling and verification work without owning the product path.

## Exit Criteria For V1
- Executes Erlang-like programs on a stable bytecode VM.
- Supports lightweight processes, mailboxes, supervision basics, and crash isolation.
- Has a defined path to distribution without redesigning core runtime abstractions.
