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
pub mod mfa;
pub mod module;
pub mod iff;

// Re-exports for convenience
pub use atom::Atom;
pub use mfa::Mfa;
pub use module::{ModuleCode, ExportEntry, ImportEntry, Literal, FunLiteral, LineInfo, Opcode};
pub use iff::{BeamFile, Chunk, BEAM_MAGIC, chunk_types, CHUNK_ATOM, CHUNK_CODE, CHUNK_EXPT, CHUNK_IMPT};

/// Result type for BEAM operations
pub type Result<T> = std::result::Result<T, std::io::Error>;