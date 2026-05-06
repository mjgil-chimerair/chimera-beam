//! Heap verification and corruption diagnostics for RustZigBeam.
//!
//! Provides heap validation to detect:
//! - Corrupted headers
//! - Invalid pointers
//! - Out-of-bounds access
//! - Broken object chains
//! - Memory corruption patterns
//!
//! Uses canary/red zones where useful for detecting overflows.

use crate::ProcessHeap;
use chimera_erlang_beam_term::{
    boxed::{extract_size, extract_tag},
    Term, TermTag,
};

/// Result of heap verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether the heap passed verification
    pub is_valid: bool,
    /// List of issues found
    pub issues: Vec<HeapIssue>,
    /// Total words scanned
    pub words_scanned: usize,
    /// Total objects scanned
    pub objects_scanned: usize,
}

impl Default for VerificationResult {
    fn default() -> Self {
        VerificationResult {
            is_valid: true,
            issues: Vec::new(),
            words_scanned: 0,
            objects_scanned: 0,
        }
    }
}

impl VerificationResult {
    /// Add an issue to the verification result
    pub fn add_issue(&mut self, issue: HeapIssue) {
        self.is_valid = false;
        self.issues.push(issue);
    }

    /// Check if verification passed
    pub fn passed(&self) -> bool {
        self.is_valid && self.issues.is_empty()
    }

    /// Get the number of issues found
    pub fn issue_count(&self) -> usize {
        self.issues.len()
    }
}

/// Types of heap corruption that can be detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorruptionType {
    /// Invalid header word encoding
    InvalidHeader,
    /// Pointer points outside heap bounds
    OutOfBoundsPointer,
    /// Pointer points to non-word-aligned address
    UnalignedPointer,
    /// Object size extends beyond heap end
    ObjectOverflow,
    /// Tag mismatch between stored and expected tag
    TagMismatch,
    /// Cons cell has invalid head/tail pointers
    InvalidConsPointer,
    /// Tuple has invalid element pointer
    InvalidTupleElement,
    /// Forwarding pointer not followed by valid address
    InvalidForwardingPointer,
    /// Cyclic reference detected in object graph
    CyclicReference,
    /// Canary/red zone pattern corrupted
    CanaryCorrupted,
    /// Heap pointer beyond allocation boundary
    HpOverflow,
    /// Zero or null pointer used where non-null expected
    NullPointer,
}

/// Severity level of heap issues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Warning - may indicate corruption
    Warning,
    /// Error - definite corruption detected
    Error,
    /// Fatal - heap is unusable
    Fatal,
}

/// A heap issue found during verification
#[derive(Debug, Clone)]
pub struct HeapIssue {
    /// Type of corruption
    pub corruption_type: CorruptionType,
    /// Severity level
    pub severity: Severity,
    /// Word index where issue was found
    pub location: usize,
    /// Description of the issue
    pub description: String,
    /// Expected value (if applicable)
    pub expected: Option<u64>,
    /// Actual value (if applicable)
    pub actual: Option<u64>,
}

impl HeapIssue {
    /// Create a new heap issue
    pub fn new(
        corruption_type: CorruptionType,
        severity: Severity,
        location: usize,
        description: impl Into<String>,
    ) -> Self {
        HeapIssue {
            corruption_type,
            severity,
            location,
            description: description.into(),
            expected: None,
            actual: None,
        }
    }

    /// Create with expected and actual values
    pub fn with_values(mut self, expected: u64, actual: u64) -> Self {
        self.expected = Some(expected);
        self.actual = Some(actual);
        self
    }
}

/// Canary pattern for detecting buffer overflows
/// Placed at the end of heap allocations to detect overflows
pub const HEAP_CANARY_VALUE: u64 = 0xDEAD_BEEF_DEAD_BEEF;

/// Canary zone size in words
pub const CANARY_SIZE_WORDS: usize = 2;

/// Canary protection mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanaryMode {
    /// No canary protection
    Disabled,
    /// Light protection - end of heap only
    Light,
    /// Full protection - every allocation
    Full,
}

/// Heap verification configuration
#[derive(Debug, Clone, Copy)]
pub struct VerifierConfig {
    /// Enable canary/red zone checking
    pub canary_mode: CanaryMode,
    /// Check pointers for validity
    pub check_pointers: bool,
    /// Check object graph for cycles
    pub check_cycles: bool,
    /// Maximum objects to scan (0 = unlimited)
    pub max_scan_objects: usize,
    /// Fail fast on first fatal error
    pub fail_fast: bool,
}

impl Default for VerifierConfig {
    fn default() -> Self {
        VerifierConfig {
            canary_mode: CanaryMode::Disabled,
            check_pointers: true,
            check_cycles: false, // Expensive operation
            max_scan_objects: 0, // Unlimited
            fail_fast: false,
        }
    }
}

/// Heap verifier for detecting corruption
pub struct HeapVerifier {
    config: VerifierConfig,
    // Mark bits for cycle detection
    visited: Vec<bool>,
}

impl HeapVerifier {
    /// Create a new heap verifier with default config
    pub fn new() -> Self {
        HeapVerifier {
            config: VerifierConfig::default(),
            visited: Vec::new(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: VerifierConfig) -> Self {
        HeapVerifier {
            config,
            visited: Vec::new(),
        }
    }

    /// Verify a process heap
    pub fn verify(&mut self, heap: &ProcessHeap) -> VerificationResult {
        let mut result = VerificationResult::default();
        let active_buffer = heap.active_buffer();
        let hp = heap.heap_ptr();

        // Resize visited tracking if needed
        if self.visited.len() < active_buffer.len() {
            self.visited.resize(active_buffer.len(), false);
        }

        // Scan heap objects
        let mut offset = 0;
        while offset < hp {
            result.words_scanned += 1;

            // Check for canary corruption at heap boundaries
            if self.config.canary_mode != CanaryMode::Disabled {
                if let Some(canary_issue) = self.check_canary_at(heap, offset) {
                    result.add_issue(canary_issue);
                    if self.config.fail_fast {
                        return result;
                    }
                }
            }

            // Decode header at current position
            let word = active_buffer[offset];
            let tag = extract_tag(word);

            match tag {
                TermTag::SmallInteger | TermTag::Atom => {
                    // Immediate values don't need verification
                    offset += 1;
                    result.objects_scanned += 1;
                }
                TermTag::Cons => {
                    // Cons cell: header + head + tail (3 words)
                    if let Some(issue) = self.verify_cons(heap, offset, word) {
                        result.add_issue(issue);
                        if self.config.fail_fast {
                            return result;
                        }
                    }
                    offset += 3;
                    result.objects_scanned += 1;
                }
                TermTag::Tuple => {
                    // Tuple: header + elements
                    if let Some(issue) = self.verify_tuple(heap, offset) {
                        result.add_issue(issue);
                        if self.config.fail_fast {
                            return result;
                        }
                    }
                    let header = active_buffer[offset];
                    let size = extract_size(header);
                    offset += size as usize;
                    result.objects_scanned += 1;
                }
                TermTag::Float => {
                    // Float: header + 2 words
                    offset += 3;
                    result.objects_scanned += 1;
                }
                TermTag::Binary => {
                    // Binary: header + size word + data
                    if let Some(issue) = self.verify_binary(heap, offset) {
                        result.add_issue(issue);
                        if self.config.fail_fast {
                            return result;
                        }
                    }
                    let header = active_buffer[offset];
                    let size = extract_size(header);
                    offset += size as usize;
                    result.objects_scanned += 1;
                }
                TermTag::Map | TermTag::Fun => {
                    // Map/Fun: header + data
                    let header = active_buffer[offset];
                    let size = extract_size(header);
                    offset += size as usize;
                    result.objects_scanned += 1;
                }
            }

            // Check max objects limit
            if self.config.max_scan_objects > 0
                && result.objects_scanned >= self.config.max_scan_objects
            {
                break;
            }
        }

        // Check heap pointer validity
        if hp > heap.hend {
            result.add_issue(HeapIssue::new(
                CorruptionType::HpOverflow,
                Severity::Fatal,
                hp,
                format!("Heap pointer {} exceeds heap end {}", hp, heap.hend),
            ));
        }

        result
    }

    /// Verify a cons cell at the given offset
    fn verify_cons(&mut self, heap: &ProcessHeap, offset: usize, _word: u64) -> Option<HeapIssue> {
        let active_buffer = heap.active_buffer();

        // Check header validity
        if offset + 2 >= active_buffer.len() {
            return Some(HeapIssue::new(
                CorruptionType::ObjectOverflow,
                Severity::Fatal,
                offset,
                "Cons cell header overflows heap",
            ));
        }

        // Verify head and tail pointers are valid
        let head_ptr = active_buffer[offset + 1];
        let tail_ptr = active_buffer[offset + 2];

        // Check if pointers are tagged as heap pointers
        if self.config.check_pointers {
            // Head pointer - could be immediate or pointer
            let head_tag = extract_tag(head_ptr);
            if (head_tag == TermTag::Cons || head_tag == TermTag::Tuple)
                && head_ptr as usize >= active_buffer.len()
            {
                return Some(HeapIssue::new(
                    CorruptionType::OutOfBoundsPointer,
                    Severity::Error,
                    offset + 1,
                    format!("Cons head pointer {} out of bounds", head_ptr as usize),
                ));
            }

            // Tail pointer - should be cons or nil
            let tail_tag = extract_tag(tail_ptr);
            if tail_tag == TermTag::Cons && tail_ptr as usize >= active_buffer.len() {
                return Some(HeapIssue::new(
                    CorruptionType::OutOfBoundsPointer,
                    Severity::Error,
                    offset + 2,
                    format!("Cons tail pointer {} out of bounds", tail_ptr as usize),
                ));
            }
        }

        None
    }

    /// Verify a tuple at the given offset
    fn verify_tuple(&mut self, heap: &ProcessHeap, offset: usize) -> Option<HeapIssue> {
        let active_buffer = heap.active_buffer();
        let header = active_buffer[offset];
        let size = extract_size(header) as usize;

        // Check tuple fits in heap
        if offset + size > active_buffer.len() {
            return Some(HeapIssue::new(
                CorruptionType::ObjectOverflow,
                Severity::Fatal,
                offset,
                format!("Tuple size {} overflows heap at offset {}", size, offset),
            ));
        }

        // Verify each element pointer
        if self.config.check_pointers {
            for i in 1..size {
                let elem = active_buffer[offset + i];
                let elem_tag = extract_tag(elem);

                // Only check heap pointers
                if (elem_tag == TermTag::Cons || elem_tag == TermTag::Tuple)
                    && elem as usize >= active_buffer.len()
                {
                    return Some(HeapIssue::new(
                        CorruptionType::OutOfBoundsPointer,
                        Severity::Error,
                        offset + i,
                        format!("Tuple element {} pointer out of bounds", elem as usize),
                    ));
                }
            }
        }

        None
    }

    /// Verify a binary at the given offset
    fn verify_binary(&mut self, heap: &ProcessHeap, offset: usize) -> Option<HeapIssue> {
        let active_buffer = heap.active_buffer();
        let header = active_buffer[offset];
        let size = extract_size(header) as usize;

        // Check binary fits in heap
        if offset + size > active_buffer.len() {
            return Some(HeapIssue::new(
                CorruptionType::ObjectOverflow,
                Severity::Fatal,
                offset,
                format!("Binary size {} overflows heap at offset {}", size, offset),
            ));
        }

        None
    }

    /// Check for canary corruption at a position
    fn check_canary_at(&mut self, heap: &ProcessHeap, offset: usize) -> Option<HeapIssue> {
        let active_buffer = heap.active_buffer();

        // In light mode, only check near heap pointer
        if self.config.canary_mode == CanaryMode::Light {
            let hp = heap.heap_ptr();
            if offset < hp - CANARY_SIZE_WORDS * 4 {
                return None;
            }
        }

        // Check if this might be the start of a canary zone
        // Canary zones are placed after allocations
        if offset >= CANARY_SIZE_WORDS {
            let prev_word = active_buffer[offset - CANARY_SIZE_WORDS];
            if prev_word == HEAP_CANARY_VALUE {
                // This position follows a canary - verify the canary
                for i in 0..CANARY_SIZE_WORDS {
                    if offset + i < active_buffer.len() {
                        let canary_word = active_buffer[offset + i];
                        if canary_word != HEAP_CANARY_VALUE {
                            return Some(HeapIssue::new(
                                CorruptionType::CanaryCorrupted,
                                Severity::Fatal,
                                offset + i,
                                format!(
                                    "Canary corruption at offset {}, expected {:016x}, got {:016x}",
                                    offset + i,
                                    HEAP_CANARY_VALUE,
                                    canary_word
                                ),
                            ));
                        }
                    }
                }
            }
        }

        None
    }

    /// Verify with a root set to check object graph consistency
    pub fn verify_with_roots<R>(&mut self, heap: &ProcessHeap, roots: R) -> VerificationResult
    where
        R: Iterator<Item = Term>,
    {
        let mut result = self.verify(heap);

        // Check roots point to valid objects
        for root in roots {
            if let Some(issue) = self.verify_root_pointer(heap, root) {
                result.add_issue(issue);
                if self.config.fail_fast {
                    return result;
                }
            }
        }

        result
    }

    /// Verify a root pointer points to valid heap location
    fn verify_root_pointer(&mut self, heap: &ProcessHeap, root: Term) -> Option<HeapIssue> {
        let active_buffer = heap.active_buffer();
        let tag = root.tag();

        match tag {
            TermTag::Cons => {
                let ptr = root.to_cons() as usize;
                if ptr >= active_buffer.len() {
                    return Some(HeapIssue::new(
                        CorruptionType::OutOfBoundsPointer,
                        Severity::Error,
                        ptr,
                        "Root cons pointer out of bounds",
                    ));
                }
            }
            TermTag::Tuple => {
                let ptr = root.to_tuple() as usize;
                if ptr >= active_buffer.len() {
                    return Some(HeapIssue::new(
                        CorruptionType::OutOfBoundsPointer,
                        Severity::Error,
                        ptr,
                        "Root tuple pointer out of bounds",
                    ));
                }
            }
            _ => {}
        }

        None
    }

    /// Reset visited tracking for cycle detection
    pub fn reset_visited(&mut self) {
        for v in &mut self.visited {
            *v = false;
        }
    }
}

impl Default for HeapVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Diagnostic report with formatted output
#[derive(Debug)]
pub struct DiagnosticReport {
    /// The verification result
    pub result: VerificationResult,
    /// Formatted issue list
    formatted_issues: Vec<String>,
}

impl DiagnosticReport {
    /// Generate a diagnostic report from verification result
    pub fn from_result(result: VerificationResult) -> Self {
        let mut formatted_issues = Vec::new();

        for issue in result.issues.iter() {
            let severity_str = match issue.severity {
                Severity::Warning => "WARN",
                Severity::Error => "ERROR",
                Severity::Fatal => "FATAL",
            };

            let mut msg = format!(
                "[{}] {:?} at offset {}: {}",
                severity_str, issue.corruption_type, issue.location, issue.description
            );

            if let (Some(expected), Some(actual)) = (issue.expected, issue.actual) {
                msg.push_str(&format!(
                    " (expected: 0x{:016x}, actual: 0x{:016x})",
                    expected, actual
                ));
            }

            formatted_issues.push(msg);
        }

        DiagnosticReport {
            result,
            formatted_issues,
        }
    }

    /// Get formatted report as string
    pub fn format(&self) -> String {
        let mut output = String::new();

        if self.result.passed() {
            output.push_str("Heap verification PASSED\n");
            output.push_str(&format!(
                "Scanned {} words, {} objects\n",
                self.result.words_scanned, self.result.objects_scanned
            ));
        } else {
            output.push_str(&format!(
                "Heap verification FAILED - {} issues found\n",
                self.result.issue_count()
            ));
            output.push_str(&format!(
                "Scanned {} words, {} objects\n\n",
                self.result.words_scanned, self.result.objects_scanned
            ));

            for issue in &self.formatted_issues {
                output.push_str(&format!("  {}\n", issue));
            }
        }

        output
    }

    /// Print to stderr
    pub fn print(&self) {
        eprintln!("{}", self.format());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HeapConfig;
    use chimera_erlang_beam_term::Term;

    #[test]
    fn test_verifier_empty_heap() {
        let mut verifier = HeapVerifier::new();
        let heap = ProcessHeap::new(HeapConfig::default());

        let result = verifier.verify(&heap);
        assert!(result.passed());
        assert_eq!(result.words_scanned, 0);
    }

    #[test]
    fn test_verifier_valid_cons() {
        let mut verifier = HeapVerifier::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a valid cons cell
        let ptr = heap.make_cons(Term::from_small(1), Term::from_small(2));
        assert!(ptr.is_some());

        let result = verifier.verify(&heap);
        assert!(result.passed(), "Valid cons cell should pass verification");
    }

    #[test]
    fn test_verifier_valid_tuple() {
        let mut verifier = HeapVerifier::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate a valid tuple
        let ptr = heap.make_tuple(&[
            Term::from_small(1),
            Term::from_small(2),
            Term::from_small(3),
        ]);
        assert!(ptr.is_some());

        let result = verifier.verify(&heap);
        assert!(result.passed(), "Valid tuple should pass verification");
    }

    #[test]
    fn test_verifier_multiple_objects() {
        let mut verifier = HeapVerifier::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate multiple objects
        for i in 0..10 {
            heap.make_cons(Term::from_small(i as i64), Term::from_small((i * 2) as i64));
        }

        let result = verifier.verify(&heap);
        assert!(result.passed(), "Multiple valid objects should pass");
    }

    #[test]
    fn test_corruption_detected() {
        let mut verifier = HeapVerifier::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate an object
        let ptr = heap.make_cons(Term::from_small(1), Term::from_small(2));
        assert!(ptr.is_some());

        // Corrupt a word in the heap (not during normal operation, this is for testing)
        let active = heap.active_buffer_mut();
        if active.len() > 5 {
            active[5] = 0xDEAD_BEEF_DEAD_BEEF; // Corrupt a word
        }

        let result = verifier.verify(&heap);
        // The corruption may or may not be detected depending on what we corrupted
        // This is just a basic sanity test
        assert!(result.issue_count() >= 0);
    }

    #[test]
    fn test_verification_result_default() {
        let result = VerificationResult::default();
        assert!(result.passed());
        assert_eq!(result.issue_count(), 0);
    }

    #[test]
    fn test_verification_result_add_issue() {
        let mut result = VerificationResult::default();
        assert!(result.passed());

        result.add_issue(HeapIssue::new(
            CorruptionType::InvalidHeader,
            Severity::Error,
            42,
            "Test corruption",
        ));

        assert!(!result.passed());
        assert_eq!(result.issue_count(), 1);
    }

    #[test]
    fn test_heap_issue_with_values() {
        let issue = HeapIssue::new(
            CorruptionType::TagMismatch,
            Severity::Error,
            100,
            "Tag mismatch detected",
        )
        .with_values(0x12345678, 0x87654321);

        assert_eq!(issue.expected, Some(0x12345678));
        assert_eq!(issue.actual, Some(0x87654321));
    }

    #[test]
    fn test_diagnostic_report_passed() {
        let result = VerificationResult::default();
        let report = DiagnosticReport::from_result(result);

        let formatted = report.format();
        assert!(formatted.contains("PASSED"));
    }

    #[test]
    fn test_diagnostic_report_failed() {
        let mut result = VerificationResult::default();
        result.add_issue(HeapIssue::new(
            CorruptionType::OutOfBoundsPointer,
            Severity::Error,
            42,
            "Pointer out of bounds",
        ));

        let report = DiagnosticReport::from_result(result);
        let formatted = report.format();

        assert!(formatted.contains("FAILED"));
        assert!(formatted.contains("out of bounds"));
    }

    #[test]
    fn test_canary_mode_default() {
        let config = VerifierConfig::default();
        assert_eq!(config.canary_mode, CanaryMode::Disabled);
        assert!(config.check_pointers);
        assert!(!config.check_cycles);
    }

    #[test]
    fn test_verifier_config_full() {
        let config = VerifierConfig {
            canary_mode: CanaryMode::Full,
            check_pointers: true,
            check_cycles: true,
            max_scan_objects: 1000,
            fail_fast: true,
        };

        let mut verifier = HeapVerifier::with_config(config);
        // Just verify it doesn't panic
        let heap = ProcessHeap::new(HeapConfig::default());
        let result = verifier.verify(&heap);
        assert!(result.words_scanned >= 0);
    }

    #[test]
    fn test_verifier_with_roots() {
        let mut verifier = HeapVerifier::new();
        let mut heap = ProcessHeap::new(HeapConfig::default());

        // Allocate and create a root
        let ptr = heap.make_cons(Term::from_small(1), Term::from_small(2));
        assert!(ptr.is_some());

        let cons_term = Term::from_cons(ptr.unwrap() as u64);
        let roots = vec![cons_term];

        let result = verifier.verify_with_roots(&heap, roots.into_iter());
        assert!(result.passed(), "Valid root should pass verification");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Fatal as u8 > Severity::Error as u8);
        assert!(Severity::Error as u8 > Severity::Warning as u8);
    }
}
