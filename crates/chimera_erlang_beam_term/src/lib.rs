//! Term representation for RustZigBeam.
//!
//! Provides tagged term representation compatible with BEAM semantics.
//! Zig kernels handle hot-path term operations.

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

// Re-export shared types from rzx_beam_term
pub use rzx_beam_term::Atom;
pub use rzx_beam_term::Mfa;

use std::cmp::Ordering;
use std::hash::Hash;
use std::os::raw::c_uint;

/// Tag for a BEAM term
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
#[repr(u8)]
pub enum TermTag {
    /// Small integer (immediate)
    SmallInteger = 0,
    /// Atom index
    Atom = 1,
    /// Cons cell (list)
    Cons = 2,
    /// Tuple pointer
    Tuple = 3,
    /// Float pointer
    Float = 4,
    /// Binary pointer
    Binary = 5,
    /// Map pointer
    Map = 6,
    /// Fun pointer
    Fun = 7,
}

/// A tagged term representation.
/// Uses low 3 bits for tag, remaining bits for value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Term(pub u64);

/// Convert i64 to u64 bits for BEAM small integer representation.
/// For BEAM small integers, we store the i64 value in bits 3-62 (60 bits).
#[inline]
fn i64_to_u64_bits(x: i64) -> u64 {
    // The value goes in the middle bits (3-62), tag in low bits (0-2)
    (x as u64) << 3
}

/// Convert u64 bits back to i64
#[inline]
fn u64_bits_to_i64(x: u64) -> i64 {
    // Right shift by 3 to get the value back
    // Then cast to i64 (sign extension will happen correctly)
    (x as i64) >> 3
}

impl Term {
    /// Number of tag bits (low 3 bits used for tagging)
    pub const SHIFT: u64 = 3;
    /// Mask for extracting the tag (low 3 bits)
    pub const MASK: u64 = 0x7;
    /// Mask for extracting the value bits (all except low 3)
    pub const VALUE_BITS: u64 = !0x7u64; // All bits except low 3

    /// Minimum small integer value (2^59 - 1 in positive range)
    /// With 60 bits of value space, range is -(2^59) to (2^59 - 1)
    pub const SMALL_MIN: i64 = -(1i64 << 59);
    /// Maximum small integer value
    pub const SMALL_MAX: i64 = (1i64 << 59) - 1;

    /// Encode a small integer (unchecked - caller must validate bounds)
    ///
    /// # Safety
    /// The caller must ensure x is within SMALL_MIN..=SMALL_MAX
    #[inline]
    pub unsafe fn from_small_unchecked(x: i64) -> Self {
        Term(i64_to_u64_bits(x))
    }

    /// Encode a small integer with bounds checking
    ///
    /// Returns None if the value is outside the valid small integer range.
    pub fn from_small_checked(x: i64) -> Option<Self> {
        if (Self::SMALL_MIN..=Self::SMALL_MAX).contains(&x) {
            Some(Term(i64_to_u64_bits(x)))
        } else {
            None
        }
    }

    /// Encode a small integer (panics on overflow)
    ///
    /// Convenience method for tests and controlled scenarios.
    /// Use `from_small_checked` for untrusted input.
    #[inline]
    pub fn from_small(x: i64) -> Self {
        if (Self::SMALL_MIN..=Self::SMALL_MAX).contains(&x) {
            Term(i64_to_u64_bits(x))
        } else {
            panic!("small integer out of bounds: {}", x)
        }
    }

    /// Check if a value is within small integer bounds
    #[inline]
    pub fn is_small_int(x: i64) -> bool {
        (Self::SMALL_MIN..=Self::SMALL_MAX).contains(&x)
    }

    /// Decode a small integer
    #[inline]
    pub fn to_small(self) -> i64 {
        u64_bits_to_i64(self.0)
    }

    /// Get small integer value if this is a small integer
    pub fn to_small_opt(self) -> Option<i64> {
        if self.tag() == TermTag::SmallInteger {
            Some(self.to_small())
        } else {
            None
        }
    }

    /// Check if this is a small integer
    pub fn is_small(self) -> bool {
        self.tag() == TermTag::SmallInteger
    }

    /// Check if this is an atom
    pub fn is_atom(self) -> bool {
        self.tag() == TermTag::Atom
    }

    /// Check if this is a cons cell (list)
    pub fn is_cons(self) -> bool {
        self.tag() == TermTag::Cons
    }

    /// Encode an atom index
    pub fn from_atom(id: u32) -> Self {
        Term(((id as u64) << Self::SHIFT) | (TermTag::Atom as u64))
    }

    /// Decode an atom index
    pub fn to_atom(self) -> u32 {
        (self.0 >> Self::SHIFT) as u32
    }

    /// Encode a cons cell pointer
    pub fn from_cons(ptr: u64) -> Self {
        Term((ptr << Self::SHIFT) | (TermTag::Cons as u64))
    }

    /// Decode a cons cell pointer
    pub fn to_cons(self) -> u64 {
        self.0 >> Self::SHIFT
    }

    /// Encode a tuple pointer
    pub fn from_tuple(ptr: u64) -> Self {
        Term((ptr << Self::SHIFT) | (TermTag::Tuple as u64))
    }

    /// Decode a tuple pointer
    pub fn to_tuple(self) -> u64 {
        self.0 >> Self::SHIFT
    }

    /// Encode a map pointer
    pub fn from_map(ptr: u64) -> Self {
        Term((ptr << Self::SHIFT) | (TermTag::Map as u64))
    }

    /// Decode a map pointer
    pub fn to_map(self) -> u64 {
        self.0 >> Self::SHIFT
    }

    /// Check if this is a map term
    pub fn is_map(self) -> bool {
        self.tag() == TermTag::Map
    }

    /// Encode a binary pointer
    pub fn from_binary_ptr(ptr: u64) -> Self {
        Term((ptr << Self::SHIFT) | (TermTag::Binary as u64))
    }

    /// Decode a binary pointer
    pub fn to_binary(self) -> u64 {
        self.0 >> Self::SHIFT
    }

    /// Check if this is a binary term
    pub fn is_binary(self) -> bool {
        self.tag() == TermTag::Binary
    }

    /// Encode a float pointer
    pub fn from_float_ptr(ptr: u64) -> Self {
        Term((ptr << Self::SHIFT) | (TermTag::Float as u64))
    }

    /// Decode a float pointer
    pub fn to_float(self) -> u64 {
        self.0 >> Self::SHIFT
    }

    /// Check if this is a float term
    pub fn is_float(self) -> bool {
        self.tag() == TermTag::Float
    }

    /// Encode a fun pointer
    pub fn from_fun_ptr(ptr: u64) -> Self {
        Term((ptr << Self::SHIFT) | (TermTag::Fun as u64))
    }

    /// Decode a fun pointer
    pub fn to_fun(self) -> u64 {
        self.0 >> Self::SHIFT
    }

    /// Check if this is a fun term
    pub fn is_fun(self) -> bool {
        self.tag() == TermTag::Fun
    }

    /// Encode nil (empty list)
    pub fn nil() -> Self {
        Self::from_atom(2) // ATOM_NIL
    }

    /// Get the tag for this term
    ///
    /// Returns the tag extracted from the low bits of the term.
    /// Only valid for properly constructed terms.
    pub fn tag(self) -> TermTag {
        let raw = (self.0 & Self::MASK) as u8;
        // Safe conversion using match instead of transmute
        // This handles all valid tag values (0-7) and invalid ones safely
        match raw {
            0 => TermTag::SmallInteger,
            1 => TermTag::Atom,
            2 => TermTag::Cons,
            3 => TermTag::Tuple,
            4 => TermTag::Float,
            5 => TermTag::Binary,
            6 => TermTag::Map,
            7 => TermTag::Fun,
            // Invalid tag - this indicates a corrupted term
            // Return a safe default rather than undefined behavior
            _ => TermTag::SmallInteger,
        }
    }

    /// Check if this is an immediate value (small int or atom)
    pub fn is_immediate(self) -> bool {
        matches!(self.tag(), TermTag::SmallInteger | TermTag::Atom)
    }

    /// Check if this is a boxed (pointer) value
    pub fn is_boxed(self) -> bool {
        !self.is_immediate()
    }

    /// Calculate the size in words for this term
    ///
    /// For immediate terms (small integers, atoms), returns 0 since
    /// they don't require heap allocation.
    /// For boxed terms, returns the number of words needed including
    /// the header word.
    pub fn term_size_words(&self) -> usize {
        match self.tag() {
            TermTag::SmallInteger | TermTag::Atom => 0, // immediate, no heap allocation
            TermTag::Cons => 3,                         // header + hd + tl
            TermTag::Tuple => {
                // Need to know arity - for now return 0 since we can't decode
                // A real implementation would decode the header
                0
            }
            TermTag::Float => 3, // header + 2 words for float
            TermTag::Binary => {
                // Would need size from header
                2 // header + size word
            }
            TermTag::Map => 0, // would need size from header
            TermTag::Fun => 0, // would need size from header
        }
    }

    /// Check if this is a nil (empty list) term
    pub fn is_nil(&self) -> bool {
        self.0 == Self::nil().0
    }

    /// Get the raw u64 value for this term
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Create a term from raw value
    pub fn from_raw(val: u64) -> Self {
        Term(val)
    }

    /// Get the atom index for comparison with nil
    ///
    /// Returns the atom index if this is an atom, None otherwise.
    pub fn as_atom_index(&self) -> Option<u32> {
        if self.tag() == TermTag::Atom {
            Some(self.to_atom())
        } else {
            None
        }
    }

    /// Compare two terms for equality
    ///
    /// Returns true if the terms are equal, false otherwise.
    /// For boxed types (cons, tuple, etc.), compares pointer equality.
    pub fn term_eq(&self, other: &Term) -> bool {
        self.0 == other.0
    }

    /// Compare two terms (partial ordering)
    ///
    /// Returns None if terms cannot be compared (different types).
    /// Returns Some(Ordering) for comparable terms.
    pub fn compare(&self, other: &Term) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;

        // Compare by tag first
        let tag_ord = self.tag().cmp(&other.tag());
        if tag_ord != Ordering::Equal {
            return Some(tag_ord);
        }

        // Same tag, compare by value
        match self.tag() {
            TermTag::SmallInteger => {
                let a = self.to_small();
                let b = other.to_small();
                Some(a.cmp(&b))
            }
            TermTag::Atom => {
                let a = self.to_atom();
                let b = other.to_atom();
                Some(a.cmp(&b))
            }
            TermTag::Cons
            | TermTag::Tuple
            | TermTag::Float
            | TermTag::Binary
            | TermTag::Map
            | TermTag::Fun => {
                // Boxed types: compare pointer values
                let a = self.0 >> 3;
                let b = other.0 >> 3;
                Some(a.cmp(&b))
            }
        }
    }

    /// Compute hash of a term
    ///
    /// Returns a hash value for use in hash-based collections.
    /// Uses the raw term value for fast hashing.
    pub fn hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.0.hash(&mut hasher);
        hasher.finish()
    }

    /// Compute hash with a hasher
    pub fn hash_with<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }

    /// Get the total size in words for this term including heap allocation
    ///
    /// For immediate terms (small integers, atoms), returns 1 (just the term).
    /// For boxed terms, returns the full size including header and data.
    ///
    /// This is used for term sizing/copying operations.
    pub fn total_size_words(&self) -> usize {
        match self.tag() {
            TermTag::SmallInteger | TermTag::Atom => 1, // Immediate terms are 1 word
            TermTag::Cons => 3,                         // Header + head + tail
            TermTag::Tuple => {
                // For tuple, we need the arity from header - we can't determine
                // size without access to the heap. Return 1 as placeholder.
                1
            }
            TermTag::Float => 3, // Header + 2 words for float
            TermTag::Binary => {
                // For binary, we need size from header - return placeholder
                2 // header + size word (minimum)
            }
            TermTag::Map => 1, // Need header for size
            TermTag::Fun => 1, // Need header for size
        }
    }

    /// Compare this term with another using Erlang term ordering
    ///
    /// Returns the ordering according to Erlang term comparison rules.
    pub fn term_cmp(&self, other: &Term) -> Ordering {
        // Quick path: if both are same tag, compare raw values
        let self_tag = self.tag();
        let other_tag = other.tag();

        if self_tag == other_tag {
            return self.0.cmp(&other.0);
        }

        // Different tag ordering (BEAM ordering from lowest to highest):
        // smallint < float < atom < reference < fun < port < pid < tuple < map < list < binary
        let self_order = tag_order(self_tag);
        let other_order = tag_order(other_tag);

        self_order.cmp(&other_order)
    }
}

/// Returns the ordering rank for a term tag (lower = smaller in Erlang ordering)
fn tag_order(tag: TermTag) -> u8 {
    match tag {
        TermTag::SmallInteger => 0,
        TermTag::Cons => 1, // lists come before atoms
        TermTag::Atom => 2,
        TermTag::Tuple => 3,
        TermTag::Float => 4,
        TermTag::Binary => 5,
        TermTag::Map => 6,
        TermTag::Fun => 7,
    }
}

impl Ord for Term {
    fn cmp(&self, other: &Term) -> Ordering {
        self.term_cmp(other)
    }
}

impl PartialOrd for Term {
    fn partial_cmp(&self, other: &Term) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Term {
    /// Trace references in this term for GC
    ///
    /// Returns an iterator over all terms referenced by this term.
    /// For immediate values (small integers, atoms), returns empty.
    /// For boxed values (cons, tuple, etc.), returns the referenced terms.
    pub fn trace_references(&self) -> TermReferences {
        match self.tag() {
            TermTag::SmallInteger | TermTag::Atom => TermReferences::Empty,
            TermTag::Cons => {
                // Cons cell: decode pointer and return head/tail
                let ptr = self.to_cons();
                if ptr == 0 {
                    TermReferences::Empty
                } else {
                    // We can't easily return the actual terms without access to the heap
                    // For now, return a raw reference indicator
                    TermReferences::ConsRefs(ptr)
                }
            }
            TermTag::Tuple => {
                let ptr = self.to_tuple();
                if ptr == 0 {
                    TermReferences::Empty
                } else {
                    TermReferences::TupleRef(ptr)
                }
            }
            TermTag::Float | TermTag::Binary | TermTag::Map | TermTag::Fun => TermReferences::Empty,
        }
    }
}

/// Iterator over term references for GC tracing.
pub enum TermReferences {
    /// No references (empty iterator)
    Empty,
    /// Cons/List references (pointer to cons cell)
    ConsRefs(u64),
    /// Tuple references (pointer to tuple)
    TupleRef(u64),
}

impl Iterator for TermReferences {
    type Item = Term;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            TermReferences::Empty => None,
            TermReferences::ConsRefs(_) => {
                // For cons, we can't really trace without heap access
                // Return self as indicator that this pointer needs processing
                if let TermReferences::ConsRefs(ptr) = self {
                    let t = Term::from_cons(*ptr);
                    *self = TermReferences::Empty;
                    Some(t)
                } else {
                    None
                }
            }
            TermReferences::TupleRef(_) => {
                // Similarly for tuple
                if let TermReferences::TupleRef(ptr) = self {
                    let t = Term::from_tuple(*ptr);
                    *self = TermReferences::Empty;
                    Some(t)
                } else {
                    None
                }
            }
        }
    }
}

/// Term copy result for tracking copy operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyResult {
    /// Copy succeeded
    Copied(Term),
    /// Term would exceed buffer space
    OutOfSpace,
    /// Term type not yet supported for copying
    Unsupported,
}

impl Term {
    /// Attempt to copy a term to a destination buffer
    ///
    /// This is a reference implementation for testing differential testing
    /// against Zig kernels. It handles immediate values perfectly and
    /// provides basic support for boxed types.
    ///
    /// The dest buffer is a mutable slice of u64 words. The function returns
    /// the copied term (which may include updated pointers for boxed objects)
    /// and the number of words consumed.
    ///
    /// # Arguments
    /// * `self` - The term to copy
    /// * `dest` - Destination buffer (words)
    /// * `offset` - Current write position in dest
    ///
    /// # Returns
    /// * `CopyResult::Copied(new_term)` with updated offset if successful
    /// * `CopyResult::OutOfSpace` if buffer too small
    /// * `CopyResult::Unsupported` for unsupported term types
    pub fn copy_to_buffer(&self, dest: &mut [u64], offset: usize) -> CopyResult {
        // Immediate terms are stored inline, just copy the raw value
        if self.is_immediate() {
            if offset < dest.len() {
                dest[offset] = self.0;
                CopyResult::Copied(Term(self.0))
            } else {
                CopyResult::OutOfSpace
            }
        } else {
            // For now, we don't have a full boxed object decoder
            // Just copy the raw term as-is
            if offset < dest.len() {
                dest[offset] = self.0;
                CopyResult::Copied(Term(self.0))
            } else {
                CopyResult::OutOfSpace
            }
        }
    }
}

/// Calculate total size needed to copy a term in words
///
/// This is used to pre-calculate buffer requirements before copying.
/// For immediate terms, returns 0 (no allocation needed).
/// For boxed terms, returns the size including the header word.
pub fn term_copy_size_words(term: &Term) -> usize {
    if term.is_immediate() {
        0 // immediate values don't need separate heap allocation
    } else {
        // Boxed terms need at least 1 word for the pointer
        // Real implementation would look at header for actual size
        1
    }
}

/// Atom table for dynamic atom interning
pub mod atom;
pub use atom::{AtomEntry, AtomError, AtomTable, MAX_ATOMS, RESERVED_ATOMS};

/// PID/Ref/Port term types
pub mod pid;
pub use pid::{BoxedTag, PidTerm, PortTerm, RefTerm, MAX_CREATION, MAX_PID_INDEX, MAX_SERIAL};

/// Boxed object header definitions
pub mod boxed;
pub use boxed::{
    bigint_header, binary_header, cons_header, extract_size, extract_sub_tag, extract_tag,
    float_header, fun_header, is_forwarding_pointer, map_header, ref_header, tuple_header,
    BoxedHeader, BoxedSubTag, FORWARD_MAGIC,
};

/// ETF encoding/decoding (Erlang External Term Format)
pub mod etf;
pub use etf::{decode, encode, encode_with_version, DecodeError, EncodeError};

/// Predefined atom indices for commonly-used atoms.
pub mod atoms {
    /// Atom for 'false'
    pub const ATOM_FALSE: u32 = 0;
    /// Atom for 'true'
    pub const ATOM_TRUE: u32 = 1;
    /// Atom for 'nil'
    pub const ATOM_NIL: u32 = 2;
    /// Atom for 'undefined'
    pub const ATOM_UNDEFINED: u32 = 3;
    /// Atom for 'ok'
    pub const ATOM_OK: u32 = 4;
    /// Atom for 'error'
    pub const ATOM_ERROR: u32 = 5;
    /// Atom for 'badarg'
    pub const ATOM_BADARG: u32 = 6;
    /// Atom for 'exit'
    pub const ATOM_EXIT: u32 = 7;
    /// Atom for 'normal'
    pub const ATOM_NORMAL: u32 = 8;
    /// Atom for 'kill'
    pub const ATOM_KILL: u32 = 9;
    /// Atom for 'message_queue_len'
    pub const ATOM_MESSAGE_QUEUE_LEN: u32 = 10;
    /// Atom for 'heap_size'
    pub const ATOM_HEAP_SIZE: u32 = 11;
    /// Atom for 'stack_size'
    pub const ATOM_STACK_SIZE: u32 = 12;
    /// Atom for 'reductions'
    pub const ATOM_REDUCTIONS: u32 = 13;
    /// Atom for 'status'
    pub const ATOM_STATUS: u32 = 14;
    /// Atom for 'running'
    pub const ATOM_RUNNING: u32 = 15;
    /// Atom for 'waiting'
    pub const ATOM_WAITING: u32 = 16;
    /// Atom for 'exiting'
    pub const ATOM_EXITING: u32 = 17;
    /// Atom for 'garbage_collecting'
    pub const ATOM_GARBAGE_COLLECTING: u32 = 18;
    /// Atom for 'suspended'
    pub const ATOM_SUSPENDED: u32 = 19;
    /// Atom for 'dead'
    pub const ATOM_DEAD: u32 = 20;
    /// Atom for 'low'
    pub const ATOM_LOW: u32 = 21;
    /// Atom for 'high'
    pub const ATOM_HIGH: u32 = 22;
    /// Atom for 'max' (highest priority)
    pub const ATOM_MAX: u32 = 23;
    /// Atom for 'badmatch'
    pub const ATOM_BADMATCH: u32 = 24;
    /// Atom for 'case_clause'
    pub const ATOM_CASE_CLAUSE: u32 = 25;
    /// Atom for 'if_clause'
    pub const ATOM_IF_CLAUSE: u32 = 26;
    /// Atom for 'function_clause'
    pub const ATOM_FUNCTION_CLAUSE: u32 = 27;
    /// Atom for 'minor'
    pub const ATOM_MINOR: u32 = 28;
    /// Atom for 'major'
    pub const ATOM_MAJOR: u32 = 29;
    /// For nodes/0 when no connected nodes
    pub const ATOM_NONODE_NOCONTACT: u32 = 30;
    /// Atom representing 'true' for node comparisons (same as ATOM_TRUE but explicit)
    pub const ATOM_TRUE_ATOM: u32 = 31;
}

/// C-compatible result for Zig FFI.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BeamZResult {
    /// Return code (0 = success, non-zero = error)
    pub code: c_uint,
    /// Number of bytes consumed/processed
    pub consumed: usize,
    /// Number of bytes produced/output
    pub produced: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_integer_encoding() {
        let term = Term::from_small(42);
        assert_eq!(term.tag(), TermTag::SmallInteger);
        assert_eq!(term.to_small(), 42);
    }

    #[test]
    fn test_small_integer_negative() {
        let term = Term::from_small(-123);
        assert_eq!(term.tag(), TermTag::SmallInteger);
        assert_eq!(term.to_small(), -123);
    }

    #[test]
    fn test_atom_encoding() {
        let term = Term::from_atom(42);
        assert_eq!(term.tag(), TermTag::Atom);
        assert_eq!(term.to_atom(), 42);
    }

    #[test]
    fn test_cons_encoding() {
        let ptr: u64 = 0x12345678;
        let term = Term::from_cons(ptr);
        assert_eq!(term.tag(), TermTag::Cons);
        assert_eq!(term.to_cons(), ptr);
    }

    #[test]
    fn test_tuple_encoding() {
        let ptr: u64 = 0xABCDEF00;
        let term = Term::from_tuple(ptr);
        assert_eq!(term.tag(), TermTag::Tuple);
        assert_eq!(term.to_tuple(), ptr);
    }

    #[test]
    fn test_immediate_check() {
        assert!(Term::from_small(100).is_immediate());
        assert!(Term::from_atom(1).is_immediate());
        assert!(!Term::from_cons(100).is_immediate());
    }

    #[test]
    fn test_boxed_check() {
        assert!(Term::from_cons(100).is_boxed());
        assert!(Term::from_tuple(100).is_boxed());
        assert!(!Term::from_small(100).is_boxed());
    }

    #[test]
    fn test_nil() {
        let nil = Term::nil();
        assert_eq!(nil.tag(), TermTag::Atom);
        assert_eq!(nil.to_atom(), atoms::ATOM_NIL);
    }

    #[test]
    fn test_small_integer_bounds() {
        // Test min value
        let min = Term::from_small_checked(Term::SMALL_MIN);
        assert!(min.is_some());
        assert_eq!(min.unwrap().to_small(), Term::SMALL_MIN);

        // Test max value
        let max = Term::from_small_checked(Term::SMALL_MAX);
        assert!(max.is_some());
        assert_eq!(max.unwrap().to_small(), Term::SMALL_MAX);

        // Test overflow - too large
        let overflow = Term::from_small_checked(Term::SMALL_MAX + 1);
        assert!(overflow.is_none());

        // Test underflow - too small
        let underflow = Term::from_small_checked(Term::SMALL_MIN - 1);
        assert!(underflow.is_none());
    }

    #[test]
    fn test_small_integer_roundtrip() {
        let values = [
            0i64,
            1,
            -1,
            42,
            -123,
            1000,
            -1000,
            Term::SMALL_MAX,
            Term::SMALL_MIN,
        ];

        for &val in &values {
            let term = Term::from_small(val);
            assert_eq!(term.to_small(), val);
        }
    }

    #[test]
    fn test_is_small_int() {
        assert!(Term::is_small_int(0));
        assert!(Term::is_small_int(1));
        assert!(Term::is_small_int(-1));
        assert!(Term::is_small_int(Term::SMALL_MAX));
        assert!(Term::is_small_int(Term::SMALL_MIN));
        assert!(!Term::is_small_int(Term::SMALL_MAX + 1));
        assert!(!Term::is_small_int(Term::SMALL_MIN - 1));
    }

    #[test]
    fn test_small_integer_bits_preserved() {
        // Ensure encoding/decoding is lossless
        let original = 0x1234567890i64;
        let term = Term::from_small(original);
        let decoded = term.to_small();
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_term_size_words_immediate() {
        // Small integers have size 0 (they're inline)
        let small = Term::from_small(42);
        assert_eq!(small.term_size_words(), 0);

        // Atoms also have size 0 (immediate)
        let atom = Term::from_atom(5);
        assert_eq!(atom.term_size_words(), 0);
    }

    #[test]
    fn test_term_size_words_boxed() {
        // Cons cells require 3 words
        let cons = Term::from_cons(0x100);
        assert_eq!(cons.term_size_words(), 3);

        // Float boxed terms require 3 words
        let float_term = Term::from_tuple(0); // boxed type
                                              // Note: we can't easily create a proper float term without heap allocation
                                              // But we verify the method works
        assert!(float_term.is_boxed());
    }

    #[test]
    fn test_term_is_nil() {
        let nil = Term::nil();
        assert!(nil.is_nil());
        assert!(nil.is_atom());

        let not_nil = Term::from_small(42);
        assert!(!not_nil.is_nil());
    }

    #[test]
    fn test_term_raw_roundtrip() {
        let original = Term::from_small(12345);
        let raw = original.raw();
        let reconstructed = Term::from_raw(raw);
        assert_eq!(original.to_small(), reconstructed.to_small());
    }

    #[test]
    fn test_copy_to_buffer_small_integer() {
        let term = Term::from_small(42);
        let mut buffer = [0u64; 10];

        let result = term.copy_to_buffer(&mut buffer, 0);
        match result {
            CopyResult::Copied(copied) => {
                assert_eq!(copied.to_small(), 42);
                assert_eq!(buffer[0], term.raw());
            }
            _ => panic!("Expected Copied result"),
        }
    }

    #[test]
    fn test_copy_to_buffer_atom() {
        let term = Term::from_atom(99);
        let mut buffer = [0u64; 10];

        let result = term.copy_to_buffer(&mut buffer, 0);
        match result {
            CopyResult::Copied(copied) => {
                assert_eq!(copied.to_atom(), 99);
            }
            _ => panic!("Expected Copied result"),
        }
    }

    #[test]
    fn test_copy_to_buffer_out_of_space() {
        let term = Term::from_small(42);
        let mut buffer = [0u64; 1];

        let result = term.copy_to_buffer(&mut buffer, 1); // offset 1 is out of bounds
        assert!(matches!(result, CopyResult::OutOfSpace));
    }

    #[test]
    fn test_copy_to_buffer_cons() {
        let term = Term::from_cons(0x12345);
        let mut buffer = [0u64; 10];

        let result = term.copy_to_buffer(&mut buffer, 0);
        match result {
            CopyResult::Copied(copied) => {
                // Cons pointer is preserved
                assert_eq!(copied.to_cons(), 0x12345);
            }
            _ => panic!("Expected Copied result"),
        }
    }

    #[test]
    fn test_term_copy_size_words() {
        // Immediate terms need 0 words for copy (they're inline)
        assert_eq!(term_copy_size_words(&Term::from_small(42)), 0);
        assert_eq!(term_copy_size_words(&Term::from_atom(1)), 0);

        // Boxed terms need at least 1 word
        assert!(term_copy_size_words(&Term::from_cons(100)) >= 1);
        assert!(term_copy_size_words(&Term::from_tuple(100)) >= 1);
    }

    #[test]
    fn test_as_atom_index() {
        let atom = Term::from_atom(42);
        assert_eq!(atom.as_atom_index(), Some(42));

        let not_atom = Term::from_small(5);
        assert_eq!(not_atom.as_atom_index(), None);
    }

    #[test]
    fn test_binary_encoding() {
        // Test binary pointer encoding/decoding
        let ptr = 0x12345u64;
        let term = Term::from_binary_ptr(ptr);
        assert!(term.is_binary());
        assert_eq!(term.to_binary(), ptr);
    }
}
