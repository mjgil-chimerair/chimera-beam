//! PID/Ref/Port term encoding for RustZigBeam.
//!
//! Provides proper boxed term encoding for process IDs, references, and ports.
//! These are boxed terms with header words that identify the specific type.

/// Maximum number of PIDs per node
pub const MAX_PID_INDEX: u32 = 0x3FFF; // 14 bits for PID index
/// Maximum serial number
pub const MAX_SERIAL: u32 = 0x1FFF; // 13 bits for serial
/// Maximum creation value
pub const MAX_CREATION: u32 = 3; // 2 bits for creation

/// PID term - represents a process identifier
///
/// Layout in heap (boxed):
/// - Header word: tag = BOXED_TAG, size = 3 words
/// - Word 1: id (14 bits), bits 14-26 reserved
/// - Word 2: serial (13 bits), extra bits
/// - Word 3: creation (2 bits), node high bits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PidTerm {
    /// Process ID index (0-16383)
    pub id: u32,
    /// Serial number (incremented each time a process with this id is created)
    pub serial: u32,
    /// Creation number (for distributed PIDs, tracks node creation)
    pub creation: u32,
}

impl PidTerm {
    /// Create a local PID term
    pub const fn new(id: u32, serial: u32, creation: u32) -> Self {
        PidTerm {
            id,
            serial,
            creation,
        }
    }

    /// Create a PID term from raw components packed into 64 bits
    /// Format: creation(2) | serial(13) | id(14) | 35 bits unused
    pub fn from_raw(raw: u64) -> Option<Self> {
        // Validate upper bits are zero (only 29 bits used)
        if raw >> 29 != 0 {
            return None;
        }
        let id = (raw & 0x3FFF) as u32;
        let serial = ((raw >> 14) & 0x1FFF) as u32;
        let creation = ((raw >> 27) & 0x3) as u32;
        // Validate ranges
        if id > MAX_PID_INDEX || serial > MAX_SERIAL || creation > MAX_CREATION {
            return None;
        }
        Some(PidTerm {
            id,
            serial,
            creation,
        })
    }

    /// Encode PID into a 64-bit value for storage
    pub fn to_raw(&self) -> u64 {
        let id = (self.id & 0x3FFF) as u64;
        let serial = (self.serial & 0x1FFF) as u64;
        let creation = (self.creation & 0x3) as u64;
        id | (serial << 14) | (creation << 27)
    }

    /// Check if this is a local PID (creation = 0)
    pub fn is_local(&self) -> bool {
        self.creation == 0
    }

    /// Get the number of words needed to store this PID (1 header + 1 data = 2 words)
    pub const fn heap_size(&self) -> usize {
        2 // 1 header word + 1 data word
    }
}

/// Reference term - represents a reference (used for monitors, etc.)
///
/// Layout in heap (boxed):
/// - Header word: tag = BOXED_TAG, sub-tag = REF_TAG
/// - Word 1: id (28 bits), creation (2 bits), node (high bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefTerm {
    /// Reference ID
    pub id: u32,
    /// Creation number
    pub creation: u32,
}

impl RefTerm {
    /// Create a new reference term
    pub const fn new(id: u32, creation: u32) -> Self {
        RefTerm { id, creation }
    }

    /// Create a RefTerm from raw 64-bit value
    pub fn from_raw(raw: u64) -> Option<Self> {
        // Validate upper bits are zero (only 30 bits used)
        if raw >> 30 != 0 {
            return None;
        }
        let id = (raw & 0xFFFFFFF) as u32; // 28 bits
        let creation = ((raw >> 28) & 0x3) as u32;
        if creation > MAX_CREATION {
            return None;
        }
        Some(RefTerm { id, creation })
    }

    /// Encode RefTerm to raw 64-bit value
    pub fn to_raw(&self) -> u64 {
        let id = (self.id & 0xFFFFFFF) as u64;
        let creation = (self.creation & 0x3) as u64;
        id | (creation << 28)
    }

    /// Get heap size
    pub const fn heap_size(&self) -> usize {
        2 // 1 header word + 1 data word
    }
}

/// Port term - represents a port identifier
///
/// Layout in heap (boxed):
/// - Header word: tag = BOXED_TAG, sub-tag = PORT_TAG
/// - Word 1: id (28 bits), creation (2 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortTerm {
    /// Port ID index
    pub id: u32,
    /// Creation number
    pub creation: u32,
}

impl PortTerm {
    /// Create a new port term
    pub const fn new(id: u32, creation: u32) -> Self {
        PortTerm { id, creation }
    }

    /// Create a PortTerm from raw 64-bit value
    pub fn from_raw(raw: u64) -> Option<Self> {
        // Validate upper bits are zero (only 30 bits used)
        if raw >> 30 != 0 {
            return None;
        }
        let id = (raw & 0xFFFFFFF) as u32; // 28 bits
        let creation = ((raw >> 28) & 0x3) as u32;
        if creation > MAX_CREATION {
            return None;
        }
        Some(PortTerm { id, creation })
    }

    /// Encode PortTerm to raw 64-bit value
    pub fn to_raw(&self) -> u64 {
        let id = (self.id & 0xFFFFFFF) as u64;
        let creation = (self.creation & 0x3) as u64;
        id | (creation << 28)
    }

    /// Get heap size
    pub const fn heap_size(&self) -> usize {
        2 // 1 header word + 1 data word
    }
}

/// Boxed term header tags (stored in the header word)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BoxedTag {
    /// Pid term
    Pid = 0,
    /// Reference term
    Ref = 1,
    /// Port term
    Port = 2,
    /// Float term
    Float = 3,
    /// Binary term
    Binary = 4,
    /// Fun term (closure)
    Fun = 5,
    /// Tuple term
    Tuple = 6,
    /// Map term
    Map = 7,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_term_new() {
        let pid = PidTerm::new(1, 0, 0);
        assert_eq!(pid.id, 1);
        assert_eq!(pid.serial, 0);
        assert_eq!(pid.creation, 0);
    }

    #[test]
    fn test_pid_term_is_local() {
        let local = PidTerm::new(1, 0, 0);
        let remote = PidTerm::new(1, 0, 1);
        assert!(local.is_local());
        assert!(!remote.is_local());
    }

    #[test]
    fn test_pid_term_raw_roundtrip() {
        let original = PidTerm::new(100, 50, 2);
        let raw = original.to_raw();
        let recovered = PidTerm::from_raw(raw).unwrap();
        assert_eq!(original.id, recovered.id);
        assert_eq!(original.serial, recovered.serial);
        assert_eq!(original.creation, recovered.creation);
    }

    #[test]
    fn test_pid_term_heap_size() {
        let pid = PidTerm::new(1, 0, 0);
        assert_eq!(pid.heap_size(), 2);
    }

    #[test]
    fn test_ref_term_new() {
        let r = RefTerm::new(123, 1);
        assert_eq!(r.id, 123);
        assert_eq!(r.creation, 1);
    }

    #[test]
    fn test_ref_term_raw_roundtrip() {
        let original = RefTerm::new(0x1234567, 2);
        let raw = original.to_raw();
        let recovered = RefTerm::from_raw(raw).unwrap();
        assert_eq!(original.id, recovered.id);
        assert_eq!(original.creation, recovered.creation);
    }

    #[test]
    fn test_port_term_new() {
        let p = PortTerm::new(456, 1);
        assert_eq!(p.id, 456);
        assert_eq!(p.creation, 1);
    }

    #[test]
    fn test_port_term_raw_roundtrip() {
        let original = PortTerm::new(0x765432, 3);
        let raw = original.to_raw();
        let recovered = PortTerm::from_raw(raw).unwrap();
        assert_eq!(original.id, recovered.id);
        assert_eq!(original.creation, recovered.creation);
    }

    #[test]
    fn test_pid_bounds() {
        // Test max values
        let pid = PidTerm::new(MAX_PID_INDEX, MAX_SERIAL, MAX_CREATION);
        let raw = pid.to_raw();
        let recovered = PidTerm::from_raw(raw).unwrap();
        assert_eq!(pid.id, recovered.id);
        assert_eq!(pid.serial, recovered.serial);
        assert_eq!(pid.creation, recovered.creation);
    }

    #[test]
    fn test_pid_invalid_raw() {
        // Invalid: id too large
        assert!(PidTerm::from_raw(0x4000_0000_0000).is_none()); // id > MAX_PID_INDEX
    }
}
