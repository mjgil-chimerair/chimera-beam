//! ETF (Erlang Term Format) kernels for RustZigBeam.
//!
//! Handles scanning, encoding, and decoding of ETF format.

const std = @import("std");

/// Helper function to read big-endian u16 (compatible with all Zig versions)
fn readBigU16(input: []const u8) u16 {
    return @as(u16, input[0]) << 8 | @as(u16, input[1]);
}

/// Helper function to read big-endian u32 (compatible with all Zig versions)
fn readBigU32(input: []const u8) u32 {
    return (@as(u32, input[0]) << 24) |
           (@as(u32, input[1]) << 16) |
           (@as(u32, input[2]) << 8) |
           (@as(u32, input[3]));
}

/// ETF constants
pub const ETF_VERSION_HEADER: u8 = 131;
pub const ETF_SMALL_INTEGER_EXT: u8 = 97;
pub const ETF_INTEGER_EXT: u8 = 98;
pub const ETF_FLOAT_EXT: u8 = 99;
pub const ETF_ATOM_EXT: u8 = 100;
pub const ETF_SMALL_ATOM_EXT: u8 = 118;
pub const ETF_REFERENCE_EXT: u8 = 101;
pub const ETF_NEW_REFERENCE_EXT: u8 = 114;
pub const ETF_PORT_EXT: u8 = 102;
pub const ETF_NEW_PORT_EXT: u8 = 89;
pub const ETF_PID_EXT: u8 = 103;
pub const ETF_SMALL_TUPLE_EXT: u8 = 104;
pub const ETF_LARGE_TUPLE_EXT: u8 = 105;
pub const ETF_NIL_EXT: u8 = 106;
pub const ETF_STRING_EXT: u8 = 107;
pub const ETF_LIST_EXT: u8 = 108;
pub const ETF_BINARY_EXT: u8 = 109;
pub const ETF_BIT_BINARY_EXT: u8 = 77;
pub const ETF_FUN_EXT: u8 = 117;
pub const ETF_NEW_FUN_EXT: u8 = 112;
pub const ETF_EXPORT_EXT: u8 = 113;
pub const ETF_BIT_STRING_EXT: u8 = 77;

/// ETF scan result
pub const ScanResult = extern struct {
    code: u32,
    consumed: usize,
    version: u8,
};

/// ETF term size result
pub const TermSizeResult = extern struct {
    code: u32,
    consumed: usize,
    produced: usize,
};

/// Scan for ETF version header
pub fn scan(input_ptr: [*]const u8, input_len: usize) ScanResult {
    const input = input_ptr[0..input_len];

    for (input, 0..) |byte, i| {
        if (byte == ETF_VERSION_HEADER) {
            return .{
                .code = 0,
                .consumed = i + 1,
                .version = ETF_VERSION_HEADER,
            };
        }
    }

    return .{
        .code = 1,
        .consumed = 0,
        .version = 0,
    };
}

/// Calculate the encoded size of a term
pub fn termSize(input_ptr: [*]const u8, input_len: usize) TermSizeResult {
    const input = input_ptr[0..input_len];

    if (input.len < 1) {
        return .{ .code = 1, .consumed = 0, .produced = 0 };
    }

    const tag = input[0];

    switch (tag) {
        ETF_VERSION_HEADER => {
            if (input.len < 2) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const term_size = calculateTermSize(input[1..input.len]);
            return .{ .code = 0, .consumed = 1, .produced = 1 + term_size };
        },
        ETF_SMALL_INTEGER_EXT => {
            return .{ .code = 0, .consumed = 1, .produced = 2 };
        },
        ETF_INTEGER_EXT => {
            return .{ .code = 0, .consumed = 1, .produced = 5 };
        },
        ETF_ATOM_EXT => {
            if (input.len < 3) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const atom_len = readBigU16(input[1..3]);
            return .{ .code = 0, .consumed = 1, .produced = 3 + atom_len };
        },
        ETF_NIL_EXT => {
            return .{ .code = 0, .consumed = 1, .produced = 1 };
        },
        ETF_STRING_EXT => {
            if (input.len < 3) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const str_len = readBigU16(input[1..3]);
            return .{ .code = 0, .consumed = 1, .produced = 3 + str_len };
        },
        ETF_LIST_EXT => {
            if (input.len < 5) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const count = readBigU32(input[1..5]);
            var size: usize = 5;
            var remaining = input[5..input.len];
            for (0..count) |_| {
                if (remaining.len < 1) {
                    return .{ .code = 1, .consumed = 0, .produced = 0 };
                }
                const term_size = calculateTermSize(remaining);
                if (term_size == 0) {
                    return .{ .code = 1, .consumed = 0, .produced = 0 };
                }
                size += term_size;
                remaining = remaining[term_size..remaining.len];
            }
            return .{ .code = 0, .consumed = 1, .produced = size + 1 };
        },
        ETF_SMALL_TUPLE_EXT => {
            if (input.len < 2) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const arity = input[1];
            var size: usize = 2;
            var remaining = input[2..input.len];
            for (0..arity) |_| {
                if (remaining.len < 1) {
                    return .{ .code = 1, .consumed = 0, .produced = 0 };
                }
                const term_size = calculateTermSize(remaining);
                if (term_size == 0) {
                    return .{ .code = 1, .consumed = 0, .produced = 0 };
                }
                size += term_size;
                remaining = remaining[term_size..remaining.len];
            }
            return .{ .code = 0, .consumed = 1, .produced = size };
        },
        ETF_BINARY_EXT => {
            if (input.len < 5) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const bin_len = readBigU32(input[1..5]);
            return .{ .code = 0, .consumed = 1, .produced = 5 + bin_len };
        },
        ETF_LARGE_TUPLE_EXT => {
            if (input.len < 5) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const arity = readBigU32(input[1..5]);
            var size: usize = 5;
            var remaining = input[5..input.len];
            for (0..arity) |_| {
                if (remaining.len < 1) {
                    return .{ .code = 1, .consumed = 0, .produced = 0 };
                }
                const term_size = calculateTermSize(remaining);
                if (term_size == 0) {
                    return .{ .code = 1, .consumed = 0, .produced = 0 };
                }
                size += term_size;
                remaining = remaining[term_size..remaining.len];
            }
            return .{ .code = 0, .consumed = 1, .produced = size };
        },
        ETF_SMALL_ATOM_EXT => {
            if (input.len < 2) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            const atom_len = input[1];
            return .{ .code = 0, .consumed = 1, .produced = 2 + atom_len };
        },
        ETF_FLOAT_EXT => {
            if (input.len < 32) {
                return .{ .code = 1, .consumed = 0, .produced = 0 };
            }
            return .{ .code = 0, .consumed = 1, .produced = 32 };
        },
        else => {
            return .{ .code = 2, .consumed = 1, .produced = 0 };
        },
    }
}

/// Internal helper to calculate term size
fn calculateTermSize(data: []const u8) usize {
    if (data.len < 1) {
        return 0;
    }

    const tag = data[0];

    switch (tag) {
        ETF_SMALL_INTEGER_EXT => return if (data.len >= 2) 2 else 0,
        ETF_INTEGER_EXT => return if (data.len >= 5) 5 else 0,
        ETF_ATOM_EXT => {
            if (data.len < 3) return 0;
            const atom_len = readBigU16(data[1..3]);
            return if (data.len >= 3 + atom_len) 3 + atom_len else 0;
        },
        ETF_NIL_EXT => return 1,
        ETF_STRING_EXT => {
            if (data.len < 3) return 0;
            const str_len = readBigU16(data[1..3]);
            return if (data.len >= 3 + str_len) 3 + str_len else 0;
        },
        ETF_SMALL_TUPLE_EXT => {
            if (data.len < 2) return 0;
            const arity = data[1];
            var size: usize = 2;
            var remaining = data[2..data.len];
            for (0..arity) |_| {
                const term_size = calculateTermSize(remaining);
                if (term_size == 0) return 0;
                size += term_size;
                remaining = remaining[term_size..remaining.len];
            }
            return size;
        },
        ETF_LIST_EXT => {
            if (data.len < 5) return 0;
            const count = readBigU32(data[1..5]);
            var size: usize = 5;
            var remaining = data[5..data.len];
            for (0..count) |_| {
                const term_size = calculateTermSize(remaining);
                if (term_size == 0) return 0;
                size += term_size;
                remaining = remaining[term_size..remaining.len];
            }
            return size + 1;
        },
        ETF_BINARY_EXT => {
            if (data.len < 5) return 0;
            const bin_len = readBigU32(data[1..5]);
            return if (data.len >= 5 + bin_len) 5 + bin_len else 0;
        },
        ETF_SMALL_ATOM_EXT => {
            if (data.len < 2) return 0;
            const atom_len = data[1];
            return if (data.len >= 2 + atom_len) 2 + atom_len else 0;
        },
        ETF_LARGE_TUPLE_EXT => {
            if (data.len < 5) return 0;
            const arity = readBigU32(data[1..5]);
            var size: usize = 5;
            var remaining = data[5..data.len];
            for (0..arity) |_| {
                const term_size = calculateTermSize(remaining);
                if (term_size == 0) return 0;
                size += term_size;
                remaining = remaining[term_size..remaining.len];
            }
            return size;
        },
        ETF_FLOAT_EXT => {
            return if (data.len >= 32) 32 else 0;
        },
        else => return 1,
    }
}

test "etf scan finds version header" {
    const data = [_]u8{ 131, 97, 42 };
    const result = scan(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.version == 131);
    try std.testing.expect(result.consumed == 1);
}

test "etf scan no header" {
    const data = [_]u8{ 1, 2, 3, 4 };
    const result = scan(&data, data.len);
    try std.testing.expect(result.code == 1);
}

test "etf term size small integer" {
    const data = [_]u8{ 97, 42 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 2);
}

test "etf term size atom" {
    // ETF_ATOM_EXT (100) + length (big-endian u16) + atom name
    const data = [_]u8{ 100, 0, 3, 101, 97, 116 }; // "eat"
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 6);
}

test "etf term size nil" {
    // ETF_NIL_EXT (106)
    const data = [_]u8{ 106 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 1);
}

test "etf term size string" {
    // ETF_STRING_EXT (107) + length (big-endian u16) + string
    const data = [_]u8{ 107, 0, 4, 104, 101, 108, 108 }; // "hell"
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 7);
}

test "etf term size small tuple" {
    // ETF_SMALL_TUPLE_EXT (104) + arity + elements
    // Tuple with 2 elements: small_int 42 and atom "test" (4 chars)
    // 1 + 1 + 2 + 7 = 11 bytes
    const data = [_]u8{ 104, 2, 97, 42, 100, 0, 4, 116, 101, 115, 116 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 11);
}

test "etf term size list" {
    // ETF_LIST_EXT (108) + count (u32) + elements + NIL_EXT
    // List of 2 small integers: [42, 99]
    // 1 + 4 + 2 + 2 + 1 = 10 bytes
    const data = [_]u8{ 108, 0, 0, 0, 2, 97, 42, 97, 99, 106 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 10);
}

test "etf term size binary" {
    // ETF_BINARY_EXT (109) + length (u32) + data
    const data = [_]u8{ 109, 0, 0, 0, 5, 1, 2, 3, 4, 5 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 10);
}

test "etf term size float" {
    // ETF_FLOAT_EXT (99) + 31 bytes float string
    var data: [32]u8 = undefined;
    data[0] = 99; // FLOAT_EXT tag
    @memset(data[1..31], 0); // Fill with zeros (float string representation)
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 32);
}

test "etf term size large tuple" {
    // ETF_LARGE_TUPLE_EXT (105) + arity (u32) + elements
    // Large tuple with 1 element (small_int 1)
    const data = [_]u8{ 105, 0, 0, 0, 1, 97, 1 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 7);
}

test "etf term size truncated" {
    // Truncated atom (needs 3 bytes min for tag + length)
    const data = [_]u8{ 100, 0 }; // ATOM_EXT but no length byte
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code != 0);
}

test "etf term size empty list" {
    // Empty list: LIST_EXT + zero count + NIL_EXT
    const data = [_]u8{ 108, 0, 0, 0, 0, 106 };
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 6); // 5 header + 1 nil tail
}

test "etf term size small atom" {
    // ETF_SMALL_ATOM_EXT (118) + length + atom name
    const data = [_]u8{ 118, 5, 116, 114, 117, 101, 50 }; // "true2"
    const result = termSize(&data, data.len);
    try std.testing.expect(result.code == 0);
    try std.testing.expect(result.produced == 7);
}