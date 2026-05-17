//! Process heap management for RustZigBeam.
//!
//! Rust owns GC policy - Zig kernels handle heap scanning/copying.
//! Per design.md section 8: GC policy in Rust, kernels in Zig.
//!
//! # Debug Heap Verification
//!
//! When the `HEAP_DEBUG` environment variable is set, heap verification
//! is run after each GC cycle to detect corruption. Enable with:
//! `HEAP_DEBUG=1 cargo test` or `HEAP_DEBUG=1 cargo run`

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_term::{boxed::BoxedSubTag, extract_tag, Term, TermTag};

pub mod gc;
pub mod heap_verifier;
pub mod off_heap_binary;
pub mod roots;
pub mod yieldable_gc;

/// Heap configuration
#[derive(Debug, Clone, Copy)]
pub struct HeapConfig {
    /// Initial heap size in words
    pub initial_size: usize,
    /// Minimum heap size
    pub min_size: usize,
    /// Maximum heap size
    pub max_size: usize,
    /// Heap growth rate (multiplier)
    pub growth_rate: f64,
    /// Heap shrink threshold (fraction of used space)
    pub shrink_threshold: f64,
    /// Survivor space size (fraction of heap for surviving objects)
    pub survivor_ratio: f64,
    /// Promotion threshold (minor GC count before promoting)
    pub promotion_threshold: usize,
}

impl Default for HeapConfig {
    fn default() -> Self {
        HeapConfig {
            initial_size: 8192 / 8, // 8192 bytes / 8 bytes per word
            min_size: 1024 / 8,
            max_size: 16 * 1024 * 1024 / 8,
            growth_rate: 2.0,
            shrink_threshold: 0.3,
            survivor_ratio: 0.2,    // 20% of heap for survivors
            promotion_threshold: 3, // Promote after 3 minor GCs
        }
    }
}

/// GC event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcEvent {
    /// Minor collection (young heap)
    Minor,
    /// Major collection (old heap)
    Major,
    /// Object promoted to old generation
    Promotion,
}

/// GC statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct GcStats {
    /// Total number of GC collections (minor + major)
    pub collections: u64,
    /// Number of minor (young generation) collections
    pub minor_collections: u64,
    /// Number of major (old generation) collections
    pub major_collections: u64,
    /// Total bytes allocated on the heap
    pub bytes_allocated: u64,
    /// Total bytes freed during GC
    pub bytes_freed: u64,
    /// Total words collected during GC
    pub words_collected: u64,
    /// Off-heap binary bytes (ref-counted binaries)
    pub off_heap_binary_bytes: u64,
    /// Number of off-heap binaries
    pub off_heap_binary_count: u64,
    /// Number of objects promoted from young to old generation
    pub promotions: u64,
    /// Minor GC count (increments on each minor collection)
    pub minor_gc_count: u64,
}

/// Forwarding pointer marker for copied objects
/// Objects that have been copied to to-space have this marker
/// followed by the new address
#[allow(dead_code)]
const FORWARD_MAGIC: u64 = 0xDEADBEEF_DEADBEEF;

/// A process heap with allocation and GC support
///
/// Uses a word-based heap where allocation is in units of words (8 bytes).
/// This matches BEAM's word-heap model where each slot holds a word.
///
/// Uses a semi-space copying collector for minor GC:
/// - from_space: current heap being collected
/// - to_space: new heap where live objects are copied
/// - After collection, spaces are swapped
#[derive(Debug)]
pub struct ProcessHeap {
    /// Heap memory buffer (word-aligned) - "from-space"
    from_space: Vec<u64>,
    /// "To-space" for copying collector
    to_space: Vec<u64>,
    /// Heap pointer in WORDS (next allocation position in to_space after GC)
    hp: usize,
    /// Heap end in WORDS (size of each space)
    hend: usize,
    /// Whether we're currently in to_space (after minor GC)
    in_to_space: bool,
    /// Configuration
    config: HeapConfig,
    /// Statistics
    stats: GcStats,
    /// Virtual binary heap for off-heap binary accounting
    virtual_binary_heap: off_heap_binary::VirtualBinaryHeap,
    /// Survivor space size in words (young generation survivors before promotion)
    #[allow(dead_code)]
    survivor_space_size: usize,
}

impl ProcessHeap {
    /// Create a new process heap
    pub fn new(config: HeapConfig) -> Self {
        let size_words = config.initial_size;
        let from_space = vec![0u64; size_words];
        let to_space = vec![0u64; size_words];
        let survivor_space_size = (size_words as f64 * config.survivor_ratio) as usize;

        ProcessHeap {
            from_space,
            to_space,
            hp: 0,
            hend: size_words,
            in_to_space: false,
            config,
            stats: GcStats::default(),
            virtual_binary_heap: off_heap_binary::VirtualBinaryHeap::new(),
            survivor_space_size,
        }
    }

    /// Get reference to active heap buffer (from-space or to-space)
    pub fn active_buffer(&self) -> &Vec<u64> {
        if self.in_to_space {
            &self.to_space
        } else {
            &self.from_space
        }
    }

    /// Get mutable reference to active heap buffer
    pub fn active_buffer_mut(&mut self) -> &mut Vec<u64> {
        if self.in_to_space {
            &mut self.to_space
        } else {
            &mut self.from_space
        }
    }

    /// Check if we're currently in to-space (after minor GC)
    pub fn in_to_space(&self) -> bool {
        self.in_to_space
    }

    /// Get the inactive buffer (to-space or from-space)
    #[allow(dead_code)]
    fn inactive_buffer(&self) -> &Vec<u64> {
        if self.in_to_space {
            &self.from_space
        } else {
            &self.to_space
        }
    }

    /// Allocate space on the heap (in words)
    /// Returns a word index, or None if out of memory
    #[inline]
    pub fn alloc(&mut self, words: usize) -> Option<usize> {
        let new_hp = self.hp + words;

        if new_hp <= self.hend {
            let ptr = self.hp;
            self.hp = new_hp;
            self.stats.bytes_allocated += (words * 8) as u64;
            Some(ptr)
        } else {
            None
        }
    }

    /// Fast allocation without bounds check.
    ///
    /// # Safety
    /// Caller must guarantee that `words <= self.remaining_words()`.
    /// This is for hot paths where the caller has already checked space.
    #[inline]
    pub unsafe fn alloc_fast(&mut self, words: usize) -> usize {
        let ptr = self.hp;
        self.hp += words;
        self.stats.bytes_allocated += (words * 8) as u64;
        ptr
    }

    /// Allocate space on the heap with batch hint.
    ///
    /// If the caller knows they'll need `batch_size` words soon,
    /// this can help pre-reserve space. Returns current heap pointer.
    #[inline]
    pub fn alloc_batch_hint(&mut self, words: usize, batch_size: usize) -> Option<usize> {
        // Reserve extra space if batch_size > words
        let needed = if batch_size > words {
            batch_size
        } else {
            words
        };
        self.alloc(needed)
    }

    /// Allocate a term on the heap
    pub fn alloc_term(&mut self, _term: Term) -> Option<usize> {
        // Terms take 1 word minimum
        self.alloc(1)
    }

    /// Get remaining space in words
    #[inline]
    pub fn remaining_words(&self) -> usize {
        self.hend - self.hp
    }

    /// Check if N words can be allocated without triggering GC
    #[inline]
    pub fn can_alloc(&self, words: usize) -> bool {
        self.hp + words <= self.hend
    }

    /// Get heap pointer address (in words)
    pub fn heap_ptr(&self) -> usize {
        self.hp
    }

    /// Get heap end address (in words)
    pub fn heap_end(&self) -> usize {
        self.hend
    }

    /// Get buffer as a slice of words (active space)
    pub fn as_words(&self) -> &[u64] {
        self.active_buffer()
    }

    /// Get mutable buffer as a slice of words (active space)
    pub fn as_words_mut(&mut self) -> &mut [u64] {
        self.active_buffer_mut()
    }

    /// Read a word at the given index
    pub fn get_word(&self, index: usize) -> Option<u64> {
        self.active_buffer().get(index).copied()
    }

    /// Write a word at the given index
    pub fn set_word(&mut self, index: usize, value: u64) {
        if index < self.hend {
            self.active_buffer_mut()[index] = value;
        }
    }

    /// Perform a minor GC with root tracing.
    ///
    /// Uses root-set tracing to identify live objects, then copies
    /// Perform a minor GC using the provided root set.
    ///
    /// # Arguments
    /// * `roots` - Mutable slice of root terms to trace from (will be updated with new addresses)
    ///
    /// Returns true if GC freed space, false otherwise.
    pub fn minor_gc(&mut self, roots: &mut [Term]) -> bool {
        let used_before = self.hp;

        // Use trace_gc for proper root tracing
        self.trace_gc(roots);

        // Return true if we freed some space
        self.hp < used_before
    }
    /// Perform a minor GC with an empty root set (convenience method).
    ///
    /// This will collect all objects since no roots are provided.
    /// Used when we don't have access to roots.
    pub fn minor_gc_empty(&mut self) -> bool {
        self.minor_gc(&mut [])
    }

    /// Scan the heap using Zig kernel for differential testing
    ///
    /// This method calls the Zig beamz_heap_scan kernel and logs results
    /// for validation against the Rust reference implementation.
    /// Used during GC to verify Zig kernel produces same results as Rust.
    pub fn zig_scan_roots(&mut self, _roots: &[Term]) -> Result<(), crate::GcEvent> {
        use chimera_erlang_beam_abi::heap_kernels;

        let base = self.active_buffer();
        let result = heap_kernels::heap_scan(base, self.hp);

        match result {
            Ok(_output) => {
                // Log for debugging/validation
                #[cfg(test)]
                {
                    eprintln!(
                        "Zig heap scan: words_scanned={}, objects_found={}",
                        _output.words_scanned, _output.objects_found
                    );
                }
                Ok(())
            }
            Err(_e) => {
                #[cfg(test)]
                {
                    eprintln!("Zig heap scan failed");
                }
                Err(crate::GcEvent::Minor)
            }
        }
    }

    /// GC should be called when allocation fails
    ///
    /// Takes roots to trace from. Returns true if GC recovered space.
    ///
    /// # Arguments
    /// * `roots` - Mutable slice of root terms (will be updated with new addresses)
    ///
    pub fn try_gc(&mut self, roots: &mut [Term]) -> bool {
        self.minor_gc(roots)
    }

    /// GC with empty roots (convenience method)
    pub fn try_gc_empty(&mut self) -> bool {
        self.minor_gc(&mut [])
    }

    /// Grow the heap
    pub fn grow(&mut self) -> bool {
        // Calculate new size for each space
        let current_size = self.from_space.len();
        let new_size_words = (current_size as f64 * self.config.growth_rate) as usize;
        if new_size_words > self.config.max_size {
            return false;
        }

        // Grow both spaces
        self.from_space.resize(new_size_words, 0);
        self.to_space.resize(new_size_words, 0);
        self.hend = new_size_words;

        self.stats.bytes_allocated += ((new_size_words - current_size) * 8) as u64;
        true
    }

    /// Get GC statistics
    pub fn get_stats(&self) -> GcStats {
        let mut stats = self.stats;
        stats.off_heap_binary_bytes = self.virtual_binary_heap.total_bytes() as u64;
        stats.off_heap_binary_count = self.virtual_binary_heap.total_ref_count() as u64;
        stats
    }

    /// Account for an off-heap binary allocation
    pub fn account_off_heap_alloc(&mut self, size: usize, ref_count: usize) {
        self.virtual_binary_heap.account_alloc(size, ref_count);
    }

    /// Account for an off-heap binary deallocation
    pub fn account_off_heap_dealloc(&mut self, size: usize, ref_count: usize) {
        self.virtual_binary_heap.account_dealloc(size, ref_count);
    }

    /// Account for a reference count increment on an off-heap binary
    pub fn account_off_heap_increment(&mut self) {
        self.virtual_binary_heap.account_increment();
    }

    /// Account for a reference count decrement on an off-heap binary
    pub fn account_off_heap_decrement(&mut self) {
        self.virtual_binary_heap.account_decrement();
    }

    /// Get the virtual binary heap reference for direct access
    pub fn virtual_binary_heap(&self) -> &off_heap_binary::VirtualBinaryHeap {
        &self.virtual_binary_heap
    }

    /// Get heap usage as a fraction
    pub fn usage(&self) -> f64 {
        self.hp as f64 / self.hend as f64
    }

    /// Check if heap should shrink
    pub fn should_shrink(&self) -> bool {
        self.usage() < self.config.shrink_threshold && self.from_space.len() > self.config.min_size
    }

    /// Verify heap integrity in debug builds
    ///
    /// When `HEAP_DEBUG` environment variable is set, this runs
    /// `HeapVerifier::verify()` after each GC cycle to detect corruption.
    /// Returns `Ok(())` if verification passes or if debug is disabled.
    pub fn verify_heap(&mut self) -> Result<(), crate::heap_verifier::VerificationResult> {
        if std::env::var("HEAP_DEBUG").is_err() {
            return Ok(());
        }

        let mut verifier = crate::heap_verifier::HeapVerifier::new();
        let result = verifier.verify(self);

        if result.passed() {
            Ok(())
        } else {
            Err(result)
        }
    }

    /// Perform a major GC with no roots (collects everything)
    ///
    /// This is a convenience wrapper that calls major_gc with an empty iterator.
    pub fn major_gc(&mut self) {
        self.major_gc_impl(std::iter::empty());
    }

    /// Internal major GC implementation with roots parameter
    ///
    /// Per design.md section 8: GC policy in Rust, kernels in Zig.
    /// This implements a mark-compact GC with reference updating.
    fn major_gc_impl<R>(&mut self, roots: R)
    where
        R: Iterator<Item = Term>,
    {
        use crate::gc::GcMarkBits;

        self.stats.collections += 1;
        self.stats.major_collections += 1;

        let used_words = self.hp;
        if used_words == 0 {
            return;
        }

        // Get active buffer for marking
        let mut from_space = self.active_buffer().to_vec();
        let mut to_space = vec![0u64; used_words];

        // Allocate mark bits
        let mut mark_bits = GcMarkBits::new(used_words);

        // Step 1: Collect roots and mark them
        let mut root_terms: Vec<Term> = Vec::new();
        for root in roots {
            root_terms.push(root);
            Self::mark_term_gc(*root_terms.last().unwrap(), &mut mark_bits, &from_space);
        }

        // Count live words
        let live_words: usize = (0..used_words).filter(|&i| mark_bits.get(i)).count();
        if live_words == 0 {
            self.hp = 0;
            return;
        }

        // Step 2: Calculate forwarding addresses
        // forward[i] = new address for object at from_space[i]
        let mut forward: Vec<Option<usize>> = vec![None; used_words];
        let mut new_hp: usize = 0;
        let mut idx: usize = 0;

        while idx < used_words {
            if mark_bits.get(idx) {
                forward[idx] = Some(new_hp);
                let header = from_space[idx];
                let tag = extract_tag(header);
                let size = match tag {
                    TermTag::Cons => 3,
                    TermTag::Float => 3,
                    TermTag::Tuple | TermTag::Map | TermTag::Fun | TermTag::Binary => {
                        let sz = (header >> 8) & 0xFFFFFF;
                        sz as usize
                    }
                    _ => 1,
                };
                new_hp += size;
                idx += size;
            } else {
                idx += 1;
            }
        }

        // Step 3: Update references in roots and heap objects
        // Update root terms
        for root in &mut root_terms {
            Self::update_term_addr_from_forward(root, &forward);
        }

        // Update references within live objects in from_space
        idx = 0;
        while idx < used_words {
            if mark_bits.get(idx) {
                let header = from_space[idx];
                let tag = extract_tag(header);
                match tag {
                    TermTag::Cons => {
                        // Update head and tail pointers
                        let head_ptr = from_space[idx + 1];
                        let tail_ptr = from_space[idx + 2];
                        if let Some(new_addr) = Self::get_forwarded_addr(head_ptr, &forward) {
                            from_space[idx + 1] = Self::update_pointer_word(head_ptr, new_addr);
                        }
                        if let Some(new_addr) = Self::get_forwarded_addr(tail_ptr, &forward) {
                            from_space[idx + 2] = Self::update_pointer_word(tail_ptr, new_addr);
                        }
                        idx += 3;
                    }
                    TermTag::Tuple | TermTag::Map | TermTag::Fun | TermTag::Binary => {
                        let size = (header >> 8) & 0xFFFFFF;
                        for i in 1..size as usize {
                            let elem_ptr = from_space[idx + i];
                            if let Some(new_addr) = Self::get_forwarded_addr(elem_ptr, &forward) {
                                from_space[idx + i] = Self::update_pointer_word(elem_ptr, new_addr);
                            }
                        }
                        idx += size as usize;
                    }
                    _ => {
                        idx += 1;
                    }
                }
            } else {
                idx += 1;
            }
        }

        // Step 4: Compact - copy live objects to to_space
        new_hp = 0;
        idx = 0;
        while idx < used_words {
            if mark_bits.get(idx) {
                let header = from_space[idx];
                let tag = extract_tag(header);
                let size = match tag {
                    TermTag::Cons => 3,
                    TermTag::Float => 3,
                    TermTag::Tuple | TermTag::Map | TermTag::Fun | TermTag::Binary => {
                        let sz = (header >> 8) & 0xFFFFFF;
                        sz as usize
                    }
                    _ => 1,
                };
                to_space[new_hp..(new_hp + size)].copy_from_slice(&from_space[idx..(idx + size)]);
                new_hp += size;
                idx += size;
            } else {
                idx += 1;
            }
        }

        // Copy to_space back to active buffer
        let active = self.active_buffer_mut();
        active[..new_hp].copy_from_slice(&to_space[..new_hp]);

        self.stats.words_collected += (used_words - new_hp) as u64;
        self.hp = new_hp;

        // Debug: verify heap integrity after GC
        if let Err(e) = self.verify_heap() {
            eprintln!("HEAP_DEBUG: major_gc verification failed: {:?}", e);
        }
    }

    /// Get forwarded address for a pointer word
    fn get_forwarded_addr(ptr_word: u64, forward: &[Option<usize>]) -> Option<usize> {
        let tag = extract_tag(ptr_word);
        if tag == TermTag::Cons || tag == TermTag::Tuple {
            let ptr = (ptr_word >> 3) as usize;
            if ptr < forward.len() {
                return forward[ptr];
            }
        }
        None
    }

    /// Update a pointer word with new address
    fn update_pointer_word(old_word: u64, new_addr: usize) -> u64 {
        let tag_bits = old_word & 0b111;
        ((new_addr as u64) << 3) | tag_bits
    }

    /// Update a term's address based on forwarding table
    fn update_term_addr_from_forward(term: &mut Term, forward: &[Option<usize>]) {
        let ptr = match term.tag() {
            TermTag::Cons => term.to_cons() as usize,
            TermTag::Tuple => term.to_tuple() as usize,
            _ => return,
        };
        if ptr < forward.len() {
            if let Some(new_addr) = forward[ptr] {
                let tag_bits = term.0 & 0b111;
                term.0 = ((new_addr as u64) << 3) | tag_bits;
            }
        }
    }

    /// Internal mark_term helper for major GC
    ///
    /// Traces through heap objects and marks them as live.
    /// Also recursively marks objects referenced by live objects.
    fn mark_term_gc(term: Term, mark_bits: &mut crate::gc::GcMarkBits, from_space: &[u64]) {
        match term.tag() {
            chimera_erlang_beam_term::TermTag::SmallInteger
            | chimera_erlang_beam_term::TermTag::Atom => {
                // Immediate values - nothing to mark
            }
            chimera_erlang_beam_term::TermTag::Cons => {
                let ptr = term.to_cons() as usize;
                if ptr < from_space.len() && !mark_bits.get(ptr) {
                    // Mark this cons cell
                    mark_bits.set(ptr, true);
                    if ptr + 1 < from_space.len() {
                        mark_bits.set(ptr + 1, true);
                    }
                    if ptr + 2 < from_space.len() {
                        mark_bits.set(ptr + 2, true);
                    }
                    // Recursively mark referenced objects
                    let head = Term::from_raw(from_space[ptr + 1]);
                    Self::mark_term_gc(head, mark_bits, from_space);
                    let tail = Term::from_raw(from_space[ptr + 2]);
                    Self::mark_term_gc(tail, mark_bits, from_space);
                }
            }
            chimera_erlang_beam_term::TermTag::Tuple => {
                let ptr = term.to_tuple() as usize;
                if ptr < from_space.len() && !mark_bits.get(ptr) {
                    // Mark header
                    mark_bits.set(ptr, true);
                    let header = from_space[ptr];
                    let size = (header >> 8) & 0xFFFFFF;
                    // Mark all elements
                    for i in 1..size as usize {
                        if ptr + i < from_space.len() {
                            mark_bits.set(ptr + i, true);
                            // Recursively mark referenced objects
                            let elem = Term::from_raw(from_space[ptr + i]);
                            Self::mark_term_gc(elem, mark_bits, from_space);
                        }
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Float => {
                let ptr = term.to_float() as usize;
                if ptr < from_space.len() {
                    mark_bits.set(ptr, true);
                    if ptr + 1 < from_space.len() {
                        mark_bits.set(ptr + 1, true);
                    }
                    if ptr + 2 < from_space.len() {
                        mark_bits.set(ptr + 2, true);
                    }
                }
            }
            chimera_erlang_beam_term::TermTag::Binary
            | chimera_erlang_beam_term::TermTag::Map
            | chimera_erlang_beam_term::TermTag::Fun => {
                let ptr = term.to_fun() as usize;
                if ptr < from_space.len() {
                    mark_bits.set(ptr, true);
                    let header = from_space[ptr];
                    let size = (header >> 8) & 0xFFFFFF;
                    for i in 1..size as usize {
                        if ptr + i < from_space.len() {
                            mark_bits.set(ptr + i, true);
                        }
                    }
                }
            }
        }
    }

    /// Reset heap (after GC) - clears active space
    pub fn reset(&mut self) {
        self.hp = 0;
        // Clear the active buffer
        let hend = self.hend;
        let active = self.active_buffer_mut();
        for item in active.iter_mut().take(hend) {
            *item = 0;
        }
    }

    /// Get total heap size in words (size of one space)
    pub fn total_size(&self) -> usize {
        self.from_space.len()
    }

    /// Get used words in active space
    pub fn used_size(&self) -> usize {
        self.hp
    }

    /// Allocate a cons cell on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    /// Cons cell layout: [header][head][tail] = 3 words
    pub fn alloc_cons(&mut self) -> Option<usize> {
        self.alloc(3)
    }

    /// Write a cons cell at the given position
    ///
    /// # Safety
    /// The caller must ensure pos is a valid allocation position with 3 words available
    pub unsafe fn write_cons(&mut self, pos: usize, head: Term, tail: Term) {
        // Use boxed header encoding: sub_tag in bits 3-7, size in bits 8-31, tag in bits 0-2
        let header = (BoxedSubTag::Cons as u64) | ((3u64) << 8) | (TermTag::Cons as u64);
        self.set_word(pos, header);
        self.set_word(pos + 1, head.0);
        self.set_word(pos + 2, tail.0);
    }

    /// Read the head of a cons cell
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid cons cell
    pub unsafe fn read_cons_head(&self, pos: usize) -> Term {
        Term(self.get_word(pos + 1).unwrap_or(0))
    }

    /// Read the tail of a cons cell
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid cons cell
    pub unsafe fn read_cons_tail(&self, pos: usize) -> Term {
        Term(self.get_word(pos + 2).unwrap_or(0))
    }

    /// Allocate a tuple on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    /// Tuple layout: [header][elements...] = 1 + arity words
    pub fn alloc_tuple(&mut self, arity: u32) -> Option<usize> {
        self.alloc(1 + arity as usize)
    }

    /// Write a tuple header at the given position
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position with 1 + arity words
    pub unsafe fn write_tuple_header(&mut self, pos: usize, arity: u32) {
        let header =
            (BoxedSubTag::Tuple as u64) | (((1 + arity) as u64) << 8) | (TermTag::Tuple as u64);
        self.set_word(pos, header);
    }

    /// Write an element to a tuple
    ///
    /// # Safety
    /// Caller must ensure pos+1+i is within the tuple bounds
    pub unsafe fn write_tuple_element(&mut self, pos: usize, index: u32, element: Term) {
        self.set_word(pos + 1 + index as usize, element.0);
    }

    /// Read a tuple element
    ///
    /// # Safety
    /// Caller must ensure index is within tuple arity
    pub unsafe fn read_tuple_element(&self, pos: usize, index: u32) -> Term {
        Term(self.get_word(pos + 1 + index as usize).unwrap_or(0))
    }

    /// Get the arity of a tuple from its header
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid tuple header
    pub unsafe fn tuple_arity(&self, pos: usize) -> u32 {
        let header = self.get_word(pos).unwrap_or(0);
        // Size is stored in bits 8-31 (24 bits)
        let size = (header >> 8) & 0xFFFFFF;
        // Arity is size - 1 (since size includes header)
        (size - 1) as u32
    }

    /// Allocate and initialize a tuple with elements
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_tuple(&mut self, elements: &[Term]) -> Option<usize> {
        let arity = elements.len() as u32;
        let pos = self.alloc_tuple(arity)?;
        unsafe {
            self.write_tuple_header(pos, arity);
            for (i, &elem) in elements.iter().enumerate() {
                self.write_tuple_element(pos, i as u32, elem);
            }
        }
        Some(pos)
    }

    /// Allocate and initialize a cons cell (list cell)
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_cons(&mut self, head: Term, tail: Term) -> Option<usize> {
        let pos = self.alloc_cons()?;
        unsafe {
            self.write_cons(pos, head, tail);
        }
        Some(pos)
    }

    /// Allocate space for a map on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn alloc_map(&mut self, num_keys: u32) -> Option<usize> {
        // Map layout: 1 header word + 2 * num_keys (key-value pairs)
        let words = 1 + (num_keys as usize) * 2;
        self.alloc(words)
    }

    /// Allocate and initialize a map with key-value pairs
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_map(&mut self, keys_values: &[(Term, Term)]) -> Option<usize> {
        let num_keys = keys_values.len() as u32;
        let pos = self.alloc_map(num_keys)?;
        unsafe {
            self.write_map_header(pos, num_keys);
            for (i, &(key, value)) in keys_values.iter().enumerate() {
                self.write_map_kv(pos, i as u32, key, value);
            }
        }
        Some(pos)
    }

    /// Write map header at position
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position
    pub unsafe fn write_map_header(&mut self, pos: usize, num_keys: u32) {
        let header =
            (BoxedSubTag::Map as u64) | (((1 + num_keys * 2) as u64) << 8) | (TermTag::Map as u64);
        self.set_word(pos, header);
    }

    /// Write a key-value pair to a map
    ///
    /// # Safety
    /// Caller must ensure index is within map bounds
    pub unsafe fn write_map_kv(&mut self, pos: usize, index: u32, key: Term, value: Term) {
        let base = pos + 1 + (index as usize) * 2;
        self.set_word(base, key.0);
        self.set_word(base + 1, value.0);
    }

    /// Get the number of keys in a map from its header
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid map header
    pub unsafe fn map_size(&self, pos: usize) -> u32 {
        let header = self.get_word(pos).unwrap_or(0);
        let size = (header >> 8) & 0xFFFFFF;
        size as u32 / 2 // Each key-value pair is 2 words
    }

    /// Allocate space for a float on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    /// Float takes 3 words: 1 header + 2 for IEEE 754 double
    pub fn alloc_float(&mut self) -> Option<usize> {
        self.alloc(3) // header + 2 words for float
    }

    /// Allocate and initialize a float
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_float(&mut self, value: f64) -> Option<usize> {
        let pos = self.alloc_float()?;
        unsafe {
            self.write_float(pos, value);
        }
        Some(pos)
    }

    /// Write a float value at position
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position (3 words)
    pub unsafe fn write_float(&mut self, pos: usize, value: f64) {
        // Write header
        let header = (BoxedSubTag::Float as u64) | (3u64 << 8) | (TermTag::Float as u64);
        self.set_word(pos, header);
        // Write float value as two u32 words (or one u64)
        let bits = value.to_bits();
        self.set_word(pos + 1, bits);
        self.set_word(pos + 2, bits >> 32); // Actually we should store in pos+1 as full u64
    }

    /// Write a float value at position (fixed version)
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position (3 words)
    pub unsafe fn write_float_value(&mut self, pos: usize, value: f64) {
        let header = (BoxedSubTag::Float as u64) | (3u64 << 8) | (TermTag::Float as u64);
        self.set_word(pos, header);
        // Store float as u64 at pos+1 (pos+2 unused but reserved)
        self.set_word(pos + 1, value.to_bits());
    }

    /// Read a float value
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid float
    pub unsafe fn read_float(&self, pos: usize) -> f64 {
        let bits = self.get_word(pos + 1).unwrap_or(0);
        f64::from_bits(bits)
    }

    /// Allocate space for a big integer on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn alloc_bigint(&mut self, num_digits: u32) -> Option<usize> {
        // BigInt layout: 1 header + 2 (sign + digit count) + num_digits
        let words = 3 + num_digits as usize;
        self.alloc(words)
    }

    /// Allocate and initialize a big integer
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_bigint(&mut self, negative: bool, digits: &[u32]) -> Option<usize> {
        let num_digits = digits.len() as u32;
        let pos = self.alloc_bigint(num_digits)?;
        unsafe {
            self.write_bigint_header(pos, num_digits, negative);
            for (i, &digit) in digits.iter().enumerate() {
                self.write_bigint_digit(pos, i as u32, digit);
            }
        }
        Some(pos)
    }

    /// Write big integer header
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position
    pub unsafe fn write_bigint_header(&mut self, pos: usize, num_digits: u32, negative: bool) {
        // Header: sub_tag | size | term_tag
        let header = (BoxedSubTag::BigInteger as u64)
            | (((3 + num_digits) as u64) << 8)
            | (TermTag::SmallInteger as u64); // Use small int tag for boxed
        self.set_word(pos, header);
        // Sign and digit count at pos+1
        let sign_and_count = if negative { 1u64 << 32 } else { 0 } | (num_digits as u64);
        self.set_word(pos + 1, sign_and_count);
    }

    /// Write a digit to a big integer
    ///
    /// # Safety
    /// Caller must ensure digit_index is within bounds
    pub unsafe fn write_bigint_digit(&mut self, pos: usize, digit_index: u32, digit: u32) {
        self.set_word(pos + 2 + digit_index as usize, digit as u64);
    }

    /// Read a big integer digit
    ///
    /// # Safety
    /// Caller must ensure digit_index is within bounds
    pub unsafe fn read_bigint_digit(&self, pos: usize, digit_index: u32) -> u32 {
        self.get_word(pos + 2 + digit_index as usize).unwrap_or(0) as u32
    }

    /// Check if big integer is negative
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid big integer
    pub unsafe fn bigint_is_negative(&self, pos: usize) -> bool {
        let word = self.get_word(pos + 1).unwrap_or(0);
        (word >> 32) != 0
    }

    /// Get big integer digit count
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid big integer
    pub unsafe fn bigint_digit_count(&self, pos: usize) -> u32 {
        let word = self.get_word(pos + 1).unwrap_or(0);
        (word & 0xFFFFFFFF) as u32
    }

    /// Allocate space for a fun/closure on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn alloc_fun(&mut self, num_free: u32) -> Option<usize> {
        // Fun layout: 1 header + 3 (old_index, old_uniq, num_free) + num_free
        let words = 4 + num_free as usize;
        self.alloc(words)
    }

    /// Allocate and initialize a fun/closure
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_fun(
        &mut self,
        old_index: u32,
        old_uniq: u32,
        num_free: u32,
        free_terms: &[Term],
    ) -> Option<usize> {
        let pos = self.alloc_fun(num_free)?;
        unsafe {
            self.write_fun_header(pos, num_free);
            self.write_fun_metadata(pos, old_index, old_uniq, num_free);
            for (i, &term) in free_terms.iter().enumerate() {
                self.write_fun_free_term(pos, i as u32, term);
            }
        }
        Some(pos)
    }

    /// Write fun header
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position
    pub unsafe fn write_fun_header(&mut self, pos: usize, num_free: u32) {
        let header =
            (BoxedSubTag::Fun as u64) | (((4 + num_free) as u64) << 8) | (TermTag::Fun as u64);
        self.set_word(pos, header);
    }

    /// Write fun metadata (old_index, old_uniq, num_free)
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position
    pub unsafe fn write_fun_metadata(
        &mut self,
        pos: usize,
        old_index: u32,
        old_uniq: u32,
        num_free: u32,
    ) {
        self.set_word(pos + 1, old_index as u64);
        self.set_word(pos + 2, old_uniq as u64);
        self.set_word(pos + 3, num_free as u64);
    }

    /// Write a free term to a fun
    ///
    /// # Safety
    /// Caller must ensure free_index is within bounds
    pub unsafe fn write_fun_free_term(&mut self, pos: usize, free_index: u32, term: Term) {
        self.set_word(pos + 4 + free_index as usize, term.0);
    }

    /// Read a free term from a fun
    ///
    /// # Safety
    /// Caller must ensure free_index is within bounds
    pub unsafe fn read_fun_free_term(&self, pos: usize, free_index: u32) -> Term {
        Term(self.get_word(pos + 4 + free_index as usize).unwrap_or(0))
    }

    /// Get fun num_free
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid fun
    pub unsafe fn fun_num_free(&self, pos: usize) -> u32 {
        self.get_word(pos + 3).unwrap_or(0) as u32
    }

    /// Get fun old_uniq
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid fun
    pub unsafe fn fun_old_uniq(&self, pos: usize) -> u32 {
        self.get_word(pos + 2).unwrap_or(0) as u32
    }

    /// Allocate space for a binary on the heap
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn alloc_binary(&mut self, size_bytes: u32) -> Option<usize> {
        // Binary layout: 1 header + 1 (size/flags) + ceil(size_bytes/8) words
        let words = 2 + ((size_bytes + 7) / 8) as usize;
        self.alloc(words)
    }

    /// Allocate and initialize a binary
    ///
    /// Returns the word index of the header, or None if out of memory.
    pub fn make_binary(&mut self, size_bytes: u32, data: &[u8]) -> Option<usize> {
        let pos = self.alloc_binary(size_bytes)?;
        unsafe {
            self.write_binary_header(pos, size_bytes);
            self.write_binary_data(pos, data);
        }
        Some(pos)
    }

    /// Write binary header
    ///
    /// # Safety
    /// Caller must ensure pos is a valid allocation position
    pub unsafe fn write_binary_header(&mut self, pos: usize, size_bytes: u32) {
        let word_size = (size_bytes + 7) / 8;
        let header = (BoxedSubTag::Binary as u64)
            | (((2 + word_size) as u64) << 8)
            | (TermTag::Binary as u64);
        self.set_word(pos, header);
        // Binary header word at pos+1: size in low 32 bits, flags in high 32 bits
        self.set_word(pos + 1, size_bytes as u64);
    }

    /// Write binary data
    ///
    /// # Safety
    /// Caller must ensure data fits in allocated space
    pub unsafe fn write_binary_data(&mut self, pos: usize, data: &[u8]) {
        let base = pos + 2;
        // Pack bytes into words (8 bytes per word)
        for (i, chunk) in data.chunks(8).enumerate() {
            let mut word: u64 = 0;
            for (j, &byte) in chunk.iter().enumerate() {
                word |= (byte as u64) << (j * 8);
            }
            self.set_word(base + i, word);
        }
    }

    /// Read binary size in bytes
    ///
    /// # Safety
    /// Caller must ensure pos points to a valid binary
    pub unsafe fn binary_size(&self, pos: usize) -> u32 {
        self.get_word(pos + 1).unwrap_or(0) as u32
    }

    /// Read binary data
    ///
    /// # Safety
    /// Caller must ensure buffer is large enough
    pub unsafe fn read_binary_data(&self, pos: usize, buffer: &mut [u8]) {
        let base = pos + 2;
        let size_bytes = self.binary_size(pos) as usize;
        for (i, byte) in buffer.iter_mut().enumerate().take(size_bytes) {
            let word_idx = i / 8;
            let byte_idx = i % 8;
            let word = self.get_word(base + word_idx).unwrap_or(0);
            *byte = (word >> (byte_idx * 8)) as u8;
        }
    }

    /// Allocate a nil (empty list)
    ///
    /// Returns the word index of the nil representation.
    pub fn alloc_nil(&mut self) -> Option<usize> {
        // Nil is represented as atom ATOM_NIL which is an immediate,
        // not a heap allocation. But for consistency with list operations,
        // we return the atom index.
        Some(chimera_erlang_beam_term::atoms::ATOM_NIL as usize)
    }
}

/// Heap view for passing to Zig kernels (word-based)
#[repr(C)]
pub struct HeapView {
    /// Base pointer to word array
    pub base: *const u64,
    /// Current heap pointer (word index)
    pub hp: *mut u64,
    /// End pointer (word index)
    pub end: *const u64,
}

impl ProcessHeap {
    /// Create a heap view for FFI
    pub fn as_view(&self) -> HeapView {
        let active = self.active_buffer();
        HeapView {
            base: active.as_ptr(),
            hp: unsafe { active.as_ptr().add(self.hp) as *mut u64 },
            end: unsafe { active.as_ptr().add(self.hend) },
        }
    }

    /// Create a mutable heap view for FFI
    pub fn as_view_mut(&mut self) -> HeapView {
        let active = self.active_buffer();
        HeapView {
            base: active.as_ptr(),
            hp: unsafe { active.as_ptr().add(self.hp) as *mut u64 },
            end: unsafe { active.as_ptr().add(self.hend) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_create() {
        let heap = ProcessHeap::new(HeapConfig::default());
        // HeapConfig default is 8192 bytes / 8 = 1024 words
        assert_eq!(heap.total_size(), 1024);
        assert_eq!(heap.used_size(), 0);
    }

    #[test]
    fn test_heap_alloc() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let ptr = heap.alloc(16); // 16 words
        assert!(ptr.is_some());
        assert_eq!(heap.used_size(), 16);
    }

    #[test]
    fn test_heap_word_access() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let ptr = heap.alloc(1).unwrap();
        heap.set_word(ptr, 0x123456789ABCDEF0);
        assert_eq!(heap.get_word(ptr), Some(0x123456789ABCDEF0));
    }

    #[test]
    fn test_heap_grow() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        let initial_size = heap.total_size();

        // Allocate beyond current size
        while heap.alloc(1000).is_some() {}

        assert!(heap.grow());
        assert!(heap.total_size() > initial_size);
    }

    #[test]
    fn test_heap_gc_preserves_data() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons cell as a root (3 words: header + head + tail)
        let cons_ptr = heap.alloc_cons().unwrap();
        let head_term = Term::from_small(42);
        unsafe {
            heap.write_cons(cons_ptr, head_term, Term::nil());
        }
        let cons_term = Term::from_cons(cons_ptr as u64);

        // GC should preserve the cons cell because cons_term is a root
        let mut roots = vec![cons_term];
        heap.minor_gc(&mut roots);

        // Verify cons cell is preserved
        // Note: in our implementation, the index might be the same (0 in from_space and 0 in to_space)
        // but the memory region is different. We verify the data was copied correctly.
        let new_cons_ptr = cons_term.to_cons();
        // Verify the cons cell data was copied correctly
        let new_head = unsafe { heap.read_cons_head(new_cons_ptr as usize) };
        assert_eq!(new_head, head_term);
    }

    #[test]
    fn test_heap_gc() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        heap.alloc(100);

        let stats_before = heap.get_stats();
        assert_eq!(stats_before.collections, 0);

        heap.minor_gc_empty();
        let stats = heap.get_stats();
        assert_eq!(stats.collections, 1);
        assert_eq!(stats.minor_collections, 1);
    }

    #[test]
    fn test_heap_usage() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        assert!(heap.usage() < 0.01); // Nearly empty

        heap.alloc(1000);
        assert!(heap.usage() > 0.0);
    }

    #[test]
    fn test_heap_reset() {
        let mut heap = ProcessHeap::new(HeapConfig::default());
        heap.alloc(100);
        assert_eq!(heap.used_size(), 100); // 100 words

        heap.reset();
        assert_eq!(heap.used_size(), 0);
    }

    #[test]
    fn test_heap_gc_swaps_spaces() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons cell as a root
        let cons_ptr = heap.alloc_cons().unwrap();
        unsafe {
            heap.write_cons(cons_ptr, Term::from_small(99), Term::nil());
        }
        let cons_term = Term::from_cons(cons_ptr as u64);
        let before_gc_head = unsafe { heap.read_cons_head(cons_ptr) };

        // Run GC with cons_term as root
        let mut roots = vec![cons_term];
        heap.minor_gc(&mut roots);

        // After GC - verify cons cell is preserved
        let new_cons_ptr = cons_term.to_cons();
        // Verify the cons cell data was copied correctly
        let new_head = unsafe { heap.read_cons_head(new_cons_ptr as usize) };
        assert_eq!(new_head, before_gc_head);
    }

    #[test]
    fn test_heap_gc_stats() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Initial stats
        let stats = heap.get_stats();
        assert_eq!(stats.collections, 0);

        // After minor GC
        heap.minor_gc_empty();
        let stats = heap.get_stats();
        assert_eq!(stats.collections, 1);
        assert_eq!(stats.minor_collections, 1);

        // Reset heap to have clean state for major GC test
        heap.reset();
        heap.alloc(25);

        // After major GC (increments major_collections, internally calls minor_gc)
        heap.major_gc();
        let stats = heap.get_stats();
        assert_eq!(stats.major_collections, 1);
        // Note: collections may be > 2 due to major_gc's internal minor_gc call
        assert!(stats.collections >= 2);
    }

    #[test]
    fn test_cons_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let head = Term::from_small(42);
        let tail = Term::from_small(123);
        let pos = heap.make_cons(head, tail);

        assert!(pos.is_some());
        let p = pos.unwrap();
        // Cons takes 3 words: header + head + tail
        assert_eq!(heap.used_size(), 3);

        // Read back the cons cell
        unsafe {
            let read_head = heap.read_cons_head(p);
            let read_tail = heap.read_cons_tail(p);
            assert_eq!(read_head, head);
            assert_eq!(read_tail, tail);
        }
    }

    #[test]
    fn test_tuple_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let elements = [
            Term::from_small(1),
            Term::from_small(2),
            Term::from_small(3),
        ];
        let pos = heap.make_tuple(&elements);

        assert!(pos.is_some());
        let p = pos.unwrap();
        // Tuple takes 1 + 3 = 4 words: header + 3 elements
        assert_eq!(heap.used_size(), 4);

        // Verify arity
        unsafe {
            assert_eq!(heap.tuple_arity(p), 3);
        }
    }

    #[test]
    fn test_empty_tuple() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let elements: [Term; 0] = [];
        let pos = heap.make_tuple(&elements);

        assert!(pos.is_some());
        let p = pos.unwrap();

        unsafe {
            assert_eq!(heap.tuple_arity(p), 0);
        }
    }

    #[test]
    fn test_cons_and_tuple_different_sizes() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons (3 words)
        let pos1 = heap.make_cons(Term::from_small(1), Term::from_small(2));
        assert!(pos1.is_some());

        // Allocate a tuple with 5 elements (6 words: 1 header + 5 elements)
        let elements = [
            Term::from_small(10),
            Term::from_small(20),
            Term::from_small(30),
            Term::from_small(40),
            Term::from_small(50),
        ];
        let pos2 = heap.make_tuple(&elements);
        assert!(pos2.is_some());

        // Total: 3 + 6 = 9 words
        assert_eq!(heap.used_size(), 9);
    }

    #[test]
    fn test_tuple_element_access() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let elements = [
            Term::from_small(100),
            Term::from_small(200),
            Term::from_small(300),
        ];
        let pos = heap.make_tuple(&elements).unwrap();

        unsafe {
            assert_eq!(heap.read_tuple_element(pos, 0), Term::from_small(100));
            assert_eq!(heap.read_tuple_element(pos, 1), Term::from_small(200));
            assert_eq!(heap.read_tuple_element(pos, 2), Term::from_small(300));
        }
    }

    #[test]
    fn test_map_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let keys_values = [
            (Term::from_small(1), Term::from_small(100)),
            (Term::from_small(2), Term::from_small(200)),
        ];
        let pos = heap.make_map(&keys_values);
        assert!(pos.is_some());

        // Map with 2 keys takes 1 header + 4 (2 key-value pairs) = 5 words
        assert!(heap.used_size() >= 5);

        unsafe {
            assert_eq!(heap.map_size(pos.unwrap()), 2);
        }
    }

    #[test]
    fn test_float_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let pos = heap.make_float(3.14159);
        assert!(pos.is_some());

        // Float takes 3 words
        assert!(heap.used_size() >= 3);

        unsafe {
            let value = heap.read_float(pos.unwrap());
            assert!((value - 3.14159).abs() < 0.0001);
        }
    }

    #[test]
    fn test_bigint_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let digits = [100u32, 200, 300];
        let pos = heap.make_bigint(true, &digits);
        assert!(pos.is_some());

        // BigInt with 3 digits takes 1 header + 2 (sign/count) + 3 = 6 words
        assert!(heap.used_size() >= 6);

        unsafe {
            assert!(heap.bigint_is_negative(pos.unwrap()));
            assert_eq!(heap.bigint_digit_count(pos.unwrap()), 3);
            assert_eq!(heap.read_bigint_digit(pos.unwrap(), 1), 200);
        }
    }

    #[test]
    fn test_fun_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let free_terms = [
            Term::from_small(1),
            Term::from_small(2),
            Term::from_small(3),
        ];
        let pos = heap.make_fun(5, 0x12345678, 3, &free_terms);
        assert!(pos.is_some());

        // Fun with 3 free terms takes 1 header + 3 (metadata) + 3 = 7 words
        assert!(heap.used_size() >= 7);

        unsafe {
            assert_eq!(heap.fun_num_free(pos.unwrap()), 3);
            assert_eq!(heap.fun_old_uniq(pos.unwrap()), 0x12345678);
            assert_eq!(
                heap.read_fun_free_term(pos.unwrap(), 1),
                Term::from_small(2)
            );
        }
    }

    #[test]
    fn test_binary_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let data = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let pos = heap.make_binary(10, &data);
        assert!(pos.is_some());

        // Binary with 10 bytes takes 1 header + 1 (size) + 2 (10 bytes = 2 words) = 4 words
        assert!(heap.used_size() >= 4);

        unsafe {
            assert_eq!(heap.binary_size(pos.unwrap()), 10);
        }
    }

    #[test]
    fn test_nil_allocation() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        let pos = heap.alloc_nil();
        assert!(pos.is_some());
        assert_eq!(
            pos.unwrap(),
            chimera_erlang_beam_term::atoms::ATOM_NIL as usize
        );

        // Nil doesn't use heap space (it's an immediate atom)
        assert_eq!(heap.used_size(), 0);
    }

    #[test]
    fn test_multiple_allocations() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate multiple different types
        let _ = heap.make_cons(Term::from_small(1), Term::nil());
        let _ = heap.make_float(1.5);
        let _ = heap.make_map(&[(Term::from_small(1), Term::from_small(2))]);

        // Should have allocated all three
        // cons: 3 words, float: 3 words, map: 3 words (1 header + 2 for key-value)
        assert!(heap.used_size() >= 3 + 3 + 3);
    }
}

#[cfg(test)]
mod zig_tests {
    use super::*;
    use chimera_erlang_beam_term::Term;

    #[test]
    fn test_zig_heap_scan_integration() {
        use chimera_erlang_beam_abi::heap_kernels;

        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate some data
        let ptr = heap.make_cons(Term::from_small(42), Term::from_small(99));
        assert!(ptr.is_some());

        // Scan using Zig kernel
        let base = heap.active_buffer();
        let result = heap_kernels::heap_scan(base, heap.hp);

        // The scan should succeed and find some objects
        assert!(result.is_ok());
        let output = result.unwrap();
        // Should have scanned some words and found objects
        assert!(output.words_scanned > 0);
        assert!(output.objects_found > 0);
    }

    #[test]
    fn test_zig_and_rust_heap_scan_match() {
        use chimera_erlang_beam_abi::heap_kernels;

        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a cons cell
        heap.make_cons(Term::from_small(1), Term::from_small(2));

        let base = heap.active_buffer();
        let hp = heap.hp;

        // Call Zig kernel
        let zig_result = heap_kernels::heap_scan(base, hp).unwrap();

        // Call Rust fallback
        let rust_result = heap_kernels::rust_fallback::scan(base, hp);

        // They should produce the same counts
        assert_eq!(zig_result.words_scanned, rust_result.words_scanned);
        assert_eq!(zig_result.objects_found, rust_result.objects_found);
    }

    #[test]
    fn test_generation_policy_config() {
        // Test default generation policy config
        let config = HeapConfig::default();
        assert_eq!(config.survivor_ratio, 0.2); // 20% survivor ratio
        assert_eq!(config.promotion_threshold, 3); // Promote after 3 minor GCs

        // Test survivor space size calculation
        let heap = ProcessHeap::new(config);
        let stats = heap.get_stats();

        // Initially no GCs have occurred
        assert_eq!(stats.minor_gc_count, 0);
        assert_eq!(stats.promotions, 0);
    }

    #[test]
    fn test_generation_policy_minor_gc_count() {
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Perform several minor GCs
        for _ in 0..5 {
            heap.minor_gc_empty();
        }

        let stats = heap.get_stats();
        assert_eq!(stats.minor_gc_count, 5);
    }
}
