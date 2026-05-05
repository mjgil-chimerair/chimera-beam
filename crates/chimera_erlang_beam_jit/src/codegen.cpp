//! Code Generation for JIT
//!
//! Generates native x86-64 code from the IR.
//!
//! This module provides the C API wrapper around the codegen namespace.

#include "jit.h"
#include "codegen.h"
#include <cstring>
#include <cstdlib>
#include <vector>

// C API implementation
extern "C" {

// Generate add function code
JitResult jit_generate_add() {
    JitResult result = {0};

    auto code = codegen::generate_add_function();

    if (code.empty()) {
        result.code = 1;
        result.error = "Failed to generate code";
        return result;
    }

    result.code = 0;
    result.native_code = std::malloc(code.size());
    if (!result.native_code) {
        result.code = 2;
        result.error = "Out of memory";
        return result;
    }

    std::memcpy(result.native_code, code.data(), code.size());
    result.code_size = code.size();

    return result;
}

// Generate return immediate function
JitResult jit_generate_return_imm(int value) {
    JitResult result = {0};

    auto code = codegen::generate_return_imm(value);

    if (code.empty()) {
        result.code = 1;
        result.error = "Failed to generate code";
        return result;
    }

    result.code = 0;
    result.native_code = std::malloc(code.size());
    if (!result.native_code) {
        result.code = 2;
        result.error = "Out of memory";
        return result;
    }

    std::memcpy(result.native_code, code.data(), code.size());
    result.code_size = code.size();

    return result;
}

} // extern "C"