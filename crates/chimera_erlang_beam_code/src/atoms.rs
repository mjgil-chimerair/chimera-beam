//! Atom table parsing for BEAM files.
//!
//! Atoms are stored in the AtU8 chunk (UTF-8 atoms) or legacy Atom chunk.

use super::iff::Chunk;

/// A decoded atom table entry
#[derive(Debug, Clone)]
pub struct AtomEntry {
    pub index: u32,
    pub name: String,
    pub is_utf8: bool,
}

/// A decoded atom table
#[derive(Debug, Clone, Default)]
pub struct AtomTable {
    pub entries: Vec<AtomEntry>,
}

impl AtomTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, index: u32) -> Option<&AtomEntry> {
        self.entries.get(index as usize)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Decode UTF-8 atom chunk (AtU8)
pub fn decode_utf8_atom_chunk(chunk: &Chunk) -> Result<AtomTable, LoadError> {
    let data = &chunk.data;
    if data.len() < 4 {
        return Err(LoadError::InvalidAtomTable);
    }

    let mut offset = 0;
    let num_atoms = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    let mut atoms = AtomTable::new();

    for i in 0..num_atoms {
        let entry_idx = i as u32;
        if offset + 2 > data.len() {
            return Err(LoadError::TruncatedAtomEntry {
                entry: entry_idx,
                need: 2,
                got: data.len().saturating_sub(offset),
            });
        }

        let name_len = u16::from_be_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        if offset + name_len > data.len() {
            return Err(LoadError::TruncatedAtomEntry {
                entry: entry_idx,
                need: name_len,
                got: data.len().saturating_sub(offset),
            });
        }

        let name = String::from_utf8(data[offset..offset + name_len].to_vec()).map_err(|_| {
            LoadError::InvalidAtomEncoding {
                entry: entry_idx,
                reason: "invalid UTF-8".to_string(),
            }
        })?;
        offset += name_len;

        atoms.entries.push(AtomEntry {
            index: entry_idx,
            name,
            is_utf8: true,
        });
    }

    Ok(atoms)
}

/// Load error types for atom parsing
#[derive(Debug)]
pub enum LoadError {
    InvalidAtomTable,
    TruncatedAtomEntry { entry: u32, need: usize, got: usize },
    InvalidAtomEncoding { entry: u32, reason: String },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::InvalidAtomTable => write!(f, "invalid atom table"),
            LoadError::TruncatedAtomEntry { entry, need, got } => {
                write!(
                    f,
                    "truncated atom entry {}: need {} got {}",
                    entry, need, got
                )
            }
            LoadError::InvalidAtomEncoding { entry, reason } => {
                write!(f, "invalid atom encoding at entry {}: {}", entry, reason)
            }
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atom_table_decode() {
        let mut data = Vec::new();
        data.extend_from_slice(&(2u32).to_be_bytes());
        data.extend_from_slice(&(3u16).to_be_bytes());
        data.extend_from_slice(b"foo");
        data.extend_from_slice(&(3u16).to_be_bytes());
        data.extend_from_slice(b"bar");

        let chunk = Chunk {
            tag: crate::iff::ChunkTag(*b"AtU8"),
            data,
        };

        let atoms = decode_utf8_atom_chunk(&chunk).unwrap();
        assert_eq!(atoms.len(), 2);
        assert_eq!(atoms.get(0).unwrap().name, "foo");
        assert_eq!(atoms.get(1).unwrap().name, "bar");
    }

    #[test]
    fn test_atom_entry_is_utf8() {
        let entry = AtomEntry {
            index: 0,
            name: "test".to_string(),
            is_utf8: true,
        };
        assert!(entry.is_utf8);
    }
}
