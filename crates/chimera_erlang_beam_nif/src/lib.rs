//! NIF (Native Implemented Functions) support for RustZigBeam.
//!
//! Provides infrastructure for loading and calling native functions
//! implemented in Rust, similar to Erlang/OTP's ErlNif API.
//!
//! Per task-3.md Task B-1: Implement NIF Framework.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_heap::ProcessHeap;
use chimera_erlang_beam_term::{Term, TermTag};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Result type for NIF calls
pub type NifResult = Result<Term, NifError>;

/// Error types for NIF operations
#[derive(Debug, Clone, Copy)]
pub enum NifError {
    /// Generic bad argument error
    BadArg,
    /// Function not found
    NotFound,
    /// System limit reached
    SystemLimit,
    /// Library load failed
    LoadFailed,
    /// NIF raised an exception
    Exception(Term),
}

impl NifError {
    pub fn to_term(&self) -> Term {
        match self {
            NifError::BadArg => Term::from_atom(6),      // ATOM_BADARG
            NifError::NotFound => Term::from_atom(3),    // ATOM_UNDEFINED
            NifError::SystemLimit => Term::from_atom(3), // ATOM_UNDEFINED
            NifError::LoadFailed => Term::from_atom(5),  // ATOM_ERROR
            NifError::Exception(t) => *t,
        }
    }
}

/// NIF environment - provides context for NIF calls
///
/// Analogous to ErlNifEnv in OTP. Contains:
/// - Access to the calling process
/// - Resource handle for the current NIF call
/// - Thread-local state
pub struct NifEnv {
    /// PID of the calling process
    pub pid: u32,
    /// Current module being executed
    pub current_module: Option<u32>,
    /// Current function being called
    pub current_function: Option<u32>,
    /// Call depth for nested NIF calls
    pub call_depth: u32,
    /// Reference to the process heap for allocations
    heap: Option<*mut ProcessHeap>,
}

impl NifEnv {
    pub fn new(pid: u32) -> Self {
        NifEnv {
            pid,
            current_module: None,
            current_function: None,
            call_depth: 0,
            heap: None,
        }
    }

    /// Create a NifEnv with heap access for a process
    pub fn with_heap(pid: u32, heap: *mut ProcessHeap) -> Self {
        NifEnv {
            pid,
            current_module: None,
            current_function: None,
            call_depth: 0,
            heap: Some(heap),
        }
    }

    /// Get mutable access to the process heap if available
    ///
    /// # Safety
    ///
    /// The raw pointer `h` must have been created from a valid mutable reference
    /// to a `ProcessHeap` that remains valid and is not accessed through other
    /// references while the returned reference is live.
    pub fn heap_mut(&mut self) -> Option<&mut ProcessHeap> {
        self.heap.map(|h| unsafe { &mut *h })
    }
}

/// NIF term representation within a NIF context
///
/// Unlike regular Terms, NifTerm can be validated and
/// converted to/from Erlang terms safely.
#[derive(Debug, Clone, Copy)]
pub struct NifTerm(u64);

impl NifTerm {
    /// Convert to a regular Term
    pub fn to_term(self) -> Term {
        Term(self.0)
    }

    /// Create from a regular Term
    pub fn from_term(term: Term) -> Self {
        NifTerm(term.0)
    }

    /// Get the raw u64 value
    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Trait for NIF callback functions
///
/// Implement this trait to define a NIF function.
/// The callback receives a NifEnv and arguments, and returns NifResult.
pub trait NifCallback: Send + Sync {
    fn call(&self, env: &mut NifEnv, argc: usize, argv: *const Term) -> NifResult;
}

/// A single NIF function entry
pub struct NifEntry {
    /// Function name
    pub name: String,
    /// arity
    pub arity: u8,
    /// Callback implementation
    pub callback: Arc<dyn NifCallback>,
}

/// A library of NIF functions for a module
pub struct NifLibrary {
    /// Module name
    pub name: String,
    /// NIF functions indexed by {function_name}_{arity}
    entries: HashMap<String, NifEntry>,
}

impl NifLibrary {
    pub fn new(name: &str) -> Self {
        NifLibrary {
            name: name.to_string(),
            entries: HashMap::new(),
        }
    }

    /// Register a NIF function
    pub fn add(&mut self, name: &str, arity: u8, callback: Arc<dyn NifCallback>) {
        let key = format!("{}_{}", name, arity);
        self.entries.insert(
            key,
            NifEntry {
                name: name.to_string(),
                arity,
                callback,
            },
        );
    }

    /// Look up a NIF by name and arity
    pub fn get(&self, name: &str, arity: u8) -> Option<&NifEntry> {
        let key = format!("{}_{}", name, arity);
        self.entries.get(&key)
    }
}

/// Global NIF registry
struct NifRegistry {
    libraries: HashMap<String, NifLibrary>,
}

impl NifRegistry {
    fn new() -> Self {
        NifRegistry {
            libraries: HashMap::new(),
        }
    }
}

static NIF_REGISTRY: RwLock<Option<NifRegistry>> = RwLock::new(None);

/// Initialize the NIF registry
pub fn init() {
    let mut guard = NIF_REGISTRY.write().unwrap();
    *guard = Some(NifRegistry::new());
}

/// Load a NIF library from a shared object
///
/// In a full implementation, this would dynamically load a .so file.
/// For now, this is a stub that returns an error.
pub fn load_nif(_path: &str) -> Result<(), NifError> {
    // Stub implementation - would load .so and call its on_load
    Err(NifError::LoadFailed)
}

/// Call a NIF function
pub fn call_nif(
    module: &str,
    function: &str,
    arity: u8,
    env: &mut NifEnv,
    argc: usize,
    argv: *const Term,
) -> NifResult {
    let guard = NIF_REGISTRY.read().unwrap();
    if let Some(ref registry) = *guard {
        if let Some(library) = registry.libraries.get(module) {
            if let Some(entry) = library.get(function, arity) {
                return entry.callback.call(env, argc, argv);
            }
        }
    }
    Err(NifError::NotFound)
}

// =====================================================================
// Built-in NIF Implementations (Task B-2)
// =====================================================================
// These NIFs are implemented in Rust for performance.
// They mirror the BEAM BIF equivalents but run as NIFs.

// SetElement NIF: Returns a new tuple with element at Index replaced
// erlang:setelement(Index, Tuple, Value)
// Index is 1-based (Erlang semantics)
struct SetElementNif;
impl NifCallback for SetElementNif {
    fn call(&self, env: &mut NifEnv, argc: usize, argv: *const Term) -> NifResult {
        if argc != 3 {
            return Err(NifError::BadArg);
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };

        let index = args[0];
        let tuple = args[1];
        let value = args[2];

        // Validate inputs
        if !index.is_small() {
            return Err(NifError::BadArg);
        }
        if tuple.tag() != TermTag::Tuple {
            return Err(NifError::BadArg);
        }

        let idx = index.to_small() as i64;
        if idx < 1 {
            return Err(NifError::BadArg);
        }

        // Get heap access
        let Some(heap) = env.heap_mut() else {
            return Err(NifError::BadArg); // No heap available
        };

        // Decode tuple pointer (word index on heap)
        let tuple_ptr = tuple.to_tuple() as usize;

        // Read tuple arity from header at tuple_ptr
        let header = heap.get_word(tuple_ptr).ok_or(NifError::BadArg)?;
        // Size is stored in bits 8-31 of the boxed header
        let size = (header >> 8) & 0xFFFFFF;
        let arity = size.saturating_sub(1) as u32;

        // Validate index is within bounds (1-based, max is arity)
        let replace_idx = idx as u32;
        if replace_idx == 0 || replace_idx > arity {
            return Err(NifError::BadArg);
        }

        // Read all elements from the tuple first
        let mut elements = Vec::with_capacity(arity as usize);
        for i in 0..arity {
            let elem_word = heap
                .get_word(tuple_ptr + 1 + i as usize)
                .ok_or(NifError::BadArg)?;
            elements.push(Term(elem_word));
        }

        // Replace the element at the given index (1-based)
        elements[(replace_idx - 1) as usize] = value;

        // Allocate new tuple and write elements
        let Some(new_pos) = heap.make_tuple(&elements) else {
            return Err(NifError::SystemLimit);
        };

        Ok(Term::from_tuple(new_pos as u64))
    }
}

// TupleToList NIF: Converts a tuple to a list
// erlang:tuple_to_list(Tuple)
struct TupleToListNif;
impl NifCallback for TupleToListNif {
    fn call(&self, env: &mut NifEnv, argc: usize, argv: *const Term) -> NifResult {
        if argc != 1 {
            return Err(NifError::BadArg);
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let tuple = args[0];

        if tuple.tag() != TermTag::Tuple {
            return Err(NifError::BadArg);
        }

        // Get heap access
        let Some(heap) = env.heap_mut() else {
            return Err(NifError::BadArg); // No heap available
        };

        // Decode tuple pointer (word index on heap)
        let tuple_ptr = tuple.to_tuple() as usize;

        // Read tuple arity from header at tuple_ptr
        let header = heap.get_word(tuple_ptr).ok_or(NifError::BadArg)?;
        let size = (header >> 8) & 0xFFFFFF;
        let arity = size.saturating_sub(1) as u32;

        // Read all elements first
        let mut elements = Vec::with_capacity(arity as usize);
        for i in 0..arity {
            let elem_word = heap
                .get_word(tuple_ptr + 1 + i as usize)
                .ok_or(NifError::BadArg)?;
            elements.push(Term(elem_word));
        }

        // Build list from back to front (last element becomes nil)
        // We need to iterate backwards and build cons cells
        let mut list_term = Term::nil(); // Start with nil (empty list)

        for elem in elements.into_iter().rev() {
            // Prepend elem to list by creating a cons cell
            if let Some(pos) = heap.make_cons(elem, list_term) {
                list_term = Term::from_cons(pos as u64);
            } else {
                return Err(NifError::SystemLimit);
            }
        }

        Ok(list_term)
    }
}

// ListToTuple NIF: Converts a list to a tuple
// erlang:list_to_tuple(List)
struct ListToTupleNif;
impl NifCallback for ListToTupleNif {
    fn call(&self, env: &mut NifEnv, argc: usize, argv: *const Term) -> NifResult {
        if argc != 1 {
            return Err(NifError::BadArg);
        }
        let args = unsafe { std::slice::from_raw_parts(argv, argc) };
        let list = args[0];

        // list_to_tuple accepts only proper lists (cons cells or nil)
        // An atom other than nil is not a valid list
        if !list.is_cons() && !list.is_nil() {
            return Err(NifError::BadArg);
        }

        // Empty list returns empty tuple
        if list.is_nil() {
            // Get heap access
            let Some(heap) = env.heap_mut() else {
                return Err(NifError::BadArg); // No heap available
            };
            // Allocate empty tuple on heap
            let Some(pos) = heap.make_tuple(&[]) else {
                return Err(NifError::SystemLimit);
            };
            return Ok(Term::from_tuple(pos as u64));
        }

        // Get heap access
        let Some(heap) = env.heap_mut() else {
            return Err(NifError::BadArg); // No heap available
        };

        // First pass: count elements in the list
        let mut count = 0;
        let mut current = list;
        loop {
            if current.is_nil() {
                break;
            }
            if !current.is_cons() {
                return Err(NifError::BadArg); // Malformed list
            }
            count += 1;
            // Get tail of cons cell
            let cons_ptr = current.to_cons() as usize;
            let tl_word = heap.get_word(cons_ptr + 2).ok_or(NifError::BadArg)?;
            current = Term(tl_word);
        }

        // Second pass: collect elements
        let mut elements = Vec::with_capacity(count);
        current = list;
        loop {
            if current.is_nil() {
                break;
            }
            let cons_ptr = current.to_cons() as usize;
            // Get head (word at cons_ptr + 1)
            let hd_word = heap.get_word(cons_ptr + 1).ok_or(NifError::BadArg)?;
            elements.push(Term(hd_word));
            // Get tail
            let tl_word = heap.get_word(cons_ptr + 2).ok_or(NifError::BadArg)?;
            current = Term(tl_word);
        }

        // Allocate tuple with collected elements
        let Some(pos) = heap.make_tuple(&elements) else {
            return Err(NifError::SystemLimit);
        };

        Ok(Term::from_tuple(pos as u64))
    }
}

/// Register all built-in NIFs
pub fn init_builtin_nifs() {
    let mut guard = NIF_REGISTRY.write().unwrap();
    if let Some(ref mut registry) = *guard {
        let mut erlang_lib = NifLibrary::new("erlang");
        erlang_lib.add("setelement", 3, Arc::new(SetElementNif));
        erlang_lib.add("tuple_to_list", 1, Arc::new(TupleToListNif));
        erlang_lib.add("list_to_tuple", 1, Arc::new(ListToTupleNif));
        registry.libraries.insert("erlang".to_string(), erlang_lib);
    } else {
        // Initialize registry if not yet initialized
        let mut registry = NifRegistry::new();
        let mut erlang_lib = NifLibrary::new("erlang");
        erlang_lib.add("setelement", 3, Arc::new(SetElementNif));
        erlang_lib.add("tuple_to_list", 1, Arc::new(TupleToListNif));
        erlang_lib.add("list_to_tuple", 1, Arc::new(ListToTupleNif));
        registry.libraries.insert("erlang".to_string(), erlang_lib);
        *guard = Some(registry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chimera_erlang_beam_heap::{HeapConfig, ProcessHeap};

    #[test]
    fn test_nif_env_new() {
        let env = NifEnv::new(42);
        assert_eq!(env.pid, 42);
        assert_eq!(env.call_depth, 0);
    }

    #[test]
    fn test_nif_env_with_heap() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let heap_ptr = &mut heap as *mut ProcessHeap;
        let mut env = NifEnv::with_heap(42, heap_ptr);
        assert_eq!(env.pid, 42);
        assert!(env.heap_mut().is_some());
    }

    #[test]
    fn test_nif_term_conversion() {
        let term = Term::from_small(42);
        let nif_term = NifTerm::from_term(term);
        assert_eq!(nif_term.to_term(), term);
    }

    #[test]
    fn test_nif_library_add_get() {
        use std::sync::Arc;

        let mut lib = NifLibrary::new("test");
        lib.add("foo", 1, Arc::new(TestNif));
        assert!(lib.get("foo", 1).is_some());
        assert!(lib.get("foo", 2).is_none());
        assert!(lib.get("bar", 1).is_none());
    }

    struct TestNif;
    impl NifCallback for TestNif {
        fn call(&self, _env: &mut NifEnv, _argc: usize, _argv: *const Term) -> NifResult {
            Ok(Term::from_small(42))
        }
    }

    #[test]
    fn test_builtin_nif_setelement_badarg() {
        init_builtin_nifs();
        let mut env = NifEnv::new(1);

        // Test with wrong arity
        let result = call_nif("erlang", "setelement", 3, &mut env, 0, std::ptr::null());
        assert!(matches!(result, Err(NifError::BadArg)));
    }

    #[test]
    fn test_builtin_nif_setelement_no_heap() {
        init_builtin_nifs();
        // NifEnv without heap should return BadArg
        let mut env = NifEnv::new(1);

        // Create valid arguments but no heap
        let tuple = Term::from_tuple(100); // Some tuple pointer
        let args = [Term::from_small(1), tuple, Term::from_small(99)];

        let result = call_nif("erlang", "setelement", 3, &mut env, 3, args.as_ptr());
        assert!(matches!(result, Err(NifError::BadArg)));
    }

    #[test]
    fn test_builtin_nif_setelement_with_heap() {
        init_builtin_nifs();
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let heap_ptr = &mut heap as *mut ProcessHeap;

        // Create a tuple on the heap: {a, b, c}
        let tuple_pos = heap
            .make_tuple(&[
                Term::from_atom(1), // a
                Term::from_atom(2), // b
                Term::from_atom(3), // c
            ])
            .expect("could not allocate tuple");

        let mut env = NifEnv::with_heap(1, heap_ptr);

        // Call setelement(2, {a,b,c}, new_value)
        let args = [
            Term::from_small(2),                // index 2
            Term::from_tuple(tuple_pos as u64), // tuple
            Term::from_atom(99),                // new value
        ];

        let result = call_nif("erlang", "setelement", 3, &mut env, 3, args.as_ptr());
        assert!(result.is_ok());

        // The result should be a tuple with the second element replaced
        let result_term = result.unwrap();
        assert_eq!(result_term.tag(), TermTag::Tuple);

        // Verify the tuple elements
        let result_ptr = result_term.to_tuple() as usize;
        // Read header to get arity
        let header = heap.get_word(result_ptr).unwrap();
        let arity = ((header >> 8) & 0xFFFFFF) as u32 - 1;

        assert_eq!(arity, 3);
        let elem0 = Term(heap.get_word(result_ptr + 1).unwrap());
        let elem1 = Term(heap.get_word(result_ptr + 2).unwrap());
        let elem2 = Term(heap.get_word(result_ptr + 3).unwrap());

        assert_eq!(elem0.to_atom(), 1); // 'a' unchanged
        assert_eq!(elem1.to_atom(), 99); // 'b' replaced with 99
        assert_eq!(elem2.to_atom(), 3); // 'c' unchanged
    }

    #[test]
    fn test_builtin_nif_setelement_out_of_bounds() {
        init_builtin_nifs();
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let heap_ptr = &mut heap as *mut ProcessHeap;

        // Create a 3-element tuple
        let tuple_pos = heap
            .make_tuple(&[Term::from_atom(1), Term::from_atom(2), Term::from_atom(3)])
            .expect("could not allocate tuple");

        let mut env = NifEnv::with_heap(1, heap_ptr);

        // Index 5 is out of bounds for a 3-element tuple
        let args = [
            Term::from_small(5), // index 5 (out of bounds)
            Term::from_tuple(tuple_pos as u64),
            Term::from_atom(99),
        ];

        let result = call_nif("erlang", "setelement", 3, &mut env, 3, args.as_ptr());
        assert!(matches!(result, Err(NifError::BadArg)));
    }

    #[test]
    fn test_builtin_nif_tuple_to_list() {
        init_builtin_nifs();
        let mut env = NifEnv::new(1);

        // Pass an atom term (not a tuple) - should return BadArg
        let atom_term = Term::from_atom(1);
        let args = [atom_term];

        let result = call_nif("erlang", "tuple_to_list", 1, &mut env, 1, args.as_ptr());
        assert!(matches!(result, Err(NifError::BadArg)));
    }

    #[test]
    fn test_builtin_nif_tuple_to_list_with_heap() {
        init_builtin_nifs();
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let heap_ptr = &mut heap as *mut ProcessHeap;

        // Create a tuple {x, y, z}
        let tuple_pos = heap
            .make_tuple(&[
                Term::from_atom(10), // x
                Term::from_atom(20), // y
                Term::from_atom(30), // z
            ])
            .expect("could not allocate tuple");

        let mut env = NifEnv::with_heap(1, heap_ptr);

        let args = [Term::from_tuple(tuple_pos as u64)];
        let result = call_nif("erlang", "tuple_to_list", 1, &mut env, 1, args.as_ptr());
        assert!(result.is_ok());

        // Result should be a list [x, y, z]
        let list_term = result.unwrap();
        assert_eq!(list_term.tag(), TermTag::Cons);

        // Walk the list and verify elements
        let mut current = list_term;
        let mut count = 0;
        loop {
            if current.is_nil() {
                break;
            }
            assert!(current.is_cons());
            let cons_ptr = current.to_cons() as usize;
            let hd = Term(heap.get_word(cons_ptr + 1).unwrap());
            // Verify element
            if count == 0 {
                assert_eq!(hd.to_atom(), 10);
            } else if count == 1 {
                assert_eq!(hd.to_atom(), 20);
            } else if count == 2 {
                assert_eq!(hd.to_atom(), 30);
            }
            let tl = Term(heap.get_word(cons_ptr + 2).unwrap());
            current = tl;
            count += 1;
            if count > 10 {
                break; // Safety limit
            }
        }
        assert_eq!(count, 3); // Should have 3 elements
    }

    #[test]
    fn test_builtin_nif_list_to_tuple() {
        init_builtin_nifs();
        let mut env = NifEnv::new(1);

        // Pass an atom (not a cons cell or nil) - should return BadArg
        let atom_term = Term::from_atom(1);
        let args = [atom_term];

        let result = call_nif("erlang", "list_to_tuple", 1, &mut env, 1, args.as_ptr());
        assert!(matches!(result, Err(NifError::BadArg)));
    }

    #[test]
    fn test_builtin_nif_list_to_tuple_empty() {
        init_builtin_nifs();
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let heap_ptr = &mut heap as *mut ProcessHeap;
        let mut env = NifEnv::with_heap(1, heap_ptr);

        // Empty list should return empty tuple
        let args = [Term::nil()];
        let result = call_nif("erlang", "list_to_tuple", 1, &mut env, 1, args.as_ptr());
        assert!(result.is_ok());

        let result_term = result.unwrap();
        assert_eq!(result_term.tag(), TermTag::Tuple);
        // Empty tuple has arity 0
        let ptr = result_term.to_tuple() as usize;
        let header = heap.get_word(ptr).unwrap();
        // Header size is 1 + arity (for arity 0, header size is 1)
        let size = (header >> 8) & 0xFFFFFF;
        let arity = size.saturating_sub(1) as u32;
        assert_eq!(arity, 0);
    }

    #[test]
    fn test_builtin_nif_list_to_tuple_with_heap() {
        init_builtin_nifs();
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let heap_ptr = &mut heap as *mut ProcessHeap;

        // Build a list [p, q, r]
        // List is built as cons cells: [p | [q | [r | nil]]]
        let nil_term = Term::nil();
        let r_pos = heap
            .make_cons(Term::from_atom(30), nil_term)
            .expect("could not alloc cons");
        let q_pos = heap
            .make_cons(Term::from_atom(20), Term::from_cons(r_pos as u64))
            .expect("could not alloc cons");
        let p_pos = heap
            .make_cons(Term::from_atom(10), Term::from_cons(q_pos as u64))
            .expect("could not alloc cons");
        let list_term = Term::from_cons(p_pos as u64);

        let mut env = NifEnv::with_heap(1, heap_ptr);

        let args = [list_term];
        let result = call_nif("erlang", "list_to_tuple", 1, &mut env, 1, args.as_ptr());
        assert!(result.is_ok());

        // Result should be {p, q, r}
        let result_term = result.unwrap();
        assert_eq!(result_term.tag(), TermTag::Tuple);

        let ptr = result_term.to_tuple() as usize;
        let header = heap.get_word(ptr).unwrap();
        let arity = ((header >> 8) & 0xFFFFFF) as u32 - 1;
        assert_eq!(arity, 3);

        // Verify elements
        assert_eq!(Term(heap.get_word(ptr + 1).unwrap()).to_atom(), 10);
        assert_eq!(Term(heap.get_word(ptr + 2).unwrap()).to_atom(), 20);
        assert_eq!(Term(heap.get_word(ptr + 3).unwrap()).to_atom(), 30);
    }
}
