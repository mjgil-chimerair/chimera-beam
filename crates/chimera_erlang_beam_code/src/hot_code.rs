//! Hot code loading and module versioning.
//!
//! In Erlang/OTP, modules can be hot-loaded while the system is running.
//! This module tracks code generations and ensures safe code replacement.
//!
//! Key concepts:
//! - Each module has a current version and optionally old versions
//! - Processes execute a specific code version until they yield
//! - Old code is only purged when no processes reference it
//! - Code replacement updates the current version atomically

use std::collections::HashMap;

/// A version of module code
#[derive(Debug, Clone)]
pub struct CodeVersion {
    /// Version number (increments on each load)
    pub version: u32,
    /// The code bytes
    pub code: Vec<u8>,
    /// When this version was loaded
    pub loaded_at: std::time::SystemTime,
    /// Number of processes currently running this version
    pub active_processes: u32,
}

impl CodeVersion {
    pub fn new(version: u32, code: Vec<u8>) -> Self {
        Self {
            version,
            code,
            loaded_at: std::time::SystemTime::now(),
            active_processes: 0,
        }
    }

    /// Increment active process count
    pub fn add_process(&mut self) {
        self.active_processes += 1;
    }

    /// Decrement active process count
    pub fn remove_process(&mut self) {
        self.active_processes = self.active_processes.saturating_sub(1);
    }

    /// Check if this version can be purged (no active processes)
    pub fn can_purge(&self) -> bool {
        self.active_processes == 0
    }
}

/// Module code state with versioning support
#[derive(Debug, Clone)]
pub struct ModuleCode {
    /// Module name
    pub name: String,
    /// All known versions (current + old)
    versions: HashMap<u32, CodeVersion>,
    /// Current active version number
    current_version: u32,
    /// Old version number (previous version, if any)
    old_version: Option<u32>,
}

impl ModuleCode {
    pub fn new(name: String) -> Self {
        Self {
            name,
            versions: HashMap::new(),
            current_version: 0,
            old_version: None,
        }
    }

    /// Load a new version of the module code
    pub fn load_version(&mut self, code: Vec<u8>) -> u32 {
        // Increment version
        let new_version = self.current_version + 1;

        // Move current to old if it exists
        if self.current_version > 0 {
            self.old_version = Some(self.current_version);
        }

        // Add new version
        self.versions
            .insert(new_version, CodeVersion::new(new_version, code));
        self.current_version = new_version;

        new_version
    }

    /// Get the current code version number
    pub fn current_version(&self) -> u32 {
        self.current_version
    }

    /// Get the old code version number if any
    pub fn old_version(&self) -> Option<u32> {
        self.old_version
    }

    /// Get current code bytes
    pub fn current_code(&self) -> Option<&Vec<u8>> {
        self.versions.get(&self.current_version).map(|v| &v.code)
    }

    /// Get old code bytes if any
    pub fn old_code(&self) -> Option<&Vec<u8>> {
        self.old_version
            .and_then(|v| self.versions.get(&v).map(|ver| &ver.code))
    }

    /// Get code for a specific version
    pub fn get_version(&self, version: u32) -> Option<&CodeVersion> {
        self.versions.get(&version)
    }

    /// Check if old version can be purged
    pub fn can_purge_old(&self) -> bool {
        if let Some(old_v) = self.old_version {
            if let Some(version) = self.versions.get(&old_v) {
                return version.can_purge();
            }
        }
        false
    }

    /// Purge old version and remove from versions map
    pub fn purge_old(&mut self) -> bool {
        if !self.can_purge_old() {
            return false;
        }
        if let Some(old_v) = self.old_version {
            self.versions.remove(&old_v);
            self.old_version = None;
            return true;
        }
        false
    }

    /// Increment active process count for a version
    pub fn add_process_to_version(&mut self, version: u32) {
        if let Some(v) = self.versions.get_mut(&version) {
            v.add_process();
        }
    }

    /// Decrement active process count for a version
    pub fn remove_process_from_version(&mut self, version: u32) {
        if let Some(v) = self.versions.get_mut(&version) {
            v.remove_process();
        }
    }

    /// Number of versions stored
    pub fn num_versions(&self) -> usize {
        self.versions.len()
    }
}

/// Manages code loading and versioning for all modules
#[derive(Debug)]
pub struct CodeServer {
    modules: HashMap<String, ModuleCode>,
}

impl CodeServer {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Load new code for a module
    pub fn load_module(&mut self, module: &str, code: Vec<u8>) -> u32 {
        let module_code = self
            .modules
            .entry(module.to_string())
            .or_insert_with(|| ModuleCode::new(module.to_string()));
        module_code.load_version(code)
    }

    /// Get current code for a module
    pub fn get_module_code(&self, module: &str) -> Option<&Vec<u8>> {
        self.modules.get(module).and_then(|m| m.current_code())
    }

    /// Get a specific version of a module
    pub fn get_module_version(&self, module: &str, version: u32) -> Option<&CodeVersion> {
        self.modules
            .get(module)
            .and_then(|m| m.get_version(version))
    }

    /// Check if a module is loaded
    pub fn is_loaded(&self, module: &str) -> bool {
        self.modules
            .get(module)
            .map(|m| m.current_version() > 0)
            .unwrap_or(false)
    }

    /// Purge old code for a module if safe
    pub fn purge_old_code(&mut self, module: &str) -> bool {
        if let Some(m) = self.modules.get_mut(module) {
            m.purge_old()
        } else {
            false
        }
    }

    /// Get all loaded module names
    pub fn loaded_modules(&self) -> Vec<&str> {
        self.modules
            .iter()
            .filter(|(_, m)| m.current_version() > 0)
            .map(|(n, _)| n.as_str())
            .collect()
    }

    /// Check if old code can be purged
    pub fn can_purge_old(&self, module: &str) -> bool {
        self.modules
            .get(module)
            .map(|m| m.can_purge_old())
            .unwrap_or(false)
    }
}

impl Default for CodeServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_version_new() {
        let code = vec![1, 2, 3];
        let version = CodeVersion::new(1, code.clone());
        assert_eq!(version.version, 1);
        assert_eq!(version.code, code);
        assert_eq!(version.active_processes, 0);
        assert!(version.can_purge());
    }

    #[test]
    fn test_code_version_process_tracking() {
        let mut version = CodeVersion::new(1, vec![1, 2, 3]);
        version.add_process();
        version.add_process();
        assert_eq!(version.active_processes, 2);
        assert!(!version.can_purge());

        version.remove_process();
        assert_eq!(version.active_processes, 1);
        assert!(!version.can_purge());

        version.remove_process();
        assert_eq!(version.active_processes, 0);
        assert!(version.can_purge());
    }

    #[test]
    fn test_module_code_new() {
        let module = ModuleCode::new("test".to_string());
        assert_eq!(module.current_version(), 0);
        assert!(module.old_version().is_none());
        assert!(module.current_code().is_none());
    }

    #[test]
    fn test_module_code_load_version() {
        let mut module = ModuleCode::new("test".to_string());

        let v1 = module.load_version(vec![1, 2, 3]);
        assert_eq!(v1, 1);
        assert_eq!(module.current_version(), 1);
        assert_eq!(module.current_code(), Some(&vec![1, 2, 3]));
        assert!(module.old_version().is_none());

        let v2 = module.load_version(vec![4, 5, 6]);
        assert_eq!(v2, 2);
        assert_eq!(module.current_version(), 2);
        assert_eq!(module.current_code(), Some(&vec![4, 5, 6]));
        assert_eq!(module.old_version(), Some(1));
        assert_eq!(module.old_code(), Some(&vec![1, 2, 3]));
    }

    #[test]
    fn test_module_code_cannot_purge_with_active_processes() {
        let mut module = ModuleCode::new("test".to_string());
        module.load_version(vec![1, 2, 3]);
        module.load_version(vec![4, 5, 6]);

        // Add process to old version
        module.add_process_to_version(1);
        assert!(!module.can_purge_old());

        // Remove process - can now purge
        module.remove_process_from_version(1);
        assert!(module.can_purge_old());
    }

    #[test]
    fn test_module_code_purge_old() {
        let mut module = ModuleCode::new("test".to_string());
        module.load_version(vec![1, 2, 3]);
        module.load_version(vec![4, 5, 6]);

        assert_eq!(module.num_versions(), 2);
        assert!(module.purge_old());
        assert_eq!(module.num_versions(), 1);
        assert!(module.old_version().is_none());
        assert!(module.old_code().is_none());
    }

    #[test]
    fn test_code_server_new() {
        let server = CodeServer::new();
        assert!(server.loaded_modules().is_empty());
    }

    #[test]
    fn test_code_server_load_module() {
        let mut server = CodeServer::new();

        let v1 = server.load_module("test", vec![1, 2, 3]);
        assert_eq!(v1, 1);
        assert!(server.is_loaded("test"));
        assert_eq!(server.get_module_code("test"), Some(&vec![1, 2, 3]));

        let v2 = server.load_module("test", vec![4, 5, 6]);
        assert_eq!(v2, 2);
        assert_eq!(
            server.get_module_version("test", 1).map(|v| v.version),
            Some(1)
        );
        assert_eq!(
            server.get_module_version("test", 2).map(|v| v.version),
            Some(2)
        );
    }

    #[test]
    fn test_code_server_purge_old() {
        let mut server = CodeServer::new();
        server.load_module("test", vec![1, 2, 3]);
        server.load_module("test", vec![4, 5, 6]);

        // Old version has 0 active processes, so it CAN be purged
        assert!(server.can_purge_old("test"));

        // Purge
        assert!(server.purge_old_code("test"));
        assert!(!server.can_purge_old("test"));
    }

    #[test]
    fn test_code_server_loaded_modules() {
        let mut server = CodeServer::new();
        server.load_module("mod1", vec![1]);
        server.load_module("mod2", vec![2]);
        server.load_module("mod3", vec![3]);

        let loaded = server.loaded_modules();
        assert_eq!(loaded.len(), 3);
        assert!(loaded.contains(&"mod1"));
        assert!(loaded.contains(&"mod2"));
        assert!(loaded.contains(&"mod3"));
    }
}
