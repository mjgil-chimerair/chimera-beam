//! chimera_erlang_beam JIT Compiler
//!
//! Just-in-time compiler from BEAM bytecode to native code.

#include "jit.h"
#include "ir.h"
#include "codegen.h"
#include <cstring>
#include <cstdlib>
#include <unordered_map>
#include <mutex>
#include <atomic>

// Compilation statistics
static std::atomic<uint64_t> g_total_compilations(0);
static std::atomic<uint64_t> g_total_recompilations(0);
static std::atomic<uint64_t> g_code_cache_hits(0);

// Maximum number of functions to cache
constexpr size_t MAX_CACHE_ENTRIES = 1024;

// Hotness threshold for recompilation
constexpr uint32_t HOTNESS_THRESHOLD = 1000;

// Code cache entry
struct CacheEntry {
    void* native_code;
    size_t code_size;
    uint32_t hotness;
    bool operator==(const CacheEntry& other) const {
        return native_code == other.native_code;
    }
};

// Simple code cache with function-keyed storage
class CodeCache {
    std::unordered_map<uint64_t, CacheEntry> entries;
    std::mutex cache_mutex;

public:
    uint64_t lookup(uint64_t func_key) {
        std::lock_guard<std::mutex> lock(cache_mutex);
        auto it = entries.find(func_key);
        if (it != entries.end()) {
            it->second.hotness++;
            g_code_cache_hits++;
            return reinterpret_cast<uint64_t>(it->second.native_code);
        }
        return 0;
    }

    void store(uint64_t func_key, void* native_code, size_t code_size) {
        std::lock_guard<std::mutex> lock(cache_mutex);
        CacheEntry entry;
        entry.native_code = native_code;
        entry.code_size = code_size;
        entry.hotness = 1;
        entries[func_key] = entry;

        // Evict oldest if cache is too large
        if (entries.size() > MAX_CACHE_ENTRIES) {
            auto oldest = entries.begin();
            std::free(oldest->second.native_code);
            entries.erase(oldest);
        }
    }

    bool is_hot(uint64_t func_key) {
        std::lock_guard<std::mutex> lock(cache_mutex);
        auto it = entries.find(func_key);
        return it != entries.end() && it->second.hotness >= HOTNESS_THRESHOLD;
    }

    void increment_hotness(uint64_t func_key) {
        std::lock_guard<std::mutex> lock(cache_mutex);
        auto it = entries.find(func_key);
        if (it != entries.end()) {
            it->second.hotness++;
        }
    }

    size_t size() const {
        std::lock_guard<std::mutex> lock(const_cast<std::mutex&>(cache_mutex));
        return entries.size();
    }
};

static CodeCache g_code_cache;

// Compute function key from bytecode and arity
uint64_t compute_func_key(const uint8_t* bytecode, size_t bytecode_len, uint32_t arity) {
    // Simple hash: combine bytecode length, arity, and first/last bytes
    uint64_t key = static_cast<uint64_t>(bytecode_len);
    key = (key << 32) | static_cast<uint64_t>(arity);
    if (bytecode_len > 0) {
        key ^= static_cast<uint64_t>(bytecode[0]) << 24;
        key ^= static_cast<uint64_t>(bytecode[bytecode_len - 1]) << 16;
    }
    return key;
}

JitResult jit_compile(const uint8_t* bytecode, size_t bytecode_len, uint32_t arity) {
    JitResult result = {0};

    if (!bytecode || bytecode_len == 0) {
        result.code = 1;
        result.error = "Invalid bytecode";
        return result;
    }

    g_total_compilations++;

    // Compute function key
    uint64_t func_key = compute_func_key(bytecode, bytecode_len, arity);

    // Check if already compiled
    uint64_t cached_addr = g_code_cache.lookup(func_key);
    if (cached_addr != 0) {
        result.code = 0;
        result.native_code = reinterpret_cast<void*>(cached_addr);
        // Note: we can't return cached size here without changing API
        // The caller should use jit_get_code_size if needed
        return result;
    }

    // Decode bytecode to IR
    ir::IRBuilder builder;
    if (!builder.decode_bytecode(bytecode, bytecode_len, arity)) {
        result.code = 2;
        result.error = "Failed to decode bytecode";
        return result;
    }

    // Generate native code
    codegen::CodeEmitter emitter;
    builder.emit_code(emitter);

    auto code = emitter.get_code();
    if (code.empty()) {
        result.code = 3;
        result.error = "Code generation produced empty output";
        return result;
    }

    // Allocate and copy code
    result.native_code = std::malloc(code.size());
    if (!result.native_code) {
        result.code = 4;
        result.error = "Out of memory";
        return result;
    }

    std::memcpy(result.native_code, code.data(), code.size());
    result.code_size = code.size();

    // Cache the compiled code
    g_code_cache.store(func_key, result.native_code, result.code_size);

    // Check if function is hot enough to trigger recompilation optimization
    if (g_code_cache.is_hot(func_key)) {
        g_total_recompilations++;
        // TODO: trigger recompilation with optimizations
    }

    return result;
}

void jit_free(void* native_code) {
    if (native_code) {
        std::free(native_code);
    }
}

int jit_is_available(void) {
    return 1;  // JIT is available
}

// Get compilation statistics
void jit_get_stats(uint64_t* total_compilations, uint64_t* total_recompilations,
                  uint64_t* cache_hits, size_t* cache_size) {
    if (total_compilations) *total_compilations = g_total_compilations;
    if (total_recompilations) *total_recompilations = g_total_recompilations;
    if (cache_hits) *cache_hits = g_code_cache_hits;
    if (cache_size) *cache_size = g_code_cache.size();
}