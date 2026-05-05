//! Zig heap kernel wrappers for RustZigBeam.
//!
//! Provides safe Rust wrappers around Zig heap scan/copy kernels.
//! Falls back to Rust reference implementations when Zig kernels are unavailable.
//!
//! Per design.md: GC policy in Rust, kernels in Zig.

use chimera_erlang_beam_core::{VmError, VmResult};

/// C-compatible result structure for heap operations
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamZHeapResult {
    /// Error code (0 = success)
    pub code: u32,
    /// Number of words successfully copied
    pub words_copied: usize,
    /// Number of words scanned in the heap
    pub words_scanned: usize,
}

// ============================================================================
// Zig kernel declarations (FFI)
// ============================================================================

// Heap scan kernel - scans heap words and identifies term boundaries
// Safety: base must point to valid heap memory of at least size words
unsafe extern "C" {
    fn beamz_heap_scan(base: *const u64, size: usize, hp: usize) -> BeamZHeapResult;
}

// Heap copy kernel - copies live objects from src to dst within bounded budget
// Safety: src and dst must be valid, non-overlapping regions
unsafe extern "C" {
    fn beamz_heap_copy(
        src: *const u64,
        dst: *mut u64,
        src_size: usize,
        budget_words: usize,
    ) -> BeamZHeapResult;
}

// Term copy kernel - copies a single term and its referenced objects
// Safety: term must be a valid tagged term
unsafe extern "C" {
    fn beamz_term_copy(term: u64, heap_base: *const u64, heap_size: usize) -> u64;
}

// Heap compact kernel - compacts live objects based on live_indices
// Safety: src, dst, and live_indices must point to valid memory regions
unsafe extern "C" {
    fn beamz_heap_compact(
        src_base: *const u64,
        dst_base: *mut u64,
        live_indices: *const usize,
        live_count: usize,
    ) -> BeamZHeapResult;
}

// ============================================================================
// Binary matching kernels (FFI)
// ============================================================================

/// Result structure for binary pattern matching
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamZBinaryMatchResult {
    /// Error code (0 = success, 2 = insufficient buffer)
    pub code: u32,
    /// Bytes consumed from input during matching
    pub consumed: usize,
    /// Position where match was found (or error position)
    pub produced: usize,
}

// Binary match kernel - byte-aligned pattern matching
// Safety: input and pattern must point to valid memory regions
unsafe extern "C" {
    fn beamz_binary_match(
        input_ptr: *const u8,
        input_len: usize,
        pattern_ptr: *const u8,
        pattern_len: usize,
        budget: usize,
    ) -> BeamZBinaryMatchResult;
}

// Binary match bits kernel - bit-aligned pattern matching
// Safety: input and pattern must point to valid memory regions
unsafe extern "C" {
    fn beamz_binary_match_bits(
        input_ptr: *const u8,
        input_len: usize,
        pattern_ptr: *const u8,
        pattern_len: usize,
        bit_offset: usize,
        budget_bits: usize,
    ) -> BeamZBinaryMatchResult;
}

// ============================================================================
// Safe wrappers for Zig kernels
// ============================================================================

/// Heap scan result with Rust-friendly types
#[derive(Debug, Clone)]
pub struct HeapScanOutput {
    /// Number of words scanned in the heap
    pub words_scanned: usize,
    /// Number of terms found during scan
    pub terms_found: usize,
    /// Number of objects (cons cells, tuples) found
    pub objects_found: usize,
}

impl HeapScanOutput {
    /// Convert from C FFI result to Rust-friendly output
    pub fn from_result(result: BeamZHeapResult) -> Self {
        HeapScanOutput {
            words_scanned: result.words_scanned,
            terms_found: result.words_copied, // Reused field
            objects_found: result.words_copied,
        }
    }
}

/// Scan a heap and identify term boundaries
///
/// Returns `Ok(HeapScanOutput)` on success, `Err(VmError)` on failure.
pub fn heap_scan(base: &[u64], hp: usize) -> VmResult<HeapScanOutput> {
    if base.is_empty() {
        return Err(VmError::InvalidTerm);
    }

    let result = unsafe { beamz_heap_scan(base.as_ptr(), base.len(), hp) };

    if result.code == 0 {
        Ok(HeapScanOutput::from_result(result))
    } else {
        Err(VmError::Generic("heap scan failed".to_string()))
    }
}

/// Copy heap data with a budget (for incremental GC)
///
/// Copies live objects from source to destination within the word budget.
/// Returns the number of words actually copied.
pub fn heap_copy(dst: &mut [u64], src: &[u64], budget_words: usize) -> VmResult<usize> {
    if dst.is_empty() || src.is_empty() {
        return Err(VmError::InvalidTerm);
    }

    let result =
        unsafe { beamz_heap_copy(src.as_ptr(), dst.as_mut_ptr(), src.len(), budget_words) };

    if result.code == 0 {
        Ok(result.words_copied)
    } else {
        Err(VmError::Generic("heap copy failed".to_string()))
    }
}

/// Copy a term and its referenced objects
pub fn term_copy(term: u64, heap_base: &[u64]) -> VmResult<u64> {
    if heap_base.is_empty() {
        return Err(VmError::InvalidTerm);
    }

    let result = unsafe { beamz_term_copy(term, heap_base.as_ptr(), heap_base.len()) };

    if result != 0 {
        Ok(result)
    } else {
        Err(VmError::Generic("term copy failed".to_string()))
    }
}

/// Compact heap by copying only live objects based on live_indices
///
/// Returns the number of words copied.
pub fn heap_compact(dst: &mut [u64], src: &[u64], live_indices: &[usize]) -> VmResult<usize> {
    if dst.is_empty() || src.is_empty() || live_indices.is_empty() {
        return Err(VmError::InvalidTerm);
    }

    let result = unsafe {
        beamz_heap_compact(
            src.as_ptr(),
            dst.as_mut_ptr(),
            live_indices.as_ptr(),
            live_indices.len(),
        )
    };

    if result.code == 0 {
        Ok(result.words_copied)
    } else {
        Err(VmError::Generic("heap compact failed".to_string()))
    }
}

/// Binary pattern matching result with Rust-friendly types
#[derive(Debug, Clone)]
pub struct BinaryMatchOutput {
    /// Error code (0 = success)
    pub code: u32,
    /// Bytes consumed from input
    pub consumed: usize,
    /// Position where match was found (or error position)
    pub produced: usize,
}

impl BinaryMatchOutput {
    /// Create a success result
    pub fn success(consumed: usize, produced: usize) -> Self {
        BinaryMatchOutput {
            code: 0,
            consumed,
            produced,
        }
    }

    /// Create an insufficient buffer result
    pub fn insufficient_buffer(consumed: usize, produced: usize) -> Self {
        BinaryMatchOutput {
            code: 2,
            consumed,
            produced,
        }
    }

    /// Check if the operation was successful
    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}

/// Match a binary pattern against input data (byte-aligned)
///
/// Returns `Ok(BinaryMatchOutput)` on success, `Err(VmError)` on failure.
/// On success, consumed = bytes matched, produced = match position.
/// On insufficient buffer, consumed = bytes scanned, produced = last position.
pub fn binary_match(input: &[u8], pattern: &[u8], budget: usize) -> VmResult<BinaryMatchOutput> {
    if pattern.is_empty() {
        return Ok(BinaryMatchOutput::success(0, 0));
    }

    let result = unsafe {
        beamz_binary_match(
            input.as_ptr(),
            input.len(),
            pattern.as_ptr(),
            pattern.len(),
            budget,
        )
    };

    Ok(BinaryMatchOutput {
        code: result.code,
        consumed: result.consumed,
        produced: result.produced,
    })
}

/// Match a binary pattern at bit granularity (bitstring operations)
///
/// Matches pattern at bit_offset within input, with budget_bits limit.
pub fn binary_match_bits(
    input: &[u8],
    pattern: &[u8],
    bit_offset: usize,
    budget_bits: usize,
) -> VmResult<BinaryMatchOutput> {
    if pattern.is_empty() {
        return Ok(BinaryMatchOutput::success(0, 0));
    }

    let result = unsafe {
        beamz_binary_match_bits(
            input.as_ptr(),
            input.len(),
            pattern.as_ptr(),
            pattern.len(),
            bit_offset,
            budget_bits,
        )
    };

    Ok(BinaryMatchOutput {
        code: result.code,
        consumed: result.consumed,
        produced: result.produced,
    })
}

// ============================================================================
// Rust reference implementations (fallback when Zig unavailable)
// ============================================================================

/// Rust reference implementation for heap scanning
pub mod rust_fallback {
    use super::HeapScanOutput;

    /// Scan heap using pure Rust (reference implementation)
    pub fn scan(base: &[u64], hp: usize) -> HeapScanOutput {
        let mut words_scanned = 0;
        let mut terms_found = 0;
        let mut objects_found = 0;
        let mut offset = 0;

        while offset < hp.min(base.len()) {
            words_scanned += 1;

            let word = base[offset];
            let tag = word & 0x7;

            match tag {
                0 | 1 => {
                    // Immediate values (small int, atom)
                    terms_found += 1;
                    offset += 1;
                }
                2 => {
                    // Cons cell: header + head + tail = 3 words
                    terms_found += 1;
                    objects_found += 1;
                    offset += 3;
                }
                3 => {
                    // Tuple - need to read header for size
                    terms_found += 1;
                    objects_found += 1;
                    if offset < base.len() {
                        let header = base[offset];
                        let size = ((header >> 8) & 0xFFFFFF) as usize;
                        offset += size.min(base.len() - offset);
                    } else {
                        offset += 1;
                    }
                }
                _ => {
                    // Other tags - advance by 1
                    offset += 1;
                }
            }
        }

        HeapScanOutput {
            words_scanned,
            terms_found,
            objects_found,
        }
    }

    /// Copy heap data using pure Rust
    pub fn copy(dst: &mut [u64], src: &[u64], budget: usize) -> usize {
        let to_copy = (budget.min(dst.len())).min(src.len());
        let copied = &src[..to_copy];
        dst[..to_copy].copy_from_slice(copied);
        to_copy
    }

    /// Compact heap by copying only live objects based on indices
    pub fn compact(dst: &mut [u64], src: &[u64], live_indices: &[usize]) -> usize {
        let mut dst_offset = 0;
        for &src_idx in live_indices.iter() {
            if src_idx < src.len() {
                dst[dst_offset] = src[src_idx];
                dst_offset += 1;
            }
        }
        dst_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_result_size() {
        // Verify C-compatible struct size
        assert!(std::mem::size_of::<BeamZHeapResult>() >= 16);
    }

    #[test]
    fn test_rust_fallback_scan_empty() {
        let base = [0u64; 0];
        let result = rust_fallback::scan(&base, 0);
        assert_eq!(result.words_scanned, 0);
    }

    #[test]
    fn test_rust_fallback_scan_immediates() {
        // Small integers (tag 0) and atoms (tag 1)
        let base = [0u64, 1u64, 2u64]; // tags: 0, 1, 2
        let result = rust_fallback::scan(&base, 3);
        assert_eq!(result.words_scanned, 3);
    }

    #[test]
    fn test_rust_fallback_copy() {
        let mut dst = [0u64; 10];
        let src = [1u64, 2u64, 3u64, 4u64, 5u64];

        let copied = rust_fallback::copy(&mut dst, &src, 5);
        assert_eq!(copied, 5);
        assert_eq!(dst[0], 1);
        assert_eq!(dst[4], 5);
    }

    #[test]
    fn test_rust_fallback_copy_with_budget() {
        let mut dst = [0u64; 10];
        let src = [1u64, 2u64, 3u64, 4u64, 5u64];

        // Copy only 3 words
        let copied = rust_fallback::copy(&mut dst, &src, 3);
        assert_eq!(copied, 3);
        assert_eq!(dst[0], 1);
        assert_eq!(dst[2], 3);
        assert_eq!(dst[3], 0); // Should not be copied
    }

    #[test]
    fn test_rust_fallback_compact() {
        let mut dst = [0u64; 10];
        let src = [1u64, 2u64, 3u64, 4u64, 5u64];
        let live_indices = [0usize, 2, 4]; // indices 0, 2, 4 are live

        let copied = rust_fallback::compact(&mut dst, &src, &live_indices);
        assert_eq!(copied, 3);
        assert_eq!(dst[0], 1);
        assert_eq!(dst[1], 3);
        assert_eq!(dst[2], 5);
    }

    #[test]
    fn test_binary_match_result_empty_pattern() {
        let input = [1u8, 2, 3, 4, 5];
        let pattern = [];
        let result = binary_match(&input, &pattern, 100);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_success());
        assert_eq!(output.consumed, 0);
        assert_eq!(output.produced, 0);
    }

    #[test]
    fn test_binary_match_result_code() {
        // Verify C-compatible struct size
        assert!(std::mem::size_of::<BeamZBinaryMatchResult>() >= 16);
    }

    #[test]
    fn test_binary_match_output_success() {
        let output = BinaryMatchOutput::success(5, 0);
        assert!(output.is_success());
        assert_eq!(output.code, 0);
    }

    #[test]
    fn test_binary_match_output_insufficient_buffer() {
        let output = BinaryMatchOutput::insufficient_buffer(10, 10);
        assert!(!output.is_success());
        assert_eq!(output.code, 2);
    }
}
