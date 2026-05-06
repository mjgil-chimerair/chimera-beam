//! Symbol table parsing for BEAM files.
//!
//! Handles export tables (ExpT), import tables (ImpT), and local tables.

use super::atoms::AtomTable;
use super::iff::Chunk;
use super::mfa::Mfa;

/// A decoded symbol table entry
#[derive(Debug, Clone)]
pub struct SymbolEntry {
    pub index: u32,
    pub mfa: Mfa,
    pub module_name: Option<String>,
    pub function_name: Option<String>,
}

impl SymbolEntry {
    pub fn module_index(&self) -> u32 {
        self.mfa.module_index()
    }

    pub fn function_index(&self) -> u32 {
        self.mfa.function_index()
    }

    pub fn arity(&self) -> u8 {
        self.mfa.arity()
    }
}

/// A decoded symbol table
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    entries: Vec<SymbolEntry>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, index: u32) -> Option<&SymbolEntry> {
        self.entries.get(index as usize)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, SymbolEntry> {
        self.entries.iter()
    }

    pub fn all(&self) -> &[SymbolEntry] {
        &self.entries
    }
}

impl IntoIterator for SymbolTable {
    type Item = SymbolEntry;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

/// Decode export table (ExpT chunk)
/// Decode export table (ExpT chunk)
/// Custom format: u32 count + (module_index u32 + function_index u32 + arity u8) per entry
pub fn decode_export_table(chunk: &Chunk, atoms: &AtomTable) -> Result<SymbolTable, LoadError> {
    let data = &chunk.data;
    if data.len() < 4 {
        return Err(LoadError::InvalidExportTable);
    }

    let mut offset = 0;
    let num_exports = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    let mut table = SymbolTable {
        entries: Vec::with_capacity(num_exports),
    };

    for i in 0..num_exports {
        let entry_idx = i as u32;
        if offset + 9 > data.len() {
            return Err(LoadError::TruncatedSymbolEntry {
                table: "ExpT".to_string(),
                entry: entry_idx,
                need: 9,
                got: data.len().saturating_sub(offset),
            });
        }

        // module_index: atom index for module name (4 bytes)
        let module_index = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        // function_index: atom index for function name (4 bytes)
        let function_index = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        let arity = data[offset];
        offset += 1;

        // Validate atom indices
        let module_name = atoms.get(module_index).map(|a| a.name.clone());
        let function_name = atoms.get(function_index).map(|a| a.name.clone());

        table.entries.push(SymbolEntry {
            index: entry_idx,
            mfa: Mfa::new(module_index, function_index, arity),
            module_name,
            function_name,
        });
    }

    Ok(table)
}

/// Decode import table (ImpT chunk)
pub fn decode_import_table(chunk: &Chunk, atoms: &AtomTable) -> Result<SymbolTable, LoadError> {
    let data = &chunk.data;
    if data.len() < 4 {
        return Err(LoadError::InvalidImportTable);
    }

    let mut offset = 0;
    let num_imports = u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    let mut table = SymbolTable {
        entries: Vec::with_capacity(num_imports),
    };

    for i in 0..num_imports {
        let entry_idx = i as u32;
        if offset + 9 > data.len() {
            return Err(LoadError::TruncatedSymbolEntry {
                table: "ImpT".to_string(),
                entry: entry_idx,
                need: 9,
                got: data.len().saturating_sub(offset),
            });
        }

        let module_index = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        let function_index = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        let arity = data[offset];
        offset += 1;

        let module_name = atoms.get(module_index).map(|a| a.name.clone());
        let function_name = atoms.get(function_index).map(|a| a.name.clone());

        table.entries.push(SymbolEntry {
            index: entry_idx,
            mfa: Mfa::new(module_index, function_index, arity),
            module_name,
            function_name,
        });
    }

    Ok(table)
}

/// Load error types for symbol parsing
#[derive(Debug)]
pub enum LoadError {
    InvalidExportTable,
    TruncatedSymbolEntry {
        table: String,
        entry: u32,
        need: usize,
        got: usize,
    },
    InvalidImportTable,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::InvalidExportTable => write!(f, "invalid export table"),
            LoadError::TruncatedSymbolEntry {
                table,
                entry,
                need,
                got,
            } => {
                write!(
                    f,
                    "truncated symbol entry {}[{}]: need {} got {}",
                    table, entry, need, got
                )
            }
            LoadError::InvalidImportTable => write!(f, "invalid import table"),
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_table_empty() {
        let table = SymbolTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_symbol_table_iter() {
        use crate::iff::Chunk;
        use crate::{AtomEntry, AtomTable};

        let body = vec![
            0x00, 0x00, 0x00, 0x01, // count = 1
            0x00, 0x00, 0x00, 0x00, // module_index = 0
            0x00, 0x00, 0x00, 0x01, // function_index = 1
            0x02, // arity = 2
        ];
        let chunk = Chunk {
            tag: crate::iff::ChunkTag(*b"ExpT"),
            data: body,
        };
        let mut atoms = AtomTable::new();
        atoms.entries.push(AtomEntry {
            index: 0,
            name: "mod".to_string(),
            is_utf8: true,
        });
        atoms.entries.push(AtomEntry {
            index: 1,
            name: "fun".to_string(),
            is_utf8: true,
        });

        let table = decode_export_table(&chunk, &atoms).unwrap();
        assert_eq!(table.len(), 1);

        let entry = table.get(0).unwrap();
        assert_eq!(entry.module_index(), 0);
        assert_eq!(entry.function_index(), 1);
        assert_eq!(entry.arity(), 2);
        assert_eq!(entry.module_name.as_deref(), Some("mod"));
        assert_eq!(entry.function_name.as_deref(), Some("fun"));
    }

    #[test]
    fn test_symbol_entry_mfa() {
        let mfa = Mfa::new(5, 10, 3);
        assert_eq!(mfa.module_index(), 5);
        assert_eq!(mfa.function_index(), 10);
        assert_eq!(mfa.arity(), 3);
    }

    #[test]
    fn test_symbol_table_into_iter() {
        use crate::iff::Chunk;
        use crate::{AtomEntry, AtomTable};

        let body = vec![
            0x00, 0x00, 0x00, 0x01, // count = 1
            0x00, 0x00, 0x00, 0x00, // module_index = 0
            0x00, 0x00, 0x00, 0x01, // function_index = 1
            0x02, // arity = 2
        ];
        let chunk = Chunk {
            tag: crate::iff::ChunkTag(*b"ImpT"),
            data: body,
        };
        let mut atoms = AtomTable::new();
        atoms.entries.push(AtomEntry {
            index: 0,
            name: "mod".to_string(),
            is_utf8: true,
        });
        atoms.entries.push(AtomEntry {
            index: 1,
            name: "fun".to_string(),
            is_utf8: true,
        });

        let table = decode_import_table(&chunk, &atoms).unwrap();
        let entries: Vec<_> = table.into_iter().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].arity(), 2);
    }
}
