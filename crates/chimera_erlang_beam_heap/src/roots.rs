//! Root set management for GC tracing.
//!
//! A root set is the set of locations that are always considered live
//! during garbage collection. This includes:
//! - X registers (procedure arguments)
//! - Y registers (temporary storage, frame pointers)
//! - Stack frames (CP/IP saved return addresses)
//! - Mailbox signals (messages awaiting processing)
//! - Save queue (selective receive saved messages)
//! - Process dictionary
//! - Links and monitors
//! - Timer references
//! - Port references
//! - Distribution references (remote PIDs/refs/ports)
//!
//! All roots must be traced during GC to ensure live objects are preserved.

use chimera_erlang_beam_term::Term;

/// Maximum number of X registers (procedure arguments)
pub const MAX_X_REGISTERS: usize = 64;
/// Maximum number of Y registers (temporary storage)
pub const MAX_Y_REGISTERS: usize = 64;
/// Maximum call stack depth
pub const MAX_CALL_STACK_DEPTH: usize = 1024;

/// Root category for debugging and tracing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootCategory {
    /// X register (procedure argument)
    XRegister(u8),
    /// Y register (temporary storage)
    YRegister(u8),
    /// Stack frame saved PC (continuation pointer)
    CP,
    /// Stack frame instruction pointer
    IP,
    /// Mailbox signal/message
    MailboxSignal,
    /// Save queue for selective receive
    SaveQueue,
    /// Process dictionary entry
    DictionaryEntry,
    /// Link target
    Link,
    /// Monitor reference
    Monitor,
    /// Timer reference
    Timer,
    /// Port reference
    Port,
    /// Distribution remote PID
    RemotePid,
    /// Distribution remote reference
    RemoteRef,
    /// Distribution remote port
    RemotePort,
    /// Group leader
    GroupLeader,
    /// Registered name
    RegisteredName,
}

impl RootCategory {
    /// Get a human-readable name for this root category
    pub fn name(&self) -> &'static str {
        match self {
            RootCategory::XRegister(_) => "XRegister",
            RootCategory::YRegister(_) => "YRegister",
            RootCategory::CP => "CP",
            RootCategory::IP => "IP",
            RootCategory::MailboxSignal => "MailboxSignal",
            RootCategory::SaveQueue => "SaveQueue",
            RootCategory::DictionaryEntry => "DictionaryEntry",
            RootCategory::Link => "Link",
            RootCategory::Monitor => "Monitor",
            RootCategory::Timer => "Timer",
            RootCategory::Port => "Port",
            RootCategory::RemotePid => "RemotePid",
            RootCategory::RemoteRef => "RemoteRef",
            RootCategory::RemotePort => "RemotePort",
            RootCategory::GroupLeader => "GroupLeader",
            RootCategory::RegisteredName => "RegisteredName",
        }
    }
}

/// A root location with its category for debugging
#[derive(Debug)]
pub struct RootLocation {
    /// The term value stored at this root
    pub term: Term,
    /// Category for debugging and tracing
    pub category: RootCategory,
}

impl RootLocation {
    /// Create a new root location
    pub fn new(term: Term, category: RootCategory) -> Self {
        RootLocation { term, category }
    }
}

/// Root set for garbage collection.
///
/// Contains all locations that must be traced during GC.
/// Roots are categorized to help with debugging and analysis.
#[derive(Debug, Default)]
pub struct RootSet {
    /// All root locations
    roots: Vec<RootLocation>,
}

impl RootSet {
    /// Create a new empty root set
    pub fn new() -> Self {
        RootSet { roots: Vec::new() }
    }

    /// Clear all roots
    pub fn clear(&mut self) {
        self.roots.clear();
    }

    /// Add a root
    pub fn add(&mut self, term: Term, category: RootCategory) {
        self.roots.push(RootLocation::new(term, category));
    }

    /// Add an X register root
    pub fn add_x(&mut self, index: u8, term: Term) {
        self.add(term, RootCategory::XRegister(index));
    }

    /// Add a Y register root
    pub fn add_y(&mut self, index: u8, term: Term) {
        self.add(term, RootCategory::YRegister(index));
    }

    /// Add a CP (continuation pointer) root
    pub fn add_cp(&mut self, term: Term) {
        self.add(term, RootCategory::CP);
    }

    /// Add an IP (instruction pointer) root
    pub fn add_ip(&mut self, term: Term) {
        self.add(term, RootCategory::IP);
    }

    /// Add mailbox signals as roots
    pub fn add_mailbox(&mut self, signals: &[Term]) {
        for term in signals {
            self.add(*term, RootCategory::MailboxSignal);
        }
    }

    /// Add save queue signals as roots
    pub fn add_save_queue(&mut self, signals: &[Term]) {
        for term in signals {
            self.add(*term, RootCategory::SaveQueue);
        }
    }

    /// Add dictionary entries as roots
    pub fn add_dictionary(&mut self, entries: &[(Term, Term)]) {
        for &(key, value) in entries {
            self.add(key, RootCategory::DictionaryEntry);
            self.add(value, RootCategory::DictionaryEntry);
        }
    }

    /// Add links as roots
    ///
    /// Links are represented as raw u64 values (PID.to_raw())
    pub fn add_links(&mut self, links: &[u64]) {
        for &pid_raw in links {
            self.add(Term::from_small(pid_raw as i64), RootCategory::Link);
        }
    }

    /// Add monitors as roots
    ///
    /// Monitors are represented as tuples of (ref_id, target_pid_raw, target_name)
    pub fn add_monitors(&mut self, monitors: &[(u64, u64, Option<u32>)]) {
        for &(ref_id, target_raw, _) in monitors {
            self.add(Term::from_small(ref_id as i64), RootCategory::Monitor);
            self.add(Term::from_small(target_raw as i64), RootCategory::Monitor);
        }
    }

    /// Get the total number of roots
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Check if root set is empty
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Iterate over roots
    pub fn iter(&self) -> impl Iterator<Item = &RootLocation> {
        self.roots.iter()
    }

    /// Get roots by category
    pub fn by_category(&self, category: RootCategory) -> Vec<&RootLocation> {
        self.roots
            .iter()
            .filter(|r| r.category == category)
            .collect()
    }

    /// Count roots by category
    pub fn count_by_category(&self) -> usize {
        self.roots.len()
    }
}

/// Trait for objects that can contribute roots to the root set
pub trait RootSetProvider {
    /// Fill the root set with all roots from this object
    ///
    /// The default implementation does nothing (no roots).
    fn fill_roots(&self, _roots: &mut RootSet) {}

    /// Get the total number of roots this provider manages
    ///
    /// Used for debugging and metrics.
    fn root_count(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_set_new() {
        let roots = RootSet::new();
        assert!(roots.is_empty());
        assert_eq!(roots.len(), 0);
    }

    #[test]
    fn test_root_set_add() {
        let mut roots = RootSet::new();
        roots.add(Term::from_small(42), RootCategory::XRegister(0));
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn test_root_set_clear() {
        let mut roots = RootSet::new();
        roots.add(Term::from_small(1), RootCategory::XRegister(0));
        roots.add(Term::from_small(2), RootCategory::XRegister(1));
        roots.clear();
        assert!(roots.is_empty());
    }

    #[test]
    fn test_root_set_x_registers() {
        let mut roots = RootSet::new();
        roots.add_x(0, Term::from_small(10));
        roots.add_x(1, Term::from_small(20));
        roots.add_x(5, Term::from_small(50));

        assert_eq!(roots.len(), 3);
        let x0_roots = roots.by_category(RootCategory::XRegister(0));
        assert_eq!(x0_roots.len(), 1);
        assert_eq!(x0_roots[0].term, Term::from_small(10));
    }

    #[test]
    fn test_root_set_y_registers() {
        let mut roots = RootSet::new();
        roots.add_y(0, Term::from_small(100));
        roots.add_y(3, Term::from_small(300));

        assert_eq!(roots.len(), 2);
        let y0_roots = roots.by_category(RootCategory::YRegister(0));
        assert_eq!(y0_roots.len(), 1);
    }

    #[test]
    fn test_root_set_cp_ip() {
        let mut roots = RootSet::new();
        roots.add_cp(Term::from_small(0xDEADBEEF));
        roots.add_ip(Term::from_small(0xCAFEBABE));

        assert_eq!(roots.len(), 2);
        assert_eq!(roots.by_category(RootCategory::CP).len(), 1);
        assert_eq!(roots.by_category(RootCategory::IP).len(), 1);
    }

    #[test]
    fn test_root_set_iteration() {
        let mut roots = RootSet::new();
        roots.add(Term::from_small(1), RootCategory::XRegister(0));
        roots.add(Term::from_small(2), RootCategory::XRegister(1));

        let mut count = 0;
        for root in roots.iter() {
            assert!(root.term.to_small_opt().is_some());
            count += 1;
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn test_root_category_names() {
        assert_eq!(RootCategory::XRegister(0).name(), "XRegister");
        assert_eq!(RootCategory::YRegister(5).name(), "YRegister");
        assert_eq!(RootCategory::CP.name(), "CP");
        assert_eq!(RootCategory::IP.name(), "IP");
        assert_eq!(RootCategory::MailboxSignal.name(), "MailboxSignal");
        assert_eq!(RootCategory::SaveQueue.name(), "SaveQueue");
        assert_eq!(RootCategory::DictionaryEntry.name(), "DictionaryEntry");
        assert_eq!(RootCategory::Link.name(), "Link");
        assert_eq!(RootCategory::Monitor.name(), "Monitor");
        assert_eq!(RootCategory::Timer.name(), "Timer");
        assert_eq!(RootCategory::Port.name(), "Port");
        assert_eq!(RootCategory::RemotePid.name(), "RemotePid");
        assert_eq!(RootCategory::RemoteRef.name(), "RemoteRef");
        assert_eq!(RootCategory::RemotePort.name(), "RemotePort");
        assert_eq!(RootCategory::GroupLeader.name(), "GroupLeader");
        assert_eq!(RootCategory::RegisteredName.name(), "RegisteredName");
    }

    #[test]
    fn test_default_root_set() {
        let roots = RootSet::default();
        assert!(roots.is_empty());
    }
}
