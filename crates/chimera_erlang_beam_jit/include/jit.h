#pragma once

#include <cstdint>
#include <cstddef>

#ifdef __cplusplus
extern "C" {
#endif

// JIT compilation result
struct JitResult {
    uint32_t code;          // 0 = success
    void* native_code;      // pointer to compiled code
    size_t code_size;        // size in bytes
    const char* error;       // error message if failed
};

// Compile BEAM bytecode to native code
// Returns JitResult with native_code pointer or error
JitResult jit_compile(const uint8_t* bytecode, size_t bytecode_len, uint32_t arity);

// Free compiled code
void jit_free(void* native_code);

// Check if JIT is available
int jit_is_available(void);

// Get compilation statistics
void jit_get_stats(uint64_t* total_compilations, uint64_t* total_recompilations,
                  uint64_t* cache_hits, size_t* cache_size);

#ifdef __cplusplus
}
#endif