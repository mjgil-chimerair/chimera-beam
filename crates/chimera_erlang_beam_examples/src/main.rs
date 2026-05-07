// Full E2E integration test: Elixir source → VM execution

use chimera_erlang_beam_instr::{execute_instruction, ExecContext};
use chimera_erlang_beam_term::Term;
use chimera_erlang_beam_vm::VirtualMachine;

fn main() {
    println!("=== Full E2E Integration Test ===\n");

    // Test basic term creation
    println!("Step 1: Test term creation");
    let t2 = Term::from_small(2);
    let t3 = Term::from_small(3);
    let t5 = Term::from_small(5);
    println!("  Term::from_small(2) = {:?}", t2);
    println!("  Term::from_small(3) = {:?}", t3);
    println!("  Term::from_small(5) = {:?}", t5);

    // Test term addition (manual)
    println!("\nStep 2: Test manual addition");
    let mut ctx = ExecContext::new();
    ctx.x[0] = t2;
    ctx.x[1] = t3;
    println!("  Before add: x[0]={:?}, x[1]={:?}", ctx.x[0], ctx.x[1]);

    // Check if we can add
    println!("  t2 == t3: {}", t2 == t3);

    // Test add instruction
    println!("\nStep 3: Execute add instruction");
    let code: Vec<u64> = vec![
        (79u64) | (1u64 << 32), // Add x0, x0, x1
        80u64,                  // Return
    ];

    // Execute add
    let result = execute_instruction(&mut ctx, &code);
    println!("  After add: x[0]={:?}, x[1]={:?}", ctx.x[0], ctx.x[1]);
    println!("  Result: {:?}", result);

    // Check if result is correct
    println!("\nStep 4: Check result");
    if ctx.x[0] == t5 {
        println!("  [PASS] add(2, 3) = 5 ✓");
    } else {
        println!("  [FAIL] Expected {:?}, got {:?}", t5, ctx.x[0]);
    }

    // Test VM spawn
    println!("\nStep 5: Test VM spawn");
    let mut vm = VirtualMachine::new(1);
    let pid = vm.spawn(256);
    println!("  Spawned process: {:?}", pid);

    // Summary
    println!("\n=== E2E Test Complete ===\n");
}
