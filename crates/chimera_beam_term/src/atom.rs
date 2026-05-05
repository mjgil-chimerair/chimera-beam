//! Atom representation for BEAM-compatible runtime.
//!
//! Atoms are interned strings identified by a 32-bit index.
//! The first 3 atoms are reserved: nil=0, true=1, false=2.

/// An atom representation (BEAM-compatible).
///
/// Atoms are interned strings identified by a 32-bit index.
/// The first 3 atoms are reserved: nil=0, true=1, false=2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Atom(pub u32);

impl Atom {
    /// The nil atom (index 0)
    pub const NIL: Atom = Atom(0);
    /// The true atom (index 1)
    pub const TRUE: Atom = Atom(1);
    /// The false atom (index 2)
    pub const FALSE: Atom = Atom(2);

    /// Create a new atom from an index
    #[inline]
    pub fn new(id: u32) -> Self {
        Atom(id)
    }

    /// Get the atom's index
    #[inline]
    pub fn id(self) -> u32 {
        self.0
    }

    /// Check if this is a reserved atom (nil, true, false)
    #[inline]
    pub fn is_reserved(self) -> bool {
        self.0 <= 2
    }

    /// Check if this is the nil atom
    #[inline]
    pub fn is_nil(self) -> bool {
        self.0 == 0
    }

    /// Check if this is the true atom
    #[inline]
    pub fn is_true(self) -> bool {
        self.0 == 1
    }

    /// Check if this is the false atom
    #[inline]
    pub fn is_false(self) -> bool {
        self.0 == 2
    }

    /// Convert to string representation (for debugging)
    pub fn to_str(&self) -> &'static str {
        match self.0 {
            0 => "nil",
            1 => "true",
            2 => "false",
            _ => "unknown",
        }
    }
}

impl Default for Atom {
    fn default() -> Self {
        Atom::NIL
    }
}

impl std::fmt::Display for Atom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atom_constants() {
        assert_eq!(Atom::NIL, Atom(0));
        assert_eq!(Atom::TRUE, Atom(1));
        assert_eq!(Atom::FALSE, Atom(2));
    }

    #[test]
    fn test_atom_new() {
        let a = Atom::new(42);
        assert_eq!(a.id(), 42);
    }

    #[test]
    fn test_atom_is_reserved() {
        assert!(Atom::NIL.is_reserved());
        assert!(Atom::TRUE.is_reserved());
        assert!(Atom::FALSE.is_reserved());
        assert!(!Atom::new(10).is_reserved());
    }

    #[test]
    fn test_atom_is_nil_true_false() {
        assert!(Atom::NIL.is_nil());
        assert!(!Atom::NIL.is_true());
        assert!(!Atom::NIL.is_false());

        assert!(Atom::TRUE.is_true());
        assert!(!Atom::TRUE.is_nil());
        assert!(!Atom::TRUE.is_false());

        assert!(Atom::FALSE.is_false());
        assert!(!Atom::FALSE.is_nil());
        assert!(!Atom::FALSE.is_true());
    }

    #[test]
    fn test_atom_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Atom::new(1));
        set.insert(Atom::new(2));
        set.insert(Atom::new(1)); // duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_atom_eq() {
        assert_eq!(Atom::new(1), Atom::new(1));
        assert_ne!(Atom::new(1), Atom::new(2));
    }

    #[test]
    fn test_atom_default() {
        assert_eq!(Atom::default(), Atom::NIL);
    }
}