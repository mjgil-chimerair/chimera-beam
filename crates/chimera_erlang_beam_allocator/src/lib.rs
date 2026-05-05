//! Test allocator that caps memory usage for RustZigBeam test binaries.
//!
//! Add this crate as a dev-dependency and force-link it from test builds with
//! `#[cfg(test)] use chimera_erlang_beam_allocator as _;`.

use std::alloc;

const TEST_ALLOCATION_LIMIT_BYTES: usize = 128 * 1024 * 1024;

#[global_allocator]
static TEST_ALLOCATOR: cap::Cap<alloc::System> =
    cap::Cap::new(alloc::System, TEST_ALLOCATION_LIMIT_BYTES);
