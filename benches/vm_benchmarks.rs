//! Benchmarks for RustZigBeam VM components.
//!
//! Per task-3.md Task D-1: Add Benchmark Suite.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use chimera_erlang_beam_vm::VirtualMachine;
use chimera_erlang_beam_term::Term;
use chimera_erlang_beam_process::Pid;

/// Benchmark: VM spawn overhead
fn bench_vm_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("vm_spawn");

    for &heap_size in &[1024, 4096, 8192, 16384] {
        group.bench_with_input(
            BenchmarkId::from_parameter(heap_size),
            &heap_size,
            |b, &hs| {
                b.iter(|| {
                    let mut vm = VirtualMachine::new(0);
                    black_box(vm.spawn(hs));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Term creation
fn bench_term_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("term_creation");

    group.bench_function("small_integer", |b| {
        b.iter(|| black_box(Term::from_small(42)));
    });

    group.bench_function("atom", |b| {
        b.iter(|| black_box(Term::from_atom(1)));
    });

    group.bench_function("nil", |b| {
        b.iter(|| black_box(Term::nil()));
    });

    group.finish();
}

/// Benchmark: Instruction execution
fn bench_instruction_execution(c: &mut Criterion) {
    use chimera_erlang_beam_instr::{ExecContext, Opcode, step};

    let mut group = c.benchmark_group("instruction_execution");

    // LoadInt instruction
    group.bench_function("load_int", |b| {
        let mut ctx = ExecContext::new();
        let instr: u64 = (Opcode::LoadInt as u64) | (1_u64 << 16) | ((42_u64) << 32);
        let code = [instr];
        b.iter(|| {
            ctx.ip = 0;
            black_box(step(black_box(&mut ctx), &code));
        });
    });

    // Add instruction
    group.bench_function("add", |b| {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(10));
        ctx.set_x(2, Term::from_small(32));
        let instr: u64 = (Opcode::Add as u64) | (0_u64 << 16) | (1_u64 << 24) | (2_u64 << 32);
        let code = [instr];
        b.iter(|| {
            ctx.ip = 0;
            ctx.x[0] = Term::nil();
            black_box(step(black_box(&mut ctx), &code));
        });
    });

    // Jump instruction
    group.bench_function("jump", |b| {
        let mut ctx = ExecContext::new();
        ctx.ip = 5;
        let instr: u64 = (Opcode::Jump as u64) | ((10 as u64) << 32);
        let code = [0, 0, 0, 0, 0, instr];
        b.iter(|| {
            ctx.ip = 5;
            black_box(step(black_box(&mut ctx), &code));
        });
    });

    group.finish();
}

/// Benchmark: Send message between processes
fn bench_send_message(c: &mut Criterion) {
    let mut group = c.benchmark_group("send_message");

    group.bench_function("local_send", |b| {
        let mut vm = VirtualMachine::new(0);
        let sender = vm.spawn(8192);
        let receiver = vm.spawn(8192);
        let msg = Term::from_small(42);

        b.iter(|| {
            black_box(vm.send(sender, receiver, msg));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_vm_spawn,
    bench_term_creation,
    bench_instruction_execution,
    bench_send_message
);
criterion_main!(benches);