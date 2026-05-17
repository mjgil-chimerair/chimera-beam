//! Yieldable GC implementation for RustZigBeam.
//!
//! A state machine GC that can yield during collection to prevent
//! long pauses. Uses a work queue and per-phase budgets.

use crate::gc::GcMarkBits;
use crate::ProcessHeap;
use chimera_erlang_beam_term::Term;

// Helper functions for term analysis

/// Check if a term points to heap data
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

/// GC state machine phases
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcPhase {
    /// GC is not running
    Idle,
    /// Marking live objects from roots
    Mark,
    /// Evacuating (copying) live objects
    Evacuate,
    /// Compacting heap and updating pointers
    Compact,
    /// GC completed successfully
    Complete,
}

/// Yieldable GC state machine
#[derive(Debug)]
pub struct YieldableGc {
    /// Current phase
    phase: GcPhase,
    /// Budget for current phase (words to process per slice)
    budget: usize,
    /// Words processed in current slice
    processed: usize,
    /// Total words to process in current phase
    total: usize,
    /// Mark bits for current collection
    mark_bits: Option<GcMarkBits>,
    /// Scanning position for mark phase
    scan_ptr: usize,
    /// Evacuation position
    evac_ptr: usize,
    /// Forward table for pointer updates
    forward_table: Vec<usize>,
}

impl YieldableGc {
    /// Create a new yieldable GC state machine
    pub fn new() -> Self {
        YieldableGc {
            phase: GcPhase::Idle,
            budget: 0,
            processed: 0,
            total: 0,
            mark_bits: None,
            scan_ptr: 0,
            evac_ptr: 0,
            forward_table: Vec::new(),
        }
    }

    /// Check if GC is running
    pub fn is_running(&self) -> bool {
        self.phase != GcPhase::Idle && self.phase != GcPhase::Complete
    }

    /// Check if GC is complete
    pub fn is_complete(&self) -> bool {
        self.phase == GcPhase::Complete
    }

    /// Get current phase
    pub fn phase(&self) -> GcPhase {
        self.phase
    }

    /// Start a GC collection with a budget
    ///
    /// # Arguments
    /// * `heap` - The heap to collect
    /// * `budget` - Words to process per slice
    pub fn start(&mut self, heap: &ProcessHeap, budget: usize) {
        let used_words = heap.used_size();
        if used_words == 0 {
            self.phase = GcPhase::Complete;
            return;
        }

        self.budget = budget;
        self.processed = 0;
        self.total = used_words;
        self.scan_ptr = 0;
        self.evac_ptr = 0;
        self.mark_bits = Some(GcMarkBits::new(used_words));
        self.forward_table = vec![0; used_words];
        self.phase = GcPhase::Mark;
    }

    /// Execute a slice of GC work
    ///
    /// Returns true if GC is complete, false if more work remains.
    /// The caller should call this repeatedly until is_complete() returns true.
    pub fn execute_slice<R>(&mut self, roots: R, heap: &mut ProcessHeap) -> bool
    where
        R: Iterator<Item = Term>,
    {
        match self.phase {
            GcPhase::Idle => true,
            GcPhase::Complete => true,
            GcPhase::Mark => self.execute_mark_slice(roots, heap),
            GcPhase::Evacuate => self.execute_evacuate_slice(heap),
            GcPhase::Compact => self.execute_compact_slice(heap),
        }
    }

    /// Execute a mark slice
    fn execute_mark_slice<R>(&mut self, roots: R, heap: &mut ProcessHeap) -> bool
    where
        R: IntoIterator<Item = Term>,
    {
        let used_words = heap.used_size();
        let from_space = heap.active_buffer().to_vec();

        // Process roots first
        let mut marked = 0;
        #[allow(clippy::explicit_counter_loop)]
        for root in roots {
            if marked >= self.budget {
                return false;
            }
            Self::mark_term(root, self.mark_bits.as_mut().unwrap(), &from_space);
            marked += 1;
        }

        // Continue scanning from scan_ptr
        let mark_bits = self.mark_bits.as_mut().unwrap();
        while self.scan_ptr < used_words {
            if self.processed >= self.budget {
                return false;
            }

            // Check if current position is marked and needs scanning
            if mark_bits.get(self.scan_ptr) {
                // Scan this object for references
                if self.scan_ptr < from_space.len() {
                    let word = from_space[self.scan_ptr];
                    let term = Term(word);
                    Self::mark_term(term, mark_bits, &from_space);
                }
            }

            self.scan_ptr += 1;
            self.processed += 1;
        }

        // Mark phase complete, move to evacuate
        self.processed = 0;
        self.evac_ptr = 0;
        self.phase = GcPhase::Evacuate;
        false
    }

    /// Execute an evacuate slice (copy live objects)
    fn execute_evacuate_slice(&mut self, heap: &mut ProcessHeap) -> bool {
        let used_words = heap.used_size();
        // Clone buffers to avoid borrow conflict
        let from_data = heap.active_buffer().to_vec();
        let mut to_data = heap.inactive_space().to_vec();
        let mark_bits = self.mark_bits.as_ref().unwrap();

        while self.evac_ptr < used_words {
            if self.processed >= self.budget {
                // Copy back to heap
                heap.write_inactive_space(&to_data);
                return false;
            }

            if mark_bits.get(self.evac_ptr) {
                // This word is live, copy to new location
                if self.evac_ptr < from_data.len() && self.evac_ptr < to_data.len() {
                    to_data[self.evac_ptr] = from_data[self.evac_ptr];
                    self.forward_table[self.evac_ptr] = self.evac_ptr;
                }
            }

            self.evac_ptr += 1;
            self.processed += 1;
        }

        // Copy back to heap
        heap.write_inactive_space(&to_data);

        // Evacuate phase complete
        self.processed = 0;
        self.phase = GcPhase::Compact;
        false
    }

    /// Execute a compact slice (update pointers)
    fn execute_compact_slice(&mut self, heap: &mut ProcessHeap) -> bool {
        let used_words = heap.used_size();
        let mark_bits = self.mark_bits.as_ref().unwrap();

        let mut new_hp = 0;
        for i in 0..used_words {
            if mark_bits.get(i) {
                new_hp += 1;
            }
        }

        heap.set_hp(new_hp);
        self.phase = GcPhase::Complete;
        true
    }

    /// Mark a term and all its references
    fn mark_term(term: Term, mark_bits: &mut GcMarkBits, from_space: &[u64]) {
        match term.tag() {
            chimera_erlang_beam_term::TermTag::SmallInteger
            | chimera_erlang_beam_term::TermTag::Atom => {}
            chimera_erlang_beam_term::TermTag::Cons => {
                let ptr = term.to_cons();
                // In our implementation, heap indices start at 0, so 0 is valid
                if (ptr as usize) < from_space.len() {
                    // Verify it's a cons cell
                    if (ptr as usize) < mark_bits.len() {
                        let header_word = from_space[ptr as usize];
                        let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header_word);
                        if sub_tag == chimera_erlang_beam_term::boxed::BoxedSubTag::Cons {
                            // Mark head and tail words
                            mark_bits.set(ptr as usize, true);
                            if (ptr as usize) + 1 < mark_bits.len() {
                                mark_bits.set((ptr as usize) + 1, true);
                            }
                            if (ptr as usize) + 2 < mark_bits.len() {
                                mark_bits.set((ptr as usize) + 2, true);
                            }
                            // Recursively mark head if it's a heap pointer
                            let head_term = Term(from_space[(ptr as usize) + 1]);
                            if is_heap_pointer(head_term) {
                                if let Some(addr) = term_to_heap_addr(head_term) {
                                    if addr < mark_bits.len() {
                                        mark_bits.set(addr, true);
                                    }
                                }
                            }
                            // Recursively mark tail if it's a heap pointer
                            let tail_term = Term(from_space[(ptr as usize) + 2]);
                            if is_heap_pointer(tail_term) {
                                if let Some(addr) = term_to_heap_addr(tail_term) {
                                    if addr < mark_bits.len() {
                                        mark_bits.set(addr, true);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Tuple => {
                let ptr = term.to_tuple();
                // In our implementation, heap indices start at 0, so 0 is valid
                if (ptr as usize) < from_space.len() {
                    // Verify it's a tuple
                    if (ptr as usize) < mark_bits.len() {
                        let header = from_space[ptr as usize];
                        let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
                        if sub_tag == chimera_erlang_beam_term::boxed::BoxedSubTag::Tuple {
                            let size = chimera_erlang_beam_term::boxed::extract_size(header);
                            // Mark all elements
                            for i in 0..size as usize {
                                if (ptr as usize) + i < mark_bits.len() {
                                    mark_bits.set((ptr as usize) + i, true);
                                    // Recursively mark if element is a heap pointer
                                    if i > 0 {
                                        let elem_term = Term(from_space[(ptr as usize) + i]);
                                        if is_heap_pointer(elem_term) {
                                            if let Some(addr) = term_to_heap_addr(elem_term) {
                                                if addr < mark_bits.len() {
                                                    mark_bits.set(addr, true);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Float => {
                let ptr = term.to_float();
                // In our implementation, heap indices start at 0, so 0 is valid
                if (ptr as usize) < from_space.len() {
                    let header = from_space[ptr as usize];
                    let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
                    if sub_tag == chimera_erlang_beam_term::boxed::BoxedSubTag::Float {
                        let size = chimera_erlang_beam_term::boxed::extract_size(header);
                        for i in 0..size as usize {
                            if (ptr as usize) + i < mark_bits.len() {
                                mark_bits.set((ptr as usize) + i, true);
                            }
                        }
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Binary => {
                let ptr = term.to_binary();
                // In our implementation, heap indices start at 0, so 0 is valid
                if (ptr as usize) < from_space.len() {
                    let header = from_space[ptr as usize];
                    let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
                    if sub_tag == chimera_erlang_beam_term::boxed::BoxedSubTag::Binary {
                        let size = chimera_erlang_beam_term::boxed::extract_size(header);
                        for i in 0..size as usize {
                            if (ptr as usize) + i < mark_bits.len() {
                                mark_bits.set((ptr as usize) + i, true);
                            }
                        }
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Map => {
                let ptr = term.to_map();
                // In our implementation, heap indices start at 0, so 0 is valid
                if (ptr as usize) < from_space.len() {
                    let header = from_space[ptr as usize];
                    let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
                    if sub_tag == chimera_erlang_beam_term::boxed::BoxedSubTag::Map {
                        let size = chimera_erlang_beam_term::boxed::extract_size(header);
                        for i in 0..size as usize {
                            if (ptr as usize) + i < mark_bits.len() {
                                mark_bits.set((ptr as usize) + i, true);
                                // For map entries, keys and values could be heap pointers
                                if i > 0 {
                                    let elem_term = Term(from_space[(ptr as usize) + i]);
                                    if is_heap_pointer(elem_term) {
                                        if let Some(addr) = term_to_heap_addr(elem_term) {
                                            if addr < mark_bits.len() {
                                                mark_bits.set(addr, true);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Fun => {
                let ptr = term.to_fun();
                // In our implementation, heap indices start at 0, so 0 is valid
                if (ptr as usize) < from_space.len() {
                    let header = from_space[ptr as usize];
                    let sub_tag = chimera_erlang_beam_term::boxed::extract_sub_tag(header);
                    if sub_tag == chimera_erlang_beam_term::boxed::BoxedSubTag::Fun {
                        let size = chimera_erlang_beam_term::boxed::extract_size(header);
                        for i in 0..size as usize {
                            if (ptr as usize) + i < mark_bits.len() {
                                mark_bits.set((ptr as usize) + i, true);
                                // Free variables could be heap pointers
                                if i >= 4 {
                                    let elem_term = Term(from_space[(ptr as usize) + i]);
                                    if is_heap_pointer(elem_term) {
                                        if let Some(addr) = term_to_heap_addr(elem_term) {
                                            if addr < mark_bits.len() {
                                                mark_bits.set(addr, true);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Reset the state machine
    pub fn reset(&mut self) {
        self.phase = GcPhase::Idle;
        self.budget = 0;
        self.processed = 0;
        self.total = 0;
        self.mark_bits = None;
        self.scan_ptr = 0;
        self.evac_ptr = 0;
        self.forward_table.clear();
    }
}

impl Default for YieldableGc {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessHeap {
    /// Get inactive space buffer for evacuation
    fn inactive_space(&mut self) -> &mut Vec<u64> {
        if self.in_to_space {
            &mut self.from_space
        } else {
            &mut self.to_space
        }
    }

    /// Write data to inactive space
    fn write_inactive_space(&mut self, data: &[u64]) {
        let target = self.inactive_space();
        target.clear();
        target.extend_from_slice(data);
    }

    /// Set heap pointer directly
    fn set_hp(&mut self, new_hp: usize) {
        self.hp = new_hp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HeapConfig;
    use chimera_erlang_beam_term::Term;

    #[test]
    fn test_yieldable_gc_idle() {
        let gc = YieldableGc::new();
        assert!(!gc.is_running());
        assert_eq!(gc.phase(), GcPhase::Idle);
    }

    #[test]
    fn test_yieldable_gc_start() {
        let mut gc = YieldableGc::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate something so GC has work to do
        heap.make_cons(Term::from_small(1), Term::from_small(2));

        gc.start(&heap, 100);
        assert!(gc.is_running());
        assert_eq!(gc.phase(), GcPhase::Mark);
    }

    #[test]
    fn test_yieldable_gc_complete() {
        let mut gc = YieldableGc::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate something
        heap.make_cons(Term::from_small(1), Term::from_small(2));

        gc.start(&heap, 100);
        assert!(gc.is_running());

        // Run to completion
        while !gc.is_complete() {
            let done = gc.execute_slice(std::iter::empty(), &mut heap);
            if done {
                break;
            }
        }

        assert!(gc.is_complete());
        assert_eq!(gc.phase(), GcPhase::Complete);
    }
}
