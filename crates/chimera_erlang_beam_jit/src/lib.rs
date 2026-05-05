//! Chimera-Erlang-BEAM JIT Compiler
//!
//! C++ JIT compiler integration for on-demand native code generation.
//!
//! Per design.md: C++ reserved for optional JIT/optimizer tiers.
//!
//! # Integration with VM
//!
//! The JIT integrates with the VM through the following flow:
//! 1. VM identifies hot function during interpretation
//! 2. VM calls `compile()` with BEAM bytecode
//! 3. JIT compiles to native x86-64 code
//! 4. VM replaces function pointer with native code address
//! 5. Subsequent calls go directly to native code
//!
//! # Code Cache
//!
//! The JIT maintains a code cache keyed by (bytecode, arity).
//! Repeated compilations of the same function return cached code.

use std::os::raw::c_char;

/// JIT compilation result
#[repr(C)]
#[derive(Debug)]
pub struct JitResult {
    pub code: u32,
    pub native_code: *mut std::ffi::c_void,
    pub code_size: usize,
    pub error: *const c_char,
}

/// FFI declaration for C++ JIT compiler
#[cfg(feature = "use_cpp_jit")]
#[link(name = "chimera_jit", kind = "static")]
extern "C" {
    fn jit_compile(bytecode: *const u8, bytecode_len: usize, arity: u32) -> JitResult;
    fn jit_free(native_code: *mut std::ffi::c_void);
    fn jit_is_available() -> std::os::raw::c_int;
}

/// Check if JIT is available
#[cfg(feature = "use_cpp_jit")]
pub fn is_available() -> bool {
    unsafe { jit_is_available() != 0 }
}

/// Compile BEAM bytecode to native code
#[cfg(feature = "use_cpp_jit")]
pub fn compile(bytecode: &[u8], arity: u32) -> Result<JitResult, String> {
    let result = unsafe {
        jit_compile(bytecode.as_ptr(), bytecode.len(), arity)
    };

    if result.code == 0 {
        Ok(result)
    } else {
        let error_msg = if result.error.is_null() {
            "Unknown error".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(result.error) }
                .to_string_lossy()
                .into_owned()
        };
        Err(error_msg)
    }
}

/// Free compiled native code
#[cfg(feature = "use_cpp_jit")]
pub fn free(native_code: *mut std::ffi::c_void) {
    unsafe { jit_free(native_code) }
}

/// Stub when C++ JIT is not available
#[cfg(not(feature = "use_cpp_jit"))]
pub fn is_available() -> bool {
    false
}

/// Stub compile when C++ JIT is not available
#[cfg(not(feature = "use_cpp_jit"))]
pub fn compile(_bytecode: &[u8], _arity: u32) -> Result<JitResult, String> {
    Err("C++ JIT not available - use --features use_cpp_jit".to_string())
}

/// Stub free when C++ JIT is not available (no-op)
#[cfg(not(feature = "use_cpp_jit"))]
pub fn free(_native_code: *mut std::ffi::c_void) {
    // No-op when JIT is not available
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_jit_stub_available() {
        // Stub always returns false since use_cpp_jit is not enabled
        // Real tests require C++ JIT to be built
        assert!(!super::is_available());
    }

    #[test]
    fn test_jit_compile_stub() {
        // Test that compile returns error when JIT not available
        let result = super::compile(&[0x01, 0x02, 0x03], 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not available"));
    }

    #[test]
    fn test_jit_free_stub() {
        // Test that free doesn't panic on null
        super::free(std::ptr::null_mut());
    }

    #[test]
    fn test_jit_result_defaults() {
        // Verify JitResult struct has correct defaults
        let result = super::JitResult {
            code: 0,
            native_code: std::ptr::null_mut(),
            code_size: 0,
            error: std::ptr::null(),
        };
        assert_eq!(result.code, 0);
        assert!(result.native_code.is_null());
        assert_eq!(result.code_size, 0);
        assert!(result.error.is_null());
    }
}