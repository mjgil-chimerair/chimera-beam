//! Code loader for RustZigBeam.
//!
//! Rust owns code loading - bytecode reading, module verification,
//! and code placement into memory.
//!
//! This crate provides the canonical BEAM/IFF code loader with:
//! - IFF container parsing for BEAM files
//! - Chunk-based BEAM file decoding
//! - Module table for tracking loaded modules
//! - Atom table parsing with UTF-8 support
//! - Export/import table parsing with MFA resolution
//! - Code chunk header parsing and instruction framing
//!
//! Per task-list-3.md Task 59: Complete BEAM/IFF loader in chimera_erlang_beam_code

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

pub mod atoms;
pub mod code_header;
pub mod hot_code;
pub mod iff;
pub mod mfa;
pub mod symbols;

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub use atoms::{AtomEntry, AtomTable};
pub use code_header::{CodeHeader, CODE_HEADER_MAGIC, CODE_HEADER_SIZE};
pub use iff::{Chunk, ChunkTag, Container, FOR1_MAGIC, IFF_HEADER_SIZE};
pub use symbols::{SymbolEntry, SymbolTable};

use chimera_erlang_beam_core::{LoadErrorKind, VmError, VmResult};

// Re-export LoadError types from submodules
pub use atoms::LoadError as AtomLoadError;
pub use code_header::LoadError as CodeHeaderLoadError;
pub use iff::LoadError as IffLoadError;
pub use symbols::LoadError as SymbolLoadError;

/// An export entry with resolved MFA
#[derive(Debug, Clone)]
pub struct ExportEntry {
    pub name: String,
    pub arity: u8,
    pub address: usize,
    pub label: usize,
}

/// A loaded module with decoded metadata
#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub name: String,
    pub atoms: AtomTable,
    pub exports: Vec<ExportEntry>,
    pub imports: SymbolTable,
    pub code: Vec<u8>,
    pub code_size: usize,
    pub header: CodeHeader,
    pub original_index: usize,
}

impl LoadedModule {
    pub fn new(name: String) -> Self {
        LoadedModule {
            name,
            atoms: AtomTable::new(),
            exports: Vec::new(),
            imports: SymbolTable::new(),
            code: Vec::new(),
            code_size: 0,
            header: CodeHeader {
                magic: 0,
                version: 0,
                flags: 0,
                code_size: 0,
                export_count: 0,
                import_count: 0,
                local_count: 0,
                lambda_count: 0,
                code_label_count: 0,
                function_count: 0,
            },
            original_index: 0,
        }
    }

    pub fn get_function_address(&self, name: &str, arity: u8) -> Option<usize> {
        self.exports
            .iter()
            .find(|e| e.name == name && e.arity == arity)
            .map(|e| e.address)
    }

    pub fn get_export(&self, name: &str, arity: u8) -> Option<&ExportEntry> {
        self.exports
            .iter()
            .find(|e| e.name == name && e.arity == arity)
    }
}

/// Module table for tracking all loaded modules
#[derive(Debug)]
#[allow(dead_code)]
pub struct ModuleTable {
    modules: HashMap<String, LoadedModule>,
    _next_index: usize,
}

impl ModuleTable {
    pub fn new() -> Self {
        ModuleTable {
            modules: HashMap::new(),
            _next_index: 0,
        }
    }

    pub fn add(&mut self, module: LoadedModule) {
        let name = module.name.clone();
        self.modules.insert(name, module);
    }

    pub fn get(&self, name: &str) -> Option<&LoadedModule> {
        self.modules.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut LoadedModule> {
        self.modules.get_mut(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn module_names(&self) -> Vec<String> {
        self.modules.keys().cloned().collect()
    }
}

impl Default for ModuleTable {
    fn default() -> Self {
        Self::new()
    }
}

/// A complete code loader with module tracking
#[derive(Debug)]
pub struct CodeLoader {
    module_table: ModuleTable,
    code_area: Vec<u8>,
    next_address: usize,
}

impl CodeLoader {
    pub fn new() -> Self {
        CodeLoader {
            module_table: ModuleTable::new(),
            code_area: Vec::with_capacity(1024 * 1024),
            next_address: 0,
        }
    }

    /// Load a module from a .beam file
    pub fn load_module<P: AsRef<Path>>(&mut self, path: P) -> VmResult<LoadedModule> {
        let path = path.as_ref();
        let module_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or(VmError::BadArg)?
            .to_string();

        let mut file = File::open(path).map_err(|e| VmError::IoError(e.to_string()))?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| VmError::IoError(e.to_string()))?;

        self.load_module_from_bytes(&module_name, &buffer)
    }

    /// Load a module from raw bytes
    pub fn load_module_from_bytes(
        &mut self,
        module_name: &str,
        data: &[u8],
    ) -> VmResult<LoadedModule> {
        // Parse IFF container
        let container = iff::parse_container(data)
            .map_err(|_e| VmError::LoadError(LoadErrorKind::InvalidFormat))?;

        // Find atom table chunk
        let atom_chunk = container
            .chunks
            .iter()
            .find(|c| c.tag.0 == *b"AtU8" || c.tag.0 == *b"Atom")
            .ok_or(VmError::LoadError(LoadErrorKind::ImportError))?;

        let atoms = if atom_chunk.tag.0 == *b"AtU8" {
            atoms::decode_utf8_atom_chunk(atom_chunk)
        } else {
            return Err(VmError::Unimplemented);
        }
        .map_err(|_e| VmError::LoadError(LoadErrorKind::ImportError))?;

        // Find export table chunk
        let exp_chunk = container
            .chunks
            .iter()
            .find(|c| c.tag.0 == *b"ExpT")
            .ok_or(VmError::LoadError(LoadErrorKind::ExportError))?;

        let exports = symbols::decode_export_table(exp_chunk, &atoms)
            .map_err(|_e| VmError::LoadError(LoadErrorKind::ExportError))?;

        // Find code chunk
        let code_chunk = container
            .chunks
            .iter()
            .find(|c| c.tag.0 == *b"Code")
            .ok_or(VmError::LoadError(LoadErrorKind::NoCodeFound))?;

        let header = code_header::parse_code_header(&code_chunk.data)
            .map_err(|_e| VmError::LoadError(LoadErrorKind::InvalidFormat))?;

        // Extract opcode stream (after 40-byte header)
        let code_start = CODE_HEADER_SIZE;
        let code_end = code_start + header.code_size as usize;
        if code_end > code_chunk.data.len() {
            return Err(VmError::LoadError(LoadErrorKind::TruncatedChunk));
        }
        let code = code_chunk.data[code_start..code_end].to_vec();

        // Build export entries with addresses from export table
        let mut export_entries = Vec::new();
        for (i, entry) in exports.all().iter().enumerate() {
            if let Some(fun_name) = &entry.function_name {
                export_entries.push(ExportEntry {
                    name: fun_name.clone(),
                    arity: entry.mfa.arity(),
                    address: 0,
                    label: i,
                });
            }
        }

        // Find import table if present
        let imports = if let Some(imp_chunk) = container.chunks.iter().find(|c| c.tag.0 == *b"ImpT")
        {
            symbols::decode_import_table(imp_chunk, &atoms)
                .map_err(|_e| VmError::LoadError(LoadErrorKind::ImportError))?
        } else {
            SymbolTable::new()
        };

        // Place code in code area
        let address = self.next_address;
        self.next_address += code.len();

        let mut module = LoadedModule {
            name: module_name.to_string(),
            atoms,
            exports: export_entries,
            imports,
            code: code.clone(),
            code_size: code.len(),
            header,
            original_index: address,
        };

        // Resolve export addresses relative to module base
        for export in &mut module.exports {
            export.address = address;
        }

        // Copy code into code area
        self.code_area.extend_from_slice(&code);

        // Add to module table
        self.module_table.add(module.clone());

        Ok(module)
    }

    /// Get a loaded module by name
    pub fn get_module(&self, name: &str) -> Option<&LoadedModule> {
        self.module_table.get(name)
    }

    /// Get module table for iteration
    pub fn module_table(&self) -> &ModuleTable {
        &self.module_table
    }

    /// Get code at an absolute address
    pub fn get_code_at(&self, address: usize) -> Option<u8> {
        self.code_area.get(address).copied()
    }

    /// Get total code size
    pub fn total_code_size(&self) -> usize {
        self.code_area.len()
    }

    /// Resolve a function MFA (Module:Function/Arity) from loaded module
    ///
    /// Returns the function address and entry if found.
    pub fn resolve_mfa(
        &self,
        module: &str,
        function: &str,
        arity: u8,
    ) -> Option<(usize, &ExportEntry)> {
        let loaded = self.module_table.get(module)?;
        let export = loaded.get_export(function, arity)?;
        Some((export.address, export))
    }

    /// Get all exports for a module
    pub fn get_exports(&self, module: &str) -> Option<&Vec<ExportEntry>> {
        let loaded = self.module_table.get(module)?;
        Some(&loaded.exports)
    }

    /// Get all imports for a module
    pub fn get_imports(&self, module: &str) -> Option<&SymbolTable> {
        let loaded = self.module_table.get(module)?;
        Some(&loaded.imports)
    }

    /// Get the code area for execution
    pub fn code_area(&self) -> &[u8] {
        &self.code_area
    }
}

impl Default for CodeLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_loader_new() {
        let loader = CodeLoader::new();
        assert!(loader.module_table().is_empty());
        assert_eq!(loader.total_code_size(), 0);
    }

    #[test]
    fn test_code_loader_invalid_beam() {
        let mut loader = CodeLoader::new();
        let result = loader.load_module_from_bytes("test", b"NOT_A_BEAM_FILE");
        assert!(result.is_err());
    }

    #[test]
    fn test_module_table() {
        let mut table = ModuleTable::new();
        assert!(table.is_empty());

        let module = LoadedModule::new("test".to_string());
        table.add(module);

        assert!(table.contains("test"));
        assert_eq!(table.len(), 1);
        assert!(table.get("test").is_some());
    }

    #[test]
    fn test_code_loader_with_minimal_beam_fixture() {
        use crate::code_header::CODE_HEADER_MAGIC;

        let mut loader = CodeLoader::new();

        // Build a minimal valid FOR1 container with AtU8 and Code chunks
        // FOR1 format: FOR1 + size(4) + BEAM + chunks (NO chunk count - read until EOF)
        let mut container_data = Vec::new();
        container_data.extend_from_slice(b"FOR1");
        container_data.extend_from_slice(&0u32.to_be_bytes()); // size placeholder

        // BEAM container header (NO chunk count - IFF format uses EOF)
        container_data.extend_from_slice(b"BEAM");

        // Add AtU8 chunk with one atom
        // AtU8 format: count(u32) + (len(u16) + bytes)*count
        let mut atom_data = Vec::new();
        atom_data.extend_from_slice(&(1u32).to_be_bytes()); // 1 atom
        atom_data.extend_from_slice(&(3u16).to_be_bytes()); // "foo" length
        atom_data.extend_from_slice(b"foo");

        container_data.extend_from_slice(b"AtU8");
        container_data.extend_from_slice(&(atom_data.len() as u32).to_be_bytes());
        container_data.extend_from_slice(&atom_data);

        // Add Code chunk with valid header (40 bytes)
        let mut code_data = vec![0u8; 40];
        code_data[0..4].copy_from_slice(&CODE_HEADER_MAGIC.to_be_bytes());
        code_data[4..8].copy_from_slice(&0u32.to_be_bytes()); // version
        code_data[12..16].copy_from_slice(&0u32.to_be_bytes()); // code_size = 0

        container_data.extend_from_slice(b"Code");
        container_data.extend_from_slice(&(code_data.len() as u32).to_be_bytes());
        container_data.extend_from_slice(&code_data);

        // Update FOR1 size (body size after FOR1 header)
        let body_size = container_data.len() - 8; // exclude FOR1 + size
        container_data[4..8].copy_from_slice(&(body_size as u32).to_be_bytes());

        // Load should succeed (will fail at export table since no ExpT chunk)
        let result = loader.load_module_from_bytes("test", &container_data);
        eprintln!("DEBUG test: container_data len={}", container_data.len());
        eprintln!(
            "DEBUG test: data={:?}",
            &container_data[..std::cmp::min(32, container_data.len())]
        );
        // Basic IFF parsing works but we'll fail at export table since no ExpT
        // This test verifies the IFF parsing works correctly
        assert!(
            result.is_err(),
            "Expected failure due to missing ExpT, got {:?}",
            result
        );
        // Check the error is about missing export, not parsing
        match &result {
            Err(VmError::LoadError(LoadErrorKind::ExportError)) => {
                // This is the expected error - we found BEAM, parsed atoms, but no ExpT chunk
            }
            _ => panic!("Expected ExportError, got {:?}", result),
        }
    }

    #[test]
    fn test_code_loader_with_export_table() {
        use crate::code_header::CODE_HEADER_MAGIC;

        let mut loader = CodeLoader::new();

        // Build a minimal valid FOR1 container with AtU8, ExpT, and Code chunks
        let mut container_data = Vec::new();
        container_data.extend_from_slice(b"FOR1");

        // Add AtU8 chunk with one atom "test"
        let mut atom_data = Vec::new();
        atom_data.extend_from_slice(&(1u32).to_be_bytes()); // 1 atom
        atom_data.extend_from_slice(&(4u16).to_be_bytes()); // "test" length
        atom_data.extend_from_slice(b"test");

        // Add ExpT chunk with one export entry
        // Export entry: module(4) + function(4) + arity(4) + address(4) = 16 bytes
        let mut export_data = Vec::new();
        export_data.extend_from_slice(&(1u32).to_be_bytes()); // 1 export entry
        export_data.extend_from_slice(&(0u32).to_be_bytes()); // module atom index = 0
        export_data.extend_from_slice(&(0u32).to_be_bytes()); // function atom index = 0
        export_data.extend_from_slice(&(0u32).to_be_bytes()); // arity = 0
        export_data.extend_from_slice(&(0u32).to_be_bytes()); // address = 0

        // Add Code chunk with valid header (40 bytes)
        let mut code_data = vec![0u8; 40];
        code_data[0..4].copy_from_slice(&CODE_HEADER_MAGIC.to_be_bytes());
        code_data[4..8].copy_from_slice(&0u32.to_be_bytes()); // version
        code_data[12..16].copy_from_slice(&0u32.to_be_bytes()); // code_size = 0

        // Calculate chunks size
        let chunks_size = 8 + atom_data.len()
            + 8 + export_data.len()
            + 8 + code_data.len();

        // Update FOR1 size
        let body_size = 4 + chunks_size; // "BEAM" + chunks
        container_data.extend_from_slice(&(body_size as u32).to_be_bytes());
        container_data.extend_from_slice(b"BEAM");

        // Add AtU8 chunk
        container_data.extend_from_slice(b"AtU8");
        container_data.extend_from_slice(&(atom_data.len() as u32).to_be_bytes());
        container_data.extend_from_slice(&atom_data);

        // Add ExpT chunk
        container_data.extend_from_slice(b"ExpT");
        container_data.extend_from_slice(&(export_data.len() as u32).to_be_bytes());
        container_data.extend_from_slice(&export_data);

        // Add Code chunk
        container_data.extend_from_slice(b"Code");
        container_data.extend_from_slice(&(code_data.len() as u32).to_be_bytes());
        container_data.extend_from_slice(&code_data);

        // Load should succeed now with export table
        let result = loader.load_module_from_bytes("test", &container_data);
        assert!(
            result.is_ok(),
            "Expected successful load with ExpT, got {:?}",
            result
        );

        // Verify module was loaded
        let exports = loader.get_exports("test").expect("Module should exist");
        assert_eq!(exports.len(), 1, "Should have 1 export");
    }

    #[test]
    fn test_resolve_mfa_no_module() {
        let loader = CodeLoader::new();
        let result = loader.resolve_mfa("nonexistent", "function", 0);
        assert!(result.is_none());
    }

    #[test]
    fn test_get_exports_no_module() {
        let loader = CodeLoader::new();
        let result = loader.get_exports("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_imports_no_module() {
        let loader = CodeLoader::new();
        let result = loader.get_imports("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_code_loader_code_area_empty() {
        let loader = CodeLoader::new();
        assert!(loader.code_area().is_empty());
    }

    #[test]
    fn test_loaded_module_new_has_empty_exports() {
        let module = LoadedModule::new("test".to_string());
        assert!(module.exports.is_empty());
        assert!(module.imports.is_empty());
    }

    #[test]
    fn test_loaded_module_get_export_not_found() {
        let module = LoadedModule::new("test".to_string());
        assert!(module.get_export("nonexistent", 0).is_none());
        assert!(module.get_function_address("nonexistent", 0).is_none());
    }
}
