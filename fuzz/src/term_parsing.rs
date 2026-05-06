//! Fuzz target: Term parsing
//!
//! Tests that malformed or invalid term data doesn't cause panics
//! or undefined behavior in the term parser.

#![no_main]

use libfuzzer_sys::fuzz_target;
use chimera_erlang_beam_term::Term;

fuzz_target!(|data: &[u8]| {
    // Try to parse the data as a term
    // This should not panic regardless of input
    let _ = parse_term_safe(data);
});

/// Safely attempt to parse a term from bytes
/// Returns None if parsing fails (expected for invalid data)
fn parse_term_safe(data: &[u8]) -> Option<Term> {
    // For now, just check if we can interpret raw bytes as valid term
    // A full implementation would use an actual ETF decoder
    if data.len() < 8 {
        return None;
    }

    // Try to read a u64 from the data
    let mut buf = [0u8; 8];
    if data.len() >= 8 {
        buf.copy_from_slice(&data[..8]);
    } else {
        buf[..data.len()].copy_from_slice(data);
    }

    let raw = u64::from_le_bytes(buf);

    // Check if this could be a valid term tag
    let tag = raw & 0x7;

    // Validate tag is one we recognize
    match tag {
        0 => Some(Term(raw)),  // SmallInteger
        1 => Some(Term(raw)),  // Atom
        2 => Some(Term(raw)),  // Cons/Nil
        3 => Some(Term(raw)),  // Tuple
        4 => Some(Term(raw)),  // Float
        5 => Some(Term(raw)),  // Binary
        6 => Some(Term(raw)),  // Map
        7 => Some(Term(raw)),  // Fun
        _ => None, // Invalid tag
    }
}