//! Atom table for RustZigBeam.
//!
//! Provides dynamic atom interning with string lookup.
//! Atoms are unique identifiers for constants in the VM.

use std::collections::HashMap;
use std::sync::RwLock;

/// Maximum number of atoms allowed in the table
pub const MAX_ATOMS: usize = 1 << 20; // ~1 million atoms

/// Reserved atom indices (must match atoms module in lib.rs)
pub const RESERVED_ATOMS: usize = 18;

/// Atom table entry
#[derive(Debug, Clone)]
pub struct AtomEntry {
    /// Atom index
    pub index: u32,
    /// Atom name string
    pub name: String,
}

/// Thread-safe atom table for dynamic interning
pub struct AtomTable {
    /// Map from atom name to atom index
    name_to_index: RwLock<HashMap<String, u32>>,
    /// Map from atom index to atom name
    index_to_name: RwLock<HashMap<u32, String>>,
    /// Next available atom index
    next_index: RwLock<u32>,
}

impl Default for AtomTable {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomTable {
    /// Create a new atom table with reserved atoms pre-registered
    pub fn new() -> Self {
        let table = AtomTable {
            name_to_index: RwLock::new(HashMap::new()),
            index_to_name: RwLock::new(HashMap::new()),
            next_index: RwLock::new(RESERVED_ATOMS as u32),
        };

        // Pre-register reserved atoms
        let reserved = [
            ("false", 0),
            ("true", 1),
            ("nil", 2),
            ("undefined", 3),
            ("ok", 4),
            ("error", 5),
            ("badarg", 6),
            ("exit", 7),
            ("normal", 8),
            ("kill", 9),
            ("message_queue_len", 10),
            ("heap_size", 11),
            ("stack_size", 12),
            ("reductions", 13),
            ("status", 14),
            ("running", 15),
            ("waiting", 16),
            ("exiting", 17),
        ];

        for (name, index) in reserved {
            table.register_static(name, index as u32);
        }

        table
    }

    /// Register a static (predefined) atom at a specific index
    fn register_static(&self, name: &str, index: u32) {
        let mut name_to_index = self.name_to_index.write().unwrap();
        let mut index_to_name = self.index_to_name.write().unwrap();
        name_to_index.insert(name.to_string(), index);
        index_to_name.insert(index, name.to_string());
    }

    /// Intern a string and return its atom index.
    /// If the atom already exists, returns the existing index.
    /// Returns error if atom limit would be exceeded.
    pub fn intern(&self, name: &str) -> Result<u32, AtomError> {
        // Check if already interned
        {
            let name_to_index = self.name_to_index.read().unwrap();
            if let Some(&index) = name_to_index.get(name) {
                return Ok(index);
            }
        }

        // Need to intern new atom
        let mut name_to_index = self.name_to_index.write().unwrap();
        let mut next_index = self.next_index.write().unwrap();

        // Double-check after acquiring write lock
        if let Some(&index) = name_to_index.get(name) {
            return Ok(index);
        }

        // Check limit
        if *next_index >= MAX_ATOMS as u32 {
            return Err(AtomError::LimitExceeded);
        }

        let index = *next_index;
        *next_index += 1;

        let mut index_to_name = self.index_to_name.write().unwrap();
        name_to_index.insert(name.to_string(), index);
        index_to_name.insert(index, name.to_string());

        Ok(index)
    }

    /// Look up an atom index and return its name.
    /// Returns None if the atom index is not found.
    pub fn lookup(&self, index: u32) -> Option<String> {
        let index_to_name = self.index_to_name.read().unwrap();
        index_to_name.get(&index).cloned()
    }

    /// Look up an atom name and return its index.
    /// Returns None if the atom name is not interned.
    pub fn lookup_name(&self, name: &str) -> Option<u32> {
        let name_to_index = self.name_to_index.read().unwrap();
        name_to_index.get(name).copied()
    }

    /// Get the number of interned atoms (including reserved)
    pub fn len(&self) -> usize {
        let name_to_index = self.name_to_index.read().unwrap();
        name_to_index.len()
    }

    /// Check if the atom table is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if an atom index is valid (exists in table)
    pub fn is_valid(&self, index: u32) -> bool {
        let index_to_name = self.index_to_name.read().unwrap();
        index_to_name.contains_key(&index)
    }

    /// Get the next available atom index (for testing)
    pub fn next_index(&self) -> u32 {
        *self.next_index.read().unwrap()
    }
}

/// Atom table errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomError {
    /// Atom table has reached its maximum capacity
    LimitExceeded,
}

impl std::fmt::Display for AtomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtomError::LimitExceeded => write!(f, "atom table limit exceeded"),
        }
    }
}

impl std::error::Error for AtomError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atom_table_new() {
        let table = AtomTable::new();
        // After registering 18 reserved atoms, table is not empty
        assert!(!table.is_empty());
        assert_eq!(table.len(), 18); // RESERVED_ATOMS count
    }

    #[test]
    fn test_reserved_atoms() {
        let table = AtomTable::new();

        // Check first reserved atoms
        assert_eq!(table.lookup(0), Some("false".to_string()));
        assert_eq!(table.lookup(1), Some("true".to_string()));
        assert_eq!(table.lookup(2), Some("nil".to_string()));
    }

    #[test]
    fn test_intern_new_atom() {
        let table = AtomTable::new();

        let index1 = table.intern("test_atom").unwrap();
        assert!(index1 >= 18); // After reserved atoms

        // Same atom should return same index
        let index2 = table.intern("test_atom").unwrap();
        assert_eq!(index1, index2);

        assert_eq!(table.lookup(index1), Some("test_atom".to_string()));
    }

    #[test]
    fn test_intern_multiple_atoms() {
        let table = AtomTable::new();

        let a1 = table.intern("atom1").unwrap();
        let a2 = table.intern("atom2").unwrap();
        let a3 = table.intern("atom3").unwrap();

        assert_ne!(a1, a2);
        assert_ne!(a2, a3);
        assert_ne!(a1, a3);

        assert_eq!(table.lookup(a1), Some("atom1".to_string()));
        assert_eq!(table.lookup(a2), Some("atom2".to_string()));
        assert_eq!(table.lookup(a3), Some("atom3".to_string()));
    }

    #[test]
    fn test_lookup_name() {
        let table = AtomTable::new();

        // Reserved atoms
        assert_eq!(table.lookup_name("false"), Some(0));
        assert_eq!(table.lookup_name("true"), Some(1));
        assert_eq!(table.lookup_name("nil"), Some(2));

        // New atom
        let index = table.intern("lookup_test").unwrap();
        assert_eq!(table.lookup_name("lookup_test"), Some(index));
    }

    #[test]
    fn test_is_valid() {
        let table = AtomTable::new();

        // Reserved atoms are valid
        assert!(table.is_valid(0));
        assert!(table.is_valid(1));
        assert!(table.is_valid(17));

        // New atoms
        let index = table.intern("valid_test").unwrap();
        assert!(table.is_valid(index));

        // Invalid index
        assert!(!table.is_valid(99999));
        assert!(!table.is_valid(u32::MAX));
    }

    #[test]
    fn test_len_increases() {
        let table = AtomTable::new();
        let initial_len = table.len();

        table.intern("len_test1").unwrap();
        assert_eq!(table.len(), initial_len + 1);

        table.intern("len_test2").unwrap();
        assert_eq!(table.len(), initial_len + 2);

        // Duplicates don't increase
        table.intern("len_test1").unwrap();
        assert_eq!(table.len(), initial_len + 2);
    }

    #[test]
    fn test_next_index() {
        let table = AtomTable::new();

        // After registering 18 reserved atoms, next index is 18
        assert_eq!(table.next_index(), 18);

        table.intern("next_idx_test").unwrap();
        // Next index should have advanced
        assert!(table.next_index() > 18);
    }
}
