//! ETrace infrastructure for RustZigBeam.
//!
//! Provides Erlang-style tracing for runtime debugging and introspection.
//! Supports call tracing, receive tracing, and sequential tracing.
//!
//! Per task-3.md Task A-1: Implement ETrace Infrastructure.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_term::Term;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Trace event flags (can be OR'd together)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TraceFlags(u8);

impl TraceFlags {
    pub const CALL: Self = TraceFlags(1 << 0);
    pub const RETURN_TO: Self = TraceFlags(1 << 1);
    pub const RAISE: Self = TraceFlags(1 << 2);
    pub const RUNNING: Self = TraceFlags(1 << 3);
    pub const PROCS: Self = TraceFlags(1 << 4);
    pub const SMP: Self = TraceFlags(1 << 5);
    pub const RECEIVE: Self = TraceFlags(1 << 6);
    pub const RUNNING_PROCS: Self = TraceFlags(1 << 7);

    pub fn is_set(&self, flag: TraceFlags) -> bool {
        (self.0 & flag.0) != 0
    }

    pub fn set(&mut self, flag: TraceFlags) {
        self.0 |= flag.0;
    }

    pub fn clear(&mut self, flag: TraceFlags) {
        self.0 &= !flag.0;
    }

    pub fn from_list(flags: &[Term]) -> Self {
        let mut result = TraceFlags(0);
        for flag in flags {
            let atom_id = flag.to_atom();
            // Match atom IDs to trace flags
            // In a real implementation, these would be actual atom table indices
            match atom_id {
                0 => result.set(Self::CALL),          // 'call'
                1 => result.set(Self::RETURN_TO),     // 'return_to'
                2 => result.set(Self::RAISE),         // 'raise'
                3 => result.set(Self::RUNNING),       // 'running'
                4 => result.set(Self::PROCS),         // 'procs'
                5 => result.set(Self::SMP),           // 'smp'
                6 => result.set(Self::RECEIVE),       // 'receive'
                7 => result.set(Self::RUNNING_PROCS), // 'running_procs'
                _ => {}
            }
        }
        result
    }

    /// Parse trace flags from a Term that represents a list of atoms.
    /// Returns default (no flags) if term is not a proper list.
    ///
    /// # TODO
    /// This is a placeholder - proper list traversal requires heap access
    /// to read the tail pointer from each cons cell. The implementation
    /// needs access to a ProcessHeap or similar to dereference cons cells.
    pub fn from_list_term(list_term: Term) -> Self {
        let mut result = TraceFlags(0);
        let current = list_term;

        // Placeholder: read head from cons cell representation
        // Real implementation needs heap traversal:
        // loop {
        //     let (head, tail) = heap.read_cons(current);
        //     // process head...
        //     current = tail;
        //     if current.is_nil() { break; }
        // }
        if current.is_cons() {
            let head = current.to_cons();
            let atom_id = head as u32;
            match atom_id {
                0 => result.set(Self::CALL),          // 'call'
                1 => result.set(Self::RETURN_TO),     // 'return_to'
                2 => result.set(Self::RAISE),         // 'raise'
                3 => result.set(Self::RUNNING),       // 'running'
                4 => result.set(Self::PROCS),         // 'procs'
                5 => result.set(Self::SMP),           // 'smp'
                6 => result.set(Self::RECEIVE),       // 'receive'
                7 => result.set(Self::RUNNING_PROCS), // 'running_procs'
                _ => {}
            }
        }
        result
    }
}

/// Trace event type
#[derive(Debug, Clone)]
pub struct TraceEvent {
    /// Timestamp in nanoseconds since UNIX epoch
    pub timestamp_ns: u64,
    /// Trace flags active for this event
    pub flags: TraceFlags,
    /// Process ID that generated the event
    pub pid: u32,
    /// Serial number of the process
    pub serial: u32,
    /// Module name (atom index)
    pub module: Option<u32>,
    /// Function name (atom index)
    pub function: Option<u32>,
    /// Arity of the call
    pub arity: usize,
    /// Extra data specific to event type
    pub extra: TraceExtra,
}

/// Extra data for trace events
#[derive(Debug, Clone)]
pub enum TraceExtra {
    /// No extra data
    None,
    /// Return value from call
    ReturnValue(Term),
    /// Exception reason
    Exception(Term),
    /// Message being received
    Message(Term),
    /// Timestamp for receive timeout
    Timeout,
}

/// Tracer trait - implementors receive trace events
pub trait Tracer: Send + Sync {
    fn trace(&self, event: &TraceEvent);
}

/// No-op tracer that discards all events
pub struct NullTracer;

impl Tracer for NullTracer {
    fn trace(&self, _event: &TraceEvent) {}
}

/// Trace session - manages all active tracers and trace flags
pub struct TraceSession {
    /// All registered tracers
    tracers: Vec<Arc<dyn Tracer>>,
    /// Global trace flags
    flags: TraceFlags,
    /// Trace flags per process
    process_flags: HashMap<u32, TraceFlags>,
    /// Trace patterns (module, function, arity) -> enabled
    patterns: HashMap<(u32, u32, usize), bool>,
    /// Global match-all pattern enabled
    match_all: bool,
}

impl TraceSession {
    pub fn new() -> Self {
        TraceSession {
            tracers: Vec::new(),
            flags: TraceFlags::default(),
            process_flags: HashMap::new(),
            patterns: HashMap::new(),
            match_all: false,
        }
    }

    pub fn add_tracer(&mut self, tracer: Arc<dyn Tracer>) {
        self.tracers.push(tracer);
    }

    pub fn remove_tracer(&mut self, tracer: &Arc<dyn Tracer>) {
        self.tracers.retain(|t| Arc::ptr_eq(t, tracer));
    }

    pub fn set_flags(&mut self, flags: TraceFlags) {
        self.flags = flags;
    }

    pub fn get_flags(&self) -> TraceFlags {
        self.flags
    }

    pub fn set_process_flags(&mut self, pid: u32, flags: TraceFlags) {
        if flags.0 == 0 {
            self.process_flags.remove(&pid);
        } else {
            self.process_flags.insert(pid, flags);
        }
    }

    pub fn get_process_flags(&self, pid: u32) -> TraceFlags {
        self.process_flags.get(&pid).copied().unwrap_or(self.flags)
    }

    pub fn set_match_all(&mut self, enabled: bool) {
        self.match_all = enabled;
    }

    pub fn is_match_all(&self) -> bool {
        self.match_all
    }

    pub fn set_pattern(&mut self, module: u32, function: u32, arity: usize, enabled: bool) {
        if enabled {
            self.patterns.insert((module, function, arity), true);
        } else {
            self.patterns.remove(&(module, function, arity));
        }
    }

    pub fn is_pattern_enabled(&self, module: u32, function: u32, arity: usize) -> bool {
        if self.match_all {
            return true;
        }
        self.patterns
            .get(&(module, function, arity))
            .copied()
            .unwrap_or(false)
    }

    pub fn emit(&self, event: &TraceEvent) {
        for tracer in &self.tracers {
            tracer.trace(event);
        }
    }

    pub fn clear(&mut self) {
        self.tracers.clear();
        self.flags = TraceFlags::default();
        self.process_flags.clear();
        self.patterns.clear();
        self.match_all = false;
    }
}

impl Default for TraceSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Global trace state
static TRACE_SESSION: RwLock<Option<TraceSession>> = RwLock::new(None);

/// Initialize the global trace session
pub fn init() {
    let mut guard = TRACE_SESSION.write().unwrap();
    *guard = Some(TraceSession::new());
}

/// Get the global trace session
pub fn with_session<F, R>(f: F) -> R
where
    F: FnOnce(&TraceSession) -> R,
{
    let guard = TRACE_SESSION.read().unwrap();
    if let Some(ref session) = *guard {
        f(session)
    } else {
        panic!("trace session not initialized");
    }
}

/// Get mutable access to global trace session
pub fn with_session_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut TraceSession) -> R,
{
    let mut guard = TRACE_SESSION.write().unwrap();
    if let Some(ref mut session) = *guard {
        f(session)
    } else {
        panic!("trace session not initialized");
    }
}

/// Create a trace event with current timestamp
pub fn make_trace_event(
    pid: u32,
    serial: u32,
    flags: TraceFlags,
    module: Option<u32>,
    function: Option<u32>,
    arity: usize,
    extra: TraceExtra,
) -> TraceEvent {
    let timestamp_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    TraceEvent {
        timestamp_ns,
        flags,
        pid,
        serial,
        module,
        function,
        arity,
        extra,
    }
}

/// Emit a call trace event
pub fn emit_call(pid: u32, serial: u32, module: u32, function: u32, arity: usize) {
    with_session(|session| {
        let flags = session.get_process_flags(pid);
        if !flags.is_set(TraceFlags::CALL) && !session.is_pattern_enabled(module, function, arity) {
            return;
        }

        let event = make_trace_event(
            pid,
            serial,
            flags,
            Some(module),
            Some(function),
            arity,
            TraceExtra::None,
        );
        session.emit(&event);
    });
}

/// Emit a return trace event
pub fn emit_return(pid: u32, serial: u32, value: Term) {
    with_session(|session| {
        let flags = session.get_process_flags(pid);
        if !flags.is_set(TraceFlags::RETURN_TO) {
            return;
        }

        let event = make_trace_event(
            pid,
            serial,
            flags,
            None,
            None,
            0,
            TraceExtra::ReturnValue(value),
        );
        session.emit(&event);
    });
}

/// Emit a receive trace event
pub fn emit_receive(pid: u32, serial: u32, message: Term) {
    with_session(|session| {
        let flags = session.get_process_flags(pid);
        if !flags.is_set(TraceFlags::RECEIVE) {
            return;
        }

        let event = make_trace_event(
            pid,
            serial,
            flags,
            None,
            None,
            0,
            TraceExtra::Message(message),
        );
        session.emit(&event);
    });
}

/// Emit a receive timeout event
pub fn emit_receive_timeout(pid: u32, serial: u32) {
    with_session(|session| {
        let flags = session.get_process_flags(pid);
        if !flags.is_set(TraceFlags::RECEIVE) {
            return;
        }

        let event = make_trace_event(pid, serial, flags, None, None, 0, TraceExtra::Timeout);
        session.emit(&event);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_flags_default() {
        let flags = TraceFlags::default();
        assert!(!flags.is_set(TraceFlags::CALL));
        assert!(!flags.is_set(TraceFlags::RETURN_TO));
    }

    #[test]
    fn test_trace_flags_set_clear() {
        let mut flags = TraceFlags::default();
        flags.set(TraceFlags::CALL);
        assert!(flags.is_set(TraceFlags::CALL));

        flags.clear(TraceFlags::CALL);
        assert!(!flags.is_set(TraceFlags::CALL));
    }

    #[test]
    fn test_trace_session_new() {
        let session = TraceSession::new();
        assert!(!session.is_match_all());
        assert_eq!(session.get_flags().0, 0);
    }

    #[test]
    fn test_trace_session_patterns() {
        let mut session = TraceSession::new();
        session.set_pattern(1, 2, 3, true);
        assert!(session.is_pattern_enabled(1, 2, 3));
        assert!(!session.is_pattern_enabled(1, 2, 4));

        session.set_match_all(true);
        assert!(session.is_pattern_enabled(1, 2, 4));
    }

    #[test]
    fn test_trace_event_creation() {
        let event = make_trace_event(
            1,
            0,
            TraceFlags::CALL,
            Some(1),
            Some(2),
            3,
            TraceExtra::None,
        );
        assert_eq!(event.pid, 1);
        assert_eq!(event.module, Some(1));
        assert_eq!(event.function, Some(2));
        assert_eq!(event.arity, 3);
    }

    #[test]
    fn test_null_tracer() {
        let tracer = NullTracer;
        let event = make_trace_event(1, 0, TraceFlags::default(), None, None, 0, TraceExtra::None);
        tracer.trace(&event); // Should not panic
    }
}
