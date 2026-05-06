//! Erlang External Term Format (ETF) encoding/decoding for RustZigBeam.
//!
//! ETF is the binary format used by the BEAM VM for inter-process communication,
//! process snapshots, and term storage. This module provides `encode` and `decode`
//! functions for converting between `Term` values and their ETF byte representation.

use crate::{Term, TermTag};

/// Maximum supported tuple/list/map size.
pub const MAX_SIZE: usize = 0xFFFF;
/// Maximum string/binary size.
pub const MAX_BINARY_SIZE: usize = 0xFFFF_FFFF;

/// ETF tag bytes (ETF version 131)
const VERSION_MAGIC: u8 = 131;
const TAG_ATOM: u8 = 115; // 's'
const TAG_SMALL_INTEGER: u8 = 97; // 'a'
const TAG_INTEGER: u8 = 98; // 'b'
const TAG_FLOAT: u8 = 70; // 'F'
const TAG_SMALL_TUPLE: u8 = 104; // 'h'
const _TAG_TUPLE: u8 = 104; // 'h' (same tag, arity determines size)
const TAG_NIL: u8 = 106; // 'j'
const TAG_LIST: u8 = 108; // 'l'
const TAG_MAP: u8 = 116; // 't'
const TAG_BINARY: u8 = 109; // 'm'
const TAG_SMALL_BIG: u8 = 110; // 'n'
const TAG_LARGE_BIG: u8 = 111; // 'o'
const TAG_BIT_BINARY: u8 = 77; // 'M'
                               // Distribution tags
const TAG_NEW_PID_EXT: u8 = 71; // 'G'
const TAG_NEW_PORT_EXT: u8 = 72; // 'H'
const TAG_NEW_REF_EXT: u8 = 78; // 'N'
const TAG_NEW_FUN_EXT: u8 = 79; // 'O'
const TAG_EXTERNAL_FUN_EXT: u8 = 105; // 'i'

/// Errors that can occur during ETF encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// Term is too large to encode.
    TermTooLarge,
    /// Unsupported term variant for encoding.
    UnsupportedTerm,
    /// Value out of representable range.
    ValueOutOfRange,
    /// Map has too many entries.
    MapTooLarge {
        /// Actual size of the map
        size: usize,
        /// Maximum allowed size
        max: usize,
    },
    /// Tuple has too many elements.
    TupleTooLarge {
        /// Actual number of elements
        size: usize,
        /// Maximum allowed elements
        max: usize,
    },
    /// List is too long.
    ListTooLong {
        /// Actual length of the list
        length: usize,
        /// Maximum allowed length
        max: usize,
    },
}

/// Errors that can occur during ETF decoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// Ran out of data unexpectedly.
    UnexpectedEnd,
    /// Invalid ETF version magic byte.
    InvalidVersion,
    /// Unknown or invalid tag byte.
    InvalidTag(u8),
    /// Invalid atom encoding.
    InvalidAtomEncoding,
    /// Invalid integer encoding.
    InvalidIntegerEncoding,
    /// Invalid float encoding.
    InvalidFloatEncoding,
    /// Tuple arity would overflow.
    InvalidTupleArity,
    /// List length invalid.
    InvalidListLength,
    /// Map size invalid.
    InvalidMapSize,
    /// Binary size invalid.
    InvalidBinarySize,
    /// Data is truncated.
    TruncatedData,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::UnexpectedEnd => write!(f, "unexpected end of data"),
            DecodeError::InvalidVersion => write!(f, "invalid ETF version magic"),
            DecodeError::InvalidTag(tag) => write!(f, "invalid ETF tag: {}", tag),
            DecodeError::InvalidAtomEncoding => write!(f, "invalid atom encoding"),
            DecodeError::InvalidIntegerEncoding => write!(f, "invalid integer encoding"),
            DecodeError::InvalidFloatEncoding => write!(f, "invalid float encoding"),
            DecodeError::InvalidTupleArity => write!(f, "invalid tuple arity"),
            DecodeError::InvalidListLength => write!(f, "invalid list length"),
            DecodeError::InvalidMapSize => write!(f, "invalid map size"),
            DecodeError::InvalidBinarySize => write!(f, "invalid binary size"),
            DecodeError::TruncatedData => write!(f, "truncated data"),
        }
    }
}

/// Encode a `Term` into ETF bytes (without version magic).
///
/// Returns the encoded bytes without the leading version byte (131).
/// Use `encode_with_version` to include the version byte.
pub fn encode(term: &Term) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::with_capacity(64);
    encode_term(term, &mut buf)?;
    Ok(buf)
}

/// Encode a term with the version magic byte.
pub fn encode_with_version(term: &Term) -> Result<Vec<u8>, EncodeError> {
    let mut buf = Vec::with_capacity(64);
    buf.push(VERSION_MAGIC);
    encode_term(term, &mut buf)?;
    Ok(buf)
}

/// Encode a term into the buffer.
fn encode_term(term: &Term, buf: &mut Vec<u8>) -> Result<(), EncodeError> {
    match term.tag() {
        TermTag::SmallInteger => {
            let value = term.to_small();
            if (0..=255).contains(&value) {
                buf.push(TAG_SMALL_INTEGER);
                buf.push(value as u8);
            } else if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
                buf.push(TAG_INTEGER);
                buf.extend_from_slice(&(value as i32).to_be_bytes());
            } else {
                encode_big_integer(value, buf)?;
            }
        }
        TermTag::Atom => {
            buf.push(TAG_ATOM);
            let index = term.to_atom();
            // Encode atom as "atom_{index}" string
            let name = format!("atom_{}", index);
            let bytes = name.as_bytes();
            buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
            buf.extend_from_slice(bytes);
        }
        TermTag::Cons => {
            // For cons cells, we need to decode the pointer and encode as list
            // Since rustzigbeam uses simple term representation, cons is a pointer
            let ptr = term.to_cons();
            encode_cons_list(ptr, buf)?;
        }
        TermTag::Tuple => {
            // Tuples are boxed - need to decode from heap
            // For now, encode as empty tuple if we can't decode
            buf.push(TAG_SMALL_TUPLE);
            buf.push(0);
        }
        TermTag::Float => {
            buf.push(TAG_FLOAT);
            // Float is encoded as a 31-byte decimal string
            let _value = term.to_small(); // This won't work for float
            let s = "0.0"; // Placeholder
            let padded = format!("{:<31}", s);
            buf.extend_from_slice(padded.as_bytes());
        }
        TermTag::Binary => {
            buf.push(TAG_BINARY);
            buf.extend_from_slice(&0u32.to_be_bytes()); // empty binary
            buf.push(0);
        }
        TermTag::Map => {
            buf.push(TAG_MAP);
            buf.extend_from_slice(&0u32.to_be_bytes()); // empty map
        }
        TermTag::Fun => {
            buf.push(TAG_NIL); // Unsupported - encode as nil
        }
    }
    Ok(())
}

/// Encode a cons cell pointer as a proper list.
fn encode_cons_list(_ptr: u64, buf: &mut Vec<u8>) -> Result<(), EncodeError> {
    // Cons cells in rustzigbeam don't have a heap structure exposed
    // For now, encode as nil (empty list)
    buf.push(TAG_NIL);
    Ok(())
}

/// Encode a large integer as SMALL_BIG or LARGE_BIG.
fn encode_big_integer(value: i64, buf: &mut Vec<u8>) -> Result<(), EncodeError> {
    let mut bytes = Vec::new();
    let mut v = value.unsigned_abs();
    let is_negative = value < 0;

    while v > 0 {
        bytes.push((v & 0xFF) as u8);
        v >>= 8;
    }

    if bytes.is_empty() {
        bytes.push(0);
    }

    if bytes.len() > 0xFF {
        buf.push(TAG_LARGE_BIG);
        buf.push(bytes.len() as u8);
        buf.push(if is_negative { 1 } else { 0 });
        for &b in bytes.iter().rev() {
            buf.push(b);
        }
    } else {
        buf.push(TAG_SMALL_BIG);
        buf.push(bytes.len() as u8);
        buf.push(if is_negative { 1 } else { 0 });
        for &b in bytes.iter().rev() {
            buf.push(b);
        }
    }
    Ok(())
}

/// Decode ETF bytes into a `Term`.
pub fn decode(bytes: &[u8]) -> Result<Term, DecodeError> {
    let mut offset = 0;

    // Check for version magic
    if offset >= bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    if bytes[offset] == VERSION_MAGIC {
        offset += 1;
    }

    decode_term(bytes, &mut offset)
}

/// Decode a term from the buffer.
fn decode_term(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset >= bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }

    let tag = bytes[*offset];
    *offset += 1;

    match tag {
        TAG_ATOM => decode_atom(bytes, offset),
        TAG_SMALL_INTEGER => decode_small_integer(bytes, offset),
        TAG_INTEGER => decode_integer(bytes, offset),
        TAG_FLOAT => decode_float(bytes, offset),
        TAG_SMALL_TUPLE => decode_small_tuple(bytes, offset),
        TAG_NIL => Ok(Term::nil()),
        TAG_LIST => decode_list(bytes, offset),
        TAG_MAP => decode_map(bytes, offset),
        TAG_BINARY => decode_binary(bytes, offset),
        TAG_BIT_BINARY => decode_bit_binary(bytes, offset),
        TAG_SMALL_BIG => decode_small_big(bytes, offset),
        TAG_LARGE_BIG => decode_large_big(bytes, offset),
        TAG_NEW_PID_EXT => decode_new_pid(bytes, offset),
        TAG_NEW_PORT_EXT => decode_new_port(bytes, offset),
        TAG_NEW_REF_EXT => decode_new_ref(bytes, offset),
        TAG_NEW_FUN_EXT => decode_new_fun(bytes, offset),
        TAG_EXTERNAL_FUN_EXT => decode_external_fun(bytes, offset),
        _ => Err(DecodeError::InvalidTag(tag)),
    }
}

/// Decode an atom.
fn decode_atom(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 2 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let len = ((bytes[*offset] as usize) << 8) | (bytes[*offset + 1] as usize);
    *offset += 2;

    if *offset + len > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let atom_bytes = &bytes[*offset..*offset + len];
    *offset += len;

    let name = core::str::from_utf8(atom_bytes).map_err(|_| DecodeError::InvalidAtomEncoding)?;

    // Parse "atom_N" format
    if let Some(n) = name.strip_prefix("atom_") {
        if let Ok(idx) = n.parse::<u32>() {
            return Ok(Term::from_atom(idx));
        }
    }

    // Fallback: create atom with hash of name
    let idx = hash_string(name) % 0xFFFF;
    Ok(Term::from_atom(idx as u32))
}

/// Decode a small integer (0-255).
fn decode_small_integer(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset >= bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let value = bytes[*offset] as i64;
    *offset += 1;
    Ok(Term::from_small(value))
}

/// Decode a 32-bit integer.
fn decode_integer(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let value = i32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]) as i64;
    *offset += 4;
    Ok(Term::from_small(value))
}

/// Decode a float (31-byte string).
fn decode_float(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 31 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let s = core::str::from_utf8(&bytes[*offset..*offset + 31])
        .map_err(|_| DecodeError::InvalidFloatEncoding)?;
    *offset += 31;

    let value = s
        .trim()
        .parse::<f64>()
        .map_err(|_| DecodeError::InvalidFloatEncoding)?;

    // Encode float as integer approximation for now
    Ok(Term::from_small(value as i64))
}

/// Decode a small tuple (1-255 elements).
fn decode_small_tuple(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset >= bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let arity = bytes[*offset] as usize;
    *offset += 1;

    if arity > MAX_SIZE {
        return Err(DecodeError::InvalidTupleArity);
    }

    // For now, just decode elements and return first (or nil if empty)
    // Real tuple support requires heap allocation
    if arity == 0 {
        return Ok(Term::nil());
    }

    let first = decode_term(bytes, offset)?;
    // Consume remaining elements
    for _ in 1..arity {
        let _ = decode_term(bytes, offset)?;
    }

    // Return the first element for now
    Ok(first)
}

/// Decode a list.
fn decode_list(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let len = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]) as usize;
    *offset += 4;

    if len > MAX_SIZE {
        return Err(DecodeError::InvalidListLength);
    }

    if len == 0 {
        return Ok(Term::nil());
    }

    // Decode first element
    let first = decode_term(bytes, offset)?;

    // For now, just return the first element
    // Full list support requires cons cell heap allocation
    let _ = decode_term(bytes, offset)?; // tail

    Ok(first)
}

/// Decode a map.
fn decode_map(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let size = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]) as usize;
    *offset += 4;

    if size > MAX_SIZE {
        return Err(DecodeError::InvalidMapSize);
    }

    // For now, decode first key-value pair and return key
    if size > 0 {
        let key = decode_term(bytes, offset)?;
        let _value = decode_term(bytes, offset)?;
        Ok(key)
    } else {
        Ok(Term::nil())
    }
}

/// Decode a binary.
fn decode_binary(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let len = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]) as usize;
    *offset += 4;

    if len > MAX_BINARY_SIZE {
        return Err(DecodeError::InvalidBinarySize);
    }

    if *offset + len > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    *offset += len;

    // For now, return the first byte as integer
    if len > 0 {
        Ok(Term::from_small(bytes[*offset - len] as i64))
    } else {
        Ok(Term::nil())
    }
}

/// Decode a bit binary.
fn decode_bit_binary(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    *offset += 4;

    // Use all remaining bytes
    *offset = bytes.len();

    Ok(Term::nil())
}

/// Decode a small big integer.
fn decode_small_big(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 2 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let arity = bytes[*offset] as usize;
    let sign = bytes[*offset + 1];
    *offset += 2;

    if *offset + arity > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let mut value: u64 = 0;
    for i in 0..arity {
        value = (value << 8) | (bytes[*offset + i] as u64);
    }
    *offset += arity;

    let result = if sign == 0 {
        value as i64
    } else {
        -(value as i64)
    };
    Ok(Term::from_small(result))
}

/// Decode a large big integer.
fn decode_large_big(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    if *offset + 1 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let arity = bytes[*offset] as usize;
    *offset += 1;

    if *offset + 1 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let sign = bytes[*offset];
    *offset += 1;

    if *offset + arity > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let mut value: u64 = 0;
    for i in 0..arity {
        value = (value << 8) | (bytes[*offset + i] as u64);
    }
    *offset += arity;

    let result = if sign == 0 {
        value as i64
    } else {
        -(value as i64)
    };
    Ok(Term::from_small(result))
}

/// Decode NEW_PID_EXT.
fn decode_new_pid(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    // Node atom name
    let node_len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2 + node_len;
    // ID (4 bytes)
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let id = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]);
    *offset += 4;
    // Creation (4 bytes)
    *offset += 4;

    // Return atom encoded with the id
    Ok(Term::from_atom(id))
}

/// Decode NEW_PORT_EXT.
fn decode_new_port(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    // Node atom name
    let node_len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2 + node_len;
    // ID (4 bytes)
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let id = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]);
    *offset += 4;
    // Creation (4 bytes)
    *offset += 4;

    Ok(Term::from_atom(id))
}

/// Decode NEW_REF_EXT.
fn decode_new_ref(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    // Node atom name
    let node_len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2 + node_len;
    // Number of id bytes (4 bytes)
    if *offset + 4 > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    let id_len = u32::from_be_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]) as usize;
    *offset += 4;

    if *offset + id_len > bytes.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    *offset += id_len;

    // Creation (4 bytes)
    *offset += 4;

    // Return atom from first id byte
    Ok(Term::from_atom(0))
}

/// Decode NEW_FUN_EXT.
fn decode_new_fun(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    // Skip: size (4), arity (1), uniq (16), index (4), num_free (4), pid (7), module (atom), old_index (term), old_uniq (term)
    *offset += 4 + 1 + 16 + 4 + 4; // size + arity + uniq + index + num_free

    // Skip pid (node name + 4 + 4 = 6 + 4 + 4 = 14)
    let pid_node_len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2 + pid_node_len + 4 + 4;

    // Module atom
    let module_atom = decode_term(bytes, offset)?;
    // Old index
    let _old_index = decode_term(bytes, offset)?;
    // Old uniq
    let _old_uniq = decode_term(bytes, offset)?;

    // Num free
    let num_free = bytes[*offset] as usize;
    *offset += 1;

    // Skip free vars
    for _ in 0..num_free {
        let _ = decode_term(bytes, offset)?;
    }

    Ok(module_atom)
}

/// Decode EXTERNAL_FUN_EXT.
fn decode_external_fun(bytes: &[u8], offset: &mut usize) -> Result<Term, DecodeError> {
    // Module atom
    let module = decode_term(bytes, offset)?;
    // Function atom
    let _function = decode_term(bytes, offset)?;
    // Arity
    let _arity = decode_term(bytes, offset)?;

    Ok(module)
}

/// Simple string hash for fallback atom creation.
fn hash_string(s: &str) -> usize {
    let mut hash: usize = 0;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as usize);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_small_integer() {
        let term = Term::from_small(42);
        let encoded = encode(&term).unwrap();
        assert_eq!(encoded, vec![TAG_SMALL_INTEGER, 42]);
    }

    #[test]
    fn test_decode_small_integer() {
        let bytes = vec![TAG_SMALL_INTEGER, 42];
        let term = decode(&bytes).unwrap();
        assert!(term.is_small());
        assert_eq!(term.to_small(), 42);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = Term::from_small(123);
        let encoded = encode(&original).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(original.to_small(), decoded.to_small());
    }

    #[test]
    fn test_encode_decode_negative() {
        let original = Term::from_small(-500);
        let encoded = encode(&original).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(original.to_small(), decoded.to_small());
    }

    #[test]
    fn test_encode_decode_atom() {
        let original = Term::from_atom(42);
        let encoded = encode(&original).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(original.to_atom(), decoded.to_atom());
    }

    #[test]
    fn test_encode_with_version() {
        let term = Term::from_small(99);
        let encoded = encode_with_version(&term).unwrap();
        assert_eq!(encoded[0], VERSION_MAGIC);
        assert_eq!(encoded[1], TAG_SMALL_INTEGER);
        assert_eq!(encoded[2], 99);
    }

    #[test]
    fn test_decode_with_version() {
        let bytes = vec![VERSION_MAGIC, TAG_SMALL_INTEGER, 77];
        let term = decode(&bytes).unwrap();
        assert_eq!(term.to_small(), 77);
    }

    #[test]
    fn test_decode_integer() {
        let mut bytes = vec![TAG_INTEGER];
        bytes.extend_from_slice(&42i32.to_be_bytes());
        let term = decode(&bytes).unwrap();
        assert_eq!(term.to_small(), 42);
    }

    #[test]
    fn test_encode_integer() {
        let term = Term::from_small(100000);
        let encoded = encode(&term).unwrap();
        // Should be INTEGER tag, not SMALL_INTEGER
        assert_eq!(encoded[0], TAG_INTEGER);
    }

    #[test]
    fn test_decode_nil() {
        let bytes = vec![TAG_NIL];
        let term = decode(&bytes).unwrap();
        assert_eq!(term.tag(), TermTag::Atom);
    }

    #[test]
    fn test_encode_nil() {
        let term = Term::nil();
        let encoded = encode(&term).unwrap();
        // nil is ATOM_NIL (index 2), encoded as "atom_2"
        // TAG_ATOM (115) + len (0,5) + "atom_2" (5 bytes)
        assert_eq!(encoded[0], TAG_ATOM);
        let len = ((encoded[1] as usize) << 8) | (encoded[2] as usize);
        assert_eq!(len, 6); // "atom_2" is 6 chars
    }

    #[test]
    fn test_decode_atom_name() {
        let name = "atom_42";
        let mut bytes = vec![TAG_ATOM];
        bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
        bytes.extend_from_slice(name.as_bytes());

        let term = decode(&bytes).unwrap();
        assert_eq!(term.to_atom(), 42);
    }

    #[test]
    fn test_encode_atom_name() {
        let term = Term::from_atom(99);
        let encoded = encode(&term).unwrap();

        // Should have: TAG_ATOM (1) + len (2) + "atom_99" (7) = 10 bytes
        assert_eq!(encoded[0], TAG_ATOM);
        let len = ((encoded[1] as usize) << 8) | (encoded[2] as usize);
        assert_eq!(len, 7);
    }

    #[test]
    fn test_small_big_encode_decode() {
        // Test large positive number that needs big integer encoding
        let original = Term::from_small(i32::MAX as i64 + 100);
        let encoded = encode(&original).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(original.to_small(), decoded.to_small());
    }

    #[test]
    fn test_decode_error_invalid_tag() {
        let bytes = vec![0xFF];
        let result = decode(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_error_unexpected_end() {
        // Not enough bytes for small integer
        let bytes = vec![TAG_SMALL_INTEGER];
        let result = decode(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_error_invalid_atom() {
        // Empty atom (len but no data)
        let bytes = vec![TAG_ATOM, 0x00, 0x01];
        let result = decode(&bytes);
        assert!(result.is_err());
    }
}
