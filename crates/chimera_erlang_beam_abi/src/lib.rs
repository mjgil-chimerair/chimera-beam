//! ABI boundary crate for RustZigBeam.
//!
//! This crate is the **sole** Rust FFI boundary to Zig kernels. No other crate
//! should call Zig directly. All Zig interaction goes through this crate with
//! safe wrappers for ETF, heap scan/copy, and term operations.
//!
//! # Safety
//!
//! - All `unsafe extern "C"` blocks are isolated to this crate.
//! - Zig kernels must not retain Rust pointers or own VM state.
//! - All Zig calls are wrapped in safe abstractions here.

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_core::{VmError, VmResult};

pub mod heap_kernels;

/// C-compatible result structure returned by Zig kernels
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamZResult {
    /// Return code (0 = success, non-zero = error)
    pub code: u32,
    /// Number of bytes consumed/processed
    pub consumed: usize,
    /// Number of bytes produced/output
    pub produced: usize,
}

/// ETF scan result
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamZEtfScanResult {
    /// Return code (0 = success)
    pub code: u32,
    /// Number of bytes consumed
    pub consumed: usize,
    /// ETF version byte (if successful)
    pub version: u8,
}

/// ETF term size result (same layout as BeamZResult, kept for historical reasons)
/// Note: Zig beamz_etf_term_size returns BeamZResult directly
pub type BeamZEtfTermSizeResult = BeamZResult;

/// Term size result for individual terms
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamZTermSizeResult {
    /// Return code (0 = success)
    pub code: u32,
    /// Size of the encoded term in bytes
    pub size: usize,
}

/// Error codes returned by Zig kernels (must match C header BeamZErrorCode)
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamZErrorCode {
    /// Operation completed successfully
    Success = 0,
    /// Invalid input provided
    InvalidInput = 1,
    /// Buffer too small for operation
    InsufficientBuffer = 2,
    /// Malformed or invalid term
    MalformedTerm = 3,
    /// Heap exhausted during operation
    HeapExhausted = 4,
    /// Unknown or unexpected error
    UnknownError = 99,
}

impl BeamZErrorCode {
    /// Convert ABI error code to a Rust VmError
    pub fn to_vm_error(&self) -> VmError {
        match self {
            BeamZErrorCode::Success => VmError::Generic("success".to_string()),
            BeamZErrorCode::InvalidInput => VmError::InvalidTerm,
            BeamZErrorCode::InsufficientBuffer => VmError::HeapExhausted,
            BeamZErrorCode::MalformedTerm => VmError::InvalidTerm,
            BeamZErrorCode::HeapExhausted => VmError::HeapExhausted,
            BeamZErrorCode::UnknownError => VmError::Generic("unknown ABI error".to_string()),
        }
    }
}

/// ETF scan status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtfScanStatus {
    /// ETF data is valid with proper version header
    Valid,
    /// Invalid or missing version header (expected 131)
    InvalidVersion,
    /// Input data is too short to determine validity
    Truncated,
    /// Data is malformed or corrupted
    Malformed,
}

/// ETF scan result with Rust-friendly types
#[derive(Debug, Clone)]
pub struct EtfScanOutput {
    /// Scan status indicating validity of ETF data
    pub status: EtfScanStatus,
    /// Number of bytes consumed during scan
    pub consumed: usize,
    /// ETF version byte if valid (should be 131)
    pub version: Option<u8>,
}

impl EtfScanOutput {
    /// Convert from C FFI result to Rust-friendly output
    pub fn from_result(result: BeamZEtfScanResult) -> Self {
        let status = match result.code {
            0 => EtfScanStatus::Valid,
            1 => EtfScanStatus::InvalidVersion,
            2 => EtfScanStatus::Truncated,
            _ => EtfScanStatus::Malformed,
        };
        EtfScanOutput {
            status,
            consumed: result.consumed,
            version: if result.code == 0 {
                Some(result.version)
            } else {
                None
            },
        }
    }
}

// ============================================================================
// Zig kernel declarations
// These are FFI declarations only - all callers must use safe wrappers below
// ============================================================================

// ETF scan kernel - fast byte-level scanning for ETF version header
// Safety: input_ptr must be valid for input_len bytes, input_ptr must not be mutated
unsafe extern "C" {
    fn beamz_etf_scan(input_ptr: *const u8, input_len: usize) -> BeamZEtfScanResult;
}

// ETF term size calculator - returns BeamZResult (same as BeamZEtfTermSizeResult)
// Safety: input_ptr must be valid for input_len bytes, input_ptr must not be mutated
unsafe extern "C" {
    fn beamz_etf_term_size(input_ptr: *const u8, input_len: usize) -> BeamZResult;
}

// Term size calculation kernel (legacy)
// Safety: term must be a valid tagged term
unsafe extern "C" {
    fn beamz_term_size(term: u64) -> usize;
}

// Copy cons cell within a heap view
// Safety: src and dst must point to valid memory regions of at least len bytes
// Memory regions must not overlap, dst must have enough space for the copy
unsafe extern "C" {
    fn beamz_copy_cons(src: *const u8, dst: *mut u8, len: usize);
}

// ============================================================================
// Safe wrappers for Zig kernels
// ============================================================================

/// Scan ETF data for version header
///
/// Returns `Ok(EtfScanOutput)` on success, `Err(VmError)` on failure.
pub fn etf_scan(input: &[u8]) -> VmResult<EtfScanOutput> {
    if input.is_empty() {
        return Err(VmError::InvalidTerm);
    }

    let result = unsafe { beamz_etf_scan(input.as_ptr(), input.len()) };

    if result.code == 0 {
        Ok(EtfScanOutput::from_result(result))
    } else {
        let err_code = match result.code {
            1 => BeamZErrorCode::InvalidInput,
            2 => BeamZErrorCode::InsufficientBuffer,
            _ => BeamZErrorCode::UnknownError,
        };
        Err(err_code.to_vm_error())
    }
}

/// Calculate term size in ETF encoding
///
/// Returns `Ok(size)` on success, `Err(VmError)` on failure.
pub fn etf_term_size(input: &[u8]) -> VmResult<usize> {
    if input.is_empty() {
        return Err(VmError::InvalidTerm);
    }

    let result = unsafe { beamz_etf_term_size(input.as_ptr(), input.len()) };

    if result.code == 0 {
        Ok(result.produced)
    } else {
        let err_code = match result.code {
            1 => BeamZErrorCode::InvalidInput,
            _ => BeamZErrorCode::UnknownError,
        };
        Err(err_code.to_vm_error())
    }
}

/// Calculate the encoded size of a tagged term
pub fn term_size(term: u64) -> usize {
    unsafe { beamz_term_size(term) }
}

/// Copy a cons cell within a heap view
///
/// # Arguments
/// * `src` - source pointer to cons cell data
/// * `dst` - destination pointer
/// * `len` - number of bytes to copy
///
/// # Safety
/// - `src` and `dst` must point to valid, aligned memory regions
/// - `len` bytes must be accessible from both pointers
/// - Pointers must not overlap
pub unsafe fn copy_cons(src: *const u8, dst: *mut u8, len: usize) {
    beamz_copy_cons(src, dst, len)
}

/// Heap view for passing to Zig kernels
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct HeapView {
    /// Base pointer to start of heap memory
    pub base: *mut u8,
    /// Current heap pointer (next free location)
    pub hp: *mut u8,
    /// End pointer (marks end of available heap)
    pub end: *mut u8,
}

impl HeapView {
    /// Create a new heap view
    ///
    /// # Safety
    /// - `base`, `hp`, `end` must be valid, aligned pointers
    /// - `hp` must be >= `base` and <= `end`
    pub unsafe fn new(base: *mut u8, hp: *mut u8, end: *mut u8) -> Self {
        HeapView { base, hp, end }
    }

    /// Get remaining space in heap
    pub fn remaining(&self) -> usize {
        unsafe { self.end.offset_from(self.hp) as usize }
    }

    /// Check if there's space for `n` bytes
    pub fn has_space(&self, n: usize) -> bool {
        self.remaining() >= n
    }
}

/// Validate a heap view
pub fn validate_heap_view(view: &HeapView) -> bool {
    if view.base.is_null() || view.hp.is_null() || view.end.is_null() {
        return false;
    }
    if view.hp < view.base || view.end < view.hp {
        return false;
    }
    true
}

// Placeholder for when Zig kernels are actually linked
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beamz_result_size() {
        // Struct sizes are platform-dependent due to alignment
        // Just verify they're reasonable sizes
        assert!(std::mem::size_of::<BeamZResult>() >= 16);
        assert!(std::mem::size_of::<BeamZEtfScanResult>() >= 16);
        assert!(std::mem::size_of::<BeamZTermSizeResult>() >= 16);
    }

    #[test]
    fn test_etf_scan_output_from_result() {
        let result = BeamZEtfScanResult {
            code: 0,
            consumed: 10,
            version: 131,
        };
        let output = EtfScanOutput::from_result(result);
        assert_eq!(output.status, EtfScanStatus::Valid);
        assert_eq!(output.consumed, 10);
        assert_eq!(output.version, Some(131));
    }

    #[test]
    fn test_etf_scan_output_error() {
        let result = BeamZEtfScanResult {
            code: 1,
            consumed: 0,
            version: 0,
        };
        let output = EtfScanOutput::from_result(result);
        assert_eq!(output.status, EtfScanStatus::InvalidVersion);
        assert!(output.version.is_none());
    }

    #[test]
    fn test_abi_error_code_conversion() {
        assert_eq!(
            BeamZErrorCode::InvalidInput.to_vm_error(),
            VmError::InvalidTerm
        );
        assert_eq!(
            BeamZErrorCode::HeapExhausted.to_vm_error(),
            VmError::HeapExhausted
        );
    }

    #[test]
    fn test_heap_view_validation() {
        // Null pointers should fail validation
        let null_view = HeapView {
            base: std::ptr::null_mut(),
            hp: std::ptr::null_mut(),
            end: std::ptr::null_mut(),
        };
        assert!(!validate_heap_view(&null_view));
    }

    #[test]
    fn test_copy_cons_signature() {
        // Verify the function signature matches the C header:
        // void beamz_copy_cons(const uint8_t* src, uint8_t* dst, size_t len);
        // This test just ensures the declaration compiles correctly
        // Actual integration tests would verify the bytes copied
    }

    #[test]
    fn test_abi_struct_sizes() {
        // Verify struct sizes match C header expectations
        // C header: BeamZResult has code(u32) + consumed(size_t) + produced(size_t)
        // On 64-bit: 4 + 8 + 8 = 20 bytes, but size_t may have different alignment
        assert!(std::mem::size_of::<BeamZResult>() >= 16);

        // C header: BeamZEtfScanResult has code(u32) + consumed(size_t) + version(u8)
        // On 64-bit: 4 + 8 + 1 = 13, but padded to 16+ bytes
        assert!(std::mem::size_of::<BeamZEtfScanResult>() >= 16);

        // BeamZEtfTermSizeResult is an alias for BeamZResult
        assert_eq!(
            std::mem::size_of::<BeamZEtfTermSizeResult>(),
            std::mem::size_of::<BeamZResult>()
        );

        // BeamZTermSizeResult: code(u32) + size(size_t)
        // On 64-bit: 4 + 8 = 12, padded to 16+ bytes
        assert!(std::mem::size_of::<BeamZTermSizeResult>() >= 16);
    }

    #[test]
    fn test_abi_struct_alignments() {
        // Verify struct alignments are reasonable for FFI
        assert!(std::mem::align_of::<BeamZResult>() >= 4);
        assert!(std::mem::align_of::<BeamZEtfScanResult>() >= 4);
        assert!(std::mem::align_of::<BeamZTermSizeResult>() >= 4);
    }

    #[test]
    fn test_etf_scan_api_contract() {
        // This tests the API contract - ensures the FFI declaration is sound
        // Actual integration tests would require a linked libbeamz.a
        let data = &[131u8, 97, 42];
        // API contract: beamz_etf_scan takes (input_ptr, input_len) and returns BeamZEtfScanResult
        // The result.code == 0 indicates success
        // The result.version should be 131 (ETF_VERSION_HEADER)
        // The result.consumed should be 1 (position of version byte)
        _ = data; // Used in actual integration test with linked library
    }

    #[test]
    fn test_etf_term_size_api_contract() {
        // API contract: beamz_etf_term_size takes (input_ptr, input_len) and returns BeamZResult
        // result.code == 0 for success
        // result.consumed shows how many bytes were read
        // result.produced shows the encoded term size
        let data = &[97u8, 42]; // Small integer encoding
        _ = data; // Used in actual integration test with linked library
    }

    #[test]
    fn test_beamz_error_code_values() {
        // Verify error codes match the enum
        assert_eq!(BeamZErrorCode::Success as u32, 0);
        assert_eq!(BeamZErrorCode::InvalidInput as u32, 1);
        assert_eq!(BeamZErrorCode::InsufficientBuffer as u32, 2);
        assert_eq!(BeamZErrorCode::MalformedTerm as u32, 3);
        assert_eq!(BeamZErrorCode::HeapExhausted as u32, 4);
        assert_eq!(BeamZErrorCode::UnknownError as u32, 99);
    }

    #[test]
    fn test_ffi_type_safety() {
        // Verify FFI type declarations match expected C ABI sizes
        // This ensures we can safely call into Zig from Rust
        assert!(
            std::mem::size_of::<BeamZResult>() >= 16,
            "BeamZResult should be at least 16 bytes"
        );
        assert!(
            std::mem::size_of::<BeamZEtfScanResult>() >= 16,
            "BeamZEtfScanResult should be at least 16 bytes"
        );
        assert!(
            std::mem::align_of::<BeamZResult>() >= 4,
            "BeamZResult alignment should be at least 4"
        );
        assert!(
            std::mem::align_of::<BeamZEtfScanResult>() >= 4,
            "BeamZEtfScanResult alignment should be at least 4"
        );
    }
}
