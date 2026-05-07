//! Core types and constants for RustZigBeam.
//!
//! This crate provides shared types used across the VM: errors, node identity,
//! PID metadata, runtime configuration, reduction budgets, and common result types.
//!
//! No crate should depend on `chimera_erlang_beam_vm`, `chimera_erlang_beam_scheduler`, or
//! other semantic VM crates. `chimera_erlang_beam_core` is the foundation layer.

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

/// VM constants
pub mod constants {
    /// Default reduction budget per schedule slice
    pub const DEFAULT_REDUCTION_BUDGET: u64 = 2000;

    /// Maximum atom cache entries
    pub const MAX_ATOM_CACHE_ENTRIES: usize = 256;

    /// Default heap size in words
    pub const DEFAULT_HEAP_SIZE: usize = 8192;

    /// Default message queue max length
    pub const DEFAULT_MAX_MESSAGE_QUEUE_LEN: usize = 1000;

    /// Maximum number of schedulers
    pub const MAX_SCHEDULERS: u32 = 1024;

    /// Default number of dirty CPU schedulers
    pub const DEFAULT_DIRTY_CPU_SCHEDULERS: u32 = 2;

    /// Default number of dirty IO schedulers
    pub const DEFAULT_DIRTY_IO_SCHEDULERS: u32 = 2;

    /// Maximum atom table size
    pub const MAX_ATOM_TABLE_SIZE: usize = 1_000_000;

    /// Default tick interval in milliseconds
    pub const DEFAULT_TICK_INTERVAL_MS: u64 = 15000;

    /// Default distribution buffer size
    pub const DEFAULT_DIST_BUFFER_SIZE: usize = 4096;
}

/// Node identifier for distributed Erlang.
///
/// Uniquely identifies a node in the distributed system using a name and
/// creation number. The creation number helps distinguish restarted nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeId {
    /// Node name (e.g., "myapp@localhost")
    pub name: String,
    /// Creation number to distinguish node restarts
    pub creation: u32,
}

impl NodeId {
    /// Creates a new NodeId with the given name and default creation (0).
    pub fn new(name: &str) -> Self {
        NodeId {
            name: name.to_string(),
            creation: 0,
        }
    }

    /// Creates a new NodeId with the given name and creation number.
    pub fn with_creation(name: &str, creation: u32) -> Self {
        NodeId {
            name: name.to_string(),
            creation,
        }
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.creation)
    }
}

/// Process ID metadata (without the actual PID term encoding).
///
/// Contains the components that identify an Erlang process: id, serial number,
/// and creation number for distributed scenarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PidMeta {
    /// Process ID number
    pub id: u32,
    /// Serial number (incremented on process reuse)
    pub serial: u32,
    /// Creation number of the node where the process lives
    pub creation: u32,
}

impl PidMeta {
    /// Creates a new PidMeta with the given components.
    pub fn new(id: u32, serial: u32, creation: u32) -> Self {
        PidMeta {
            id,
            serial,
            creation,
        }
    }

    /// Returns true if this PID is local to the current node.
    ///
    /// Currently always returns true; full distributed detection
    /// would compare creation numbers.
    pub fn is_local(&self) -> bool {
        // Local PIDs have creation matching node creation
        true
    }
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Node name for distribution
    pub node_name: String,
    /// Creation number for PIDs and node
    pub creation: u32,
    /// Number of scheduler threads
    pub schedulers: u32,
    /// Number of dirty CPU threads
    pub dirty_cpu_schedulers: u32,
    /// Number of dirty IO threads
    pub dirty_io_schedulers: u32,
    /// Default heap size in words
    pub heap_size: usize,
    /// Maximum message queue length
    pub max_message_queue_len: usize,
    /// Reduction budget per schedule slice
    pub reduction_budget: u64,
    /// Enable atom cache
    pub atom_cache_enabled: bool,
    /// Atom cache size
    pub atom_cache_size: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            node_name: "rustzigbeam@localhost".to_string(),
            creation: 0,
            schedulers: 1,
            dirty_cpu_schedulers: 2,
            dirty_io_schedulers: 2,
            heap_size: constants::DEFAULT_HEAP_SIZE,
            max_message_queue_len: constants::DEFAULT_MAX_MESSAGE_QUEUE_LEN,
            reduction_budget: constants::DEFAULT_REDUCTION_BUDGET,
            atom_cache_enabled: true,
            atom_cache_size: constants::MAX_ATOM_CACHE_ENTRIES,
        }
    }
}

impl RuntimeConfig {
    /// Creates a new RuntimeConfig with the given node name and default values.
    pub fn new(node_name: &str) -> Self {
        RuntimeConfig {
            node_name: node_name.to_string(),
            ..Default::default()
        }
    }

    /// Sets the number of scheduler threads and returns self (builder pattern).
    pub fn with_schedulers(mut self, n: u32) -> Self {
        self.schedulers = n;
        self
    }

    /// Sets the default heap size in words and returns self (builder pattern).
    pub fn with_heap_size(mut self, size: usize) -> Self {
        self.heap_size = size;
        self
    }
}

/// VM error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// Process not found
    ProcessNotFound,
    /// Invalid PID
    InvalidPid,
    /// Process exited
    ProcessExited(Term),
    /// Heap exhausted
    HeapExhausted,
    /// Atom table full
    AtomTableFull,
    /// Invalid term
    InvalidTerm,
    /// Binary error
    BinaryError,
    /// Distribution error
    DistError(DistErrorKind),
    /// Code loader error
    LoadError(LoadErrorKind),
    /// Scheduler error
    SchedulerError,
    /// Timeout
    Timeout,
    /// Bad argument
    BadArg,
    /// Unimplemented feature
    Unimplemented,
    /// IO error
    IoError(String),
    /// Generic error with message
    Generic(String),
}

/// Distribution error types for node-to-node communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistErrorKind {
    /// Failed to establish TCP connection to remote node
    ConnectionFailed,
    /// Handshake protocol negotiation failed
    HandshakeFailed,
    /// Authentication (cookie) verification failed
    AuthenticationFailed,
    /// Received malformed or unexpected packet
    InvalidPacket,
    /// Operation timed out
    Timeout,
    /// EPMD (Erlang Port Mapper Daemon) operation failed
    EpmdError,
}

/// Code loading error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadErrorKind {
    /// Module file could not be found
    FileNotFound,
    /// File has invalid or unsupported format
    InvalidFormat,
    /// Module chunk is truncated or corrupted
    TruncatedChunk,
    /// No executable code found in module
    NoCodeFound,
    /// Failed to import a dependency
    ImportError,
    /// Failed to export a function
    ExportError,
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmError::ProcessNotFound => write!(f, "process not found"),
            VmError::InvalidPid => write!(f, "invalid PID"),
            VmError::ProcessExited(r) => write!(f, "process exited: {:?}", r),
            VmError::HeapExhausted => write!(f, "heap exhausted"),
            VmError::AtomTableFull => write!(f, "atom table full"),
            VmError::InvalidTerm => write!(f, "invalid term"),
            VmError::BinaryError => write!(f, "binary error"),
            VmError::DistError(e) => write!(f, "distribution error: {:?}", e),
            VmError::LoadError(e) => write!(f, "load error: {:?}", e),
            VmError::SchedulerError => write!(f, "scheduler error"),
            VmError::Timeout => write!(f, "timeout"),
            VmError::BadArg => write!(f, "bad argument"),
            VmError::Unimplemented => write!(f, "unimplemented feature"),
            VmError::IoError(e) => write!(f, "IO error: {}", e),
            VmError::Generic(msg) => write!(f, "{}", msg),
        }
    }
}

/// Result type for VM operations
pub type VmResult<T> = Result<T, VmError>;

/// Term type placeholder - actual Term is in chimera_erlang_beam_term
// Using a type alias for now to avoid circular dependencies
pub type Term = u64;

/// Scheduler priority levels for processes.
///
/// Processes with higher priority get more reduction budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Priority {
    /// Low priority - gets smallest reduction budget
    Low = 0,
    /// Normal priority - standard reduction budget
    #[default]
    Normal = 1,
    /// High priority - larger reduction budget
    High = 2,
    /// Maximum priority - gets largest reduction budget
    Max = 3,
}

/// Exit reason for process termination.
///
/// Used when a process exits or is killed, including user-defined reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    /// Normal process termination
    Normal,
    /// Process was killed
    Killed,
    /// Bad argument provided
    BadArg,
    /// System limit reached
    SystemLimit,
    /// User-defined exit reason (wraps a Term)
    UserDefined(Term),
}

impl ExitReason {
    /// Converts the exit reason to a Term representation.
    ///
    /// Maps standard reasons to well-known atoms; user-defined reasons
    /// are passed through directly.
    pub fn to_term(&self) -> Term {
        match self {
            ExitReason::Normal => 0,      // ATOM_NORMAL
            ExitReason::Killed => 0,      // Would map to ATOM_KILL
            ExitReason::BadArg => 0,      // Would map to ATOM_BADARG
            ExitReason::SystemLimit => 0, // Would map to ATOM_SYSTEM_LIMIT
            ExitReason::UserDefined(t) => *t,
        }
    }
}

/// Reduction budget for scheduler.
///
/// Tracks how many reductions (approximate work units) a process
/// can consume before yielding to the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReductionBudget {
    /// Current remaining budget
    pub budget: u64,
    /// Initial budget (for reset)
    pub initial: u64,
}

impl ReductionBudget {
    /// Creates a new ReductionBudget with the given initial value.
    pub fn new(budget: u64) -> Self {
        ReductionBudget {
            budget,
            initial: budget,
        }
    }

    /// Decrements the budget by n, saturating at zero.
    pub fn decrement(&mut self, n: u64) {
        self.budget = self.budget.saturating_sub(n);
    }

    /// Returns true if the budget has been exhausted (reached zero).
    pub fn is_exhausted(&self) -> bool {
        self.budget == 0
    }

    /// Resets the budget to its initial value.
    pub fn reset(&mut self) {
        self.budget = self.initial;
    }
}

impl Default for ReductionBudget {
    fn default() -> Self {
        Self::new(constants::DEFAULT_REDUCTION_BUDGET)
    }
}

/// Atom ID type
pub type AtomId = u32;

/// Reserved atom indices for commonly-used atoms.
pub mod atoms {
    use super::*;

    /// Atom for 'false'
    pub const ATOM_FALSE: AtomId = 0;
    /// Atom for 'true'
    pub const ATOM_TRUE: AtomId = 1;
    /// Atom for 'nil'
    pub const ATOM_NIL: AtomId = 2;
    /// Atom for 'undefined'
    pub const ATOM_UNDEFINED: AtomId = 3;
    /// Atom for 'ok'
    pub const ATOM_OK: AtomId = 4;
    /// Atom for 'error'
    pub const ATOM_ERROR: AtomId = 5;
    /// Atom for 'badarg'
    pub const ATOM_BADARG: AtomId = 6;
    /// Atom for 'exit'
    pub const ATOM_EXIT: AtomId = 7;
    /// Atom for 'normal'
    pub const ATOM_NORMAL: AtomId = 8;
    /// Atom for 'kill'
    pub const ATOM_KILL: AtomId = 9;
    /// Alias for ATOM_NORMAL
    pub const ATOM_NORMAL_EXIT: AtomId = ATOM_NORMAL;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.schedulers, 1);
        assert_eq!(config.heap_size, constants::DEFAULT_HEAP_SIZE);
    }

    #[test]
    fn test_runtime_config_builder() {
        let config = RuntimeConfig::new("test@node")
            .with_schedulers(4)
            .with_heap_size(16384);
        assert_eq!(config.node_name, "test@node");
        assert_eq!(config.schedulers, 4);
        assert_eq!(config.heap_size, 16384);
    }

    #[test]
    fn test_pid_meta() {
        let meta = PidMeta::new(1, 0, 0);
        assert_eq!(meta.id, 1);
        assert!(meta.is_local());
    }

    #[test]
    fn test_reduction_budget() {
        let mut budget = ReductionBudget::new(100);
        assert!(!budget.is_exhausted());

        budget.decrement(50);
        assert_eq!(budget.budget, 50);

        budget.decrement(100);
        assert!(budget.is_exhausted());

        budget.reset();
        assert_eq!(budget.budget, 100);
    }

    #[test]
    fn test_node_id() {
        let node = NodeId::new("test");
        assert_eq!(node.name, "test");
        assert_eq!(node.creation, 0);

        let node2 = NodeId::with_creation("test", 1);
        assert_eq!(node2.creation, 1);
    }

    #[test]
    fn test_priority_default() {
        assert_eq!(Priority::default(), Priority::Normal);
    }
}
