//! IFF (Interchange File Format) handling for BEAM files.
//!
//! BEAM files use the IFF container format for storing chunks.

use std::io::{Read, Write, Cursor};

/// IFF chunk identifier (4 bytes)
pub type ChunkId = [u8; 4];

/// An IFF chunk
#[derive(Debug, Clone)]
pub struct Chunk {
    /// 4-byte chunk type (e.g., b"Atom", b"Code")
    pub kind: ChunkId,
    /// Chunk data
    pub data: Vec<u8>,
}

impl Chunk {
    /// Create a new chunk
    pub fn new(kind: ChunkId, data: Vec<u8>) -> Self {
        Chunk { kind, data }
    }

    /// Get the chunk type as a string
    pub fn kind_str(&self) -> String {
        String::from_utf8_lossy(&self.kind).to_string()
    }

    /// Get the chunk data length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if chunk is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Write chunk to a writer
    pub fn write_to<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        // Write 4-byte type
        w.write_all(&self.kind)?;
        // Write 4-byte length (big-endian)
        let len = self.data.len() as u32;
        w.write_all(&len.to_be_bytes())?;
        // Write data
        w.write_all(&self.data)?;
        // Pad to even length
        if self.data.len() % 2 != 0 {
            w.write_all(&[0])?;
        }
        Ok(())
    }

    /// Read chunk from a reader
    pub fn read_from<R: Read>(r: &mut R) -> std::io::Result<Self> {
        let mut header = [0u8; 8];
        r.read_exact(&mut header)?;

        let kind = [header[0], header[1], header[2], header[3]];
        let len = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);

        // Data length is rounded up to even
        let data_len = (len as usize + 1) & !1;
        let mut data = vec![0u8; data_len];
        r.read_exact(&mut data)?;
        data.truncate(len as usize);

        Ok(Chunk { kind, data })
    }
}

/// BEAM file magic number
pub const BEAM_MAGIC: &[u8; 4] = b"BEAM";

/// BEAM file container (IFF format)
#[derive(Debug, Clone)]
pub struct BeamFile {
    /// File chunks
    pub chunks: Vec<Chunk>,
}

impl BeamFile {
    /// Parse a BEAM file from bytes
    pub fn parse(data: &[u8]) -> std::io::Result<Self> {
        let mut r = Cursor::new(data);

        // Read and verify magic
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)?;
        if &magic != BEAM_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid BEAM file: bad magic",
            ));
        }

        // Read chunks
        let mut chunks = Vec::new();
        while r.position() < data.len() as u64 {
            chunks.push(Chunk::read_from(&mut r)?);
        }

        Ok(BeamFile { chunks })
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        let mut w = Vec::new();

        // Write magic
        w.write_all(BEAM_MAGIC)?;

        // Write chunks
        for chunk in &self.chunks {
            chunk.write_to(&mut w)?;
        }

        Ok(w)
    }

    /// Get a chunk by type
    pub fn get_chunk(&self, kind: &ChunkId) -> Option<&Chunk> {
        self.chunks.iter().find(|c| &c.kind == kind)
    }

    /// Get a chunk by type string
    pub fn get_chunk_str(&self, kind: &str) -> Option<&Chunk> {
        let mut kind_bytes = [0u8; 4];
        kind_bytes.copy_from_slice(kind.as_bytes());
        self.get_chunk(&kind_bytes)
    }

    /// Add or replace a chunk
    pub fn set_chunk(&mut self, chunk: Chunk) {
        if let Some(existing) = self.chunks.iter_mut().find(|c| c.kind == chunk.kind) {
            *existing = chunk;
        } else {
            self.chunks.push(chunk);
        }
    }

    /// Create a new empty BEAM file
    pub fn new() -> Self {
        BeamFile { chunks: Vec::new() }
    }
}

impl Default for BeamFile {
    fn default() -> Self {
        Self::new()
    }
}

/// Common BEAM chunk types
pub mod chunk_types {
    use super::ChunkId;

    /// Atom table chunk
    pub const ATOM: ChunkId = *b"Atom";
    /// Compiled code chunk
    pub const CODE: ChunkId = *b"Code";
    /// Export table chunk
    pub const EXPT: ChunkId = *b"ExpT";
    /// Import table chunk
    pub const IMPT: ChunkId = *b"ImpT";
    /// String table chunk
    pub const STRT: ChunkId = *b"StrT";
    /// Literal table chunk
    pub const LITT: ChunkId = *b"LitT";
    /// Line information chunk
    pub const LOCT: ChunkId = *b"LocT";
    /// Abstract syntax tree chunk
    pub const ABST: ChunkId = *b"AbsT";
    /// Compile info chunk
    pub const COMT: ChunkId = *b"ComT";
    /// Optional features chunk
    pub const OPT: ChunkId = *b"Opt ";
    /// Version chunk
    pub const VRSN: ChunkId = *b"Vrsn";
}

/// Chunk type constants for use in BEAM file creation
pub const CHUNK_ATOM: ChunkId = chunk_types::ATOM;
pub const CHUNK_CODE: ChunkId = chunk_types::CODE;
pub const CHUNK_EXPT: ChunkId = chunk_types::EXPT;
pub const CHUNK_IMPT: ChunkId = chunk_types::IMPT;
pub const CHUNK_STRT: ChunkId = chunk_types::STRT;
pub const CHUNK_LITT: ChunkId = chunk_types::LITT;
pub const CHUNK_LOCT: ChunkId = chunk_types::LOCT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_new() {
        let chunk = Chunk::new(*b"Test", vec![1, 2, 3]);
        assert_eq!(chunk.kind, *b"Test");
        assert_eq!(chunk.len(), 3);
    }

    #[test]
    fn test_chunk_kind_str() {
        let chunk = Chunk::new(*b"Atom", vec![]);
        assert_eq!(chunk.kind_str(), "Atom");
    }

    #[test]
    fn test_chunk_write_read() {
        let chunk = Chunk::new(*b"Test", vec![1, 2, 3, 4, 5]);

        let mut bytes = Vec::new();
        chunk.write_to(&mut bytes).unwrap();

        let mut r = Cursor::new(&bytes);
        let read_chunk = Chunk::read_from(&mut r).unwrap();

        assert_eq!(read_chunk.kind, *b"Test");
        assert_eq!(read_chunk.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_beam_file_parse_bad_magic() {
        let data = b"XXXX".to_vec();
        let result = BeamFile::parse(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_beam_file_parse_valid() {
        // Create a minimal valid BEAM file
        let mut file = BeamFile::new();
        file.set_chunk(Chunk::new(*b"Test", vec![1, 2, 3]));
        let bytes = file.to_bytes().unwrap();
        let parsed = BeamFile::parse(&bytes).unwrap();
        assert_eq!(parsed.chunks.len(), 1);
    }

    #[test]
    fn test_beam_file_new() {
        let file = BeamFile::new();
        assert!(file.chunks.is_empty());
    }

    #[test]
    fn test_beam_file_set_chunk() {
        let mut file = BeamFile::new();
        file.set_chunk(Chunk::new(*b"Test", vec![1, 2, 3]));

        assert!(file.get_chunk_str("Test").is_some());
        assert!(file.get_chunk_str("Othr").is_none()); // 4 chars to match ChunkId size
    }

    #[test]
    fn test_beam_file_to_bytes() {
        let mut file = BeamFile::new();
        file.set_chunk(Chunk::new(*b"Test", vec![1, 2, 3]));

        let bytes = file.to_bytes().unwrap();

        // Should start with BEAM magic
        assert_eq!(&bytes[0..4], b"BEAM");

        // Should be parseable
        let parsed = BeamFile::parse(&bytes).unwrap();
        assert_eq!(parsed.get_chunk_str("Test").unwrap().data, vec![1, 2, 3]);
    }

    #[test]
    fn test_chunk_types() {
        assert_eq!(chunk_types::ATOM, *b"Atom");
        assert_eq!(chunk_types::CODE, *b"Code");
        assert_eq!(chunk_types::EXPT, *b"ExpT");
    }
}