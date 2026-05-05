//! MFA (Module, Function, Arity) representation.
//!
//! MFA is the standard way to reference functions in BEAM.

use super::atom::Atom;

/// A fully-qualified function reference (MFA).
///
/// MFA is the standard way to reference functions in BEAM.
/// It consists of a module name, function name, and arity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mfa {
    /// Module name
    pub module: Atom,
    /// Function name
    pub function: Atom,
    /// Number of arguments
    pub arity: u8,
}

impl Mfa {
    /// Create a new MFA
    #[inline]
    pub fn new(module: Atom, function: Atom, arity: u8) -> Self {
        Mfa {
            module,
            function,
            arity,
        }
    }

    /// Create from individual components (as u32 indices)
    #[inline]
    pub fn from_ids(module_id: u32, function_id: u32, arity: u8) -> Self {
        Mfa {
            module: Atom::new(module_id),
            function: Atom::new(function_id),
            arity,
        }
    }

    /// Get the module atom ID
    #[inline]
    pub fn module_id(&self) -> u32 {
        self.module.id()
    }

    /// Get the function atom ID
    #[inline]
    pub fn function_id(&self) -> u32 {
        self.function.id()
    }

    /// Get the arity
    #[inline]
    pub fn arity(&self) -> u8 {
        self.arity
    }

    /// Convert to a tuple representation
    pub fn to_tuple(&self) -> (Atom, Atom, u8) {
        (self.module, self.function, self.arity)
    }

    /// Create from a tuple representation
    pub fn from_tuple((module, function, arity): (Atom, Atom, u8)) -> Self {
        Mfa {
            module,
            function,
            arity,
        }
    }
}

impl Default for Mfa {
    fn default() -> Self {
        Mfa::new(Atom::NIL, Atom::NIL, 0)
    }
}

impl std::fmt::Display for Mfa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.module.id(), self.function.id(), self.arity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mfa_new() {
        let mfa = Mfa::new(Atom::new(1), Atom::new(2), 3);
        assert_eq!(mfa.module, Atom::new(1));
        assert_eq!(mfa.function, Atom::new(2));
        assert_eq!(mfa.arity, 3);
    }

    #[test]
    fn test_mfa_from_ids() {
        let mfa = Mfa::from_ids(10, 20, 5);
        assert_eq!(mfa.module_id(), 10);
        assert_eq!(mfa.function_id(), 20);
        assert_eq!(mfa.arity(), 5);
    }

    #[test]
    fn test_mfa_tuple_roundtrip() {
        let mfa = Mfa::new(Atom::new(1), Atom::new(2), 3);
        let tuple = mfa.to_tuple();
        assert_eq!(Mfa::from_tuple(tuple), mfa);
    }

    #[test]
    fn test_mfa_hash() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(Mfa::new(Atom::new(1), Atom::new(2), 3), "test");
        assert_eq!(map.get(&Mfa::new(Atom::new(1), Atom::new(2), 3)), Some(&"test"));
    }

    #[test]
    fn test_mfa_eq() {
        assert_eq!(Mfa::new(Atom::new(1), Atom::new(2), 3), Mfa::new(Atom::new(1), Atom::new(2), 3));
        assert_ne!(Mfa::new(Atom::new(1), Atom::new(2), 3), Mfa::new(Atom::new(1), Atom::new(2), 4));
    }

    #[test]
    fn test_mfa_default() {
        let mfa = Mfa::default();
        assert_eq!(mfa.module, Atom::NIL);
        assert_eq!(mfa.function, Atom::NIL);
        assert_eq!(mfa.arity, 0);
    }

    #[test]
    fn test_mfa_display() {
        let mfa = Mfa::new(Atom::new(5), Atom::new(10), 2);
        assert_eq!(format!("{}", mfa), "5/10/2");
    }
}