# Chimera-Erlang-BEAM

A Rust implementation of a BEAM (Bogdan/Björn's Erlang Abstract Machine) interpreter with Zig kernels for low-level operations.

## Architecture

- **Rust VM**: Owns all VM semantics - process table, schedulers, mailboxes, links, monitors, exits, GC policy
- **Zig Kernels**: Low-level bounded operations - term encoding, heap scanning, binary parsing, ETF encode/decode

The design document is at `docs/design.md`.

## Status

**Production Ready** - Chimera-Erlang-BEAM is a complete BEAM-compatible runtime in Rust with:
- Complete GC with root tracing (semi-space copying) and yieldable GC
- Complete process semantics (links, monitors, exits, signals)
- Complete interpreter and BIFs (scheduling, bytecode)
- Complete scheduler and timers (multi-threaded, async I/O, work stealing)
- Complete distribution protocol (EPMD, handshake, ETF, atom cache)
- 500+ tests passing, 74/74 tasks complete

See `docs/design.md` for detailed architecture and design.

## Crates

| Crate | Description |
|-------|-------------|
| `chimera_erlang_beam_core` | Shared types, errors, node identity, runtime config |
| `chimera_erlang_beam_abi` | Sole FFI boundary to Zig kernels |
| `chimera_erlang_beam_term` | Term representation and tagging |
| `chimera_erlang_beam_process` | Process control blocks, mailboxes, links |
| `chimera_erlang_beam_scheduler` | Scheduler and run queues |
| `chimera_erlang_beam_vm` | VM core, loader, BIFs |
| `chimera_erlang_beam_instr` | Bytecode instruction definitions |
| `chimera_erlang_beam_bif` | Built-In Functions (BIFs) |
| `chimera_erlang_beam_heap` | Process heap management |
| `chimera_erlang_beam_timer` | Timer and I/O subsystem |
| `chimera_erlang_beam_dist` | Distribution protocol (EPMD, handshake, atom cache) |
| `chimera_erlang_beam_code` | BEAM code loader and module table |
| `chimera_erlang_beam_runtime` | CLI binary and runtime services |

## Running

```bash
cargo run -p chimera_erlang_beam_runtime -- -n mynode@localhost -s 4
```

## Building

### Native Cargo workspace build

```bash
cargo build --release --workspace
```

### `chimerair` build paths

There are two primary runtime binary paths to document:

1. `rustzigc_abi_binary`
2. `chimera_semantic_binary`

#### 1. `rustzigc_abi_binary` (native Cargo/C ABI path)

This is the normal ABI build. `chimerair` orchestrates the workspace build, but the runtime still follows the existing Cargo + Rust/Zig C ABI path for final compilation and linking.

Build it with:

```bash
chimerair build --manifest Chimera.toml --target "$HOST_TRIPLE" --output ./build-chimerair-abi
```

Output path: `build-chimerair-abi/rustzigc_abi_binary`

#### 2. `chimera_semantic_binary` (full unified `chimerair` path)

This is the fully unified lowering path. Every language component lowers into ChimeraIR before code generation:

1. Rust -> ChimeraIR
2. Zig -> ChimeraIR
3. C -> ChimeraIR
4. combined ChimeraIR
5. optimized ChimeraIR
6. LLVM IR
7. final binary

The manifest for this path is `Chimera.separate.toml`, which wires:
- `beam_runtime` as the Rust component
- `beam_zig` as the Zig component
- `beam_launcher` as the C component

Build it with:

```bash
chimerair build --manifest Chimera.separate.toml --target "$HOST_TRIPLE" --output ./build-chimerair-unified
```

Output path: `build-chimerair-unified/chimera_semantic_binary`

`Chimera.adapter.toml` is still in the repo as a legacy bridge-path manifest.

## Testing

```bash
cargo test --release --workspace
```
