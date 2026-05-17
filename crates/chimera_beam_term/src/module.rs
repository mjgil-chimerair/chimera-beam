//! Module code representation (BEAM-compatible).
//!
//! This module defines the structure for compiled BEAM modules.

use super::atom::Atom;
use super::mfa::Mfa;
use std::collections::HashMap;

/// A compiled module representation (BEAM-compatible).
#[derive(Debug, Clone)]
pub struct ModuleCode {
    /// Module name (first atom in atom table)
    pub name: Atom,
    /// Module version for hot code loading
    pub version: u32,
    /// Export table: MFA → code offset
    pub exports: HashMap<Mfa, ExportEntry>,
    /// Import table
    pub imports: Vec<ImportEntry>,
    /// Literal table (pre-allocated terms)
    pub literals: Vec<Literal>,
    /// Compiled bytecode
    pub code: Vec<u8>,
    /// Local atom table
    pub atoms: Vec<Atom>,
    /// Atom names (for debugging)
    pub atom_names: Vec<String>,
    /// Line number information
    pub line_info: Vec<LineInfo>,
}

impl ModuleCode {
    /// Create a new empty module
    pub fn new(name: Atom) -> Self {
        ModuleCode {
            name,
            version: 0,
            exports: HashMap::new(),
            imports: Vec::new(),
            literals: Vec::new(),
            code: Vec::new(),
            atoms: vec![name], // Module name is first atom
            atom_names: Vec::new(),
            line_info: Vec::new(),
        }
    }

    /// Add an atom to the local atom table
    pub fn add_atom(&mut self, name: &str) -> Atom {
        let id = self.atoms.len() as u32;
        self.atoms.push(Atom::new(id));
        self.atom_names.push(name.to_string());
        Atom::new(id)
    }

    /// Add an export
    pub fn add_export(&mut self, function: Atom, arity: u8, offset: u32) {
        let mfa = Mfa::new(self.name, function, arity);
        self.exports.insert(
            mfa,
            ExportEntry {
                offset,
                unused: 0,
                native: None,
            },
        );
    }

    /// Get the export offset for an MFA
    pub fn get_export_offset(&self, mfa: &Mfa) -> Option<u32> {
        self.exports.get(mfa).map(|e| e.offset)
    }

    /// Get the module name as string
    pub fn name_str(&self) -> Option<&str> {
        self.atom_names.first().map(|s| s.as_str())
    }
}

/// Export table entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntry {
    /// Code offset
    pub offset: u32,
    /// Unused (legacy BEAM field)
    pub unused: u32,
    /// Native function (if any)
    pub native: Option<NativeEntry>,
}

/// Import table entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    /// Module
    pub module: Atom,
    /// Function
    pub function: Atom,
    /// Arity
    pub arity: u8,
}

/// Native function entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEntry {
    /// Module
    pub module: Atom,
    /// Function
    pub function: Atom,
}

/// Literal value
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// Integer literal
    Integer(i64),
    /// Float literal
    Float(f64),
    /// Atom literal
    Atom(Atom),
    /// Tuple literal
    Tuple(Vec<Literal>),
    /// List literal
    List(Vec<Literal>),
    /// Map literal
    Map(Vec<(Literal, Literal)>),
    /// Binary literal
    Binary(Vec<u8>),
    /// Fun literal
    Fun(FunLiteral),
}

/// Function literal
#[derive(Debug, Clone, PartialEq)]
pub struct FunLiteral {
    /// Module
    pub module: Atom,
    /// Function
    pub function: Atom,
    /// Arity
    pub arity: u8,
    /// Environment
    pub env: Vec<Literal>,
}

/// Line number information
#[derive(Debug, Clone, PartialEq)]
pub struct LineInfo {
    /// Code offset
    pub offset: u32,
    /// Source line number
    pub line: u32,
}

/// Instruction opcodes for BEAM bytecode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// Generic call
    Call = 1,
    /// Call with arity
    CallExt = 2,
    /// Return from function
    Return = 3,
    /// Jump
    Jump = 4,
    /// Move value to register
    Move = 5,
    /// Get list head
    GetList = 6,
    /// Get list head and tail
    GetTail = 7,
    /// Allocate on heap
    Allocate = 8,
    /// Deallocate from heap
    Deallocate = 9,
    /// Put terms
    PutTerms = 10,
    /// Bad match
    BadMatch = 11,
    /// Case head
    CaseHead = 12,
    /// Case tail
    CaseTail = 13,
    /// Select
    Select = 14,
    /// Function enter
    FuncInfo = 15,
    /// Try
    Try = 16,
    /// Try end
    TryEnd = 17,
    /// Try match
    TryMatch = 18,
    /// Try clear
    TryClear = 19,
    /// Raise
    Raise = 20,
    /// GC
    Gc = 21,
    /// Put map
    PutMap = 22,
    /// Make fun
    MakeFun = 23,
    /// Test heap
    TestHeap = 24,
    /// Reset heap
    ResetHeap = 25,
    /// Catch
    Catch = 26,
    /// Catch end
    CatchEnd = 27,
    /// Query greeting
    QueryGreeting = 28,
    /// Return to trace
    ReturnToTrace = 29,
}

impl Opcode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Opcode::Call),
            2 => Some(Opcode::CallExt),
            3 => Some(Opcode::Return),
            4 => Some(Opcode::Jump),
            5 => Some(Opcode::Move),
            6 => Some(Opcode::GetList),
            7 => Some(Opcode::GetTail),
            8 => Some(Opcode::Allocate),
            9 => Some(Opcode::Deallocate),
            10 => Some(Opcode::PutTerms),
            11 => Some(Opcode::BadMatch),
            12 => Some(Opcode::CaseHead),
            13 => Some(Opcode::CaseTail),
            14 => Some(Opcode::Select),
            15 => Some(Opcode::FuncInfo),
            16 => Some(Opcode::Try),
            17 => Some(Opcode::TryEnd),
            18 => Some(Opcode::TryMatch),
            19 => Some(Opcode::TryClear),
            20 => Some(Opcode::Raise),
            21 => Some(Opcode::Gc),
            22 => Some(Opcode::PutMap),
            23 => Some(Opcode::MakeFun),
            24 => Some(Opcode::TestHeap),
            25 => Some(Opcode::ResetHeap),
            26 => Some(Opcode::Catch),
            27 => Some(Opcode::CatchEnd),
            28 => Some(Opcode::QueryGreeting),
            29 => Some(Opcode::ReturnToTrace),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_new() {
        let module = ModuleCode::new(Atom::new(5));
        assert_eq!(module.name, Atom::new(5));
        assert_eq!(module.version, 0);
        assert!(module.exports.is_empty());
    }

    #[test]
    fn test_add_atom() {
        let mut module = ModuleCode::new(Atom::new(42));
        let atom = module.add_atom("foo");
        assert_eq!(atom, Atom::new(1));
        assert_eq!(module.atoms.len(), 2);
        assert_eq!(module.atom_names.len(), 1); // Module name not stored in names
        assert_eq!(module.atoms[1], Atom::new(1));
    }

    #[test]
    fn test_add_export() {
        let mut module = ModuleCode::new(Atom::new(1));
        module.add_export(Atom::new(2), 3, 100);
        assert_eq!(module.exports.len(), 1);

        let mfa = Mfa::new(Atom::new(1), Atom::new(2), 3);
        assert_eq!(module.get_export_offset(&mfa), Some(100));
    }

    #[test]
    fn test_opcode_from_u8() {
        assert_eq!(Opcode::from_u8(1), Some(Opcode::Call));
        assert_eq!(Opcode::from_u8(3), Some(Opcode::Return));
        assert_eq!(Opcode::from_u8(4), Some(Opcode::Jump));
        assert_eq!(Opcode::from_u8(99), None);
    }
}
