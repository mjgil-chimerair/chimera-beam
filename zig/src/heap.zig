//! Zig kernels for RustZigBeam heap operations.
//!
//! These kernels handle bounded heap scanning and copying operations.
//! All heap policy decisions are made in Rust.

const std = @import("std");
const root = @import("root.zig");

pub const GcConfig = struct {
    heap_size_initial: usize = 8192,
    heap_size_min: usize = 1024,
    heap_size_max: usize = 16 * 1024 * 1024,
    heap_growth_rate: f64 = 2.0,
    heap_shrink_threshold: f64 = 0.3,
    generational_threshold: u64 = 3,
};

pub const GcEvent = enum {
    minor,
    major,
    promotion,
};

pub const GcStats = struct {
    collections: u64 = 0,
    minor_collections: u64 = 0,
    major_collections: u64 = 0,
    bytes_allocated: u64 = 0,
    bytes_freed: u64 = 0,
    words_collected: u64 = 0,
};

pub const FORWARD_MAGIC: u64 = 0xDEADBEEF00000000;

/// Heap view passed from Rust for Zig to operate on (word-based)
/// This matches the Rust ProcessHeap::HeapView structure
pub const HeapView = struct {
    base: [*]const u64,
    hp: [*]u64,
    end: [*]const u64,
};

/// Cons cell structure (2 words: head and tail)
pub const ConsCell = struct {
    head: root.Term,
    tail: root.Term,
};

/// Tuple header structure (1 word)
pub const TupleHeader = struct {
    tag: u3,
    sub_tag: u5,
    size: u24,
};

/// Get the remaining words in a heap view
pub fn heapRemaining(view: HeapView) usize {
    return (@intFromPtr(view.end) - @intFromPtr(view.hp)) / 8;
}

/// Check if there's enough space for n words
pub fn heapHasSpace(view: HeapView, words: usize) bool {
    return heapRemaining(view) >= words;
}

/// Copy a cons cell - bounded Zig kernel
/// Returns new heap pointer value, or null if out of space
pub fn copyConsCell(view: *HeapView, src_ptr: u64) ?u64 {
    const cell_size: usize = 2; // head + tail (two words)
    const new_hp: [*]u64 = @ptrFromInt(@intFromPtr(view.hp) + cell_size);
    if (@intFromPtr(new_hp) > @intFromPtr(view.end)) {
        return null;
    }

    const src: *const ConsCell = @ptrFromInt(src_ptr);
    const dst: *ConsCell = @ptrFromInt(@intFromPtr(view.hp));
    dst.head = src.head;
    dst.tail = src.tail;

    view.hp = new_hp;
    return @intFromPtr(dst);
}

/// Copy a tuple - bounded Zig kernel
/// Returns new heap pointer value, or null if out of space
pub fn copyTuple(view: *HeapView, src_ptr: u64, arity: u32) ?u64 {
    const tuple_size: usize = 1 + arity; // header + elements
    const new_hp: [*]u64 = @ptrFromInt(@intFromPtr(view.hp) + tuple_size);
    if (@intFromPtr(new_hp) > @intFromPtr(view.end)) {
        return null;
    }

    // Copy tuple header and elements
    const src: [*]const u64 = @ptrFromInt(src_ptr);
    const dst: [*]u64 = @ptrFromInt(@intFromPtr(view.hp));
    for (0..tuple_size) |i| {
        dst[i] = src[i];
    }

    view.hp = new_hp;
    return @intFromPtr(dst);
}

/// Scan a term and copy live data - bounded Zig kernel
/// Returns the new term with updated pointers
pub fn scanAndCopy(view: *HeapView, t: root.Term) root.Term {
    const tag = root.getTag(t);
    switch (tag) {
        .small_integer, .atom => {
            return t; // Immediate, no copy needed
        },
        .cons => {
            const ptr = root.decodeCons(t);
            if (ptr == 0) return t;
            if (copyConsCell(view, ptr)) |new_ptr| {
                return root.encodeCons(new_ptr);
            }
            return t;
        },
        .tuple => {
            const ptr = root.decodeTuple(t);
            if (ptr == 0) return t;
            // For FFI, the tuple header is at the src_ptr location
            // We need to read it to get the arity
            const header_ptr: [*]const u64 = @ptrFromInt(ptr);
            const header = header_ptr[0];
            const arity = @as(u32, @intCast((header >> 8) & 0xFFFFFF));
            if (copyTuple(view, ptr, arity)) |new_ptr| {
                return root.encodeTuple(new_ptr);
            }
            return t;
        },
        else => {
            return t; // Other types pass through
        },
    }
}

test "heap remaining calculation" {
    var words: [64]u64 = undefined;
    // hp at word offset 2, end at word offset 8
    const hp_offset: usize = 2 * 8; // 2 words * 8 bytes
    const end_offset: usize = 8 * 8; // 8 words * 8 bytes
    const view = HeapView{
        .base = &words,
        .hp = @ptrFromInt(@intFromPtr(&words) + hp_offset),
        .end = @ptrFromInt(@intFromPtr(&words) + end_offset),
    };
    // Remaining = (end - hp) / 8 = (64 - 16) / 8 = 48/8 = 6 words
    try std.testing.expect(heapRemaining(view) == 6);
}

test "heap has space check" {
    var words: [64]u64 = undefined;
    // 16 words total
    const end_offset: usize = 16 * 8;
    const view = HeapView{
        .base = &words,
        .hp = &words,
        .end = @ptrFromInt(@intFromPtr(&words) + end_offset),
    };
    // 10 words available, need 8 words
    try std.testing.expect(heapHasSpace(view, 8));
    // Need 20 words, only 16 available
    try std.testing.expect(!heapHasSpace(view, 20));
}

test "gc config defaults" {
    const config = GcConfig{};
    try std.testing.expect(config.heap_size_initial == 8192);
    try std.testing.expect(config.heap_size_min == 1024);
    try std.testing.expect(config.heap_size_max == 16 * 1024 * 1024);
    try std.testing.expect(config.heap_growth_rate == 2.0);
    try std.testing.expect(config.heap_shrink_threshold == 0.3);
    try std.testing.expect(config.generational_threshold == 3);
}

test "tuple header encoding" {
    const header = TupleHeader{
        .tag = @intFromEnum(root.Tag.tuple),
        .sub_tag = 1,
        .size = 10,
    };
    // Verify bit packing works
    try std.testing.expect(header.tag == @intFromEnum(root.Tag.tuple));
    try std.testing.expect(header.sub_tag == 1);
    try std.testing.expect(header.size == 10);
}

test "scan and copy small integer" {
    // Small integers are immediate, no copy needed
    var words: [64]u64 = undefined;
    var view = HeapView{
        .base = &words,
        .hp = &words,
        .end = @ptrFromInt(@intFromPtr(&words) + 64),
    };
    const term = root.encodeSmall(42);
    const result = scanAndCopy(&view, term);
    try std.testing.expect(result == term);
    try std.testing.expect(@intFromPtr(view.hp) == @intFromPtr(&words)); // hp unchanged
}
/// Result structure for heap operations (C ABI compatible)
pub const BeamZHeapResult = extern struct {
    code: u32,
    words_copied: usize,
    words_scanned: usize,
};

/// Export heap scan kernel for FFI
/// Scans heap words and identifies term boundaries
/// base: pointer to start of heap
/// size: number of words in the heap
/// hp: current heap pointer position (in words from base)
export fn beamz_heap_scan(base: [*]const u64, size: usize, hp: usize) BeamZHeapResult {
    var words_scanned: usize = 0;
    var objects_found: usize = 0;
    var offset: usize = 0;

    // Scan from base to base + hp (used portion of heap)
    while (offset < hp and offset < size) {
        words_scanned += 1;

        const word = base[offset];
        const tag = word & root.MASK;

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
        .code = root.BEAMZ_SUCCESS,
        .words_copied = objects_found,
        .words_scanned = words_scanned,
    };
}

/// Export heap copy kernel for FFI
/// Copies live objects from src to dst with bounded budget
/// Returns number of words actually copied
export fn beamz_heap_copy(src: [*]const u64, dst: [*]u64, src_size: usize, budget_words: usize) BeamZHeapResult {
    var words_copied: usize = 0;
    var offset: usize = 0;

    while (offset < src_size and words_copied < budget_words) {
        const word = src[offset];
        const tag = word & root.MASK;

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
        .code = root.BEAMZ_SUCCESS,
        .words_copied = words_copied,
        .words_scanned = offset,
    };
}
