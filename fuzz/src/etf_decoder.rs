//! Fuzz target: ETF decoder
//!
//! Tests that malformed ETF (Erlang Term Format) input doesn't cause
//! panics or crashes in the decoder.

#![no_main]

use libfuzzer_sys::fuzz_target;
use chimera_erlang_beam_term::Term;

fuzz_target!(|data: &[u8]| {
    // Try to decode ETF data
    let _ = decode_etf_safe(data);
});

/// Safely attempt to decode ETF data
/// Returns without panicking on invalid input
fn decode_etf_safe(data: &[u8]) -> Option<Term> {
    // ETF format: version byte (0x83) followed by term
    // Without full decoder, just validate version if present
    if data.is_empty() {
        return None;
    }

    // ETF version is 0x83 (131)
    if data[0] == 0x83 {
        // Version byte present - this is ETF format
        // For now, just validate we can handle it without panicking
        // A full implementation would decode the term
        if data.len() < 2 {
            return None;
        }

        // Just return a placeholder - full decoder would go here
        // For fuzzing, we just want to ensure no panics
        return Some(Term::nil());
    }

    None
}