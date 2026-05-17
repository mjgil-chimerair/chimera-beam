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

### ChimeraIR binary builds

The three ChimeraIR build variants for this repo are driven by the `chimera`
CLI from [`../chimerair/tools`](../chimerair/tools/README.md).
Build that CLI first:

```bash
cd ../chimerair/tools
cargo build --release -p chimera-cli
cd ../../chimera-beam
```

Use your host triple or pass an explicit target, for example:

```bash
HOST_TRIPLE=x86_64-unknown-linux-gnu
CHIMERA=../chimerair/tools/target/release/chimera
```

#### 1. ABI binary

This is the Cargo/C ABI path from [`Chimera.toml`](./Chimera.toml).

```bash
"$CHIMERA" build --manifest Chimera.toml --target "$HOST_TRIPLE" --output ./build-abi
```

Output path: `build-abi/chimera_binary`

#### 2. Chimera adapter binary

This is the adapter/bridge path from [`Chimera.adapter.toml`](./Chimera.adapter.toml).

```bash
"$CHIMERA" build --manifest Chimera.adapter.toml --target "$HOST_TRIPLE" --output ./build-adapter
```

Output path: `build-adapter/chimera_binary`

#### 3. Chimera semantic binary

This is the unified semantic lowering path from [`Chimera.separate.toml`](./Chimera.separate.toml):

1. Rust lowers to ChimeraIR
2. Zig lowers to ChimeraIR
3. C lowers to ChimeraIR
4. the IR is merged and optimized
5. LLVM IR is emitted
6. the final executable is linked

```bash
"$CHIMERA" build --manifest Chimera.separate.toml --target "$HOST_TRIPLE" --output ./build-semantic
```

Output path: `build-semantic/chimera_binary`

## Testing

```bash
cargo test --release --workspace
```
