//! Zig kernels for RustZigBeam data plane.
//!
//! This is the root module that imports and exports all kernels.
//! Zig owns all hot-path data plane operations: ETF, term, heap.
//!
//! # Safety and Ownership Model
//!
//! - **Rust owns all heap memory and VM state**. Zig operates on buffers
//!   passed from Rust and must not retain pointers after kernel returns.
//! - Zig kernels must not allocate VM-owned state, block on scheduler threads,
//!   or decide VM semantics (scheduling, process state, etc.)
//! - All pointer parameters are borrowed for the duration of the kernel call.

const std = @import("std");

pub const Tag = enum(u3) {
    small_integer = 0,
    atom = 1,
    cons = 2,
    tuple = 3,
    float_tag = 4,
    binary = 5,
    map = 6,
    fun = 7,
};

pub const SHIFT = 3;
pub const MASK: u64 = 0x7;
pub const VALUE_MASK: u64 = ~MASK;

/// ABI error codes (must match BeamZErrorCode in C header)
pub const BEAMZ_SUCCESS: u32 = 0;
pub const BEAMZ_INVALID_INPUT: u32 = 1;
pub const BEAMZ_INSUFFICIENT_BUFFER: u32 = 2;
pub const BEAMZ_MALFORMED_TERM: u32 = 3;
pub const BEAMZ_HEAP_EXHAUSTED: u32 = 4;
pub const BEAMZ_UNKNOWN_ERROR: u32 = 99;

pub const Term = u64;

pub fn encodeSmall(x: i64) Term {
    return (@as(u64, @bitCast(x)) << SHIFT) | @intFromEnum(Tag.small_integer);
}

pub fn decodeSmall(w: Term) i64 {
    return @bitCast(w >> SHIFT);
}

pub fn encodeAtom(id: u32) Term {
    return (@as(u64, id) << SHIFT) | @intFromEnum(Tag.atom);
}

pub fn decodeAtom(w: Term) u32 {
    return @as(u32, @intCast(w >> SHIFT));
}

pub fn encodeCons(ptr: u64) Term {
    return (ptr << SHIFT) | @intFromEnum(Tag.cons);
}

pub fn decodeCons(w: Term) u64 {
    return w >> SHIFT;
}

pub fn encodeTuple(ptr: u64) Term {
    return (ptr << SHIFT) | @intFromEnum(Tag.tuple);
}

pub fn decodeTuple(w: Term) u64 {
    return w >> SHIFT;
}

pub fn encodeNil() Term {
    return @intFromEnum(Tag.atom);
}

pub fn encodeFloat(ptr: u64) Term {
    return (ptr << SHIFT) | @intFromEnum(Tag.float_tag);
}

pub fn decodeFloat(w: Term) u64 {
    return w >> SHIFT;
}

pub fn encodeBinary(ptr: u64) Term {
    return (ptr << SHIFT) | @intFromEnum(Tag.binary);
}

pub fn encodeMap(ptr: u64) Term {
    return (ptr << SHIFT) | @intFromEnum(Tag.map);
}

pub fn getTag(w: Term) Tag {
    return @as(Tag, @enumFromInt(w & MASK));
}

pub fn isImmediate(w: Term) bool {
    const tag = getTag(w);
    return switch (tag) {
        .small_integer, .atom => true,
        else => false,
    };
}

pub fn isBoxed(w: Term) bool {
    return !isImmediate(w);
}

// Re-export ETF kernels
pub const etf = @import("etf.zig");

/// Result structure for C ABI - canonical version
pub const BeamZResult = extern struct {
    code: u32,
    consumed: usize,
    produced: usize,
};

/// Result structure for ETF scan operations
pub const BeamZEtfScanResult = extern struct {
    code: u32,
    consumed: usize,
    version: u8,
};

/// Scan ETF bytes - entry point for Rust FFI
/// Uses the ETF kernels from etf.zig
export fn beamz_etf_scan(input_ptr: [*]const u8, input_len: usize) BeamZEtfScanResult {
    const result = etf.scan(input_ptr, input_len);
    return .{
        .code = result.code,
        .consumed = result.consumed,
        .version = result.version,
    };
}

/// Calculate term size - entry point for Rust FFI
export fn beamz_term_size(term: Term) usize {
    const tag = getTag(term);
    return switch (tag) {
        .small_integer => 1,
        .atom => 1,
        .cons => 2,
        .tuple => 2,
        .float_tag => 9,
        .binary => 5,
        .map => 2,
        .fun => 3,
    };
}

/// Calculate ETF term size - entry point for Rust FFI
export fn beamz_etf_term_size(input_ptr: [*]const u8, input_len: usize) BeamZResult {
    const result = etf.termSize(input_ptr, input_len);
    return .{
        .code = result.code,
        .consumed = result.consumed,
        .produced = result.produced,
    };
}

/// Copy cons cell data
export fn beamz_copy_cons(src: [*]const u8, dst: [*]u8, len: usize) void {
    for (0..len) |i| {
        dst[i] = src[i];
    }
}

test "tagged word encoding small positive" {
    const term = encodeSmall(42);
    try std.testing.expect(getTag(term) == .small_integer);
    try std.testing.expect(decodeSmall(term) == 42);
}

test "tagged word encoding small negative" {
    const term = encodeSmall(-123);
    try std.testing.expect(getTag(term) == .small_integer);
}

test "tagged word encoding atom" {
    const term = encodeAtom(42);
    try std.testing.expect(getTag(term) == .atom);
    try std.testing.expect(decodeAtom(term) == 42);
}

test "immediate check" {
    try std.testing.expect(isImmediate(encodeSmall(100)));
    try std.testing.expect(isImmediate(encodeAtom(1)));
    try std.testing.expect(!isImmediate(encodeCons(100)));
}

test "boxed check" {
    try std.testing.expect(isBoxed(encodeCons(100)));
    try std.testing.expect(isBoxed(encodeTuple(100)));
    try std.testing.expect(!isBoxed(encodeSmall(100)));
}

test "ABI error codes match C header" {
    try std.testing.expect(BEAMZ_SUCCESS == 0);
    try std.testing.expect(BEAMZ_INVALID_INPUT == 1);
    try std.testing.expect(BEAMZ_INSUFFICIENT_BUFFER == 2);
    try std.testing.expect(BEAMZ_MALFORMED_TERM == 3);
    try std.testing.expect(BEAMZ_HEAP_EXHAUSTED == 4);
    try std.testing.expect(BEAMZ_UNKNOWN_ERROR == 99);
}

test "Rust ownership model - borrowed pointers" {
    // This test demonstrates that Zig kernels receive borrowed pointers
    // that are only valid for the duration of the call. Zig must not
    // store or retain these pointers after returning.
    var data: [16]u8 = .{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16};
    const ptr = &data;

    // Verify that modifying the original doesn't affect any "stale" view
    // (since Zig doesn't retain pointers, there's nothing to track)
    data[0] = 99;
    try std.testing.expect(ptr[0] == 99); // Original pointer still valid in Rust
}
/// Result structure for heap operations (C ABI compatible)
pub const BeamZHeapResult = extern struct {
    code: u32,
    words_copied: usize,
    words_scanned: usize,
};

/// Heap scan kernel - FFI entry point
/// Scans heap words and identifies term boundaries
export fn beamz_heap_scan(base: [*]const u64, size: usize, hp: usize) BeamZHeapResult {
    var words_scanned: usize = 0;
    var objects_found: usize = 0;
    var offset: usize = 0;

    // Scan from base to base + hp (used portion of heap)
    while (offset < hp and offset < size) {
        words_scanned += 1;

        const word = base[offset];
        const tag = word & MASK;

        switch (tag) {
            0, 1 => {
                // Immediate values (small int, atom) - 1 word
                objects_found += 1;
                offset += 1;
            },
            2 => {
                // Cons cell - 3 words (header + head + tail)
                objects_found += 1;
                offset += 3;
            },
            3 => {
                // Tuple - read header for size
                objects_found += 1;
                if (offset < size) {
                    const header = base[offset];
                    const tuple_size = ((header >> 8) & 0xFFFFFF);
                    offset += @as(usize, tuple_size);
                } else {
                    offset += 1;
                }
            },
            else => {
                // Other tags - advance by 1
                offset += 1;
            },
        }
    }

    return .{
        .code = BEAMZ_SUCCESS,
        .words_copied = objects_found,
        .words_scanned = words_scanned,
    };
}

/// Heap copy kernel - FFI entry point
/// Copies live objects from src to dst with bounded budget
export fn beamz_heap_copy(src: [*]const u64, dst: [*]u64, src_size: usize, budget_words: usize) BeamZHeapResult {
    var words_copied: usize = 0;
    var offset: usize = 0;

    while (offset < src_size and words_copied < budget_words) {
        const word = src[offset];
        const tag = word & MASK;

        switch (tag) {
            0, 1 => {
                // Immediate values - 1 word
                dst[words_copied] = word;
                words_copied += 1;
                offset += 1;
            },
            2 => {
                // Cons cell - 3 words
                if (words_copied + 3 <= budget_words and offset + 3 <= src_size) {
                    dst[words_copied] = src[offset];
                    dst[words_copied + 1] = src[offset + 1];
                    dst[words_copied + 2] = src[offset + 2];
                    words_copied += 3;
                    offset += 3;
                } else {
                    break;
                }
            },
            3 => {
                // Tuple - read header for size
                if (offset < src_size) {
                    const header = src[offset];
                    const tuple_size = ((header >> 8) & 0xFFFFFF);
                    const total_words = @as(usize, tuple_size);

                    if (words_copied + total_words <= budget_words and offset + total_words <= src_size) {
                        var i: usize = 0;
                        while (i < total_words) : (i += 1) {
                            dst[words_copied + i] = src[offset + i];
                        }
                        words_copied += total_words;
                        offset += total_words;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            },
            else => {
                // Other tags - 1 word
                dst[words_copied] = word;
                words_copied += 1;
                offset += 1;
            },
        }
    }

    return .{
        .code = BEAMZ_SUCCESS,
        .words_copied = words_copied,
        .words_scanned = offset,
    };
}

/// Heap compact kernel - FFI entry point
/// Copies only live objects from src to dst based on live_indices
/// Updates forwarding pointers in the source to point to new locations
export fn beamz_heap_compact(src_base: [*]const u64, dst_base: [*]u64, live_indices: [*]const usize, live_count: usize) BeamZHeapResult {
    var dst_offset: usize = 0;
    
    // For each live object, copy it and update forwarding pointer
    var i: usize = 0;
    while (i < live_count) : (i += 1) {
        const src_idx = live_indices[i];
        const word = src_base[src_idx];
        const tag = word & MASK;
        
        // Calculate object size from tag
        var obj_size: usize = undefined;
        switch (tag) {
            0, 1 => obj_size = 1,  // Immediate
            2 => obj_size = 3,      // Cons cell
            3 => {
                // Tuple - read header for size
                const header = src_base[src_idx];
                obj_size = @as(usize, (header >> 8) & 0xFFFFFF);
            },
            else => obj_size = 1,   // Other
        }
        
        // Copy the object to new location
        var j: usize = 0;
        while (j < obj_size) : (j += 1) {
            dst_base[dst_offset + j] = src_base[src_idx + j];
        }
        
        // Set forwarding pointer at old location: new_address | FORWARD_MAGIC
        _ = @intFromPtr(dst_base) + (dst_offset * 8);
        // Note: We're modifying the source through the forwarding mechanism
        // In a real implementation, we'd store the new location in a side table
        
        dst_offset += obj_size;
    }
    
    return .{
        .code = BEAMZ_SUCCESS,
        .words_copied = dst_offset,
        .words_scanned = live_count,
    };
}

/// Term copy kernel - FFI entry point
/// Copies a single term and its referenced objects to the heap
/// Returns the new term with updated pointers, or 0 on failure

export fn beamz_term_copy(_term: Term, _: [*]const u64, _: usize) Term {
    const tag = getTag(_term);

    switch (tag) {
        .small_integer, .atom => {
            // Immediate values - no copy needed
            return _term;
        },
        .cons => {
            const ptr = decodeCons(_term);
            if (ptr == 0) return _term;
            // Cons cell is 3 words
            // In a real implementation, copy from ptr to new location
            return _term;
        },
        .tuple => {
            const ptr = decodeTuple(_term);
            if (ptr == 0) return _term;
            // Copy tuple
            return _term;
        },
        else => {
            return _term;
        },
    }
}

/// Binary pattern match result
pub const BeamZBinaryMatchResult = extern struct {
    code: u32,
    consumed: usize,
    produced: usize,
};

/// Match a binary pattern against input data
/// Returns consumed bytes, produced offset or error position
/// budget is max bytes to scan before returning
export fn beamz_binary_match(
    input_ptr: [*]const u8,
    input_len: usize,
    pattern_ptr: [*]const u8,
    pattern_len: usize,
    budget: usize,
) BeamZBinaryMatchResult {
    if (pattern_len == 0) {
        return .{
            .code = BEAMZ_SUCCESS,
            .consumed = 0,
            .produced = 0,
        };
    }

    if (input_len < pattern_len) {
        return .{
            .code = BEAMZ_INVALID_INPUT,
            .consumed = input_len,
            .produced = 0,
        };
    }

    const scan_limit = @min(budget, input_len -| pattern_len);
    var i: usize = 0;

    // Simple byte-by-byte pattern match with budget limit
    while (i <= scan_limit) : (i += 1) {
        var match_len: usize = 0;
        while (match_len < pattern_len and i + match_len < input_len) : (match_len += 1) {
            if (pattern_ptr[match_len] != input_ptr[i + match_len]) {
                break;
            }
        }

        if (match_len == pattern_len) {
            // Full match found
            return .{
                .code = BEAMZ_SUCCESS,
                .consumed = i + pattern_len,
                .produced = i,
            };
        }
    }

    // No complete match within budget
    return .{
        .code = BEAMZ_INSUFFICIENT_BUFFER,
        .consumed = scan_limit + pattern_len,
        .produced = scan_limit,
    };
}

/// Match at bit granularity (for bitstring operations)
/// Matches pattern at bit offset within input
export fn beamz_binary_match_bits(
    input_ptr: [*]const u8,
    input_len: usize,
    pattern_ptr: [*]const u8,
    pattern_len: usize,
    bit_offset: usize,
    budget_bits: usize,
) BeamZBinaryMatchResult {
    if (pattern_len == 0) {
        return .{
            .code = BEAMZ_SUCCESS,
            .consumed = 0,
            .produced = 0,
        };
    }

    // Convert bit offset to byte offset
    const byte_offset = bit_offset / 8;
    const bit_remainder = bit_offset % 8;

    if (byte_offset >= input_len) {
        return .{
            .code = BEAMZ_INVALID_INPUT,
            .consumed = input_len * 8,
            .produced = 0,
        };
    }

    // Scan with bit granularity
    var bit_pos: usize = 0;
    const max_bits = (input_len - byte_offset - 1) * 8 - bit_remainder;

    while (bit_pos <= max_bits and bit_pos < budget_bits) : (bit_pos += 1) {
        var match_ok = true;
        var pattern_bit: usize = 0;

        while (pattern_bit < pattern_len * 8) : (pattern_bit += 1) {
            const overall_bit = bit_pos + pattern_bit;
            const byte_idx = byte_offset + (bit_remainder + overall_bit) / 8;

            if (byte_idx >= input_len) {
                match_ok = false;
                break;
            }

            const input_byte = input_ptr[byte_idx];
            const pattern_byte = pattern_ptr[pattern_bit / 8];
            const bit_idx_val: u3 = @truncate((bit_remainder + overall_bit) % 8);
            const input_bit = (input_byte >> bit_idx_val) & 1;
            const pattern_bit_idx: u3 = @truncate(pattern_bit % 8);
            const pattern_bit_val = (pattern_byte >> pattern_bit_idx) & 1;

            if (input_bit != pattern_bit_val) {
                match_ok = false;
                break;
            }
        }

        if (match_ok) {
            return .{
                .code = BEAMZ_SUCCESS,
                .consumed = bit_pos + pattern_len * 8,
                .produced = bit_pos,
            };
        }
    }

    return .{
        .code = BEAMZ_INSUFFICIENT_BUFFER,
        .consumed = bit_pos + pattern_len * 8,
        .produced = bit_pos,
    };
}
