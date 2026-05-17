//! `chimera_beam_term` - Shared term representation for BEAM-compatible runtime.
//!
//! This crate provides shared types for both the compiler (rustzigelixir) and
//! runtime (rustzigbeam) to use when exchanging compiled code.
//!
//! # Core Types
//!
//! - [`Atom`] - BEAM-compatible atom representation
//! - [`Mfa`] - Module, Function, Arity reference
//! - [`ModuleCode`] - Compiled module representation
//! - [`BeamFile`] - IFF container for BEAM files
//!
//! # Example
//!
//! ```rust
//! use chimera_beam_term::{Atom, Mfa, ModuleCode};
//!
//! let module = ModuleCode::new(Atom::new(1));
//! let mfa = Mfa::new(Atom::new(1), Atom::new(2), 3);
//! ```

// Re-export commonly used types
pub mod atom;
pub mod iff;
pub mod mfa;
pub mod module;

// Re-exports for convenience
pub use atom::Atom;
pub use iff::{
    chunk_types, BeamFile, Chunk, BEAM_MAGIC, CHUNK_ATOM, CHUNK_CODE, CHUNK_EXPT, CHUNK_IMPT,
};
pub use mfa::Mfa;
pub use module::{ExportEntry, FunLiteral, ImportEntry, LineInfo, Literal, ModuleCode, Opcode};

/// Result type for BEAM operations
pub type Result<T> = std::result::Result<T, std::io::Error>;
