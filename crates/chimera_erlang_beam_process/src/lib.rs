//! Process control blocks, mailboxes, links, and monitors.
//!
//! Rust owns all process lifecycle and signal propagation semantics.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_core::{VmError, VmResult};
use chimera_erlang_beam_heap::{roots::RootSet, HeapConfig, ProcessHeap};
use chimera_erlang_beam_term::{atom::AtomTable, Term};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

// Re-export types from chimera_erlang_beam_instr for external use
pub use chimera_erlang_beam_instr::BifCall;
pub use chimera_erlang_beam_instr::ExceptionState;
pub use chimera_erlang_beam_instr::ReceiveState;

/// Global monitor reference ID counter
///
/// Each monitor gets a unique 64-bit ID across all processes.
static MONITOR_REF_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a unique monitor reference ID
fn next_monitor_id() -> u64 {
    MONITOR_REF_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Process ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pid {
    pub id: u32,
    pub serial: u32,
    pub creation: u32,
}

impl Pid {
    pub fn new(id: u32, serial: u32, creation: u32) -> Self {
        Pid {
            id,
            serial,
            creation,
        }
    }

    /// Check if this is a local PID (creation == 0)
    pub fn is_local(&self) -> bool {
        self.creation == 0
    }

    /// Check if this is a remote PID
    pub fn is_remote(&self) -> bool {
        self.creation != 0
    }

    /// Get the next serial number for this PID's ID
    /// Used when recreating a process with the same ID
    pub fn next_serial(&self) -> u32 {
        (self.serial + 1) & 0x1FFF // Mask to 13 bits
    }

    /// Create a new PID with incremented serial but same ID and creation
    pub fn with_incremented_serial(&self) -> Pid {
        Pid::new(self.id, self.next_serial(), self.creation)
    }

    /// Encode PID into raw u64 for storage/network transmission
    /// Format: creation(2) | serial(13) | id(14) | unused(35)
    pub fn to_raw(&self) -> u64 {
        let id = (self.id & 0x3FFF) as u64;
        let serial = (self.serial & 0x1FFF) as u64;
        let creation = (self.creation & 0x3) as u64;
        id | (serial << 14) | (creation << 27)
    }

    /// Decode PID from raw u64
    pub fn from_raw(raw: u64) -> Option<Self> {
        // Validate upper bits are zero
        if raw >> 29 != 0 {
            return None;
        }
        let id = (raw & 0x3FFF) as u32;
        let serial = ((raw >> 14) & 0x1FFF) as u32;
        let creation = ((raw >> 27) & 0x3) as u32;
        Some(Pid {
            id,
            serial,
            creation,
        })
    }
}

/// Process state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessState {
    /// Process is actively running on a scheduler
    Running = 0,
    /// Process is waiting for messages (or timeout)
    Waiting = 1,
    /// Process is exiting (cleanup in progress)
    Exiting = 2,
    /// Process is garbage collecting
    GarbageCollecting = 3,
    /// Process is suspended (e.g., debugging)
    Suspended = 4,
    /// Process has terminated and is ready for cleanup
    Dead = 5,
}

/// Process flags
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessFlags {
    pub trap_exit: bool,
    pub sensitive: bool,
    pub background_diagnostics: bool,
}

/// Reductions counter
pub type Reductions = u64;

/// Signal types that can be delivered to a process
///
/// BEAM uses a signal queue instead of just messages. Signals include:
/// - Regular messages
/// - Exit signals from linked processes
/// - Monitor DOWN notifications
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
    /// A regular message sent via send
    Message(Term),
    /// Exit signal from a linked process
    Exit { from: Pid, reason: Term },
    /// Monitor DOWN notification
    MonitorDown {
        ref_id: u64,
        target: Pid,
        reason: Term,
    },
}

impl Signal {
    /// Check if this is a regular message
    pub fn is_message(&self) -> bool {
        matches!(self, Signal::Message(_))
    }

    /// Check if this is an exit signal
    pub fn is_exit(&self) -> bool {
        matches!(self, Signal::Exit { .. })
    }

    /// Check if this is a monitor DOWN signal
    pub fn is_monitor_down(&self) -> bool {
        matches!(self, Signal::MonitorDown { .. })
    }

    /// Get the message term if this is a Message signal
    pub fn as_message(&self) -> Option<Term> {
        match self {
            Signal::Message(t) => Some(*t),
            _ => None,
        }
    }
}

/// Signal queue for process mailbox
///
/// BEAM delivers signals in FIFO order, with exit signals having
/// special handling based on trap_exit.
#[derive(Debug)]
pub struct SignalQueue {
    signals: Vec<Signal>,
}

impl SignalQueue {
    pub fn new() -> Self {
        SignalQueue {
            signals: Vec::new(),
        }
    }

    /// Push a signal to the back of the queue (FIFO ordering)
    pub fn push(&mut self, signal: Signal) {
        self.signals.push(signal);
    }

    /// Pop a signal from the front of the queue
    ///
    /// Returns None if the queue is empty.
    pub fn pop(&mut self) -> Option<Signal> {
        if self.signals.is_empty() {
            None
        } else {
            Some(self.signals.remove(0))
        }
    }

    /// Peek at the front signal without removing it
    pub fn peek(&self) -> Option<&Signal> {
        self.signals.first()
    }

    /// Get the number of signals in the queue
    pub fn len(&self) -> usize {
        self.signals.len()
    }

    /// Check if the queue has any messages (ignoring exit/monitor signals)
    pub fn has_message(&self) -> bool {
        self.signals.iter().any(|s| matches!(s, Signal::Message(_)))
    }

    /// Deliver the next message from the queue (used by receive ops)
    /// Returns the message term if available, None otherwise.
    pub fn deliver_message(&mut self) -> Option<Term> {
        loop {
            if self.signals.is_empty() {
                return None;
            }
            let signal = self.signals.remove(0);
            match signal {
                Signal::Message(t) => return Some(t),
                // Skip exit signals in regular receive
                Signal::Exit { .. } => continue,
                Signal::MonitorDown { .. } => continue,
            }
        }
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }

    /// Clear all signals from the queue
    pub fn clear(&mut self) {
        self.signals.clear();
    }

    /// Get all messages (non-signal messages) in the queue
    pub fn messages(&self) -> Vec<Term> {
        self.signals
            .iter()
            .filter_map(|s| match s {
                Signal::Message(t) => Some(*t),
                _ => None,
            })
            .collect()
    }

    /// Get all exit signals in the queue
    pub fn exits(&self) -> Vec<(Pid, Term)> {
        self.signals
            .iter()
            .filter_map(|s| match s {
                Signal::Exit { from, reason } => Some((*from, *reason)),
                _ => None,
            })
            .collect()
    }

    /// Peek at the next exit signal without removing it
    ///
    /// Returns the (from, reason) if the first non-message signal is an exit.
    /// Used for checking trap_exit conditions before receive.
    pub fn peek_exit(&self) -> Option<(Pid, Term)> {
        for signal in &self.signals {
            if let Signal::Exit { from, reason } = signal {
                return Some((*from, *reason));
            }
            // If we see a message first, there's no exit at front
            if matches!(signal, Signal::Message(_)) {
                return None;
            }
        }
        None
    }

    /// Get all monitor down signals in the queue
    pub fn monitor_downs(&self) -> Vec<(u64, Pid, Term)> {
        self.signals
            .iter()
            .filter_map(|s| match s {
                Signal::MonitorDown {
                    ref_id,
                    target,
                    reason,
                } => Some((*ref_id, *target, *reason)),
                _ => None,
            })
            .collect()
    }

    /// Iterate over all signals in the queue
    ///
    /// Used for root set filling during GC.
    pub fn iter(&self) -> impl Iterator<Item = &Signal> {
        self.signals.iter()
    }

    /// Get all signals (for testing)
    #[cfg(test)]
    pub fn get_signals(&self) -> &Vec<Signal> {
        &self.signals
    }

    /// Receive a message with trap_exit awareness
    ///
    /// When trap_exit is true, exit signals are delivered as message terms.
    /// When trap_exit is false, exit signals are skipped.
    pub fn receive_message_trap_aware(&mut self, trap_exit: bool) -> Option<Term> {
        loop {
            if self.signals.is_empty() {
                return None;
            }
            let signal = self.signals.remove(0);
            match signal {
                Signal::Message(t) => return Some(t),
                Signal::Exit { from: _, reason } => {
                    if trap_exit {
                        return Some(reason);
                    }
                    continue;
                }
                Signal::MonitorDown { .. } => continue,
            }
        }
    }

    /// Receive a signal with trap_exit consideration
    ///
    /// Returns Some(Signal) or None if empty.
    pub fn receive_signal(&mut self, _trap_exit: bool) -> Option<Signal> {
        loop {
            if self.signals.is_empty() {
                return None;
            }
            let signal = self.signals.remove(0);
            match signal {
                Signal::Message(_) => return Some(signal),
                Signal::Exit { .. } => return Some(signal),
                Signal::MonitorDown { .. } => continue,
            }
        }
    }

    /// Peek at the next message without removing
    pub fn peek_message(&self) -> Option<Term> {
        for signal in &self.signals {
            if let Signal::Message(t) = signal {
                return Some(*t);
            }
        }
        None
    }

    /// Get count of messages (excluding exit/monitor signals)
    pub fn message_count(&self) -> usize {
        self.signals
            .iter()
            .filter(|s| matches!(s, Signal::Message(_)))
            .count()
    }
}

impl Default for SignalQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Save queue for selective receive
///
/// When a process does a selective receive, messages that don't match
/// are saved here and restored after the receive completes.
#[derive(Debug, Default, PartialEq)]
pub struct SaveQueue {
    signals: Vec<Signal>,
}

impl SaveQueue {
    pub fn new() -> Self {
        SaveQueue {
            signals: Vec::new(),
        }
    }

    /// Save a signal to the save queue (typically on failed receive match)
    pub fn save(&mut self, signal: Signal) {
        self.signals.push(signal);
    }

    /// Restore all saved signals back to the main queue
    ///
    /// Returns the signals to restore so they can be re-enqueued.
    pub fn restore(&mut self) -> Vec<Signal> {
        std::mem::take(&mut self.signals)
    }

    pub fn len(&self) -> usize {
        self.signals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }

    /// Iterate over all signals in the save queue
    ///
    /// Used for root set filling during GC.
    pub fn iter(&self) -> impl Iterator<Item = &Signal> {
        self.signals.iter()
    }
}

/// Message queue for a process (backwards compatibility alias)
///
/// This is now implemented as a SignalQueue with helper methods.
pub type MessageQueue = SignalQueue;

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[derive(Default)]
pub enum Priority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Max = 3,
}

/// Monitor reference
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorRef {
    pub ref_id: u64,
    pub target: Pid,
    pub target_name: Option<u32>,
}

/// Maximum number of X registers (execution context)
pub const MAX_X_REGISTERS: usize = 64;

/// Maximum number of Y (stack) registers
pub const MAX_Y_REGISTERS: usize = 256;

/// Default reduction budget per step
pub const DEFAULT_REDUCTION_BUDGET: u64 = 1000;

/// Process control block
#[derive(Debug)]
pub struct ProcessControlBlock {
    pub pid: Pid,
    pub state: ProcessState,
    pub flags: ProcessFlags,
    pub priority: Priority,
    pub reductions: Reductions,

    /// Word-based process heap
    pub heap: ProcessHeap,

    /// Execution context
    pub exec_context: chimera_erlang_beam_instr::ExecContext,

    /// Execution context - instruction pointer
    pub ip: u64,
    /// Execution context - continuation pointer (return address)
    pub cp: u64,
    /// Execution context - bytecode code
    pub code: Vec<u64>,
    /// Execution context - X registers
    pub x: [Term; MAX_X_REGISTERS],
    /// Y registers (stack-backed)
    pub y: [Term; MAX_Y_REGISTERS],
    /// Frame pointer
    pub fp: u64,
    /// Number of live X registers
    pub live: u32,
    /// Reduction budget
    pub reduction_budget: u64,
    /// Current instruction word (for trap handling)
    pub current_instruction: u64,
    /// BIF call info extracted from trapped instruction
    pub bif_call: Option<BifCall>,
    /// Receive state for message matching
    pub receive_state: Option<ReceiveState>,
    /// Exception handling state
    pub exception_state: Option<ExceptionState>,

    /// Main signal queue (mailbox)
    pub mailbox: SignalQueue,
    /// Save queue for selective receive
    pub save_queue: SaveQueue,

    pub links: Vec<Pid>,
    pub monitors: Vec<MonitorRef>,

    pub group_leader: Pid,

    /// Process dictionary - key-value storage per process
    pub dictionary: HashMap<Term, Term>,

    pub exit_reason: Term,

    pub registered_name: Option<u32>,

    /// Initial call MFA when process was spawned
    pub initial_call: Option<(u32, u32, u32)>, // (module, function, arity)

    /// Trace flags for this process
    pub trace_flags: chimera_erlang_beam_trace::TraceFlags,
}

// =====================================================================
// Process Introspection Types
// =====================================================================

/// Process info entry key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ProcessInfoKey {
    Status = 0,
    MessageQueueLen = 1,
    Messages = 2,
    Links = 3,
    Monitors = 4,
    Dictionary = 5,
    GroupLeader = 6,
    Priority = 7,
    TrapExit = 8,
    RegisteredName = 9,
    HeapSize = 10,
    StackSize = 11,
    Reductions = 12,
    /// Total heap size in words (heap size + young heap fragments)
    TotalHeapSize = 13,
    /// Memory in bytes (heap + off-heap)
    Memory = 14,
    /// Garbage collection info {minor, major}
    GarbageCollection = 15,
    /// Initial call MFA when spawned {module, function, arity}
    InitialCall = 16,
}

/// Process info value
#[derive(Debug, Clone)]
pub enum ProcessInfoValue {
    Atom(u32),
    Int(i64),
    List(Vec<Term>),
    Pid(Pid),
}

impl ProcessControlBlock {
    pub fn new(pid: Pid, heap_size: usize) -> Self {
        let heap_config = HeapConfig {
            initial_size: heap_size,
            ..Default::default()
        };
        ProcessControlBlock {
            pid,
            state: ProcessState::Running,
            flags: ProcessFlags::default(),
            priority: Priority::default(),
            reductions: 0,
            heap: ProcessHeap::new(heap_config),
            exec_context: chimera_erlang_beam_instr::ExecContext::new(),
            ip: 0,
            cp: 0,
            code: Vec::new(),
            x: [Term::nil(); MAX_X_REGISTERS],
            y: [Term::nil(); MAX_Y_REGISTERS],
            fp: 0,
            live: 0,
            reduction_budget: DEFAULT_REDUCTION_BUDGET,
            current_instruction: 0,
            bif_call: None,
            receive_state: None,
            exception_state: None,
            mailbox: SignalQueue::new(),
            save_queue: SaveQueue::new(),
            links: Vec::new(),
            monitors: Vec::new(),
            group_leader: pid,
            dictionary: HashMap::new(),
            exit_reason: Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
            registered_name: None,
            initial_call: None,
            trace_flags: chimera_erlang_beam_trace::TraceFlags::default(),
        }
    }

    /// Load code into the process for execution
    pub fn load_code(&mut self, code: Vec<u64>) {
        self.code = code;
        self.ip = 0;
    }

    /// Get the current instruction word
    pub fn current_instruction(&self) -> Option<u64> {
        self.code.get(self.ip as usize).copied()
    }

    /// Reset the interpreter state for fresh execution
    pub fn reset_interpreter_state(&mut self) {
        self.ip = 0;
        self.cp = 0;
        self.fp = 0;
        self.live = 0;
        self.x = [Term::nil(); MAX_X_REGISTERS];
        self.y = [Term::nil(); MAX_Y_REGISTERS];
        self.current_instruction = 0;
        self.bif_call = None;
        self.receive_state = None;
        self.exception_state = None;
        self.reduction_budget = DEFAULT_REDUCTION_BUDGET;
    }

    /// Check if the process has a pending BIF call
    pub fn has_pending_bif(&self) -> bool {
        self.bif_call.is_some()
    }

    /// Check if the process is in a receive wait
    pub fn is_in_receive_wait(&self) -> bool {
        self.receive_state.is_some()
    }

    /// Check if the process has an active exception handler
    pub fn has_exception_handler(&self) -> bool {
        self.exception_state.is_some()
    }

    /// Set process state with transition validation
    ///
    /// Returns Ok(()) if transition is valid, Err(VmError) if invalid.
    pub fn set_state(&mut self, new_state: ProcessState) -> VmResult<()> {
        // Check if transition is valid
        let valid = match self.state {
            ProcessState::Running => matches!(
                new_state,
                ProcessState::Waiting
                    | ProcessState::Exiting
                    | ProcessState::GarbageCollecting
                    | ProcessState::Suspended
            ),
            ProcessState::Waiting => matches!(
                new_state,
                ProcessState::Running | ProcessState::Exiting | ProcessState::Suspended
            ),
            ProcessState::Exiting => matches!(new_state, ProcessState::Dead),
            ProcessState::GarbageCollecting => {
                matches!(new_state, ProcessState::Running | ProcessState::Waiting)
            }
            ProcessState::Suspended => {
                matches!(new_state, ProcessState::Running | ProcessState::Waiting)
            }
            ProcessState::Dead => false, // No transitions from Dead
        };

        if valid {
            self.state = new_state;
            Ok(())
        } else {
            Err(VmError::Generic(format!(
                "Invalid state transition: {:?} -> {:?}",
                self.state, new_state
            )))
        }
    }

    /// Set an X register value
    pub fn set_x(&mut self, reg: u32, value: Term) {
        if (reg as usize) < MAX_X_REGISTERS {
            self.x[reg as usize] = value;
        }
    }

    /// Get an X register value
    pub fn get_x(&self, reg: u32) -> Term {
        if (reg as usize) < MAX_X_REGISTERS {
            self.x[reg as usize]
        } else {
            Term::nil()
        }
    }

    /// Set a Y register value
    pub fn set_y(&mut self, slot: u32, value: Term) {
        if (slot as usize) < MAX_Y_REGISTERS {
            self.y[slot as usize] = value;
        }
    }

    /// Get a Y register value
    pub fn get_y(&self, slot: u32) -> Term {
        if (slot as usize) < MAX_Y_REGISTERS {
            self.y[slot as usize]
        } else {
            Term::nil()
        }
    }

    /// Send a regular message to this process
    pub fn send_message(&mut self, msg: Term) {
        self.mailbox.push(Signal::Message(msg));
    }

    /// Deliver an exit signal to this process
    pub fn send_exit(&mut self, from: Pid, reason: Term) {
        self.mailbox.push(Signal::Exit { from, reason });
    }

    /// Deliver a monitor DOWN signal
    pub fn send_monitor_down(&mut self, ref_id: u64, target: Pid, reason: Term) {
        self.mailbox.push(Signal::MonitorDown {
            ref_id,
            target,
            reason,
        });
    }

    /// Receive the next signal from the mailbox
    ///
    /// This handles the trap_exit semantics correctly:
    /// - Exit signals: delivered as reason term when trap_exit is true
    /// - Exit signals: skipped when trap_exit is false
    /// - MonitorDown signals: always skipped in regular receive
    ///
    /// Returns the message term or exit reason, or None if empty or only skipped signals.
    pub fn receive_message(&mut self) -> Option<Term> {
        self.mailbox.receive_message_trap_aware(false)
    }

    /// Receive the next signal with trap_exit consideration
    ///
    /// When trap_exit is true, exit signals are delivered as messages.
    /// When trap_exit is false, exit signals bypass receive and trigger cascading exit.
    ///
    /// Returns Some(Signal) or None if empty.
    pub fn receive_signal(&mut self, trap_exit: bool) -> Option<Signal> {
        self.mailbox.receive_signal(trap_exit)
    }

    /// Receive a message with proper trap_exit handling
    ///
    /// When trap_exit is true and an exit signal is received,
    /// returns a properly formatted {'EXIT', From, Reason} tuple.
    /// When trap_exit is false, exit signals are skipped.
    ///
    /// Returns the message term or None if empty.
    pub fn receive_message_with_trap_exit(&mut self, trap_exit: bool) -> Option<Term> {
        loop {
            if self.mailbox.is_empty() {
                return None;
            }
            let signal = self.mailbox.pop();
            match signal {
                Some(Signal::Message(t)) => return Some(t),
                Some(Signal::Exit { from, reason }) => {
                    if trap_exit {
                        // Format {'EXIT', From, Reason} tuple
                        let from_term = Term::from_small(from.to_raw() as i64);
                        let exit_atom = Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_EXIT);
                        // Create tuple: {'EXIT', From, Reason}
                        if let Some(addr) = self.heap.make_tuple(&[exit_atom, from_term, reason]) {
                            return Some(Term::from_tuple(addr as u64));
                        }
                        // Fallback: return reason only
                        return Some(reason);
                    }
                    // trap_exit is false: skip exit signal (cascading exit handled elsewhere)
                    continue;
                }
                Some(Signal::MonitorDown { .. }) => continue,
                None => return None,
            }
        }
    }

    /// Check if the next signal is an exit signal (for trap_exit handling)
    pub fn has_exit_pending(&self) -> bool {
        self.mailbox.peek_exit().is_some()
    }

    /// Peek at the next message without removing it
    pub fn peek_message(&self) -> Option<Term> {
        self.mailbox.peek_message()
    }

    /// Check if there's a next message (considering trap_exit semantics)
    pub fn has_next_message(&self) -> bool {
        self.mailbox.has_message()
    }

    /// Get count of messages (excluding exit/monitor signals)
    pub fn message_count(&self) -> usize {
        self.mailbox.message_count()
    }

    /// Get the next exit signal without removing it
    pub fn peek_exit(&self) -> Option<(Pid, Term)> {
        self.mailbox.peek_exit()
    }

    /// Receive a message, saving non-matching signals to save queue
    ///
    /// This is for selective receive. Signals that don't match the predicate
    /// are saved to the save queue and restored later.
    pub fn receive_matching<F>(&mut self, mut pred: F) -> Option<Term>
    where
        F: FnMut(&Term) -> bool,
    {
        while let Some(signal) = self.mailbox.pop() {
            match signal {
                Signal::Message(t) => {
                    if pred(&t) {
                        return Some(t);
                    } else {
                        // Save non-matching message
                        self.save_queue.save(signal);
                    }
                }
                Signal::Exit { .. } => {
                    // Exit signals bypass selective receive
                    return Some(Term::from_atom(
                        chimera_erlang_beam_term::atoms::ATOM_NORMAL,
                    ));
                }
                Signal::MonitorDown { .. } => {
                    self.save_queue.save(signal);
                }
            }
        }
        None
    }

    /// Restore saved signals back to the mailbox
    ///
    /// Called after selective receive completes or times out.
    pub fn restore_save_queue(&mut self) {
        for signal in self.save_queue.restore() {
            self.mailbox.push(signal);
        }
    }

    /// Queue a signal to the save queue (explicit)
    pub fn queue_signal(&mut self, signal: Signal) {
        self.save_queue.save(signal);
    }

    /// Queue a message to the save queue (explicit)
    pub fn queue_message(&mut self, msg: Term) {
        self.save_queue.save(Signal::Message(msg));
    }

    pub fn link(&mut self, other: Pid) {
        if !self.links.contains(&other) {
            self.links.push(other);
        }
    }

    pub fn unlink(&mut self, other: Pid) {
        self.links.retain(|p| *p != other);
    }

    pub fn has_link(&self, other: Pid) -> bool {
        self.links.contains(&other)
    }

    pub fn mailbox_len(&self) -> usize {
        self.mailbox.len()
    }

    /// Exit the process normally
    ///
    /// Sets the exit reason to normal and transitions to Exiting state.
    /// The process will be cleaned up by the VM/scheduler.
    pub fn exit(&mut self, reason: Term) {
        self.exit_reason = reason;
        let _ = self.set_state(ProcessState::Exiting); // Ignore error for now
    }

    /// Exit the process abnormally (kill)
    ///
    /// Like exit() but the reason is always 'killed' regardless of input.
    /// Used for external termination requests.
    pub fn kill(&mut self) {
        self.exit_reason = Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_KILL);
        let _ = self.set_state(ProcessState::Exiting); // Ignore error for now
    }

    /// Check if process can receive messages
    pub fn can_receive(&self) -> bool {
        self.state == ProcessState::Running || self.state == ProcessState::Waiting
    }

    /// Check if process is alive (can be scheduled)
    pub fn is_alive(&self) -> bool {
        self.state == ProcessState::Running
            || self.state == ProcessState::Waiting
            || self.state == ProcessState::Suspended
    }

    /// Check if process is currently running
    pub fn is_running(&self) -> bool {
        self.state == ProcessState::Running
    }

    /// Check if process is waiting for messages
    pub fn is_waiting(&self) -> bool {
        self.state == ProcessState::Waiting
    }

    /// Check if process is exiting
    pub fn is_exiting(&self) -> bool {
        self.state == ProcessState::Exiting
    }

    /// Check if process is suspended
    pub fn is_suspended(&self) -> bool {
        self.state == ProcessState::Suspended
    }

    /// Check if process has exited or is dead
    pub fn is_exited(&self) -> bool {
        self.state == ProcessState::Exiting || self.state == ProcessState::Dead
    }

    /// Check if process should trap exit signals
    pub fn traps_exit(&self) -> bool {
        self.flags.trap_exit
    }

    /// Mark process as dead after cleanup
    pub fn mark_dead(&mut self) {
        let _ = self.set_state(ProcessState::Dead); // Ignore error for now
    }

    /// Mark process as garbage collecting
    pub fn start_gc(&mut self) {
        let _ = self.set_state(ProcessState::GarbageCollecting); // Ignore error for now
    }

    /// Mark process as runnable after GC
    pub fn end_gc(&mut self) {
        let _ = self.set_state(ProcessState::Running); // Ignore error for now
    }

    /// Suspend the process
    pub fn suspend(&mut self) {
        let _ = self.set_state(ProcessState::Suspended); // Ignore error for now
    }

    /// Resume a suspended process
    pub fn resume(&mut self) {
        let _ = self.set_state(ProcessState::Running); // Ignore error for now
    }

    /// Add a monitor to this process
    ///
    /// Returns the monitor reference ID.
    pub fn add_monitor(&mut self, target: Pid, target_name: Option<u32>) -> u64 {
        let ref_id = next_monitor_id();
        let monitor = MonitorRef {
            ref_id,
            target,
            target_name,
        };
        self.monitors.push(monitor);
        ref_id
    }

    /// Remove a monitor by reference ID
    ///
    /// Returns true if the monitor was found and removed.
    pub fn remove_monitor(&mut self, ref_id: u64) -> bool {
        let initial_len = self.monitors.len();
        self.monitors.retain(|m| m.ref_id != ref_id);
        self.monitors.len() < initial_len
    }

    /// Remove all monitors for a given target
    ///
    /// Used when the target process exits.
    pub fn remove_monitors_for_target(&mut self, target: Pid) {
        self.monitors.retain(|m| m.target != target);
    }

    /// Check if this process has a monitor for a given target
    pub fn has_monitor_for(&self, target: Pid) -> bool {
        self.monitors.iter().any(|m| m.target == target)
    }

    /// Get all monitor references
    pub fn get_monitors(&self) -> &[MonitorRef] {
        &self.monitors
    }

    // =====================================================================
    // Process Dictionary
    // =====================================================================

    /// Put a value in the process dictionary
    pub fn put(&mut self, key: Term, value: Term) {
        self.dictionary.insert(key, value);
    }

    /// Get a value from the process dictionary
    pub fn get(&self, key: &Term) -> Option<Term> {
        self.dictionary.get(key).copied()
    }

    /// Remove a value from the process dictionary
    ///
    /// Returns the value if it existed.
    pub fn erase(&mut self, key: &Term) -> Option<Term> {
        self.dictionary.remove(key)
    }

    /// Check if a key exists in the process dictionary
    pub fn is_key(&self, key: &Term) -> bool {
        self.dictionary.contains_key(key)
    }

    /// Get the number of entries in the process dictionary
    pub fn dictionary_len(&self) -> usize {
        self.dictionary.len()
    }

    /// Get all keys in the process dictionary
    pub fn dictionary_keys(&self) -> Vec<Term> {
        self.dictionary.keys().copied().collect()
    }

    /// Get all entries in the process dictionary
    pub fn dictionary_to_vec(&self) -> Vec<(Term, Term)> {
        self.dictionary.iter().map(|(k, v)| (*k, *v)).collect()
    }

    /// Get process info as a list of {key, value} tuples
    ///
    /// Returns info for introspection BIFs like process_info/1, process_info/2.
    pub fn process_info(&self) -> Vec<(ProcessInfoKey, ProcessInfoValue)> {
        let mut info = Vec::new();

        // Status: atom describing process state
        let status_atom = match self.state {
            ProcessState::Running => chimera_erlang_beam_term::atoms::ATOM_RUNNING,
            ProcessState::Waiting => chimera_erlang_beam_term::atoms::ATOM_WAITING,
            ProcessState::Exiting => chimera_erlang_beam_term::atoms::ATOM_EXITING,
            ProcessState::GarbageCollecting => {
                chimera_erlang_beam_term::atoms::ATOM_GARBAGE_COLLECTING
            }
            ProcessState::Suspended => chimera_erlang_beam_term::atoms::ATOM_SUSPENDED,
            ProcessState::Dead => chimera_erlang_beam_term::atoms::ATOM_DEAD,
        };
        info.push((ProcessInfoKey::Status, ProcessInfoValue::Atom(status_atom)));

        // Message queue length
        info.push((
            ProcessInfoKey::MessageQueueLen,
            ProcessInfoValue::Int(self.mailbox.len() as i64),
        ));

        // Messages (list of terms)
        info.push((
            ProcessInfoKey::Messages,
            ProcessInfoValue::List(self.mailbox.messages()),
        ));

        // Links (list of PIDs)
        let link_pids: Vec<Term> = self
            .links
            .iter()
            .map(|p| Term::from_small(p.to_raw() as i64))
            .collect();
        info.push((ProcessInfoKey::Links, ProcessInfoValue::List(link_pids)));

        // Monitors (count only for now)
        info.push((
            ProcessInfoKey::Monitors,
            ProcessInfoValue::Int(self.monitors.len() as i64),
        ));

        // Dictionary (count only)
        info.push((
            ProcessInfoKey::Dictionary,
            ProcessInfoValue::Int(self.dictionary.len() as i64),
        ));

        // Group leader
        info.push((
            ProcessInfoKey::GroupLeader,
            ProcessInfoValue::Pid(self.group_leader),
        ));

        // Priority
        let priority_atom = match self.priority {
            Priority::Low => chimera_erlang_beam_term::atoms::ATOM_LOW,
            Priority::Normal => chimera_erlang_beam_term::atoms::ATOM_NORMAL,
            Priority::High => chimera_erlang_beam_term::atoms::ATOM_HIGH,
            Priority::Max => chimera_erlang_beam_term::atoms::ATOM_MAX,
        };
        info.push((
            ProcessInfoKey::Priority,
            ProcessInfoValue::Atom(priority_atom),
        ));

        // Trap exit
        info.push((
            ProcessInfoKey::TrapExit,
            ProcessInfoValue::Atom(if self.flags.trap_exit { 1 } else { 0 }),
        ));

        // Registered name
        if let Some(name) = self.registered_name {
            info.push((ProcessInfoKey::RegisteredName, ProcessInfoValue::Atom(name)));
        }

        // Heap size in words
        info.push((
            ProcessInfoKey::HeapSize,
            ProcessInfoValue::Int(self.heap.used_size() as i64),
        ));

        // Stack size (approximation - code length)
        info.push((
            ProcessInfoKey::StackSize,
            ProcessInfoValue::Int(self.code.len() as i64),
        ));

        // Reductions
        info.push((
            ProcessInfoKey::Reductions,
            ProcessInfoValue::Int(self.reductions as i64),
        ));

        info
    }

    // =====================================================================
    // Root Set Management (for GC)
    // =====================================================================

    /// Fill a root set with all roots from this process
    ///
    /// This collects all locations that must be traced during GC:
    /// - X registers
    /// - CP/IP registers
    /// - Mailbox signals
    /// - Save queue signals
    /// - Process dictionary entries
    /// - Links
    /// - Monitors
    /// - Group leader
    pub fn fill_roots(&self, roots: &mut RootSet) {
        use chimera_erlang_beam_heap::roots::{RootCategory, MAX_X_REGISTERS};

        // X registers (procedure arguments)
        for i in 0..MAX_X_REGISTERS {
            roots.add_x(i as u8, self.x[i]);
        }

        // Y registers (stack frames)
        for i in 0..MAX_Y_REGISTERS {
            roots.add_y(i as u8, self.y[i]);
        }

        // CP and IP (continuation and instruction pointers)
        // These are stored as raw u64, converted to terms for root tracing
        roots.add_cp(Term::from_small(self.cp as i64));
        roots.add_ip(Term::from_small(self.ip as i64));

        // Mailbox signals (messages)
        for signal in self.mailbox.iter() {
            match signal {
                Signal::Message(t) => {
                    roots.add(*t, RootCategory::MailboxSignal);
                }
                Signal::Exit { from, reason } => {
                    // Encode Pid as a term for root tracing
                    roots.add(
                        Term::from_small(from.to_raw() as i64),
                        RootCategory::MailboxSignal,
                    );
                    roots.add(*reason, RootCategory::MailboxSignal);
                }
                Signal::MonitorDown {
                    ref_id,
                    target,
                    reason,
                } => {
                    roots.add(
                        Term::from_small(*ref_id as i64),
                        RootCategory::MailboxSignal,
                    );
                    roots.add(
                        Term::from_small(target.to_raw() as i64),
                        RootCategory::MailboxSignal,
                    );
                    roots.add(*reason, RootCategory::MailboxSignal);
                }
            }
        }

        // Save queue signals
        for signal in self.save_queue.iter() {
            match signal {
                Signal::Message(t) => {
                    roots.add(*t, RootCategory::SaveQueue);
                }
                Signal::Exit { from, reason } => {
                    roots.add(
                        Term::from_small(from.to_raw() as i64),
                        RootCategory::SaveQueue,
                    );
                    roots.add(*reason, RootCategory::SaveQueue);
                }
                Signal::MonitorDown {
                    ref_id,
                    target,
                    reason,
                } => {
                    roots.add(Term::from_small(*ref_id as i64), RootCategory::SaveQueue);
                    roots.add(
                        Term::from_small(target.to_raw() as i64),
                        RootCategory::SaveQueue,
                    );
                    roots.add(*reason, RootCategory::SaveQueue);
                }
            }
        }

        // Process dictionary entries
        for (key, value) in &self.dictionary {
            roots.add(*key, RootCategory::DictionaryEntry);
            roots.add(*value, RootCategory::DictionaryEntry);
        }

        // Links
        for &pid in &self.links {
            roots.add(Term::from_small(pid.to_raw() as i64), RootCategory::Link);
        }

        // Monitors
        for mon in &self.monitors {
            roots.add(Term::from_small(mon.ref_id as i64), RootCategory::Monitor);
            roots.add(
                Term::from_small(mon.target.to_raw() as i64),
                RootCategory::Monitor,
            );
            if let Some(name) = mon.target_name {
                roots.add(Term::from_small(name as i64), RootCategory::Monitor);
            }
        }

        // Group leader
        roots.add(
            Term::from_small(self.group_leader.to_raw() as i64),
            RootCategory::GroupLeader,
        );

        // Registered name (if any)
        if let Some(name) = self.registered_name {
            roots.add(Term::from_small(name as i64), RootCategory::RegisteredName);
        }
    }

    /// Get the total number of roots in this process
    ///
    /// Used for debugging and metrics.
    pub fn root_count(&self) -> usize {
        use chimera_erlang_beam_heap::roots::MAX_X_REGISTERS;

        let mut count = 0;

        // X registers (always present, even if nil)
        count += MAX_X_REGISTERS;

        // CP and IP (2)
        count += 2;

        // Mailbox signals
        count += self.mailbox.len();

        // Save queue signals
        count += self.save_queue.len();

        // Dictionary entries (2 per entry: key + value)
        count += self.dictionary.len() * 2;

        // Links
        count += self.links.len();

        // Monitors (at least 2 per monitor: ref_id + target)
        count += self.monitors.len() * 2;

        // Group leader (1)
        count += 1;

        // Registered name (1 if present)
        if self.registered_name.is_some() {
            count += 1;
        }

        count
    }
}

/// Link processes together (unidirectional)
///
/// Note: For bidirectional links, use `bidirectional_link_with_table`.
pub fn link(pcb: &mut ProcessControlBlock, other: Pid) {
    pcb.link(other);
}

/// Unlink processes (removes link from both directions if bidirectional)
pub fn unlink(pcb: &mut ProcessControlBlock, other: Pid) {
    pcb.unlink(other);
}

/// Check if processes are linked
pub fn has_link(pcb: &ProcessControlBlock, other: Pid) -> bool {
    pcb.has_link(other)
}

/// Create a bidirectional link between two processes
///
/// Both processes will have each other in their links list.
/// When either exits, exit signals are sent to the other.
///
/// This version takes PIDs and the process table to avoid borrow issues.
pub fn bidirectional_link_with_table(
    table: &mut ProcessTable,
    pid_a: Pid,
    pid_b: Pid,
) -> VmResult<()> {
    // Get indices first
    let idx_a = table
        .get_index_by_pid(pid_a)
        .ok_or(VmError::ProcessNotFound)?;
    let idx_b = table
        .get_index_by_pid(pid_b)
        .ok_or(VmError::ProcessNotFound)?;

    // Access using indices with separate scopes to avoid double borrow
    {
        let pcb_a = table.get_by_index(idx_a).ok_or(VmError::ProcessNotFound)?;
        if !pcb_a.links.contains(&pid_b) {
            pcb_a.links.push(pid_b);
        }
    }
    {
        let pcb_b = table.get_by_index(idx_b).ok_or(VmError::ProcessNotFound)?;
        if !pcb_b.links.contains(&pid_a) {
            pcb_b.links.push(pid_a);
        }
    }
    Ok(())
}

/// Remove a bidirectional link between two processes
///
/// This version takes PIDs and the process table to avoid borrow issues.
pub fn bidirectional_unlink_with_table(
    table: &mut ProcessTable,
    pid_a: Pid,
    pid_b: Pid,
) -> VmResult<()> {
    let idx_a = table
        .get_index_by_pid(pid_a)
        .ok_or(VmError::ProcessNotFound)?;
    let idx_b = table
        .get_index_by_pid(pid_b)
        .ok_or(VmError::ProcessNotFound)?;

    {
        let pcb_a = table.get_by_index(idx_a).ok_or(VmError::ProcessNotFound)?;
        pcb_a.unlink(pid_b);
    }
    {
        let pcb_b = table.get_by_index(idx_b).ok_or(VmError::ProcessNotFound)?;
        pcb_b.unlink(pid_a);
    }
    Ok(())
}

/// Check if two processes are bidirectionally linked
pub fn are_linked(table: &mut ProcessTable, pid_a: Pid, pid_b: Pid) -> bool {
    let idx_a = match table.get_index_by_pid(pid_a) {
        Some(i) => i,
        None => return false,
    };
    let idx_b = match table.get_index_by_pid(pid_b) {
        Some(i) => i,
        None => return false,
    };

    // Use separate borrows
    let linked_a_to_b = {
        let pcb_a = table.get_by_index(idx_a).unwrap();
        pcb_a.has_link(pid_b)
    };
    let linked_b_to_a = {
        let pcb_b = table.get_by_index(idx_b).unwrap();
        pcb_b.has_link(pid_a)
    };
    linked_a_to_b && linked_b_to_a
}

/// Create a monitor from one process to another
///
/// The monitoring process (pid_a) will receive a 'DOWN' message
/// when the monitored process (pid_b) exits.
pub fn monitor_with_table(table: &mut ProcessTable, pid_a: Pid, pid_b: Pid) -> VmResult<u64> {
    let idx_a = table
        .get_index_by_pid(pid_a)
        .ok_or(VmError::ProcessNotFound)?;

    let pcb_a = table.get_by_index(idx_a).ok_or(VmError::ProcessNotFound)?;

    Ok(pcb_a.add_monitor(pid_b, None))
}

/// Create a named monitor
///
/// The monitoring process will receive a 'DOWN' message with
/// the registered name when the named process exits.
pub fn monitor_by_name_with_table(
    table: &mut ProcessTable,
    pid_a: Pid,
    target_name: u32,
) -> VmResult<u64> {
    let idx_a = table
        .get_index_by_pid(pid_a)
        .ok_or(VmError::ProcessNotFound)?;

    let pcb_a = table.get_by_index(idx_a).ok_or(VmError::ProcessNotFound)?;

    // Named monitors use None for target initially - resolution happens at exit
    Ok(pcb_a.add_monitor(Pid::new(0, 0, 0), Some(target_name)))
}

/// Demonitor - remove a monitor reference
///
/// Returns Ok(true) if the monitor existed, Ok(false) if it didn't.
pub fn demonitor_with_table(table: &mut ProcessTable, pid_a: Pid, ref_id: u64) -> VmResult<bool> {
    let idx_a = table
        .get_index_by_pid(pid_a)
        .ok_or(VmError::ProcessNotFound)?;

    let pcb_a = table.get_by_index(idx_a).ok_or(VmError::ProcessNotFound)?;

    Ok(pcb_a.remove_monitor(ref_id))
}

/// Propagate exit to monitors
///
/// When a process exits, send DOWN signals to all monitoring processes.
pub fn propagate_monitors(table: &mut ProcessTable, exited_pid: Pid, reason: Term) {
    // Collect all PIDs that monitor this process
    let monitoring_pids: Vec<Pid> = table
        .all_pids()
        .into_iter()
        .filter(|&pid| {
            if let Some((_, pcb)) = table.get_by_pid(pid) {
                pcb.has_monitor_for(exited_pid)
            } else {
                false
            }
        })
        .collect();

    // Send DOWN signals to each monitoring process
    for monitor_pid in monitoring_pids {
        if let Some((_, pcb)) = table.get_by_pid(monitor_pid) {
            // Find the monitor ref for this target
            if let Some(monitor) = pcb.monitors.iter().find(|m| m.target == exited_pid) {
                pcb.send_monitor_down(monitor.ref_id, exited_pid, reason);
            }
        }
    }
}

/// Propagate exit signal to linked processes
///
/// When a process exits, this function sends exit signals to all linked
/// processes based on their trap_exit flag.
///
/// If trap_exit is false, the linked process also exits (cascading exit).
/// If trap_exit is true, the exit is delivered as a message signal.
pub fn propagate_exit(table: &mut ProcessTable, from_pid: Pid, reason: Term) {
    // Look up the exiting process
    let Some((_, pcb)) = table.get_by_pid(from_pid) else {
        return;
    };

    // Collect linked PIDs first to avoid borrow issues
    let linked_pids: Vec<Pid> = pcb.links.clone();

    // Drop the borrow of pcb before we borrow table mutably
    let _ = pcb;

    for linked_pid in &linked_pids {
        // Look up the linked process
        if let Some((_, linked_pcb)) = table.get_by_pid(*linked_pid) {
            if linked_pcb.traps_exit() {
                // Deliver exit as a message signal
                linked_pcb.send_exit(from_pid, reason);
            } else {
                // Cascading exit - the linked process also exits
                linked_pcb.exit(reason);
            }
        }
    }
}

// ============================================================================
// Process Table (Task 38: Implement owned ProcessTable)
// ============================================================================

/// Process handle - stable reference to a process
///
/// Unlike raw pointers, ProcessHandle provides stable access to
/// a process through the ProcessTable lookup.
#[derive(Debug, Clone, Copy)]
pub struct ProcessHandle {
    index: usize,
    pid: Pid,
}

impl ProcessHandle {
    pub fn new(index: usize, pid: Pid) -> Self {
        ProcessHandle { index, pid }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn pid(&self) -> Pid {
        self.pid
    }
}

/// Process table - owns all process control blocks safely
///
/// The ProcessTable replaces raw pointer management with owned PCBs
/// indexed by stable handles. This eliminates Box::into_raw leaks
/// and provides proper cleanup on process exit.
pub struct ProcessTable {
    processes: Vec<Option<ProcessControlBlock>>,
    next_index: usize,
    pid_counter: AtomicU32,
    creation: u32,
    /// Global registry: registered name (atom ID) -> PID
    registry: HashMap<u32, Pid>,
    /// Atom table for interning atom names
    atom_table: AtomTable,
    /// Connected node names (for erlang:nodes/0)
    connected_nodes: Vec<String>,
    /// PID reuse queue: PIDs that can be reused with incremented serial
    reuse_queue: Vec<Pid>,
}

impl Debug for ProcessTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessTable")
            .field("next_index", &self.next_index)
            .field("creation", &self.creation)
            .field("registry", &self.registry)
            .field("connected_nodes", &self.connected_nodes)
            .finish()
    }
}

impl ProcessTable {
    pub fn new(creation: u32) -> Self {
        ProcessTable {
            processes: Vec::with_capacity(1024),
            next_index: 0,
            pid_counter: AtomicU32::new(1),
            creation,
            registry: HashMap::new(),
            atom_table: AtomTable::new(),
            connected_nodes: Vec::new(),
            reuse_queue: Vec::new(),
        }
    }

    /// Intern an atom name and return its index.
    /// If the atom already exists, returns the existing index.
    pub fn intern_atom(&mut self, name: &str) -> u32 {
        self.atom_table.intern(name).unwrap_or(u32::MAX)
    }

    /// Get the list of connected node names.
    pub fn get_connected_nodes(&self) -> &[String] {
        &self.connected_nodes
    }

    /// Add a connected node.
    pub fn add_connected_node(&mut self, node_name: String) {
        if !self.connected_nodes.contains(&node_name) {
            self.connected_nodes.push(node_name);
        }
    }

    /// Remove a connected node.
    pub fn remove_connected_node(&mut self, node_name: &str) {
        self.connected_nodes.retain(|n| n != node_name);
    }

    /// Allocate a new PID (reuses dead PIDs with incremented serial)
    fn next_pid(&mut self) -> Pid {
        // Check reuse queue first
        if let Some(old_pid) = self.reuse_queue.pop() {
            // Reuse with incremented serial
            return old_pid.with_incremented_serial();
        }

        // No reusable PIDs, create new one
        let id = self.pid_counter.fetch_add(1, Ordering::SeqCst);
        Pid::new(id, 0, self.creation)
    }

    /// Spawn a new process and return a handle
    ///
    /// Returns the process handle and PID on success.
    pub fn spawn(&mut self, heap_size: usize) -> (ProcessHandle, Pid) {
        let pid = self.next_pid();
        let index = self.next_index;

        // Expand vector if needed
        if index >= self.processes.len() {
            self.processes.push(None);
        }

        let pcb = ProcessControlBlock::new(pid, heap_size);
        self.processes[index] = Some(pcb);
        self.next_index += 1;

        (ProcessHandle::new(index, pid), pid)
    }

    /// Look up a process by PID
    ///
    /// Returns the PCB index and mutable reference, or None if not found.
    pub fn get_by_pid(&mut self, pid: Pid) -> Option<(usize, &mut ProcessControlBlock)> {
        for (i, opt) in self.processes.iter_mut().enumerate() {
            if let Some(ref pcb) = opt {
                if pcb.pid == pid {
                    return Some((i, opt.as_mut().unwrap()));
                }
            }
        }
        None
    }

    /// Look up a process by handle
    pub fn get_by_handle(&mut self, handle: ProcessHandle) -> Option<&mut ProcessControlBlock> {
        self.processes
            .get_mut(handle.index())
            .and_then(|opt| opt.as_mut())
    }

    /// Look up a process index by PID
    ///
    /// Returns the index if found, or None if not found.
    pub fn get_index_by_pid(&self, pid: Pid) -> Option<usize> {
        for (i, opt) in self.processes.iter().enumerate() {
            if let Some(ref pcb) = opt {
                if pcb.pid == pid {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Look up a process by index
    ///
    /// Returns the mutable reference if found, or None if not found.
    pub fn get_by_index(&mut self, index: usize) -> Option<&mut ProcessControlBlock> {
        self.processes.get_mut(index).and_then(|opt| opt.as_mut())
    }

    /// Mark a process as exited and optionally clean up
    pub fn mark_exited(&mut self, pid: Pid, exit_reason: Term) -> VmResult<()> {
        if let Some((_idx, pcb)) = self.get_by_pid(pid) {
            pcb.exit_reason = exit_reason;
            let _ = pcb.set_state(ProcessState::Exiting); // Ignore error for now
            Ok(())
        } else {
            Err(VmError::ProcessNotFound)
        }
    }

    /// Terminate a process completely (after exit handling)
    ///
    /// This adds the PID to the reuse queue with its current serial.
    /// The slot is cleared for reuse.
    /// Should be called after exit signals have been propagated.
    pub fn terminate(&mut self, pid: Pid) -> VmResult<()> {
        if let Some((idx, pcb)) = self.get_by_pid(pid) {
            pcb.mark_dead();
            // Add PID to reuse queue for later reuse with incremented serial
            self.reuse_queue.push(pid);
            // Clear the slot for reuse
            self.processes[idx] = None;
            Ok(())
        } else {
            Err(VmError::ProcessNotFound)
        }
    }

    /// Get exit reason for a process
    pub fn get_exit_reason(&self, pid: Pid) -> Option<Term> {
        self.processes
            .iter()
            .flatten()
            .find(|pcb| pcb.pid == pid)
            .map(|pcb| pcb.exit_reason)
    }

    /// Check if a PID is still alive (exists and not exited)
    pub fn is_alive(&self, pid: Pid) -> bool {
        self.processes.iter().any(|opt| {
            opt.as_ref()
                .is_some_and(|pcb| pcb.pid == pid && pcb.is_alive())
        })
    }

    /// Get the number of active processes
    pub fn len(&self) -> usize {
        self.processes.iter().filter(|opt| opt.is_some()).count()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all active PIDs in the table
    pub fn all_pids(&self) -> Vec<Pid> {
        self.processes
            .iter()
            .filter_map(|opt| opt.as_ref().map(|pcb| pcb.pid))
            .collect()
    }

    // =====================================================================
    // Process Registry
    // =====================================================================

    /// Register a process with a global name
    ///
    /// Returns Ok(()) if successful, Err if name already registered.
    pub fn register(&mut self, name: u32, pid: Pid) -> VmResult<()> {
        // Check if name is already taken
        if let Some(existing) = self.registry.get(&name) {
            if *existing != pid {
                return Err(VmError::ProcessNotFound); // Name already taken
            }
        }

        // Set the registered name on the process
        if let Some((_, pcb)) = self.get_by_pid(pid) {
            pcb.registered_name = Some(name);
        } else {
            return Err(VmError::ProcessNotFound);
        }

        self.registry.insert(name, pid);
        Ok(())
    }

    /// Unregister a process name
    ///
    /// Returns true if the name was registered, false otherwise.
    pub fn unregister(&mut self, name: u32) -> bool {
        // Find the PID for this name
        if let Some(pid) = self.registry.remove(&name) {
            // Clear the registered name on the process
            if let Some((_, pcb)) = self.get_by_pid(pid) {
                pcb.registered_name = None;
            }
            true
        } else {
            false
        }
    }

    /// Look up a registered name
    ///
    /// Returns the PID if found, None otherwise.
    pub fn whereis(&self, name: u32) -> Option<Pid> {
        self.registry.get(&name).copied()
    }

    /// Get all registered names
    pub fn registered_names(&self) -> Vec<u32> {
        self.registry.keys().copied().collect()
    }

    /// Clean up exited processes and compact the table
    pub fn cleanup(&mut self) {
        // Remove None slots and compact
        self.processes.retain(|opt| opt.is_some());
        self.next_index = self.processes.len();
    }

    /// Send a message to a local process by PID (local send path)
    ///
    /// Returns Ok(()) if the message was delivered, Err if the process
    /// was not found or is not alive.
    pub fn send(&mut self, to: Pid, msg: Term) -> VmResult<()> {
        if let Some((_, pcb)) = self.get_by_pid(to) {
            if pcb.is_alive() {
                pcb.send_message(msg);
                Ok(())
            } else {
                Err(VmError::ProcessNotFound)
            }
        } else {
            Err(VmError::ProcessNotFound)
        }
    }

    /// Send a message to a registered process by name
    ///
    /// Returns Ok(()) if the message was delivered, Err if the name
    /// was not registered or the process is not alive.
    pub fn send_by_name(&mut self, name: u32, msg: Term) -> VmResult<()> {
        if let Some(pid) = self.whereis(name) {
            self.send(pid, msg)
        } else {
            Err(VmError::ProcessNotFound)
        }
    }
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_create() {
        let pid = Pid::new(1, 0, 0);
        let pcb = ProcessControlBlock::new(pid, 8192);
        assert_eq!(pcb.pid, pid);
        assert_eq!(pcb.state, ProcessState::Running);
    }

    #[test]
    fn test_mailbox_send_receive() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        pcb.send_message(Term::from_small(1));
        pcb.send_message(Term::from_small(2));

        assert_eq!(pcb.receive_message(), Some(Term::from_small(1)));
        assert_eq!(pcb.receive_message(), Some(Term::from_small(2)));
        assert_eq!(pcb.receive_message(), None);
    }

    #[test]
    fn test_link_unlink() {
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid1, 8192);

        link(&mut pcb, pid2);
        assert!(has_link(&pcb, pid2));

        unlink(&mut pcb, pid2);
        assert!(!has_link(&pcb, pid2));
    }

    #[test]
    fn test_process_table_spawn() {
        let mut table = ProcessTable::new(0);
        let (handle, pid) = table.spawn(8192);

        assert!(pid.id >= 1);
        assert_eq!(handle.index(), 0);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_process_table_get_by_pid() {
        let mut table = ProcessTable::new(0);
        let (_, pid) = table.spawn(8192);

        let result = table.get_by_pid(pid);
        assert!(result.is_some());

        let (_, pcb) = result.unwrap();
        assert_eq!(pcb.pid, pid);
    }

    #[test]
    fn test_process_table_mark_exited() {
        let mut table = ProcessTable::new(0);
        let (_, pid) = table.spawn(8192);

        assert!(table.is_alive(pid));

        table.mark_exited(pid, Term::from_small(0)).unwrap();
        assert!(!table.is_alive(pid));
    }

    #[test]
    fn test_process_table_multiple_spawns() {
        let mut table = ProcessTable::new(0);

        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);
        let (_, pid3) = table.spawn(8192);

        assert_ne!(pid1, pid2);
        assert_ne!(pid2, pid3);
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn test_process_handle() {
        let pid = Pid::new(42, 0, 0);
        let handle = ProcessHandle::new(5, pid);

        assert_eq!(handle.index(), 5);
        assert_eq!(handle.pid(), pid);
    }

    #[test]
    fn test_process_exit_normal() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        assert_eq!(pcb.state, ProcessState::Running);
        assert!(pcb.is_alive());

        pcb.exit(Term::from_atom(
            chimera_erlang_beam_term::atoms::ATOM_NORMAL,
        ));

        assert_eq!(pcb.state, ProcessState::Exiting);
        assert!(!pcb.is_alive());
    }

    #[test]
    fn test_process_kill() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        pcb.kill();

        assert_eq!(pcb.state, ProcessState::Exiting);
        // kill should set reason to 'killed'
    }

    #[test]
    fn test_process_suspend_resume() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        assert_eq!(pcb.state, ProcessState::Running);

        pcb.suspend();
        assert_eq!(pcb.state, ProcessState::Suspended);
        assert!(pcb.is_alive());

        pcb.resume();
        assert_eq!(pcb.state, ProcessState::Running);
    }

    #[test]
    fn test_process_gc_start_end() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        assert_eq!(pcb.state, ProcessState::Running);

        pcb.start_gc();
        assert_eq!(pcb.state, ProcessState::GarbageCollecting);

        pcb.end_gc();
        assert_eq!(pcb.state, ProcessState::Running);
    }

    #[test]
    fn test_process_trap_exit() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        assert!(!pcb.traps_exit());

        pcb.flags.trap_exit = true;
        assert!(pcb.traps_exit());
    }

    #[test]
    fn test_process_table_terminate() {
        let mut table = ProcessTable::new(0);
        let (_, pid) = table.spawn(8192);

        assert!(table.is_alive(pid));

        // mark_exited sets state to Exiting
        table
            .mark_exited(
                pid,
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
            )
            .unwrap();

        // terminate sets state to Dead and clears slot
        table.terminate(pid).unwrap();

        // Process is no longer in table
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_process_can_receive() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        // Running can receive
        assert!(pcb.can_receive());

        // Waiting can receive
        let _ = pcb.set_state(ProcessState::Waiting);
        assert!(pcb.can_receive());

        // Exiting cannot receive
        let _ = pcb.set_state(ProcessState::Exiting);
        assert!(!pcb.can_receive());

        // Dead cannot receive
        let _ = pcb.set_state(ProcessState::Dead);
        assert!(!pcb.can_receive());
    }

    #[test]
    fn test_bidirectional_link() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Initially not linked
        assert!(!are_linked(&mut table, pid1, pid2));

        // Create bidirectional link
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();

        // Now both should be linked to each other
        assert!(are_linked(&mut table, pid1, pid2));
    }

    #[test]
    fn test_bidirectional_unlink() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create bidirectional link
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();
        assert!(are_linked(&mut table, pid1, pid2));

        // Remove bidirectional link
        bidirectional_unlink_with_table(&mut table, pid1, pid2).unwrap();

        // Neither should be linked
        assert!(!are_linked(&mut table, pid1, pid2));
    }

    #[test]
    fn test_propagate_exit_trap_exit() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create bidirectional link
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();

        // Set trap_exit on process 2
        {
            let (_, pcb2) = table.get_by_pid(pid2).unwrap();
            pcb2.flags.trap_exit = true;
        }

        // Exit process 1 and propagate
        {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            pcb1.exit(Term::from_atom(
                chimera_erlang_beam_term::atoms::ATOM_NORMAL,
            ));
        }
        propagate_exit(
            &mut table,
            pid1,
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        );

        // Check results - process 1
        let state1 = {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            pcb1.state
        };
        // Check results - process 2
        let (state2, exit_signals) = {
            let (_, pcb2) = table.get_by_pid(pid2).unwrap();
            (pcb2.state, pcb2.mailbox.exits())
        };

        // Process 1 should be exiting
        assert_eq!(state1, ProcessState::Exiting);

        // Process 2 should have received the exit as a signal (trap_exit=true)
        // and should still be running
        assert_eq!(state2, ProcessState::Running);
        // Check that an exit signal was sent to process 2's mailbox
        assert!(!exit_signals.is_empty());
    }

    #[test]
    fn test_propagate_exit_cascading() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create bidirectional link
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();
        // Do NOT set trap_exit on process 2 (default is false)

        // Exit process 1 and propagate
        {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            pcb1.exit(Term::from_atom(
                chimera_erlang_beam_term::atoms::ATOM_NORMAL,
            ));
        }
        propagate_exit(
            &mut table,
            pid1,
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        );

        // Check results - process 1
        let state1 = {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            pcb1.state
        };
        // Check results - process 2
        let state2 = {
            let (_, pcb2) = table.get_by_pid(pid2).unwrap();
            pcb2.state
        };

        // Process 1 should be exiting
        assert_eq!(state1, ProcessState::Exiting);

        // Process 2 should also exit (cascading) since trap_exit=false
        assert_eq!(state2, ProcessState::Exiting);
    }

    #[test]
    fn test_monitor_and_demonitor() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create a monitor
        let ref_id = monitor_with_table(&mut table, pid1, pid2).unwrap();
        assert!(ref_id > 0);

        // Process 1 should have a monitor for pid2
        {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            assert!(pcb1.has_monitor_for(pid2));
        }

        // Demonitor
        let removed = demonitor_with_table(&mut table, pid1, ref_id).unwrap();
        assert!(removed);

        // Process 1 should no longer have a monitor for pid2
        {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            assert!(!pcb1.has_monitor_for(pid2));
        }
    }

    #[test]
    fn test_propagate_monitors() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192); // monitor
        let (_, pid2) = table.spawn(8192); // target

        // pid1 monitors pid2
        let ref_id = monitor_with_table(&mut table, pid1, pid2).unwrap();

        // Exit pid2 and propagate
        {
            let (_, pcb2) = table.get_by_pid(pid2).unwrap();
            pcb2.exit(Term::from_atom(
                chimera_erlang_beam_term::atoms::ATOM_NORMAL,
            ));
        }
        propagate_monitors(
            &mut table,
            pid2,
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        );

        // pid1 should have received a DOWN signal
        let monitor_downs = {
            let (_, pcb1) = table.get_by_pid(pid1).unwrap();
            pcb1.mailbox.monitor_downs()
        };

        // Should have received a DOWN signal for the monitor ref
        assert!(!monitor_downs.is_empty());

        // Verify the monitor down has the correct ref_id and target
        let down = &monitor_downs[0];
        assert_eq!(down.0, ref_id); // ref_id
        assert_eq!(down.1, pid2); // target
    }

    #[test]
    fn test_process_dictionary() {
        let pid = Pid::new(1, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid, 8192);

        // Initially empty
        assert_eq!(pcb.dictionary_len(), 0);

        // Put a value
        pcb.put(Term::from_small(1), Term::from_small(100));
        assert_eq!(pcb.dictionary_len(), 1);

        // Get the value
        assert_eq!(pcb.get(&Term::from_small(1)), Some(Term::from_small(100)));

        // Overwrite
        pcb.put(Term::from_small(1), Term::from_small(200));
        assert_eq!(pcb.get(&Term::from_small(1)), Some(Term::from_small(200)));
        assert_eq!(pcb.dictionary_len(), 1);

        // Erase
        assert_eq!(pcb.erase(&Term::from_small(1)), Some(Term::from_small(200)));
        assert_eq!(pcb.dictionary_len(), 0);
    }

    #[test]
    fn test_process_registry() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Initially no registered names
        assert!(table.registered_names().is_empty());

        // Register pid1 as 'foo'
        table.register(1, pid1).unwrap();
        assert_eq!(table.whereis(1), Some(pid1));

        // Register pid2 as 'bar'
        table.register(2, pid2).unwrap();
        assert_eq!(table.whereis(1), Some(pid1));
        assert_eq!(table.whereis(2), Some(pid2));

        // Unregister 'foo'
        assert!(table.unregister(1));
        assert_eq!(table.whereis(1), None);

        // whereis for unknown name
        assert_eq!(table.whereis(999), None);
    }

    #[test]
    fn test_signal_queue_push_pop() {
        let mut queue = SignalQueue::new();
        assert!(queue.is_empty());

        queue.push(Signal::Message(Term::from_small(1)));
        queue.push(Signal::Message(Term::from_small(2)));

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop(), Some(Signal::Message(Term::from_small(1))));
        assert_eq!(queue.pop(), Some(Signal::Message(Term::from_small(2))));
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_signal_queue_messages() {
        let mut queue = SignalQueue::new();

        queue.push(Signal::Message(Term::from_small(1)));
        queue.push(Signal::Exit {
            from: Pid::new(1, 0, 0),
            reason: Term::from_atom(0),
        });
        queue.push(Signal::Message(Term::from_small(2)));

        let msgs = queue.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0], Term::from_small(1));
        assert_eq!(msgs[1], Term::from_small(2));
    }

    #[test]
    fn test_signal_queue_exits() {
        let mut queue = SignalQueue::new();
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);

        queue.push(Signal::Message(Term::from_small(1)));
        queue.push(Signal::Exit {
            from: pid1,
            reason: Term::from_atom(0),
        });
        queue.push(Signal::Exit {
            from: pid2,
            reason: Term::from_atom(5),
        });

        let exits = queue.exits();
        assert_eq!(exits.len(), 2);
        assert_eq!(exits[0].0, pid1);
        assert_eq!(exits[1].0, pid2);
    }

    #[test]
    fn test_signal_queue_monitor_downs() {
        let mut queue = SignalQueue::new();
        let pid1 = Pid::new(1, 0, 0);

        queue.push(Signal::MonitorDown {
            ref_id: 100,
            target: pid1,
            reason: Term::from_atom(0),
        });

        let downs = queue.monitor_downs();
        assert_eq!(downs.len(), 1);
        assert_eq!(downs[0].0, 100);
        assert_eq!(downs[0].1, pid1);
    }

    #[test]
    fn test_signal_queue_clear() {
        let mut queue = SignalQueue::new();
        queue.push(Signal::Message(Term::from_small(1)));
        queue.push(Signal::Message(Term::from_small(2)));
        assert_eq!(queue.len(), 2);

        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_signal_queue_peek() {
        let mut queue = SignalQueue::new();
        assert!(queue.peek().is_none());

        queue.push(Signal::Message(Term::from_small(42)));
        assert_eq!(queue.peek(), Some(&Signal::Message(Term::from_small(42))));

        queue.pop();
        assert!(queue.peek().is_none());
    }

    #[test]
    fn test_pid_is_local_remote() {
        let local = Pid::new(1, 5, 0);
        let remote = Pid::new(1, 5, 1);

        assert!(local.is_local());
        assert!(!local.is_remote());
        assert!(!remote.is_local());
        assert!(remote.is_remote());
    }

    #[test]
    fn test_pid_next_serial() {
        let pid = Pid::new(1, 0, 0);
        assert_eq!(pid.next_serial(), 1);

        let pid_max = Pid::new(1, 0x1FFF, 0);
        assert_eq!(pid_max.next_serial(), 0); // Wraps around
    }

    #[test]
    fn test_pid_with_incremented_serial() {
        let pid = Pid::new(1, 5, 2);
        let next = pid.with_incremented_serial();

        assert_eq!(next.id, 1);
        assert_eq!(next.serial, 6);
        assert_eq!(next.creation, 2);
    }

    #[test]
    fn test_pid_to_raw_from_raw() {
        let pid = Pid::new(0x1234, 0x1ABC, 3);
        let raw = pid.to_raw();

        assert_eq!(raw >> 29, 0); // Upper bits clear

        let decoded = Pid::from_raw(raw).unwrap();
        assert_eq!(decoded.id, 0x1234);
        assert_eq!(decoded.serial, 0x1ABC);
        assert_eq!(decoded.creation, 3);
    }

    #[test]
    fn test_pid_from_raw_invalid() {
        // Upper bits set - should be None
        assert!(Pid::from_raw(0x20000000).is_none());
        assert!(Pid::from_raw(0xFFFFFFFFFFFFFFFF).is_none());
    }

    #[test]
    fn test_save_queue_restore() {
        let mut save_q = SaveQueue::new();
        save_q.save(Signal::Message(Term::from_small(1)));
        save_q.save(Signal::Message(Term::from_small(2)));
        assert_eq!(save_q.len(), 2);

        let restored = save_q.restore();
        assert!(save_q.is_empty());
        assert_eq!(restored.len(), 2);
    }

    #[test]
    fn test_priority_default() {
        assert_eq!(Priority::default(), Priority::Normal);
    }

    #[test]
    fn test_signal_is_message() {
        assert!(Signal::Message(Term::from_small(1)).is_message());
        assert!(!Signal::Exit {
            from: Pid::new(1, 0, 0),
            reason: Term::nil()
        }
        .is_message());
        assert!(!Signal::MonitorDown {
            ref_id: 0,
            target: Pid::new(1, 0, 0),
            reason: Term::nil()
        }
        .is_message());
    }

    #[test]
    fn test_signal_is_exit() {
        assert!(Signal::Exit {
            from: Pid::new(1, 0, 0),
            reason: Term::nil()
        }
        .is_exit());
        assert!(!Signal::Message(Term::from_small(1)).is_exit());
    }

    #[test]
    fn test_signal_is_monitor_down() {
        assert!(Signal::MonitorDown {
            ref_id: 0,
            target: Pid::new(1, 0, 0),
            reason: Term::nil()
        }
        .is_monitor_down());
        assert!(!Signal::Message(Term::from_small(1)).is_monitor_down());
    }

    #[test]
    fn test_signal_as_message() {
        let msg = Signal::Message(Term::from_small(42));
        assert_eq!(msg.as_message(), Some(Term::from_small(42)));

        let exit = Signal::Exit {
            from: Pid::new(1, 0, 0),
            reason: Term::nil(),
        };
        assert_eq!(exit.as_message(), None);
    }

    #[test]
    fn test_monitor_ref_creation() {
        let mon = MonitorRef {
            ref_id: 12345,
            target: Pid::new(7, 3, 1),
            target_name: Some(99),
        };

        assert_eq!(mon.ref_id, 12345);
        assert_eq!(mon.target.id, 7);
        assert_eq!(mon.target_name, Some(99));
    }

    #[test]
    fn test_link_idempotent() {
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid1, 8192);

        // Link the same PID twice
        link(&mut pcb, pid2);
        link(&mut pcb, pid2);

        // Should only have one link
        assert!(has_link(&pcb, pid2));
        let link_count = pcb.links.iter().filter(|p| **p == pid2).count();
        assert_eq!(link_count, 1);
    }

    #[test]
    fn test_unlink_nonexistent() {
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let mut pcb = ProcessControlBlock::new(pid1, 8192);

        // Unlink a PID that was never linked - should not panic
        unlink(&mut pcb, pid2);
        assert!(!has_link(&pcb, pid2));
    }

    #[test]
    fn test_bidirectional_link_symmetric() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create bidirectional link
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();

        // Both processes should have links to each other
        assert!(are_linked(&mut table, pid1, pid2));
        assert!(are_linked(&mut table, pid2, pid1));
    }

    #[test]
    fn test_bidirectional_unlink_both_sides() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create bidirectional link
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();
        assert!(are_linked(&mut table, pid1, pid2));

        // Remove link from pid1's side
        bidirectional_unlink_with_table(&mut table, pid1, pid2).unwrap();

        // Should no longer be linked
        assert!(!are_linked(&mut table, pid1, pid2));
    }

    #[test]
    fn test_unlink_idempotent() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Create and then unlink
        bidirectional_link_with_table(&mut table, pid1, pid2).unwrap();
        bidirectional_unlink_with_table(&mut table, pid1, pid2).unwrap();

        // Unlink again - should not panic (idempotent)
        bidirectional_unlink_with_table(&mut table, pid1, pid2).unwrap();
        assert!(!are_linked(&mut table, pid1, pid2));
    }

    #[test]
    fn test_link_to_dead_process() {
        let mut table = ProcessTable::new(0);
        let (_, pid1) = table.spawn(8192);
        let (_, pid2) = table.spawn(8192);

        // Kill pid2
        table
            .mark_exited(
                pid2,
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
            )
            .unwrap();

        // Link pid1 to dead pid2 - should still work but pid2 is not alive
        {
            let (_, pcb) = table.get_by_pid(pid1).unwrap();
            link(pcb, pid2);
        }
        // Verify link was added
        let (_, pcb) = table.get_by_pid(pid1).unwrap();
        assert!(has_link(&pcb, pid2));
    }

    #[test]
    fn test_pid_reuse() {
        let mut table = ProcessTable::new(0);

        // Spawn a process
        let (_, pid1) = table.spawn(8192);
        let original_serial = pid1.serial;

        // Terminate it - should add to reuse queue
        table.terminate(pid1).unwrap();

        // Spawn another process - should reuse the PID with incremented serial
        let (_, pid2) = table.spawn(8192);

        // PID should be reused with incremented serial
        assert_eq!(pid2.id, pid1.id);
        assert_eq!(pid2.serial, original_serial + 1);
        assert_eq!(pid2.creation, pid1.creation);
    }

    #[test]
    fn test_state_transition_validation() {
        let mut table = ProcessTable::new(0);
        let (handle, pid) = table.spawn(8192);

        // Get the PCB
        let (_, pcb) = table.get_by_pid(pid).unwrap();

        // Running -> Waiting: valid
        assert!(pcb.set_state(ProcessState::Waiting).is_ok());

        // Waiting -> Running: valid
        assert!(pcb.set_state(ProcessState::Running).is_ok());

        // Running -> Suspended: valid
        assert!(pcb.set_state(ProcessState::Suspended).is_ok());

        // Suspended -> Running: valid
        assert!(pcb.set_state(ProcessState::Running).is_ok());

        // Running -> Exiting: valid
        assert!(pcb.set_state(ProcessState::Exiting).is_ok());

        // Exiting -> Dead: valid
        assert!(pcb.set_state(ProcessState::Dead).is_ok());

        // Dead -> anything: invalid
        assert!(pcb.set_state(ProcessState::Running).is_err());
        assert!(pcb.set_state(ProcessState::Waiting).is_err());
    }
}
