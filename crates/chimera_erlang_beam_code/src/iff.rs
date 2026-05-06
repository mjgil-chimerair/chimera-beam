//! IFF container parsing for BEAM files.
//!
//! BEAM files use the IFF (Interchange File Format) container,
//! specifically the FOR1 variant with a 12-byte header.

use std::fmt;

/// Magic bytes for BEAM IFF container
pub const FOR1_MAGIC: [u8; 4] = *b"FOR1";

/// Magic bytes for BEAM container identifier
pub const BEAM_MAGIC: [u8; 4] = *b"BEAM";

/// IFF header size (8 bytes: 4 magic + 4 size)
pub const IFF_HEADER_SIZE: usize = 8;

/// BEAM container header size (12 bytes: FOR1 + size)
pub const BEAM_HEADER_SIZE: usize = 12;

/// Chunk header size (4 bytes tag + 4 bytes length)
pub const CHUNK_HEADER_SIZE: usize = 8;

/// Maximum chunk size to prevent allocation abuse (256 MB)
const MAX_CHUNK_SIZE: usize = 256 * 1024 * 1024;

/// A parsed BEAM IFF container
#[derive(Debug, Clone)]
pub struct Container {
    pub chunk_data_len: u32,
    pub chunks: Vec<Chunk>,
}

/// A single IFF chunk within a container
#[derive(Debug, Clone)]
pub struct Chunk {
    pub tag: ChunkTag,
    pub data: Vec<u8>,
}

/// A validated 4-byte ASCII chunk tag
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkTag(pub [u8; 4]);

impl ChunkTag {
    pub fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> [u8; 4] {
        self.0
    }

    pub fn as_str(&self) -> String {
        let bytes = self
            .0
            .map(|b| if (0x20..0x7f).contains(&b) { b } else { b'?' });
        String::from_utf8_lossy(&bytes).to_string()
    }
}

impl fmt::Display for ChunkTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Parse a BEAM IFF container from bytes (FOR1 format)
///
/// FOR1 Container Layout:
/// - Bytes 0-3:   FOR1 magic
/// - Bytes 4-7:   Size (u32, total size from BEAM to end)
/// - Bytes 8+:    BEAM container identifier + chunks
///
/// BEAM container has no explicit chunk count - chunks are read until EOF.
pub fn parse_container(data: &[u8]) -> Result<Container, LoadError> {
    if data.len() < 12 {
        return Err(LoadError::Truncated);
    }

    let magic = <[u8; 4]>::try_from(&data[0..4]).map_err(|_| LoadError::Truncated)?;
    if magic != FOR1_MAGIC {
        return Err(LoadError::InvalidMagic(magic));
    }

    let chunk_data_len =
        u32::from_be_bytes(<[u8; 4]>::try_from(&data[4..8]).map_err(|_| LoadError::Truncated)?);

    // Verify BEAM container magic
    let beam_magic = <[u8; 4]>::try_from(&data[8..12]).map_err(|_| LoadError::Truncated)?;
    if beam_magic != BEAM_MAGIC {
        return Err(LoadError::InvalidMagic(beam_magic));
    }

    // BEAM container has no chunk count - read chunks until EOF
    let mut chunks = Vec::new();
    let mut offset = BEAM_HEADER_SIZE; // Start after FOR1 + size + BEAM

    while offset + CHUNK_HEADER_SIZE <= data.len() {
        let tag_bytes =
            <[u8; 4]>::try_from(&data[offset..offset + 4]).map_err(|_| LoadError::Truncated)?;
        let tag = ChunkTag::new(tag_bytes);
        offset += 4;

        let chunk_len = u32::from_be_bytes(
            <[u8; 4]>::try_from(&data[offset..offset + 4]).map_err(|_| LoadError::Truncated)?,
        );
        offset += 4;

        let chunk_len_usize = chunk_len as usize;
        if chunk_len_usize > MAX_CHUNK_SIZE {
            return Err(LoadError::ChunkTooLarge {
                declared: chunk_len_usize,
                max: MAX_CHUNK_SIZE,
            });
        }

        if offset + chunk_len_usize > data.len() {
            return Err(LoadError::ChunkDataTooShort {
                expected: chunk_len_usize,
                got: data.len().saturating_sub(offset),
            });
        }

        let chunk_data = data[offset..offset + chunk_len_usize].to_vec();
        offset += chunk_len_usize;

        chunks.push(Chunk {
            tag,
            data: chunk_data,
        });
    }

    Ok(Container {
        chunk_data_len,
        chunks,
    })
}

/// Load error types for IFF parsing
#[derive(Debug)]
pub enum LoadError {
    Truncated,
    InvalidMagic([u8; 4]),
    ChunkTooLarge { declared: usize, max: usize },
    ChunkDataTooShort { expected: usize, got: usize },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Truncated => write!(f, "truncated data"),
            LoadError::InvalidMagic(m) => write!(f, "invalid magic: {:?}", m),
            LoadError::ChunkTooLarge { declared, max } => {
                write!(f, "chunk too large: {} bytes (max {})", declared, max)
            }
            LoadError::ChunkDataTooShort { expected, got } => {
                write!(f, "chunk data too short: expected {} got {}", expected, got)
            }
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_container_valid() {
        // Build valid FOR1 container with BEAM header but no chunks:
        // FOR1 + size + BEAM
        let mut data = Vec::new();
        data.extend_from_slice(b"FOR1");
        data.extend_from_slice(&8u32.to_be_bytes()); // size = 8
        data.extend_from_slice(b"BEAM");
        // No chunks after this - empty container

        let container = parse_container(&data).unwrap();
        assert_eq!(container.chunks.len(), 0);
    }

    #[test]
    fn test_parse_container_with_chunk() {
        // Build proper BEAM IFF container with FOR1 format:
        // FOR1 + size(8) + BEAM + chunk("Test" = 5 bytes)
        let mut data = Vec::new();
        data.extend_from_slice(b"FOR1");
        data.extend_from_slice(&8u32.to_be_bytes()); // size = 8
        data.extend_from_slice(b"BEAM");
        // Chunk: "Test" (4) + 5 (4) + "hello" (5) = 13 bytes
        data.extend_from_slice(b"Test"); // tag
        data.extend_from_slice(&5u32.to_be_bytes()); // size = 5
        data.extend_from_slice(b"hello"); // data = 5 bytes

        let container = parse_container(&data).unwrap();
        assert_eq!(container.chunks.len(), 1);
        assert_eq!(container.chunks[0].tag.as_str(), "Test");
        assert_eq!(&container.chunks[0].data, b"hello");
    }

    #[test]
    fn test_chunk_tag_display() {
        let tag = ChunkTag(*b"Test");
        assert_eq!(tag.as_str(), "Test");
    }
}
