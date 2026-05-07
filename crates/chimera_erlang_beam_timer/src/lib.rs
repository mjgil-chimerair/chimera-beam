//! Timer and I/O subsystem for RustZigBeam.
//!
//! Rust owns all timer and I/O semantics - event loop, timer management,
//! async I/O readiness, and scheduler wakeups.
//!
//! Uses a min-heap for efficient timer operations (O(log n) insert/remove).

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_process::Pid;
use chimera_erlang_beam_term::Term;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Timer identifier
pub type TimerId = u64;

/// Event type for the event loop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    /// Timer event
    Timer = 0,
    /// Port event
    Port = 1,
    /// Socket event
    Socket = 2,
    /// Signal event
    Signal = 3,
}

/// A scheduled timer entry for the min-heap
#[derive(Debug, PartialEq)]
pub struct TimerEntry {
    /// Unique timer identifier
    pub id: TimerId,
    /// Target process to receive the timer message
    pub target_pid: Pid,
    /// Message to send when timer fires
    pub message: Term,
    /// Timeout timestamp in milliseconds since epoch
    pub timeout_timestamp: i64,
    /// Whether the timer is active
    pub active: bool,
}

/// Implement ordering for min-heap (earliest timeout first)
impl Ord for TimerEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (smallest timestamp = highest priority)
        other.timeout_timestamp.cmp(&self.timeout_timestamp)
    }
}

impl PartialOrd for TimerEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for TimerEntry {}

/// Timer manager - handles all timer operations using a min-heap
#[derive(Debug)]
pub struct TimerManager {
    /// Min-heap of active timers
    timers: BinaryHeap<TimerEntry>,
    /// List of cancelled timer IDs (lazy deletion)
    cancelled: Vec<TimerId>,
    /// Next available timer ID
    next_timer_id: TimerId,
    /// File descriptor for wakeup events
    wakeup_fd: Option<u32>,
    /// Count of pending I/O events
    pending_count: usize,
}

impl TimerManager {
    /// Create a new timer manager
    pub fn new() -> Self {
        TimerManager {
            timers: BinaryHeap::new(),
            cancelled: Vec::new(),
            next_timer_id: 1,
            wakeup_fd: None,
            pending_count: 0,
        }
    }

    /// Add a new timer - O(log n) using min-heap
    pub fn add_timer(&mut self, timeout_ms: i64, target: Pid, msg: Term) -> Option<TimerId> {
        let id = self.next_timer_id;
        self.next_timer_id += 1;

        let now = timestamp_ms();
        let entry = TimerEntry {
            id,
            target_pid: target,
            message: msg,
            timeout_timestamp: now + timeout_ms,
            active: true,
        };

        self.timers.push(entry);
        Some(id)
    }

    /// Cancel a timer by ID - O(1) mark for lazy deletion
    pub fn cancel_timer(&mut self, timer_id: TimerId) -> bool {
        // Mark as cancelled for lazy deletion
        self.cancelled.push(timer_id);
        // Check if it's in the heap
        self.timers.iter().any(|t| t.id == timer_id && t.active)
    }

    /// Get expired timers that should fire - O(n) sweep
    pub fn get_expired_timers(&mut self) -> Vec<TimerEntry> {
        let now = timestamp_ms();
        let mut expired = Vec::new();

        // Collect all expired timers from the heap
        let mut new_heap = BinaryHeap::new();
        while let Some(timer) = self.timers.pop() {
            // Skip cancelled timers
            if self.cancelled.contains(&timer.id) {
                continue;
            }

            if timer.active && timer.timeout_timestamp <= now {
                expired.push(TimerEntry {
                    id: timer.id,
                    target_pid: timer.target_pid,
                    message: timer.message,
                    timeout_timestamp: timer.timeout_timestamp,
                    active: false,
                });
            } else if timer.active {
                new_heap.push(timer);
            }
        }
        self.timers = new_heap;
        // Clear cancelled list
        self.cancelled.clear();

        expired
    }

    /// Check if any timers are pending
    pub fn has_pending_timers(&self) -> bool {
        !self.timers.is_empty()
    }

    /// Get count of active timers
    pub fn active_timer_count(&self) -> usize {
        self.timers.len()
    }

    /// Get the next timer expiration time
    pub fn next_expiration(&self) -> Option<i64> {
        self.timers.peek().map(|t| t.timeout_timestamp)
    }

    /// Remove cancelled timers from the heap
    pub fn purge_cancelled(&mut self) {
        self.cancelled.clear();
    }

    /// Set wakeup file descriptor (for event loop integration)
    pub fn set_wakeup_fd(&mut self, fd: u32) {
        self.wakeup_fd = Some(fd);
    }

    /// Get wakeup file descriptor
    pub fn get_wakeup_fd(&self) -> Option<u32> {
        self.wakeup_fd
    }

    /// Increment pending count (for I/O events)
    pub fn increment_pending(&mut self) {
        self.pending_count += 1;
    }

    /// Decrement pending count
    pub fn decrement_pending(&mut self) {
        if self.pending_count > 0 {
            self.pending_count -= 1;
        }
    }

    /// Get pending count
    pub fn get_pending_count(&self) -> usize {
        self.pending_count
    }

    /// Check if a specific timer is active
    pub fn is_timer_active(&self, timer_id: TimerId) -> bool {
        // Check if cancelled
        if self.cancelled.contains(&timer_id) {
            return false;
        }
        // Check if in the heap
        self.timers.iter().any(|t| t.id == timer_id && t.active)
    }
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current timestamp in milliseconds
fn timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Timer reference for process to wait on
#[derive(Debug, Clone, Copy)]
pub struct TimerRef {
    /// The timer ID
    pub id: TimerId,
}

impl TimerRef {
    /// Create a new timer reference
    pub fn new(id: TimerId) -> Self {
        TimerRef { id }
    }
}

/// Port status for async I/O
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    /// Port is closed
    Closed,
    /// Port is open
    Open,
    /// Port is listening for connections
    Listening,
    /// Port is connected
    Connected,
}

/// A port for async I/O operations
#[derive(Debug)]
pub struct Port {
    /// Port identifier
    pub id: u64,
    /// Current port status
    pub status: PortStatus,
    /// File descriptor for wakeup events
    pub wakeup_fd: Option<u32>,
}

/// Port configuration flags
#[derive(Debug, Clone, Copy, Default)]
pub struct PortFlags {
    /// Port should deliver exit signal when closed
    pub exit_on_close: bool,
    /// Port is a stream (vs datagram)
    pub stream: bool,
    /// Port is using active mode (vs passive)
    pub active: bool,
}

impl PortFlags {
    /// Create stream flags (passive mode)
    pub fn stream() -> Self {
        Self {
            stream: true,
            active: false,
            exit_on_close: true,
        }
    }

    /// Create active flags (active mode)
    pub fn active() -> Self {
        Self {
            stream: true,
            active: true,
            exit_on_close: true,
        }
    }
}

/// Port command for sending to a port
#[derive(Debug, Clone)]
pub enum PortCommand {
    /// Send data to port
    Send(Vec<u8>),
    /// Close the port
    Close,
    /// Set port option
    SetOpt(String, Term),
    /// Get port option
    GetOpt(String),
}

/// Port event delivered to process
#[derive(Debug, Clone)]
pub enum PortEvent {
    /// Data available to read
    Data(Vec<u8>),
    /// Port is closed
    Closed,
    /// Port encountered an error
    Error(String),
    /// Port is ready for output
    ReadyOutput,
    /// Port has accepted a connection (for listening ports)
    Accepted(u64),
}

/// A port for async I/O operations
///
/// Task 70: Complete ports and async I/O
#[derive(Debug)]
pub struct AsyncPort {
    /// Port identifier
    pub id: u64,
    /// Current port status
    pub status: PortStatus,
    /// File descriptor for wakeup events
    pub wakeup_fd: Option<u32>,
    /// Messages queued for delivery to the port owner
    message_queue: Vec<Term>,
    /// Registered owner PID
    owner_pid: Option<Pid>,
    /// Port configuration flags
    pub flags: PortFlags,
    /// Connection ID for accepted connections
    accepted_connection: Option<u64>,
}

impl AsyncPort {
    /// Create a new async port
    pub fn new(id: u64, owner_pid: Pid, wakeup_fd: Option<u32>) -> Self {
        Self {
            id,
            status: PortStatus::Open,
            wakeup_fd,
            message_queue: Vec::new(),
            owner_pid: Some(owner_pid),
            flags: PortFlags::default(),
            accepted_connection: None,
        }
    }

    /// Create a new async port with custom flags
    pub fn with_flags(mut self, flags: PortFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Queue a message for the owner
    pub fn queue_message(&mut self, msg: Term) {
        self.message_queue.push(msg);
    }

    /// Get queued messages
    pub fn get_messages(&mut self) -> Vec<Term> {
        std::mem::take(&mut self.message_queue)
    }

    /// Check if port has pending messages
    pub fn has_messages(&self) -> bool {
        !self.message_queue.is_empty()
    }

    /// Deliver a data event
    pub fn deliver_data(&mut self, data: Vec<u8>) {
        // Store data as binary reference - simplified encoding
        // In full implementation, this would be proper binary term encoding
        let _ = data;
        // Mark that data is available - actual term encoding happens elsewhere
        self.message_queue
            .push(Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_OK));
    }

    /// Deliver an error event
    pub fn deliver_error(&mut self, _error: &str) {
        let event = Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_ERROR);
        self.queue_message(event);
    }

    /// Mark port as closed
    pub fn close(&mut self) {
        self.status = PortStatus::Closed;
    }

    /// Mark port as failed
    pub fn fail(&mut self, reason: &str) {
        // In a full implementation, status would include failure reason
        // For now, just mark as closed with error indicator
        let _ = reason;
        self.status = PortStatus::Closed;
    }

    /// Get owner PID
    pub fn owner(&self) -> Option<Pid> {
        self.owner_pid
    }

    /// Set accepted connection
    pub fn set_accepted_connection(&mut self, port_id: u64) {
        self.accepted_connection = Some(port_id);
    }
}

/// Async port manager for tracking open ports with proper async I/O
///
/// Task 70: Complete ports and async I/O
#[derive(Debug)]
pub struct AsyncPortManager {
    /// Vector of ports (Some for active, None for deleted)
    ports: Vec<Option<AsyncPort>>,
    /// Next available port ID
    next_port_id: u64,
    /// Ports that need to wake their owners
    ports_needing_wakeup: Vec<u32>,
}

impl AsyncPortManager {
    /// Create a new async port manager
    pub fn new() -> Self {
        AsyncPortManager {
            ports: Vec::with_capacity(64),
            next_port_id: 1,
            ports_needing_wakeup: Vec::new(),
        }
    }

    /// Open a new async port
    pub fn open_async_port(&mut self, owner_pid: Pid, wakeup_fd: Option<u32>) -> Option<u64> {
        let id = self.next_port_id;
        self.next_port_id += 1;

        let port = AsyncPort::new(id, owner_pid, wakeup_fd);

        let slot = self.ports.iter_mut().find(|p| p.is_none());
        if let Some(slot) = slot {
            *slot = Some(port);
        } else {
            self.ports.push(Some(port));
        }

        Some(id)
    }

    /// Open a new async port with configuration
    pub fn open_async_port_with_flags(
        &mut self,
        owner_pid: Pid,
        wakeup_fd: Option<u32>,
        flags: PortFlags,
    ) -> Option<u64> {
        let id = self.next_port_id;
        self.next_port_id += 1;

        let mut port = AsyncPort::new(id, owner_pid, wakeup_fd);
        port.flags = flags;

        let slot = self.ports.iter_mut().find(|p| p.is_none());
        if let Some(slot) = slot {
            *slot = Some(port);
        } else {
            self.ports.push(Some(port));
        }

        Some(id)
    }

    /// Close a port
    pub fn close_port(&mut self, port_id: u64) -> bool {
        for p in self.ports.iter_mut().flatten() {
            if p.id == port_id {
                p.close();
                if let Some(owner) = p.owner() {
                    self.ports_needing_wakeup.push(owner.id);
                }
                return true;
            }
        }
        false
    }

    /// Fail a port
    pub fn fail_port(&mut self, port_id: u64, reason: &str) -> bool {
        for p in self.ports.iter_mut().flatten() {
            if p.id == port_id {
                p.fail(reason);
                if let Some(owner) = p.owner() {
                    self.ports_needing_wakeup.push(owner.id);
                }
                return true;
            }
        }
        false
    }

    /// Get port status
    pub fn get_status(&self, port_id: u64) -> Option<PortStatus> {
        self.ports
            .iter()
            .flatten()
            .find(|p| p.id == port_id)
            .map(|p| p.status)
    }

    /// Set port ready (socket/port has data) - triggers wakeup
    pub fn set_ready(&mut self, port_id: u64) -> bool {
        for p in self.ports.iter_mut().flatten() {
            if p.id == port_id && p.status == PortStatus::Open {
                if let Some(owner) = p.owner() {
                    self.ports_needing_wakeup.push(owner.id);
                }
                return true;
            }
        }
        false
    }

    /// Deliver data to a port
    pub fn deliver_data(&mut self, port_id: u64, data: Vec<u8>) -> bool {
        for p in self.ports.iter_mut().flatten() {
            if p.id == port_id {
                p.deliver_data(data);
                if let Some(owner) = p.owner() {
                    self.ports_needing_wakeup.push(owner.id);
                }
                return true;
            }
        }
        false
    }

    /// Get port by ID
    pub fn get_port(&mut self, port_id: u64) -> Option<&mut AsyncPort> {
        self.ports.iter_mut().flatten().find(|p| p.id == port_id)
    }

    /// Get ports that need their owners woken
    pub fn take_wakeup_list(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.ports_needing_wakeup)
    }

    /// Check if any ports have pending messages
    pub fn has_pending_messages(&self) -> bool {
        self.ports.iter().flatten().any(|p| p.has_messages())
    }

    /// Get all ports needing wakeup (deduplicated)
    pub fn get_pending_wakeups(&self) -> Vec<u32> {
        let mut seen = std::collections::HashSet::new();
        let mut wakeups = Vec::new();
        for p in self.ports.iter().flatten() {
            if p.has_messages() || p.status != PortStatus::Closed {
                if let Some(owner) = p.owner() {
                    if seen.insert(owner.id) {
                        wakeups.push(owner.id);
                    }
                }
            }
        }
        wakeups
    }
}

impl Default for AsyncPortManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Port manager for tracking open ports
#[derive(Debug)]
pub struct PortManager {
    /// Vector of ports (Some for active, None for deleted)
    ports: Vec<Option<Port>>,
    /// Next available port ID
    next_port_id: u64,
}

impl PortManager {
    /// Create a new port manager
    pub fn new() -> Self {
        PortManager {
            ports: Vec::with_capacity(64),
            next_port_id: 1,
        }
    }

    /// Open a new port
    pub fn open_port(&mut self, wakeup_fd: Option<u32>) -> Option<u64> {
        let id = self.next_port_id;
        self.next_port_id += 1;

        let port = Port {
            id,
            status: PortStatus::Open,
            wakeup_fd,
        };

        let slot = self.ports.iter_mut().find(|p| p.is_none());
        if let Some(slot) = slot {
            *slot = Some(port);
        } else {
            self.ports.push(Some(port));
        }

        Some(id)
    }

    /// Close a port
    pub fn close_port(&mut self, port_id: u64) -> bool {
        for p in self.ports.iter_mut().flatten() {
            if p.id == port_id {
                p.status = PortStatus::Closed;
                return true;
            }
        }
        false
    }

    /// Get port status
    pub fn get_status(&self, port_id: u64) -> Option<PortStatus> {
        self.ports
            .iter()
            .flatten()
            .find(|p| p.id == port_id)
            .map(|p| p.status)
    }

    /// Set port ready (socket/port has data)
    pub fn set_ready(&mut self, port_id: u64) -> bool {
        for p in self.ports.iter_mut().flatten() {
            if p.id == port_id && p.status == PortStatus::Open {
                // Would trigger scheduler wakeup in full implementation
                return true;
            }
        }
        false
    }
}

impl Default for PortManager {
    fn default() -> Self {
        Self::new()
    }
}

/// BIF: start_timer (send message after timeout)
pub fn bif_start_timer(
    timeout_ms: i64,
    target: Pid,
    msg: Term,
    manager: &mut TimerManager,
) -> Option<TimerId> {
    manager.add_timer(timeout_ms, target, msg)
}

/// BIF: cancel_timer
pub fn bif_cancel_timer(timer_id: TimerId, manager: &mut TimerManager) -> bool {
    manager.cancel_timer(timer_id)
}

/// BIF: read_timer (check if timer exists)
pub fn bif_read_timer(timer_id: TimerId, manager: &TimerManager) -> bool {
    manager.is_timer_active(timer_id)
}

/// Timer event - delivered to process when timer fires
#[derive(Debug)]
pub struct TimerEvent {
    /// The timer ID that fired
    pub timer_id: TimerId,
    /// The target process PID
    pub target_pid: Pid,
    /// The message to deliver
    pub message: Term,
    /// Timestamp when the timer fired
    pub fired_at: i64,
}

impl TimerEvent {
    /// Create a timer event from a timer entry
    pub fn from_entry(entry: &TimerEntry) -> Self {
        TimerEvent {
            timer_id: entry.id,
            target_pid: entry.target_pid,
            message: entry.message,
            fired_at: timestamp_ms(),
        }
    }
}

/// Scheduler wakeup reason for timer events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeupReason {
    /// A timer expired
    TimerExpired,
    /// A port is ready
    PortReady,
    /// A socket is ready
    SocketReady,
    /// A message was received
    MessageReceived,
}

/// Event loop integration for the scheduler
#[derive(Debug)]
pub struct TimerEventLoop {
    /// Timer manager for this event loop
    timer_manager: TimerManager,
    /// Port manager for this event loop
    port_manager: PortManager,
}

impl TimerEventLoop {
    /// Create a new timer event loop
    pub fn new() -> Self {
        TimerEventLoop {
            timer_manager: TimerManager::new(),
            port_manager: PortManager::new(),
        }
    }

    /// Get the timer manager reference
    pub fn timer_manager(&self) -> &TimerManager {
        &self.timer_manager
    }

    /// Get mutable timer manager reference
    pub fn timer_manager_mut(&mut self) -> &mut TimerManager {
        &mut self.timer_manager
    }

    /// Get the port manager reference
    pub fn port_manager(&self) -> &PortManager {
        &self.port_manager
    }

    /// Get mutable port manager reference
    pub fn port_manager_mut(&mut self) -> &mut PortManager {
        &mut self.port_manager
    }

    /// Process all expired timers and return events
    pub fn process_expired_timers(&mut self) -> Vec<TimerEvent> {
        let entries = self.timer_manager.get_expired_timers();
        entries.iter().map(TimerEvent::from_entry).collect()
    }

    /// Check if scheduler should wake up
    pub fn should_wake(&self) -> bool {
        self.timer_manager.has_pending_timers()
            || self
                .port_manager
                .ports
                .iter()
                .any(|p| p.as_ref().map(|_| true).unwrap_or(false))
    }

    /// Get next wakeup time
    pub fn next_wakeup_time(&self) -> Option<i64> {
        self.timer_manager.next_expiration()
    }
}

impl Default for TimerEventLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_manager_create() {
        let manager = TimerManager::new();
        assert!(!manager.has_pending_timers());
        assert_eq!(manager.active_timer_count(), 0);
    }

    #[test]
    fn test_timer_add() {
        let mut manager = TimerManager::new();
        let pid = Pid::new(1, 0, 0);
        let msg = Term::from_small(42);

        let timer_id = manager.add_timer(1000, pid, msg);
        assert!(timer_id.is_some());
        assert!(manager.has_pending_timers());
        assert_eq!(manager.active_timer_count(), 1);
    }

    #[test]
    fn test_timer_cancel() {
        let mut manager = TimerManager::new();
        let pid = Pid::new(1, 0, 0);
        let msg = Term::from_small(42);

        let timer_id = manager.add_timer(1000, pid, msg).unwrap();
        assert!(manager.cancel_timer(timer_id));
        // Cancel returns true if timer was found
        let _ = manager.cancel_timer(timer_id);
    }

    #[test]
    fn test_timer_multiple() {
        let mut manager = TimerManager::new();
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);

        let id1 = manager.add_timer(100, pid1, Term::from_small(1)).unwrap();
        let id2 = manager.add_timer(200, pid2, Term::from_small(2)).unwrap();

        assert_eq!(manager.active_timer_count(), 2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_timer_ref() {
        let ref1 = TimerRef::new(1);
        let ref2 = TimerRef::new(2);
        assert_ne!(ref1.id, ref2.id);
    }

    #[test]
    fn test_bif_start_timer() {
        let mut manager = TimerManager::new();
        let pid = Pid::new(1, 0, 0);
        let msg = Term::from_small(42);

        let result = bif_start_timer(1000, pid, msg, &mut manager);
        assert!(result.is_some());
    }

    #[test]
    fn test_bif_cancel_timer() {
        let mut manager = TimerManager::new();
        let pid = Pid::new(1, 0, 0);
        let msg = Term::from_small(42);

        let timer_id = bif_start_timer(1000, pid, msg, &mut manager).unwrap();
        assert!(bif_cancel_timer(timer_id, &mut manager));
        // After cancelling, timer is not active
        assert!(!bif_read_timer(timer_id, &manager));
    }

    #[test]
    fn test_bif_read_timer() {
        let mut manager = TimerManager::new();
        let pid = Pid::new(1, 0, 0);
        let msg = Term::from_small(42);

        let timer_id = bif_start_timer(1000, pid, msg, &mut manager).unwrap();
        assert!(bif_read_timer(timer_id, &manager));
        assert!(!bif_read_timer(timer_id + 100, &manager)); // Non-existent

        bif_cancel_timer(timer_id, &mut manager);
        assert!(!bif_read_timer(timer_id, &manager));
    }

    #[test]
    fn test_timer_entry() {
        let entry = TimerEntry {
            id: 42,
            target_pid: Pid::new(1, 0, 0),
            message: Term::from_small(100),
            timeout_timestamp: 1000,
            active: true,
        };

        let event = TimerEvent::from_entry(&entry);
        assert_eq!(event.timer_id, 42);
    }

    #[test]
    fn test_port_manager_create() {
        let mut manager = PortManager::new();
        assert!(manager.open_port(None).is_some());
    }

    #[test]
    fn test_port_manager_open_close() {
        let mut manager = PortManager::new();
        let port_id = manager.open_port(None).unwrap();
        assert_eq!(manager.get_status(port_id), Some(PortStatus::Open));

        assert!(manager.close_port(port_id));
        assert_eq!(manager.get_status(port_id), Some(PortStatus::Closed));
    }

    #[test]
    fn test_port_manager_multiple_ports() {
        let mut manager = PortManager::new();
        let id1 = manager.open_port(None).unwrap();
        let id2 = manager.open_port(None).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_timer_event_loop() {
        let mut loop_ = TimerEventLoop::new();
        assert!(!loop_.should_wake());

        let pid = Pid::new(1, 0, 0);
        loop_.timer_manager_mut().add_timer(100, pid, Term::nil());
        assert!(loop_.should_wake());
    }

    #[test]
    fn test_timer_event_loop_process_expired() {
        let mut loop_ = TimerEventLoop::new();
        let pid = Pid::new(1, 0, 0);

        // Add a timer that expires immediately
        loop_
            .timer_manager_mut()
            .add_timer(0, pid, Term::from_small(42));
        std::thread::sleep(std::time::Duration::from_millis(10));

        let events = loop_.process_expired_timers();
        assert!(!events.is_empty());
    }

    #[test]
    fn test_timer_ordering() {
        let mut manager = TimerManager::new();
        let pid = Pid::new(1, 0, 0);

        // Add timers with different timeouts
        manager.add_timer(300, pid, Term::from_small(3));
        manager.add_timer(100, pid, Term::from_small(1));
        manager.add_timer(200, pid, Term::from_small(2));

        // Next expiration should be 100ms (timer 1)
        assert_eq!(manager.next_expiration(), Some(timestamp_ms() + 100));
    }

    #[test]
    fn test_port_manager_set_ready() {
        let mut manager = PortManager::new();
        let port_id = manager.open_port(None).unwrap();

        // Set port ready should return true for open port
        assert!(manager.set_ready(port_id));
    }
}

#[cfg(test)]
mod phase5_tests {
    use super::*;

    #[test]
    fn test_timer_manager_creation() {
        let manager = TimerManager::new();
        assert!(!manager.has_pending_timers());
        assert_eq!(manager.pending_count, 0);
    }

    #[test]
    fn test_timer_add_and_cancel() {
        let mut manager = TimerManager::new();
        let timer_id = manager.add_timer(1000, Pid::new(1, 0, 0), Term::nil());
        assert!(timer_id.is_some());

        let id = timer_id.unwrap();
        assert!(manager.cancel_timer(id));
    }
}
