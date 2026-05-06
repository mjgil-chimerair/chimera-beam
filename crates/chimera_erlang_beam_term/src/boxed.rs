//! Boxed object header definitions for RustZigBeam.
//!
//! Defines the header word format and metadata for all boxed term types.
//! Headers include size info, type tags, and GC scan information.

use super::TermTag;

/// Boxed term header word structure
///
/// The header word is the first word of every boxed term on the heap.
/// It encodes the term type, size, and GC metadata.
///
/// Layout (64 bits):
/// - Bits 0-2: Term tag (always 2-7 for boxed types)
/// - Bits 3-7: Boxed sub-tag (type-specific)
/// - Bits 8-31: Size in words (24 bits = 16 MB max)
/// - Bits 32-63: GC scan metadata / type-specific data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct BoxedHeader(pub u64);

impl BoxedHeader {
    /// Create a new boxed header
    pub const fn new(sub_tag: BoxedSubTag, size: u32) -> Self {
        let tag = TermTag::Cons as u64; // Default tag for boxed
        let sub_tag_bits = (sub_tag as u64) << 3;
        let size_bits = ((size as u64) & 0xFFFFFF) << 8; // 24-bit size field
        BoxedHeader(tag | sub_tag_bits | size_bits)
    }

    /// Get the term tag (always boxed)
    pub fn tag(&self) -> TermTag {
        TermTag::Cons // All boxed terms use Cons tag for pointer encoding
    }

    /// Get the boxed sub-tag
    pub fn sub_tag(&self) -> BoxedSubTag {
        let bits = (self.0 >> 3) & 0x1F;
        match bits {
            0 => BoxedSubTag::Cons,
            1 => BoxedSubTag::Tuple,
            2 => BoxedSubTag::Float,
            3 => BoxedSubTag::Binary,
            4 => BoxedSubTag::Map,
            5 => BoxedSubTag::Fun,
            6 => BoxedSubTag::BigInteger,
            7 => BoxedSubTag::Ref,
            _ => BoxedSubTag::Cons,
        }
    }

    /// Get the size in words
    pub fn size(&self) -> u32 {
        ((self.0 >> 8) & 0xFFFFFF) as u32
    }

    /// Get raw header word value
    pub fn raw(&self) -> u64 {
        self.0
    }

    /// Create from raw value
    pub fn from_raw(raw: u64) -> Self {
        BoxedHeader(raw)
    }

    /// Check if this header indicates a list (cons cell)
    pub fn is_cons(&self) -> bool {
        self.sub_tag() == BoxedSubTag::Cons
    }

    /// Check if this header indicates a tuple
    pub fn is_tuple(&self) -> bool {
        self.sub_tag() == BoxedSubTag::Tuple
    }

    /// Check if this header indicates a float
    pub fn is_float(&self) -> bool {
        self.sub_tag() == BoxedSubTag::Float
    }

    /// Check if this header indicates a binary
    pub fn is_binary(&self) -> bool {
        self.sub_tag() == BoxedSubTag::Binary
    }
}

/// Boxed sub-tag values for type identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BoxedSubTag {
    /// Cons cell (list)
    Cons = 0,
    /// Tuple
    Tuple = 1,
    /// Float
    Float = 2,
    /// Binary
    Binary = 3,
    /// Map
    Map = 4,
    /// Fun/closure
    Fun = 5,
    /// Big integer
    BigInteger = 6,
    /// Reference (used for distributed refs)
    Ref = 7,
}

impl BoxedSubTag {
    /// Get the number of header words for this type
    pub fn header_words(&self) -> u32 {
        match self {
            BoxedSubTag::Cons => 2,       // header + cons data (hd/tl)
            BoxedSubTag::Tuple => 1,      // just header (elements inline)
            BoxedSubTag::Float => 1,      // just header (float inline after header)
            BoxedSubTag::Binary => 2,     // header + binary header (size/flags)
            BoxedSubTag::Map => 1,        // header + key-value pairs inline
            BoxedSubTag::Fun => 3,        // header + fun data (old_index, old_uniq, num_free)
            BoxedSubTag::BigInteger => 1, // header + digit array
            BoxedSubTag::Ref => 1,        // header + ref data
        }
    }

    /// Get the total size in words for this type
    pub fn total_size(&self, element_count: u32) -> u32 {
        self.header_words() + element_count
    }
}

/// Cons cell layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader
/// - Word 1: Head term (box tag bits)
/// - Word 2: Tail term (list pointer or nil)
///
/// For a proper list, the tail is either another cons or nil.
pub const fn cons_header() -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Cons, 3)
}

/// Tuple layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader with arity
/// - Words 1-N: Tuple elements (each is a term)
///
/// The arity is stored in the header size field.
pub const fn tuple_header(arity: u32) -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Tuple, 1 + arity)
}

/// Float layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader
/// - Words 1-2: IEEE 754 double (raw u64 representation)
///
/// BEAM stores floats as 8-byte IEEE 754 doubles.
pub const fn float_header() -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Float, 3)
}

/// Binary layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader
/// - Word 1: Binary header (size in bytes, flags)
/// - Words 2-N: Binary data (byte array)
///
/// Binary data is always byte-aligned.
pub const fn binary_header(size_bytes: u32) -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Binary, 2 + (size_bytes + 7) / 8)
}

/// Map layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader with size (1 + 2*num_keys)
/// - Words 1-N: Key-value pairs interleaved
pub const fn map_header(num_keys: u32) -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Map, 1 + num_keys * 2)
}

/// Fun/closure layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader
/// - Word 1: Old index
/// - Word 2: Old uniq
/// - Word 3: Num free terms
/// - Words 4-N: Free term values
pub const fn fun_header(num_free: u32) -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Fun, 4 + num_free)
}

/// Big integer layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader
/// - Word 1: Sign bit + digit count
/// - Words 2-N: Digits (base 2^32 or 2^64)
pub const fn bigint_header(num_digits: u32) -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::BigInteger, 2 + num_digits)
}

/// Reference layout in heap
///
/// Heap layout:
/// - Word 0: BoxedHeader
/// - Word 1: Ref ID (28 bits) + creation
///
/// Used for distributed references (make_ref/0).
pub const fn ref_header() -> BoxedHeader {
    BoxedHeader::new(BoxedSubTag::Ref, 2)
}

/// Extract term tag from a word on the heap
pub fn extract_tag(word: u64) -> TermTag {
    let raw = word & 0x7;
    match raw {
        0 => TermTag::SmallInteger,
        1 => TermTag::Atom,
        2 => TermTag::Cons,
        3 => TermTag::Tuple,
        4 => TermTag::Float,
        5 => TermTag::Binary,
        6 => TermTag::Map,
        7 => TermTag::Fun,
        _ => TermTag::SmallInteger,
    }
}

/// Extract boxed sub-tag from a header word
pub fn extract_sub_tag(header: u64) -> BoxedSubTag {
    let bits = (header >> 3) & 0x1F;
    match bits {
        0 => BoxedSubTag::Cons,
        1 => BoxedSubTag::Tuple,
        2 => BoxedSubTag::Float,
        3 => BoxedSubTag::Binary,
        4 => BoxedSubTag::Map,
        5 => BoxedSubTag::Fun,
        6 => BoxedSubTag::BigInteger,
        7 => BoxedSubTag::Ref,
        _ => BoxedSubTag::Cons,
    }
}

/// Extract size in words from a header word
pub fn extract_size(header: u64) -> u32 {
    ((header >> 8) & 0xFFFFFF) as u32
}

/// Forwarding pointer marker for copied objects during GC
/// Objects that have been copied to to-space have this marker
/// followed by the new address
pub const FORWARD_MAGIC: u64 = 0xDEADBEEF_DEADBEEF;

/// Check if a word is a forwarding pointer
pub fn is_forwarding_pointer(word: u64) -> bool {
    word == FORWARD_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cons_header() {
        let header = cons_header();
        assert_eq!(header.sub_tag(), BoxedSubTag::Cons);
        assert_eq!(header.size(), 3);
        assert!(header.is_cons());
    }

    #[test]
    fn test_tuple_header() {
        let header = tuple_header(5);
        assert_eq!(header.sub_tag(), BoxedSubTag::Tuple);
        assert_eq!(header.size(), 6); // 1 header + 5 elements
        assert!(header.is_tuple());
    }

    #[test]
    fn test_float_header() {
        let header = float_header();
        assert_eq!(header.sub_tag(), BoxedSubTag::Float);
        assert_eq!(header.size(), 3);
        assert!(header.is_float());
    }

    #[test]
    fn test_binary_header() {
        let header = binary_header(100);
        assert_eq!(header.sub_tag(), BoxedSubTag::Binary);
        // 100 bytes = 13 words (100 + 7) / 8 = 13
        assert_eq!(header.size(), 2 + 13);
        assert!(header.is_binary());
    }

    #[test]
    fn test_map_header() {
        let header = map_header(10);
        assert_eq!(header.sub_tag(), BoxedSubTag::Map);
        assert_eq!(header.size(), 21); // 1 header + 10*2 key-value pairs
    }

    #[test]
    fn test_fun_header() {
        let header = fun_header(3);
        assert_eq!(header.sub_tag(), BoxedSubTag::Fun);
        assert_eq!(header.size(), 7); // 4 header + 3 free terms
    }

    #[test]
    fn test_bigint_header() {
        let header = bigint_header(10);
        assert_eq!(header.sub_tag(), BoxedSubTag::BigInteger);
        assert_eq!(header.size(), 12); // 2 header + 10 digits
    }

    #[test]
    fn test_ref_header() {
        let header = ref_header();
        assert_eq!(header.sub_tag(), BoxedSubTag::Ref);
        assert_eq!(header.size(), 2);
    }

    #[test]
    fn test_extract_sub_tag() {
        let header = tuple_header(5);
        assert_eq!(extract_sub_tag(header.raw()), BoxedSubTag::Tuple);
    }

    #[test]
    fn test_extract_size() {
        let header = map_header(5);
        assert_eq!(extract_size(header.raw()), 11);
    }

    #[test]
    fn test_forwarding_pointer() {
        assert!(is_forwarding_pointer(FORWARD_MAGIC));
        assert!(!is_forwarding_pointer(0));
        assert!(!is_forwarding_pointer(123));
    }

    #[test]
    fn test_boxed_sub_tag_header_words() {
        assert_eq!(BoxedSubTag::Cons.header_words(), 2);
        assert_eq!(BoxedSubTag::Tuple.header_words(), 1);
        assert_eq!(BoxedSubTag::Float.header_words(), 1);
        assert_eq!(BoxedSubTag::Binary.header_words(), 2);
        assert_eq!(BoxedSubTag::Map.header_words(), 1);
        assert_eq!(BoxedSubTag::Fun.header_words(), 3);
        assert_eq!(BoxedSubTag::BigInteger.header_words(), 1);
        assert_eq!(BoxedSubTag::Ref.header_words(), 1);
    }
}
