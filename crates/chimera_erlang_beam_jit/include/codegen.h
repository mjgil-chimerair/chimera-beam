//! Code Generation for JIT
//!
//! Generates native x86-64 code from the IR.
//!
//! This module provides the C API wrapper around the codegen namespace.

#pragma once

#include "ir.h"
#include <cstdint>
#include <vector>

namespace codegen {

// Using CodeEmitter from ir namespace
using ::ir::CodeEmitter;

// Generate add function: result = arg1 + arg2
inline std::vector<uint8_t> generate_add_function() {
    CodeEmitter emitter;

    emitter.emit_prologue();

    // Function receives: rdi = arg1, rsi = arg2 (System V ABI)
    // mov rax, rdi
    emitter.emit_byte(0x48);
    emitter.emit_byte(0x89);
    emitter.emit_byte(0xF8);

    // mov rcx, rsi
    emitter.emit_byte(0x48);
    emitter.emit_byte(0x89);
    emitter.emit_byte(0xF1);

    // add rax, rcx
    emitter.emit_add_reg_reg(ir::Reg::rax, ir::Reg::rcx);

    emitter.emit_epilogue();

    return emitter.get_code();
}

// Generate function that returns an immediate
inline std::vector<uint8_t> generate_return_imm(int value) {
    CodeEmitter emitter;

    emitter.emit_prologue();

    // mov rax, imm32 (sign-extended)
    emitter.emit_byte(0xB8);
    emitter.emit_imm32(static_cast<uint32_t>(value));

    emitter.emit_epilogue();

    return emitter.get_code();
}

} // namespace codegen