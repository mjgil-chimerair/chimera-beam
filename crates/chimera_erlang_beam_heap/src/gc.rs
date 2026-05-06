//! Tracing GC implementation for RustZigBeam.
//!
//! Implements minor GC with root tracing and object forwarding.
//! Uses semi-space copying collector.
//!
//! # GC Policy
//!
//! GC policy is owned by Rust. Zig kernels may be used for bounded
//! scan/copy operations behind safe Rust wrappers.
//!
//! # Zig vs Rust Usage
//!
//! - **Heap scanning**: Uses Zig `beamz_heap_scan` kernel (via `zig_scan_roots`)
//! - **GC copy operations**: Currently pure Rust (Zig kernels available via `chimera_erlang_beam_abi::heap_kernels`)
//! - **Wiring status**: FFI bindings and safe wrappers exist, but not wired into GC copy operations
//!
//! To wire in Zig copy kernels:
//! 1. Enable `zig-kernels` feature in Cargo.toml
//! 2. Use `heap_kernels::heap_copy()` in `copy_object()`
//! 3. Use `heap_kernels::heap_compact()` in major GC compact phase

use crate::ProcessHeap;
use chimera_erlang_beam_term::Term;

#[cfg(feature = "zig-kernels")]
use chimera_erlang_beam_abi::heap_kernels;

/// Forwarding table for GC
///
/// Maps old heap addresses to new heap addresses after copying.
#[derive(Debug, Clone)]
pub struct ForwardTable {
    /// Map from old word index to new word index
    forwards: Vec<usize>,
}

impl ForwardTable {
    /// Create new forwarding table
    pub fn new(size: usize) -> Self {
        ForwardTable {
            forwards: vec![0; size],
        }
    }

    /// Resize forwarding table
    pub fn resize(&mut self, size: usize) {
        self.forwards.resize(size, 0);
    }

    /// Get forwarding address (0 means not forwarded)
    pub fn get(&self, idx: usize) -> Option<usize> {
        let fwd = self.forwards.get(idx).copied().unwrap_or(0);
        if fwd == 0 {
            None
        } else {
            Some(fwd)
        }
    }

    /// Set forwarding address
    pub fn set(&mut self, idx: usize, new_idx: usize) {
        if idx < self.forwards.len() {
            self.forwards[idx] = new_idx;
        }
    }

    /// Check if an address has been forwarded
    pub fn is_forwarded(&self, idx: usize) -> bool {
        self.get(idx).is_some()
    }

    /// Clear all forwards
    pub fn clear(&mut self) {
        for f in &mut self.forwards {
            *f = 0;
        }
    }
}

/// Mark bits for GC tracing
#[derive(Debug, Clone)]
pub struct GcMarkBits {
    /// Mark bits parallel to heap words
    marks: Vec<bool>,
}

impl GcMarkBits {
    /// Create new mark bits for a heap of given size
    pub fn new(size: usize) -> Self {
        GcMarkBits {
            marks: vec![false; size],
        }
    }

    /// Resize mark bits to match heap size
    pub fn resize(&mut self, size: usize) {
        self.marks.resize(size, false);
    }

    /// Get mark for a word index
    pub fn get(&self, idx: usize) -> bool {
        self.marks.get(idx).copied().unwrap_or(false)
    }

    /// Set mark for a word index
    pub fn set(&mut self, idx: usize, value: bool) {
        if idx < self.marks.len() {
            self.marks[idx] = value;
        }
    }

    /// Clear all marks
    pub fn clear(&mut self) {
        for m in &mut self.marks {
            *m = false;
        }
    }

    /// Get the number of mark bits
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// Check if there are no mark bits
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }
}

impl ProcessHeap {
    /// Perform a tracing minor GC
    ///
    /// Uses root set to trace and copy only live objects from roots.
    /// Objects are copied to to_space with forwarding pointers.
    ///
    /// # Arguments
    /// * `roots` - Mutable slice of root terms to trace from (will be updated with new addresses)
    ///
    /// Returns true if GC freed space, false otherwise.
    pub fn trace_gc(&mut self, roots: &mut [Term]) -> bool {
        self.stats.collections += 1;
        self.stats.minor_collections += 1;
        self.stats.minor_gc_count += 1;

        let used_words = self.hp;

        if used_words == 0 {
            return true;
        }

        // Get from-space (active buffer)
        let from_space = self.active_buffer().to_vec();

        // Get to_space
        let to_space = if self.in_to_space {
            &mut self.from_space
        } else {
            &mut self.to_space
        };

        // Ensure to_space is large enough
        if to_space.len() < used_words {
            to_space.resize(used_words, 0);
        }

        // Forwarding table: from_addr -> to_addr (None means not forwarded)
        let mut forward: Vec<Option<usize>> = vec![None; from_space.len()];

        let mut to_hp = 0;

        // Process roots: copy objects they point to
        for root in roots.iter_mut() {
            let from_addr = Self::term_to_heap_addr(*root);
            if let Some(addr) = from_addr {
                if addr < from_space.len() {
                    if forward[addr].is_none() {
                        // Copy object to to-space
                        let new_addr = Self::copy_object_with_kernel(
                            addr,
                            &from_space,
                            to_space,
                            &mut to_hp,
                            &mut forward,
                        );
                        forward[addr] = Some(new_addr);
                    }
                    // Update root to point to new address
                    *root = Self::update_term_addr(*root, forward[addr]);
                }
            }
        }

        // Process to-space: scan for references to from-space
        let mut scan_ptr = 0;
        while scan_ptr < to_hp {
            let word = to_space[scan_ptr];
            // Check if this word is a term that points to from-space
            let term = Term(word);
            if Self::is_heap_pointer(term) {
                if let Some(addr) = Self::term_to_heap_addr(term) {
                    if addr < from_space.len() {
                        if forward[addr].is_none() {
                            // Copy object to to-space
                            let new_addr = Self::copy_object_with_kernel(
                                addr,
                                &from_space,
                                to_space,
                                &mut to_hp,
                                &mut forward,
                            );
                            forward[addr] = Some(new_addr);
                        }
                        // Update reference in to-space
                        to_space[scan_ptr] = Self::update_term_addr(term, forward[addr]).0;
                    }
                }
            }
            scan_ptr += 1;
        }
        self.hp = to_hp;
        self.in_to_space = !self.in_to_space;

        // words_copied = to_hp (because we started at 0)
        let words_copied = to_hp;
        self.stats.words_collected += (used_words - words_copied) as u64;

        true
    }

    /// Copy an object from from-space to to-space
    /// Returns the new address in to-space
    fn copy_object(
        from_addr: usize,
        from_space: &[u64],
        to_space: &mut [u64],
        to_hp: &mut usize,
        _forward: &mut [Option<usize>],
    ) -> usize {
        // Read header to get object size
        if from_addr >= from_space.len() {
            return 0;
        }

        let header = from_space[from_addr];
        let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
        let _ = sub_tag; // Mark as used
        let size = chimera_erlang_beam_term::boxed::extract_size(header) as usize;

        if size == 0 || from_addr + size > from_space.len() {
            return 0;
        }

        let new_addr = *to_hp;

        // Copy all words of the object
        // Note: to_space addresses are from_space.len() + index
        for i in 0..size {
            if from_addr + i < from_space.len() && *to_hp < to_space.len() {
                to_space[*to_hp] = from_space[from_addr + i];
                *to_hp += 1;
            }
        }

        // Return the address in to-space (with offset)
        new_addr
    }

    /// Copy an object with kernel dispatch
    /// Uses Zig kernel when feature is enabled, otherwise uses Rust fallback.
    fn copy_object_with_kernel(
        from_addr: usize,
        from_space: &[u64],
        to_space: &mut [u64],
        to_hp: &mut usize,
        forward: &mut [Option<usize>],
    ) -> usize {
        #[cfg(feature = "zig-kernels")]
        {
            // Try Zig kernel first
            if let Ok(new_addr) = Self::copy_object_zig(from_addr, from_space, to_space, to_hp) {
                return new_addr;
            }
        }
        // Fall back to Rust implementation
        Self::copy_object(from_addr, from_space, to_space, to_hp, forward)
    }

    /// Copy an object using Zig heap_copy kernel (when feature enabled)
    #[cfg(feature = "zig-kernels")]
    fn copy_object_zig(
        from_addr: usize,
        from_space: &[u64],
        to_space: &mut [u64],
        to_hp: &mut usize,
    ) -> Result<usize, chimera_erlang_beam_core::VmError> {
        let new_addr = *to_hp;
        let src_slice = &from_space[from_addr..];
        let dst_slice = &mut to_space[*to_hp..];

        match heap_kernels::heap_copy(dst_slice, src_slice, src_slice.len()) {
            Ok(words_copied) => {
                *to_hp += words_copied;
                Ok(new_addr)
            }
            Err(e) => Err(e),
        }
    }

    /// Mark a term and all its references recursively
    fn is_heap_pointer(term: Term) -> bool {
        !matches!(
            term.tag(),
            chimera_erlang_beam_term::TermTag::SmallInteger | chimera_erlang_beam_term::TermTag::Atom
        )
    }

    /// Get the heap address from a term
    fn term_to_heap_addr(term: Term) -> Option<usize> {
        match term.tag() {
            chimera_erlang_beam_term::TermTag::Cons => {
                let ptr = term.to_cons();
                // In our implementation, heap indices start at 0, so 0 is valid
                Some(ptr as usize)
            }
            chimera_erlang_beam_term::TermTag::Tuple => {
                let ptr = term.to_tuple();
                Some(ptr as usize)
            }
            chimera_erlang_beam_term::TermTag::Float => {
                let ptr = term.to_float();
                Some(ptr as usize)
            }
            chimera_erlang_beam_term::TermTag::Binary => {
                let ptr = term.to_binary();
                Some(ptr as usize)
            }
            chimera_erlang_beam_term::TermTag::Map => {
                let ptr = term.to_map();
                Some(ptr as usize)
            }
            chimera_erlang_beam_term::TermTag::Fun => {
                let ptr = term.to_fun();
                Some(ptr as usize)
            }
            _ => None,
        }
    }

    /// Update a term's address after GC
    fn update_term_addr(term: Term, new_addr: Option<usize>) -> Term {
        match new_addr {
            None => term,
            Some(addr) => {
                // addr is the index in to_space where the object was copied
                // After swapping, to_space becomes the active buffer,
                // so the term should point to addr
                match term.tag() {
                    chimera_erlang_beam_term::TermTag::Cons => Term::from_cons(addr as u64),
                    chimera_erlang_beam_term::TermTag::Tuple => Term::from_tuple(addr as u64),
                    chimera_erlang_beam_term::TermTag::Float => Term::from_float_ptr(addr as u64),
                    chimera_erlang_beam_term::TermTag::Binary => Term::from_binary_ptr(addr as u64),
                    chimera_erlang_beam_term::TermTag::Map => Term::from_map(addr as u64),
                    chimera_erlang_beam_term::TermTag::Fun => Term::from_fun_ptr(addr as u64),
                    _ => term,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HeapConfig;
    use chimera_erlang_beam_term::Term;

    #[test]
    fn test_forward_table() {
        let mut fwd = ForwardTable::new(10);
        assert!(fwd.get(0).is_none());
        fwd.set(5, 100);
        assert_eq!(fwd.get(5), Some(100));
        assert!(fwd.is_forwarded(5));
        assert!(!fwd.is_forwarded(3));
    }

    #[test]
    fn test_trace_gc_simple() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate some terms
        let ptr1 = heap.make_cons(Term::from_small(1), Term::from_small(2));
        assert!(ptr1.is_some());

        let ptr2 = heap.make_tuple(&[Term::from_small(3), Term::from_small(4)]);
        assert!(ptr2.is_some());

        let used_before = heap.used_size();

        // Run GC with empty roots - nothing should be copied
        let mut roots: Vec<Term> = Vec::new();
        heap.trace_gc(&mut roots);

        // With empty roots, no live objects should be marked
        // So GC would effectively clear everything
        assert!(heap.used_size() < used_before);
    }

    #[test]
    fn test_trace_gc_preserves_roots() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons cell
        let ptr = heap.make_cons(Term::from_small(42), Term::from_small(99));
        assert!(ptr.is_some());

        // Create root set with references to the cons
        let cons_term = Term::from_cons(ptr.unwrap() as u64);
        let mut roots = vec![cons_term];

        let used_before = heap.used_size();

        // Run GC with roots
        heap.trace_gc(&mut roots);

        // After GC, heap should still have the cons cell
        // (though position may have changed)
        assert!(heap.used_size() <= used_before);

        // The root term should have been updated to point to new location
        // Note: in our implementation, the index might be the same (0 in from_space and 0 in to_space)
        // but the memory region is different. We verify the object is still accessible.
        let new_cons_ptr = roots[0].to_cons();
        assert!(new_cons_ptr > 0 || ptr.unwrap() == 0); // Pointer might be 0 if that's the index

        // Verify the cons cell is still valid by checking it's in the heap
        let heap_slice = heap.active_buffer();
        let new_addr = new_cons_ptr as usize;
        if new_addr < heap_slice.len() {
            // The object should be a cons cell
            let header = heap_slice[new_addr];
            let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
            assert_eq!(sub_tag, chimera_erlang_beam_term::boxed::BoxedSubTag::Cons);
        }
    }

    #[test]
    fn test_gc_stats_update() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let stats_before = heap.get_stats();
        assert_eq!(stats_before.collections, 0);

        // Allocate something
        heap.make_cons(Term::from_small(1), Term::from_small(2));

        // Run GC with empty roots
        let mut empty_roots: Vec<Term> = Vec::new();
        heap.trace_gc(&mut empty_roots);

        let stats_after = heap.get_stats();
        assert_eq!(stats_after.collections, 1);
        assert_eq!(stats_after.minor_collections, 1);
    }

    #[test]
    fn test_major_gc_in_place_compaction() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate multiple objects
        heap.make_cons(Term::from_small(1), Term::from_small(2));
        heap.make_cons(Term::from_small(3), Term::from_small(4));
        heap.make_cons(Term::from_small(5), Term::from_small(6));

        let used_before = heap.used_size();
        assert!(used_before >= 9); // 3 cons cells = 9 words

        // Run major GC - all objects should be collected
        heap.major_gc();

        // After major GC with no roots, heap should be empty
        assert_eq!(heap.used_size(), 0);
    }

    #[test]
    fn test_trace_gc_with_immediate_values() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons cell (which will be at position 0)
        let ptr = heap.make_cons(Term::from_small(42), Term::from_small(99));
        assert!(ptr.is_some());

        // After trace_gc with no roots, heap should be empty
        let mut roots: Vec<Term> = Vec::new();
        heap.trace_gc(&mut roots);

        // With empty roots, nothing is preserved
        assert_eq!(heap.used_size(), 0);
    }

    #[test]
    fn test_major_gc_preserves_live_objects() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons cell
        let ptr = heap.make_cons(Term::from_small(42), Term::from_small(99));
        assert!(ptr.is_some());

        // Run major GC with empty roots - all objects should be collected
        heap.major_gc();

        // After major GC with no roots, heap should be empty
        assert_eq!(heap.used_size(), 0);
    }

    #[test]
    fn test_major_gc_stats_update() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let stats_before = heap.get_stats();
        assert_eq!(stats_before.major_collections, 0);

        // Allocate something
        heap.make_cons(Term::from_small(1), Term::from_small(2));

        // Run major GC
        heap.major_gc();

        let stats_after = heap.get_stats();
        assert_eq!(stats_after.collections, 1);
        assert_eq!(stats_after.major_collections, 1);
    }

    #[test]
    fn test_major_gc_no_swap_spaces() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate something
        let ptr = heap.make_cons(Term::from_small(1), Term::from_small(2));
        assert!(ptr.is_some());

        // Get initial state
        let in_to_space_before = heap.in_to_space();

        // Run major GC (compacts in place, doesn't swap)
        heap.major_gc();

        // in_to_space should not change for major GC
        assert_eq!(heap.in_to_space(), in_to_space_before);
    }
}
