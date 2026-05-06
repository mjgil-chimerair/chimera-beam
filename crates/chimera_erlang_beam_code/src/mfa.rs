//! MFA (Module/Function/Arity) triple for function resolution.
//!
//! MFA is the BEAM function calling convention - Module:Function/Arity.

use std::fmt;

/// A module/function/arity triple used for function resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Mfa {
    module_index: u32,
    function_index: u32,
    arity: u8,
}

impl Mfa {
    pub fn new(module_index: u32, function_index: u32, arity: u8) -> Self {
        Self {
            module_index,
            function_index,
            arity,
        }
    }

    pub fn module_index(&self) -> u32 {
        self.module_index
    }

    pub fn function_index(&self) -> u32 {
        self.function_index
    }

    pub fn arity(&self) -> u8 {
        self.arity
    }
}

impl fmt::Display for Mfa {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.module_index, self.function_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mfa_new() {
        let mfa = Mfa::new(1, 2, 3);
        assert_eq!(mfa.module_index(), 1);
        assert_eq!(mfa.function_index(), 2);
        assert_eq!(mfa.arity(), 3);
    }

    #[test]
    fn mfa_display() {
        let mfa = Mfa::new(5, 10, 2);
        let s = format!("{}", mfa);
        assert_eq!(s, "5/10");
    }

    #[test]
    fn mfa_copy() {
        let mfa1 = Mfa::new(1, 2, 3);
        let mfa2 = mfa1;
        assert_eq!(mfa1.module_index(), mfa2.module_index());
    }

    #[test]
    fn mfa_partial_eq() {
        let mfa1 = Mfa::new(1, 2, 3);
        let mfa2 = Mfa::new(1, 2, 3);
        let mfa3 = Mfa::new(1, 2, 4);
        assert_eq!(mfa1, mfa2);
        assert_ne!(mfa1, mfa3);
    }
}
