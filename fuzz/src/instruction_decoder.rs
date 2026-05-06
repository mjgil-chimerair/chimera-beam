//! Fuzz target: Instruction decoder
//!
//! Tests that invalid opcode/operand combinations don't cause panics
//! in the instruction decoder.

#![no_main]

use libfuzzer_sys::fuzz_target;
use chimera_erlang_beam_instr::{ExecContext, step};

fuzz_target!(|data: &[u8]| {
    // Try to decode and execute random data as instructions
    let _ = execute_instruction_safe(data);
});

/// Safely attempt to execute instructions from bytes
/// Returns without panicking on invalid input
fn execute_instruction_safe(code: &[u8]) {
    if code.len() < 8 {
        return;
    }

    // Process in 8-byte chunks
    let mut ctx = ExecContext::new();

    // Limit the number of instructions to prevent infinite loops
    let max_instructions = 100;
    let mut count = 0;

    for chunk in code.chunks(8) {
        if chunk.len() < 8 || count >= max_instructions {
            break;
        }

        let mut word = [0u8; 8];
        word.copy_from_slice(chunk);
        let instruction = u64::from_le_bytes(word);

        // Try to step - this should not panic
        let _ = step(&mut ctx, &[instruction]);
        count += 1;
    }
}