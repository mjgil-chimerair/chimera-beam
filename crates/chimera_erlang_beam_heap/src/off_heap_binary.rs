//! Off-heap binary management for RustZigBeam.
//!
//! Implements ref-counted off-heap binaries for large binary data.
//! Binaries below a threshold are stored on-heap; larger ones are stored
//! off-heap with reference counting.
//!
//! Sub-binaries reference a portion of an off-heap binary without copying.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Threshold in bytes above which binaries are stored off-heap
/// BEAM typically uses 64 bytes as the threshold
pub const OFF_HEAP_BINARY_THRESHOLD: usize = 64;

/// A reference-counted off-heap binary.
///
/// Large binaries are stored outside the heap with atomic reference counting.
/// This allows efficient message passing where only the reference is copied,
/// not the entire binary data.
#[derive(Debug)]
pub struct OffHeapBinary {
    /// Reference count (atomic for thread safety)
    ref_count: AtomicUsize,
    /// Binary data stored off-heap
    data: Box<[u8]>,
}

impl OffHeapBinary {
    /// Create a new off-heap binary with the given data
    pub fn new(data: Vec<u8>) -> Self {
        OffHeapBinary {
            ref_count: AtomicUsize::new(1),
            data: data.into_boxed_slice(),
        }
    }

    /// Get current reference count
    pub fn ref_count(&self) -> usize {
        self.ref_count.load(Ordering::SeqCst)
    }

    /// Increment reference count (for sharing)
    pub fn increment_ref(&self) {
        self.ref_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Decrement reference count and return true if this was the last reference
    pub fn decrement_ref(&self) -> bool {
        // On drop, if ref count reaches 0, the binary is freed
        self.ref_count.fetch_sub(1, Ordering::SeqCst) == 1
    }

    /// Get the binary data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the size in bytes
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Create an Arc-wrapped reference to this binary for sharing
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// A sub-binary referencing a portion of an off-heap binary.
///
/// Sub-binaries are created when part of a binary is extracted (e.g., in binary pattern
/// matching or when sending a portion of a binary to another process). They share
/// the underlying off-heap binary's data without copying.
#[derive(Debug, Clone)]
pub struct SubBinary {
    /// Reference to the parent off-heap binary
    parent: Arc<OffHeapBinary>,
    /// Offset in bytes from the start of the parent's data
    offset: usize,
    /// Size in bytes of this sub-binary
    size: usize,
}

impl SubBinary {
    /// Create a new sub-binary referencing a portion of a parent binary
    pub fn new(parent: Arc<OffHeapBinary>, offset: usize, size: usize) -> Self {
        assert!(
            offset + size <= parent.size(),
            "sub-binary exceeds parent bounds"
        );
        SubBinary {
            parent,
            offset,
            size,
        }
    }

    /// Get the offset in bytes
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get the size in bytes
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the parent off-heap binary
    pub fn parent(&self) -> &Arc<OffHeapBinary> {
        &self.parent
    }

    /// Get the binary data for this sub-binary
    pub fn data(&self) -> &[u8] {
        &self.parent.data[self.offset..self.offset + self.size]
    }

    /// Check if this sub-binary is byte-aligned
    pub fn is_byte_aligned(&self) -> bool {
        self.offset % 8 == 0 && self.size % 8 == 0
    }
}

/// Either an on-heap binary header index or an off-heap binary reference
#[derive(Debug, Clone)]
pub enum BinaryRef {
    /// On-heap binary at the given word index
    OnHeap(u64),
    /// Off-heap binary with its Arc
    OffHeap(Arc<OffHeapBinary>),
    /// Sub-binary referencing a portion of an off-heap binary
    SubBinary(SubBinary),
}

impl BinaryRef {
    /// Get the size in bytes
    pub fn size(&self) -> usize {
        match self {
            BinaryRef::OnHeap(_) => {
                // On-heap binaries store size in their header word
                // Caller should know the size from context
                0
            }
            BinaryRef::OffHeap(bin) => bin.size(),
            BinaryRef::SubBinary(sub) => sub.size(),
        }
    }

    /// Check if this is an off-heap binary
    pub fn is_off_heap(&self) -> bool {
        matches!(self, BinaryRef::OffHeap(_) | BinaryRef::SubBinary(_))
    }
}

/// Virtual binary heap accounting for process memory tracking.
///
/// Tracks all off-heap binaries associated with a process for memory accounting.
/// This is used to implement BEAM's virtual binary heap concept where off-heap
/// binary memory is counted toward the process's total memory usage.
#[derive(Debug, Default)]
pub struct VirtualBinaryHeap {
    /// Total size of all off-heap binaries in bytes
    total_off_heap_bytes: usize,
    /// Total ref count sum across all off-heap binaries
    total_ref_count: usize,
}

impl VirtualBinaryHeap {
    /// Create a new virtual binary heap
    pub fn new() -> Self {
        VirtualBinaryHeap {
            total_off_heap_bytes: 0,
            total_ref_count: 0,
        }
    }

    /// Get total off-heap binary bytes
    pub fn total_bytes(&self) -> usize {
        self.total_off_heap_bytes
    }

    /// Get total ref count
    pub fn total_ref_count(&self) -> usize {
        self.total_ref_count
    }

    /// Account for a new off-heap binary being created
    pub fn account_alloc(&mut self, size: usize, ref_count: usize) {
        self.total_off_heap_bytes += size;
        self.total_ref_count += ref_count;
    }

    /// Account for a binary being deallocated
    pub fn account_dealloc(&mut self, size: usize, ref_count: usize) {
        self.total_off_heap_bytes = self.total_off_heap_bytes.saturating_sub(size);
        self.total_ref_count = self.total_ref_count.saturating_sub(ref_count);
    }

    /// Account for ref count increment (when binary is shared)
    pub fn account_increment(&mut self) {
        self.total_ref_count += 1;
    }

    /// Account for ref count decrement (when a reference is dropped)
    pub fn account_decrement(&mut self) {
        self.total_ref_count = self.total_ref_count.saturating_sub(1);
    }
}

/// Statistics for off-heap binary accounting
#[derive(Debug, Default, Clone, Copy)]
pub struct BinaryStats {
    /// Number of off-heap binaries
    pub binary_count: usize,
    /// Total bytes in off-heap binaries
    pub total_bytes: usize,
    /// Total reference count
    pub total_refs: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_off_heap_binary_creation() {
        let data = vec![1u8, 2, 3, 4, 5];
        let bin = OffHeapBinary::new(data);
        assert_eq!(bin.size(), 5);
        assert_eq!(bin.ref_count(), 1);
    }

    #[test]
    fn test_off_heap_binary_ref_count() {
        let data = vec![1u8, 2, 3, 4];
        let bin = OffHeapBinary::new(data);

        assert_eq!(bin.ref_count(), 1);
        bin.increment_ref();
        assert_eq!(bin.ref_count(), 2);
        bin.increment_ref();
        assert_eq!(bin.ref_count(), 3);

        // decrement_ref returns false when not last ref
        assert!(!bin.decrement_ref());
        assert_eq!(bin.ref_count(), 2);

        // decrement_ref returns true when last ref
        assert!(!bin.decrement_ref());
        assert_eq!(bin.ref_count(), 1);
        assert!(bin.decrement_ref()); // Now reaches 0
    }

    #[test]
    fn test_sub_binary() {
        let data = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let bin = OffHeapBinary::new(data);
        let arc = Arc::new(bin);

        let sub = SubBinary::new(arc.clone(), 2, 4);

        assert_eq!(sub.offset(), 2);
        assert_eq!(sub.size(), 4);
        assert_eq!(sub.data(), &[2u8, 3, 4, 5]);
    }

    #[test]
    fn test_binary_ref_variants() {
        let data = vec![1u8, 2, 3, 4];
        let bin = OffHeapBinary::new(data);

        // On-heap reference
        let on_heap = BinaryRef::OnHeap(100);
        assert!(!on_heap.is_off_heap());

        // Off-heap reference
        let off_heap = BinaryRef::OffHeap(Arc::new(bin));
        assert!(off_heap.is_off_heap());
        assert_eq!(off_heap.size(), 4);

        // Sub-binary
        let data2 = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let bin2 = OffHeapBinary::new(data2);
        let sub = SubBinary::new(Arc::new(bin2), 1, 3);
        let sub_ref = BinaryRef::SubBinary(sub);
        assert!(sub_ref.is_off_heap());
        assert_eq!(sub_ref.size(), 3);
    }

    #[test]
    fn test_virtual_binary_heap_accounting() {
        let mut vbh = VirtualBinaryHeap::new();

        // Account allocation
        vbh.account_alloc(100, 1);
        assert_eq!(vbh.total_bytes(), 100);
        assert_eq!(vbh.total_ref_count(), 1);

        // Account another allocation
        vbh.account_alloc(50, 1);
        assert_eq!(vbh.total_bytes(), 150);
        assert_eq!(vbh.total_ref_count(), 2);

        // Account deallocation
        vbh.account_dealloc(50, 1);
        assert_eq!(vbh.total_bytes(), 100);
        assert_eq!(vbh.total_ref_count(), 1);

        // Account ref count changes
        vbh.account_increment();
        assert_eq!(vbh.total_ref_count(), 2);

        vbh.account_decrement();
        assert_eq!(vbh.total_ref_count(), 1);
    }

    #[test]
    fn test_threshold_constant() {
        assert_eq!(OFF_HEAP_BINARY_THRESHOLD, 64);
    }
}
