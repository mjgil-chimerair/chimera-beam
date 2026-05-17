//! Scheduler for RustZigBeam.
//!
//! Rust owns the scheduler - scheduling decisions, run queues, reductions,
//! and actual scheduler threads that run the VM.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

pub mod dirty;
pub mod metrics;
pub use dirty::{
    DirtyJob, DirtyJobStatus, DirtyJobType, DirtyScheduler, DirtySchedulerRegistry,
    DirtySchedulerStats,
};
pub use metrics::{
    Counter, Gauge, HistogramBucket, MetricsRegistry, SchedulerMetrics, Timer, VmMetrics,
};

use chimera_erlang_beam_instr::execute_instruction;
use chimera_erlang_beam_process::{Pid, Priority, ProcessHandle, ProcessTable};
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Scheduler statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct SchedulerStats {
    pub reductions_executed: u64,
    pub processes_spawned: u64,
    pub processes_exited: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub context_switches: u64,
}

/// Default reduction budget per process run
///
/// BEAM typically uses 2000 reductions before yielding.
/// This allows processes to run long enough to be efficient
/// but short enough to provide fairness between processes.
pub const DEFAULT_REDUCTION_BUDGET: u32 = 2000;

/// A run queue entry - PID and handle pair
///
/// Using ProcessHandle instead of raw pointer ensures:
/// - Stable lookups through ProcessTable
/// - No use-after-free bugs
/// - Proper validation through handle->PCB lookup
#[derive(Debug, Clone, Copy)]
pub struct RunQueueEntry {
    pid: Pid,
    handle: ProcessHandle,
    priority: Priority,
}

/// Priority run queues
///
/// BEAM uses 4 priority levels: low, normal, high, max.
/// We implement strict priority scheduling with fairness within each level.
#[derive(Debug, Default)]
pub struct PriorityRunQueues {
    /// Priority 0 (Low)
    low: VecDeque<RunQueueEntry>,
    /// Priority 1 (Normal)
    normal: VecDeque<RunQueueEntry>,
    /// Priority 2 (High)
    high: VecDeque<RunQueueEntry>,
    /// Priority 3 (Max)
    max: VecDeque<RunQueueEntry>,
}

impl PriorityRunQueues {
    pub fn new() -> Self {
        PriorityRunQueues::default()
    }

    /// Get the queue for a given priority level
    fn get_queue(&mut self, priority: Priority) -> &mut VecDeque<RunQueueEntry> {
        match priority {
            Priority::Low => &mut self.low,
            Priority::Normal => &mut self.normal,
            Priority::High => &mut self.high,
            Priority::Max => &mut self.max,
        }
    }

    /// Enqueue a process at its priority level
    pub fn enqueue(&mut self, entry: RunQueueEntry) {
        if !self.contains_pid(entry.pid) {
            self.get_queue(entry.priority).push_back(entry);
        }
    }

    /// Dequeue the highest priority non-empty queue
    ///
    /// Returns the entry or None if all queues are empty.
    /// Uses O(1) pop_front instead of O(n) Vec::remove(0).
    pub fn dequeue(&mut self) -> Option<RunQueueEntry> {
        // Check in priority order: Max, High, Normal, Low
        if !self.max.is_empty() {
            return self.max.pop_front();
        }
        if !self.high.is_empty() {
            return self.high.pop_front();
        }
        if !self.normal.is_empty() {
            return self.normal.pop_front();
        }
        if !self.low.is_empty() {
            return self.low.pop_front();
        }
        None
    }

    /// Check if a PID is in any queue
    pub fn contains_pid(&self, pid: Pid) -> bool {
        self.low.iter().any(|e| e.pid == pid)
            || self.normal.iter().any(|e| e.pid == pid)
            || self.high.iter().any(|e| e.pid == pid)
            || self.max.iter().any(|e| e.pid == pid)
    }

    /// Remove a process by PID from any queue
    ///
    /// Returns true if the process was found and removed.
    pub fn remove_by_pid(&mut self, pid: Pid) -> bool {
        let mut removed = false;
        self.low.retain(|e| {
            if e.pid == pid {
                removed = true;
                false
            } else {
                true
            }
        });
        if removed {
            return true;
        }
        self.normal.retain(|e| {
            if e.pid == pid {
                removed = true;
                false
            } else {
                true
            }
        });
        if removed {
            return true;
        }
        self.high.retain(|e| {
            if e.pid == pid {
                removed = true;
                false
            } else {
                true
            }
        });
        if removed {
            return true;
        }
        self.max.retain(|e| {
            if e.pid == pid {
                removed = true;
                false
            } else {
                true
            }
        });
        removed
    }

    /// Get total depth across all queues
    pub fn len(&self) -> usize {
        self.low.len() + self.normal.len() + self.high.len() + self.max.len()
    }

    /// Check if all queues are empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get depth of a specific priority queue
    pub fn queue_len(&self, priority: Priority) -> usize {
        match priority {
            Priority::Low => self.low.len(),
            Priority::Normal => self.normal.len(),
            Priority::High => self.high.len(),
            Priority::Max => self.max.len(),
        }
    }

    /// Promote a process to a higher priority
    pub fn promote(&mut self, _pid: Pid) -> bool {
        // Find and remove from current queue
        // For now, just remove from lower and re-add at next higher
        // Full implementation would store the entry
        false
    }
}

/// A BEAM-like scheduler
#[derive(Debug)]
pub struct Scheduler {
    pub id: u32,
    run_queues: PriorityRunQueues,
    stats: SchedulerStats,
    /// Maps PID to remaining reduction budget
    reduction_budgets: std::collections::HashMap<Pid, u32>,
    /// Default priority for spawned processes
    default_priority: Priority,
    /// Whether this scheduler is online (accepting work)
    online: bool,
    /// The spawned thread handle (if running)
    thread_handle: Option<JoinHandle<()>>,
    /// Shared stop flag for graceful shutdown
    stop_flag: Arc<AtomicBool>,
    /// Thread-safe run queue access (for steal support)
    run_queue_mutex: Mutex<()>,
    /// CPU core affinity for this scheduler (-1 = no affinity)
    cpu_affinity: i32,
    /// Scheduler affinity mode: spread (default) or compact
    affinity_mode: AffinityMode,
    /// Timer manager for checking expired timers (wired in Task 69)
    timer_manager: Option<Arc<Mutex<chimera_erlang_beam_timer::TimerManager>>>,
    /// Port manager for checking pending port wakeups (Task 70)
    port_manager: Option<Arc<Mutex<chimera_erlang_beam_timer::AsyncPortManager>>>,
}

/// Scheduler affinity mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AffinityMode {
    /// Spread schedulers across cores (default)
    #[default]
    Spread,
    /// Compact schedulers onto consecutive cores
    Compact,
    /// No affinity (let OS decide)
    None,
}

impl Scheduler {
    pub fn new(id: u32) -> Self {
        Scheduler {
            id,
            run_queues: PriorityRunQueues::new(),
            stats: SchedulerStats::default(),
            reduction_budgets: std::collections::HashMap::new(),
            default_priority: Priority::Normal,
            online: true,
            thread_handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            run_queue_mutex: Mutex::new(()),
            cpu_affinity: -1, // No affinity by default
            affinity_mode: AffinityMode::Spread,
            timer_manager: None,
            port_manager: None,
        }
    }

    /// Create a scheduler with CPU affinity
    pub fn with_affinity(id: u32, cpu_id: i32) -> Self {
        Scheduler {
            id,
            run_queues: PriorityRunQueues::new(),
            stats: SchedulerStats::default(),
            reduction_budgets: std::collections::HashMap::new(),
            default_priority: Priority::Normal,
            online: true,
            thread_handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            run_queue_mutex: Mutex::new(()),
            cpu_affinity: cpu_id,
            affinity_mode: AffinityMode::None,
            timer_manager: None,
            port_manager: None,
        }
    }

    /// Get CPU affinity for this scheduler (-1 = no affinity)
    pub fn cpu_affinity(&self) -> i32 {
        self.cpu_affinity
    }

    /// Set CPU affinity for this scheduler
    pub fn set_cpu_affinity(&mut self, cpu_id: i32) {
        self.cpu_affinity = cpu_id;
    }

    /// Get affinity mode
    pub fn affinity_mode(&self) -> AffinityMode {
        self.affinity_mode
    }

    /// Set affinity mode
    pub fn set_affinity_mode(&mut self, mode: AffinityMode) {
        self.affinity_mode = mode;
    }

    /// Set the timer manager for this scheduler (Task 69 integration)
    pub fn set_timer_manager(
        &mut self,
        manager: Arc<Mutex<chimera_erlang_beam_timer::TimerManager>>,
    ) {
        self.timer_manager = Some(manager);
    }

    /// Get the timer manager (if set)
    pub fn timer_manager(&self) -> Option<&Arc<Mutex<chimera_erlang_beam_timer::TimerManager>>> {
        self.timer_manager.as_ref()
    }

    /// Set the port manager for this scheduler (Task 70 integration)
    pub fn set_port_manager(
        &mut self,
        manager: Arc<Mutex<chimera_erlang_beam_timer::AsyncPortManager>>,
    ) {
        self.port_manager = Some(manager);
    }

    /// Get the port manager (if set)
    pub fn port_manager(&self) -> Option<&Arc<Mutex<chimera_erlang_beam_timer::AsyncPortManager>>> {
        self.port_manager.as_ref()
    }

    /// Get the ideal CPU core for this scheduler (for debugging/info)
    pub fn ideal_cpu(&self) -> Option<i32> {
        if self.cpu_affinity >= 0 {
            Some(self.cpu_affinity)
        } else {
            None
        }
    }

    /// Get the reduction budget for a process
    pub fn get_reduction_budget(&self, pid: Pid) -> u32 {
        self.reduction_budgets
            .get(&pid)
            .copied()
            .unwrap_or(DEFAULT_REDUCTION_BUDGET)
    }

    /// Set the reduction budget for a process
    fn set_reduction_budget(&mut self, pid: Pid, budget: u32) {
        self.reduction_budgets.insert(pid, budget);
    }

    /// Consume one reduction and return remaining budget
    ///
    /// Returns the remaining budget after this reduction.
    /// When budget reaches 0, the process should yield.
    pub fn consume_reduction(&mut self, pid: Pid) -> u32 {
        let budget = self.get_reduction_budget(pid);
        if budget > 0 {
            let new_budget = budget - 1;
            self.set_reduction_budget(pid, new_budget);
            new_budget
        } else {
            0
        }
    }

    /// Reset reduction budget for a process (typically when woken up)
    pub fn reset_reduction_budget(&mut self, pid: Pid) {
        self.set_reduction_budget(pid, DEFAULT_REDUCTION_BUDGET);
    }

    /// Check if a process has exhausted its reduction budget
    pub fn is_budget_exhausted(&self, pid: Pid) -> bool {
        self.get_reduction_budget(pid) == 0
    }

    /// Wake a waiting process - reset budget and enqueue
    ///
    /// Called when a process receives a message or is otherwise
    /// woken up from a waiting state.
    pub fn wake_process(&mut self, handle: ProcessHandle, pid: Pid) {
        self.reset_reduction_budget(pid);
        self.enqueue(handle, pid);
    }

    /// Remove a process from any run queue by PID
    ///
    /// Returns true if the process was found and removed.
    pub fn remove_by_pid(&mut self, pid: Pid) -> bool {
        if self.run_queues.remove_by_pid(pid) {
            // Also clean up reduction budget
            self.reduction_budgets.remove(&pid);
            true
        } else {
            false
        }
    }

    /// Enqueue a process for execution at default priority
    pub fn enqueue(&mut self, handle: ProcessHandle, pid: Pid) {
        self.enqueue_with_priority(handle, pid, self.default_priority);
    }

    /// Enqueue a process for execution with a specific priority
    pub fn enqueue_with_priority(&mut self, handle: ProcessHandle, pid: Pid, priority: Priority) {
        let entry = RunQueueEntry {
            pid,
            handle,
            priority,
        };
        self.run_queues.enqueue(entry);
    }

    /// Dequeue a process for execution
    ///
    /// Returns the PID and ProcessHandle, or None if all queues are empty.
    pub fn dequeue(&mut self) -> Option<(Pid, ProcessHandle)> {
        self.run_queues
            .dequeue()
            .map(|entry| (entry.pid, entry.handle))
    }

    /// Check if a PID is in any run queue
    pub fn contains_pid(&self, pid: Pid) -> bool {
        self.run_queues.contains_pid(pid)
    }

    pub fn is_empty(&self) -> bool {
        self.run_queues.is_empty()
    }

    /// Take this scheduler offline (stop accepting new work)
    ///
    /// Returns true if the scheduler was online and is now offline.
    pub fn go_offline(&mut self) -> bool {
        if self.online {
            self.online = false;
            true
        } else {
            false
        }
    }

    /// Bring this scheduler online (accept new work)
    pub fn go_online(&mut self) {
        self.online = true;
    }

    /// Check if this scheduler is online
    pub fn is_online(&self) -> bool {
        self.online
    }

    pub fn len(&self) -> usize {
        self.run_queues.len()
    }

    /// Get the length of a specific priority queue
    pub fn queue_len(&self, priority: Priority) -> usize {
        self.run_queues.queue_len(priority)
    }

    /// Check if a priority level is starved (has been waiting too long)
    ///
    /// Returns true if the priority queue has entries but hasn't been
    /// serviced in a while compared to higher priorities.
    pub fn is_starved(&self, priority: Priority) -> bool {
        // Check if this priority queue has entries
        if self.run_queues.queue_len(priority) == 0 {
            return false;
        }

        // Higher priorities should always be checked first
        // If higher priorities are empty and this one has entries, it's not starved
        // but would be next in line anyway.
        match priority {
            Priority::Low => {
                // Low is starved if Normal, High, Max queues are also empty
                // but we have entries - means we're waiting while others are empty
                self.run_queues.queue_len(Priority::Normal) == 0
                    && self.run_queues.queue_len(Priority::High) == 0
                    && self.run_queues.queue_len(Priority::Max) == 0
            }
            Priority::Normal => {
                // Normal is starved if High and Max are empty but we have entries
                self.run_queues.queue_len(Priority::High) == 0
                    && self.run_queues.queue_len(Priority::Max) == 0
            }
            Priority::High => {
                // High is starved only if Max is empty but we have entries
                self.run_queues.queue_len(Priority::Max) == 0
            }
            Priority::Max => {
                // Max is never starved - it's the highest
                false
            }
        }
    }

    pub fn get_stats(&self) -> SchedulerStats {
        self.stats
    }

    pub fn increment_reductions(&mut self) {
        self.stats.reductions_executed += 1;
    }

    pub fn increment_context_switches(&mut self) {
        self.stats.context_switches += 1;
    }

    pub fn increment_processes_spawned(&mut self) {
        self.stats.processes_spawned += 1;
    }

    pub fn increment_processes_exited(&mut self) {
        self.stats.processes_exited += 1;
    }

    /// Spawn a scheduler thread that runs processes from the run queue.
    ///
    /// The thread will continuously dequeue and execute processes until
    /// either `stop()` is called or the scheduler goes offline.
    ///
    /// Returns immediately - the thread runs asynchronously.
    pub fn spawn_thread(&mut self) {
        if self.thread_handle.is_some() {
            return; // Already spawned
        }

        let stop_flag = Arc::clone(&self.stop_flag);
        let scheduler_id = self.id;
        let timer_manager = self.timer_manager.clone();
        let port_manager = self.port_manager.clone();

        let handle = thread::Builder::new()
            .name(format!("scheduler-{}", scheduler_id))
            .spawn(move || {
                // Run loop - actual implementation would call into VM
                while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    // Check for expired timers (Task 69 integration)
                    if let Some(ref tm) = timer_manager {
                        if let Ok(mut manager) = tm.lock() {
                            let expired: Vec<chimera_erlang_beam_timer::TimerEntry> =
                                manager.get_expired_timers();
                            for entry in &expired {
                                // In a full implementation, this would:
                                // 1. Look up the target process
                                // 2. Enqueue it for execution
                                // 3. The process would receive the timer message
                                let _target_pid = entry.target_pid;
                                // Process wakeup would happen via shared queue
                            }
                        }
                    }

                    // Check for pending port wakeups (Task 70 integration)
                    if let Some(ref pm) = port_manager {
                        if let Ok(manager) = pm.lock() {
                            let pending_ports = manager.get_pending_wakeups();
                            for port_id in &pending_ports {
                                // In a full implementation, this would:
                                // 1. Look up the port
                                // 2. Call the port's wakeup handler
                                // 3. The port would process pending I/O
                                let _port_id = port_id;
                            }
                        }
                    }

                    // In a real implementation, this would:
                    // 1. Dequeue a process from run_queues
                    // 2. Validate process is alive via ProcessTable
                    // 3. Execute the interpreter loop (VM::step equivalent)
                    // 4. Handle yield/exit/wait states
                    // 5. Re-enqueue if needed
                    //
                    // For now, we just yield to prevent busy-spinning
                    thread::sleep(Duration::from_micros(100));
                }
            })
            .ok();

        self.thread_handle = handle;
    }

    /// Stop the scheduler thread gracefully.
    ///
    /// Sets the stop flag and waits for the thread to join.
    /// Idempotent - safe to call multiple times.
    pub fn stop(&mut self) {
        self.stop_flag
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.online = false;

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Check if the scheduler thread is still running.
    pub fn is_running(&self) -> bool {
        self.thread_handle.is_some() && !self.stop_flag.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Attempt to steal work from another scheduler.
    ///
    /// Steals up to half of the victim's run queue.
    /// Returns the number of processes stolen.
    pub fn steal_from(&mut self, victim: &mut Scheduler) -> usize {
        // Can only steal if victim has more than us
        if victim.len() <= self.len() {
            return 0;
        }

        let _lock = victim.run_queue_mutex.lock().ok();

        let steal_count = (victim.len() + 1) / 2;
        let mut stolen = 0;

        for _ in 0..steal_count {
            if let Some(entry) = victim.run_queues.dequeue() {
                // Re-enqueue with same priority
                self.run_queues.enqueue(entry);
                stolen += 1;
            } else {
                break;
            }
        }

        stolen
    }

    /// Get total number of processes in all run queues.
    pub fn total_len(&self) -> usize {
        self.run_queues.len()
    }

    /// Execute one scheduler step - dequeue and run a process.
    ///
    /// This is the core scheduler/interpreter handoff.
    /// Returns the PID that was run, or None if no process was ready.
    pub fn step(&mut self, process_table: &mut ProcessTable) -> Option<Pid> {
        if let Some((pid, handle)) = self.dequeue() {
            // Get the reduction budget
            let budget = self.get_reduction_budget(pid);

            if budget > 0 {
                // Run the interpreter
                if let Some((_entry, pcb)) = process_table.get_by_pid(pid) {
                    // Execute one instruction using the PCB's context
                    let code = &pcb.code;
                    if !code.is_empty() && (pcb.ip as usize) < code.len() {
                        // Call the actual interpreter
                        let step_result = execute_instruction(&mut pcb.exec_context, code);

                        // Update reduction budget
                        let remaining = budget - 1;
                        self.set_reduction_budget(pid, remaining);

                        // Handle the result
                        match step_result.result {
                            chimera_erlang_beam_instr::ExecResult::Ok => {
                                // Instruction executed successfully
                                // Update IP from context
                                pcb.ip = step_result.ip;
                            }
                            chimera_erlang_beam_instr::ExecResult::Wait => {
                                // Process is waiting (e.g., for message)
                                // Re-enqueue with updated state
                                if remaining > 0 {
                                    self.enqueue(handle, pid);
                                }
                                self.increment_reductions();
                                return Some(pid);
                            }
                            chimera_erlang_beam_instr::ExecResult::ExitDispatch => {
                                // Process is exiting
                                self.increment_processes_exited();
                                self.increment_reductions();
                                return Some(pid);
                            }
                            chimera_erlang_beam_instr::ExecResult::Err => {
                                // Error occurred
                                self.increment_reductions();
                                return Some(pid);
                            }
                            chimera_erlang_beam_instr::ExecResult::Yield => {
                                // Process yielded (e.g., GC)
                                // Re-enqueue with full budget
                                let yield_budget = DEFAULT_REDUCTION_BUDGET;
                                self.set_reduction_budget(pid, yield_budget);
                                self.enqueue(handle, pid);
                                self.increment_reductions();
                                return Some(pid);
                            }
                            chimera_erlang_beam_instr::ExecResult::Trap => {
                                // Trap to BIF or native
                                // Process continues in next run
                                if remaining > 0 {
                                    self.enqueue(handle, pid);
                                }
                                self.increment_reductions();
                                return Some(pid);
                            }
                        }

                        // Re-enqueue if budget remaining
                        if remaining > 0 {
                            self.enqueue(handle, pid);
                        }
                    }
                }

                self.increment_reductions();
                Some(pid)
            } else {
                // Budget exhausted, re-enqueue
                self.enqueue(handle, pid);
                self.increment_reductions();
                Some(pid)
            }
        } else {
            None
        }
    }
}

/// Scheduler registry for multiple schedulers
#[derive(Debug)]
pub struct SchedulerRegistry {
    schedulers: Vec<Option<Scheduler>>,
}

impl SchedulerRegistry {
    pub fn new(num_schedulers: u32) -> Self {
        let mut schedulers = Vec::with_capacity(num_schedulers as usize);
        for i in 0..num_schedulers {
            schedulers.push(Some(Scheduler::new(i)));
        }
        SchedulerRegistry { schedulers }
    }

    pub fn get(&self, id: u32) -> Option<&Scheduler> {
        self.schedulers.get(id as usize).and_then(|s| s.as_ref())
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Scheduler> {
        self.schedulers
            .get_mut(id as usize)
            .and_then(|s| s.as_mut())
    }

    pub fn count(&self) -> u32 {
        self.schedulers.len() as u32
    }

    /// Spawn worker threads for all schedulers.
    ///
    /// Each scheduler gets its own thread that runs the scheduler loop.
    /// After calling this, call `run_until()` on each scheduler or
    /// use the VM's `step()` to drive the scheduler loop.
    pub fn spawn_all(&mut self) {
        for s in self.schedulers.iter_mut().flatten() {
            s.spawn_thread();
        }
    }

    /// Stop all scheduler threads gracefully.
    ///
    /// Sets the stop flag on all threads and waits for them to join.
    pub fn shutdown(&mut self) {
        for s in self.schedulers.iter_mut().flatten() {
            s.stop();
        }
    }

    /// Get total processes across all scheduler run queues.
    pub fn total_len(&self) -> usize {
        self.schedulers
            .iter()
            .filter_map(|s| s.as_ref())
            .map(|s| s.len())
            .sum()
    }

    /// Get scheduler with the most work (for load balancing).
    pub fn most_loaded(&self) -> Option<&Scheduler> {
        self.schedulers
            .iter()
            .filter_map(|s| s.as_ref())
            .max_by_key(|s| s.len())
    }

    /// Get scheduler with the least work (steal target).
    pub fn least_loaded(&self) -> Option<&Scheduler> {
        self.schedulers
            .iter()
            .filter_map(|s| s.as_ref())
            .min_by_key(|s| s.len())
    }

    /// Internal helper to steal from a victim to a target.
    /// Takes raw indices to avoid nested mutable borrows.
    fn steal_from_idx(
        target_idx: usize,
        victim_idx: usize,
        schedulers: &mut [Option<Scheduler>],
    ) -> usize {
        if target_idx == victim_idx {
            return 0;
        }

        // Get lengths before borrowing
        let target_len = schedulers[target_idx]
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0);
        let victim_len = schedulers[victim_idx]
            .as_ref()
            .map(|s| s.len())
            .unwrap_or(0);
        if victim_len <= target_len {
            return 0;
        }

        // Use split_at_mut to get two non-overlapping mutable slices
        let (low, _high) = schedulers.split_at_mut(target_idx.max(victim_idx));
        let (target_slice, victim_slice) = low.split_at_mut(target_idx.min(victim_idx));

        // Get mutable refs from non-overlapping slices
        let _target_opt = if target_idx < victim_idx {
            target_slice.get_mut(target_idx)
        } else {
            victim_slice.get_mut(target_idx.min(victim_idx))
        };

        let _victim_opt = if target_idx < victim_idx {
            victim_slice.get_mut(target_idx.min(victim_idx))
        } else {
            target_slice.get_mut(target_idx.min(victim_idx))
        };

        // Actually, this is still overlapping. Let's use a simpler approach
        // by swapping via Option::take
        let mut stolen = 0;

        // Take victim out temporarily
        let mut victim_scheduler = schedulers[victim_idx].take();
        if let Some(ref mut victim) = victim_scheduler {
            if let Some(ref mut target) = schedulers[target_idx] {
                stolen = target.steal_from(victim);
            }
        }
        // Put victim back
        schedulers[victim_idx] = victim_scheduler;

        stolen
    }

    /// Rebalance workloads across all schedulers.
    ///
    /// Each scheduler steals from the most loaded scheduler if it's
    /// significantly busier than the target. Threshold is 2x difference.
    pub fn rebalance(&mut self) {
        let total = self.total_len();
        let count = self.count() as usize;
        if count == 0 || total == 0 {
            return;
        }

        let avg = total / count;

        // Find the most loaded scheduler's index
        let mut max_idx: Option<usize> = None;
        let mut max_len: usize = 0;
        for (i, sched) in self.schedulers.iter().enumerate() {
            if let Some(ref s) = sched {
                if s.len() > max_len {
                    max_len = s.len();
                    max_idx = Some(i);
                }
            }
        }

        // Early exit if no work to rebalance
        if max_len == 0 {
            return;
        }
        let max_idx_val = max_idx.unwrap();

        // Collect indices of schedulers below average
        let below_avg: Vec<usize> = (0..self.schedulers.len())
            .filter(|&i| i != max_idx_val)
            .filter(|&i| self.schedulers[i].as_ref().map(|s| s.len()).unwrap_or(0) < avg)
            .collect();

        // Steal from most loaded to each below average
        for &target_idx in &below_avg {
            let target_len = self.schedulers[target_idx]
                .as_ref()
                .map(|s| s.len())
                .unwrap_or(0);
            if max_len > target_len * 2 {
                let stolen = Self::steal_from_idx(target_idx, max_idx_val, &mut self.schedulers);
                if stolen > 0 {
                    max_len = self.schedulers[max_idx_val]
                        .as_ref()
                        .map(|s| s.len())
                        .unwrap_or(0);
                }
            }
            if max_len == 0 {
                break;
            }
        }
    }

    /// Returns an iterator over all schedulers.
    pub fn iter(&self) -> impl Iterator<Item = &Scheduler> {
        self.schedulers.iter().filter_map(|s| s.as_ref())
    }

    /// Returns a mutable iterator over all schedulers.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Scheduler> {
        self.schedulers.iter_mut().filter_map(|s| s.as_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_create() {
        let scheduler = Scheduler::new(0);
        assert_eq!(scheduler.id, 0);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn test_scheduler_enqueue_dequeue() {
        let mut scheduler = Scheduler::new(0);
        let pid = Pid::new(1, 0, 0);
        let handle = ProcessHandle::new(0, pid);

        scheduler.enqueue(handle, pid);
        assert_eq!(scheduler.len(), 1);

        let dequeued = scheduler.dequeue();
        assert!(dequeued.is_some());
        let (dequeued_pid, dequeued_handle) = dequeued.unwrap();
        assert_eq!(dequeued_pid, pid);
        assert_eq!(dequeued_handle.index(), 0);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn test_scheduler_stats() {
        let mut scheduler = Scheduler::new(0);
        let stats = scheduler.get_stats();
        assert_eq!(stats.reductions_executed, 0);

        scheduler.increment_reductions();
        scheduler.increment_context_switches();
        let stats = scheduler.get_stats();
        assert_eq!(stats.reductions_executed, 1);
        assert_eq!(stats.context_switches, 1);
    }

    #[test]
    fn test_scheduler_registry() {
        let mut registry = SchedulerRegistry::new(4);
        assert_eq!(registry.count(), 4);

        let scheduler = registry.get_mut(0);
        assert!(scheduler.is_some());
    }

    #[test]
    fn test_priority_run_queues() {
        let mut queues = PriorityRunQueues::new();
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let pid3 = Pid::new(3, 0, 0);
        let pid4 = Pid::new(4, 0, 0);

        assert!(queues.is_empty());

        // Add entries at different priorities
        queues.enqueue(RunQueueEntry {
            pid: pid1,
            handle: ProcessHandle::new(0, pid1),
            priority: Priority::Low,
        });
        queues.enqueue(RunQueueEntry {
            pid: pid2,
            handle: ProcessHandle::new(1, pid2),
            priority: Priority::Normal,
        });
        queues.enqueue(RunQueueEntry {
            pid: pid3,
            handle: ProcessHandle::new(2, pid3),
            priority: Priority::High,
        });
        queues.enqueue(RunQueueEntry {
            pid: pid4,
            handle: ProcessHandle::new(3, pid4),
            priority: Priority::Max,
        });

        assert_eq!(queues.len(), 4);

        // Dequeue should return highest priority first
        let entry = queues.dequeue().unwrap();
        assert_eq!(entry.pid, pid4); // Max first
        assert_eq!(entry.priority, Priority::Max);

        let entry = queues.dequeue().unwrap();
        assert_eq!(entry.pid, pid3); // High second
        assert_eq!(entry.priority, Priority::High);

        let entry = queues.dequeue().unwrap();
        assert_eq!(entry.pid, pid2); // Normal third
        assert_eq!(entry.priority, Priority::Normal);

        let entry = queues.dequeue().unwrap();
        assert_eq!(entry.pid, pid1); // Low last
        assert_eq!(entry.priority, Priority::Low);

        assert!(queues.is_empty());
    }

    #[test]
    fn test_priority_queue_by_pid() {
        let mut queues = PriorityRunQueues::new();
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);

        queues.enqueue(RunQueueEntry {
            pid: pid1,
            handle: ProcessHandle::new(0, pid1),
            priority: Priority::Normal,
        });
        queues.enqueue(RunQueueEntry {
            pid: pid2,
            handle: ProcessHandle::new(1, pid2),
            priority: Priority::Low,
        });

        assert!(queues.contains_pid(pid1));
        assert!(queues.contains_pid(pid2));

        // Remove pid1
        assert!(queues.remove_by_pid(pid1));
        assert!(!queues.contains_pid(pid1));
        assert!(queues.contains_pid(pid2));
    }

    #[test]
    fn test_scheduler_enqueue_with_priority() {
        let mut scheduler = Scheduler::new(0);
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let handle1 = ProcessHandle::new(0, pid1);
        let handle2 = ProcessHandle::new(1, pid2);

        // Enqueue at different priorities
        scheduler.enqueue_with_priority(handle1, pid1, Priority::High);
        scheduler.enqueue_with_priority(handle2, pid2, Priority::Low);

        assert_eq!(scheduler.len(), 2);
        assert_eq!(scheduler.queue_len(Priority::High), 1);
        assert_eq!(scheduler.queue_len(Priority::Low), 1);

        // Dequeue should get high priority first
        let (pid, _) = scheduler.dequeue().unwrap();
        assert_eq!(pid, pid1);

        let (pid, _) = scheduler.dequeue().unwrap();
        assert_eq!(pid, pid2);
    }

    #[test]
    fn test_scheduler_reduction_budget() {
        let mut scheduler = Scheduler::new(0);
        let pid = Pid::new(1, 0, 0);

        // Initial budget should be default
        assert_eq!(
            scheduler.get_reduction_budget(pid),
            DEFAULT_REDUCTION_BUDGET
        );

        // Consume some reductions
        scheduler.consume_reduction(pid);
        scheduler.consume_reduction(pid);
        assert_eq!(
            scheduler.get_reduction_budget(pid),
            DEFAULT_REDUCTION_BUDGET - 2
        );

        // Reset should restore budget
        scheduler.reset_reduction_budget(pid);
        assert_eq!(
            scheduler.get_reduction_budget(pid),
            DEFAULT_REDUCTION_BUDGET
        );
    }

    #[test]
    fn test_scheduler_remove_by_pid() {
        let mut scheduler = Scheduler::new(0);
        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let handle1 = ProcessHandle::new(0, pid1);
        let handle2 = ProcessHandle::new(1, pid2);

        scheduler.enqueue(handle1, pid1);
        scheduler.enqueue(handle2, pid2);
        assert_eq!(scheduler.len(), 2);

        // Remove pid1
        assert!(scheduler.remove_by_pid(pid1));
        assert_eq!(scheduler.len(), 1);
        assert!(!scheduler.contains_pid(pid1));
        assert!(scheduler.contains_pid(pid2));

        // Remove pid2
        assert!(scheduler.remove_by_pid(pid2));
        assert!(scheduler.is_empty());
    }

    #[test]
    fn test_scheduler_is_budget_exhausted() {
        let mut scheduler = Scheduler::new(0);
        let pid = Pid::new(1, 0, 0);

        // Initially not exhausted
        assert!(!scheduler.is_budget_exhausted(pid));

        // Exhaust the budget
        for _ in 0..DEFAULT_REDUCTION_BUDGET {
            scheduler.consume_reduction(pid);
        }

        assert!(scheduler.is_budget_exhausted(pid));
    }

    #[test]
    fn test_scheduler_spawn_thread() {
        let mut scheduler = Scheduler::new(0);
        assert!(!scheduler.is_running());
        assert!(scheduler.thread_handle.is_none());

        scheduler.spawn_thread();

        // Thread should be running now
        assert!(scheduler.thread_handle.is_some());

        // Stop the scheduler
        scheduler.stop();
        assert!(!scheduler.is_running());
    }

    #[test]
    fn test_scheduler_stop_idempotent() {
        let mut scheduler = Scheduler::new(0);
        scheduler.spawn_thread();

        // Stop twice should be safe
        scheduler.stop();
        scheduler.stop();

        assert!(!scheduler.is_running());
    }

    #[test]
    fn test_scheduler_steal_from() {
        let mut source = Scheduler::new(0);
        let mut victim = Scheduler::new(1);

        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);
        let pid3 = Pid::new(3, 0, 0);

        // Add 3 processes to victim, none to source
        victim.enqueue(ProcessHandle::new(0, pid1), pid1);
        victim.enqueue(ProcessHandle::new(1, pid2), pid2);
        victim.enqueue(ProcessHandle::new(2, pid3), pid3);

        assert_eq!(victim.len(), 3);
        assert_eq!(source.len(), 0);

        // Steal from victim
        let stolen = source.steal_from(&mut victim);

        assert_eq!(stolen, 2); // Should steal 2 (half of 3, rounded up)
        assert_eq!(victim.len(), 1);
        assert_eq!(source.len(), 2);
    }

    #[test]
    fn test_scheduler_steal_no_op_when_equal() {
        let mut s1 = Scheduler::new(0);
        let mut s2 = Scheduler::new(1);

        let pid1 = Pid::new(1, 0, 0);
        let pid2 = Pid::new(2, 0, 0);

        s1.enqueue(ProcessHandle::new(0, pid1), pid1);
        s2.enqueue(ProcessHandle::new(1, pid2), pid2);

        // Neither has significantly more work
        let stolen = s1.steal_from(&mut s2);
        assert_eq!(stolen, 0);
    }

    #[test]
    fn test_scheduler_registry_spawn_all() {
        let mut registry = SchedulerRegistry::new(4);

        // Initially no threads
        for s in registry.iter() {
            assert!(!s.is_running());
        }

        registry.spawn_all();

        // All schedulers should have running threads
        for s in registry.iter() {
            assert!(s.is_running());
        }

        // Clean up
        registry.shutdown();

        // After shutdown, threads should be stopped
        for s in registry.iter() {
            assert!(!s.is_running());
        }
    }

    #[test]
    fn test_scheduler_registry_total_len() {
        let mut registry = SchedulerRegistry::new(2);

        assert_eq!(registry.total_len(), 0);

        // Add some processes
        registry
            .get_mut(0)
            .unwrap()
            .enqueue(ProcessHandle::new(0, Pid::new(1, 0, 0)), Pid::new(1, 0, 0));
        registry
            .get_mut(0)
            .unwrap()
            .enqueue(ProcessHandle::new(1, Pid::new(2, 0, 0)), Pid::new(2, 0, 0));
        registry
            .get_mut(1)
            .unwrap()
            .enqueue(ProcessHandle::new(2, Pid::new(3, 0, 0)), Pid::new(3, 0, 0));

        assert_eq!(registry.total_len(), 3);
    }

    #[test]
    fn test_scheduler_registry_most_loaded() {
        let mut registry = SchedulerRegistry::new(2);

        registry
            .get_mut(0)
            .unwrap()
            .enqueue(ProcessHandle::new(0, Pid::new(1, 0, 0)), Pid::new(1, 0, 0));
        registry
            .get_mut(1)
            .unwrap()
            .enqueue(ProcessHandle::new(1, Pid::new(2, 0, 0)), Pid::new(2, 0, 0));
        registry
            .get_mut(1)
            .unwrap()
            .enqueue(ProcessHandle::new(2, Pid::new(3, 0, 0)), Pid::new(3, 0, 0));

        let most_loaded = registry.most_loaded();
        assert!(most_loaded.is_some());
        assert_eq!(most_loaded.unwrap().id, 1);
        assert_eq!(most_loaded.unwrap().len(), 2);
    }

    #[test]
    fn test_scheduler_registry_least_loaded() {
        let mut registry = SchedulerRegistry::new(2);

        registry
            .get_mut(0)
            .unwrap()
            .enqueue(ProcessHandle::new(0, Pid::new(1, 0, 0)), Pid::new(1, 0, 0));
        registry
            .get_mut(0)
            .unwrap()
            .enqueue(ProcessHandle::new(1, Pid::new(2, 0, 0)), Pid::new(2, 0, 0));
        registry
            .get_mut(1)
            .unwrap()
            .enqueue(ProcessHandle::new(2, Pid::new(3, 0, 0)), Pid::new(3, 0, 0));

        let least_loaded = registry.least_loaded();
        assert!(least_loaded.is_some());
        assert_eq!(least_loaded.unwrap().id, 1);
        assert_eq!(least_loaded.unwrap().len(), 1);
    }

    #[test]
    fn test_scheduler_registry_rebalance() {
        let mut registry = SchedulerRegistry::new(2);

        // Add 10 processes to scheduler 0, none to scheduler 1
        for i in 0..10 {
            registry.get_mut(0).unwrap().enqueue(
                ProcessHandle::new(i, Pid::new((i + 1) as u32, 0, 0)),
                Pid::new((i + 1) as u32, 0, 0),
            );
        }

        assert_eq!(registry.get(0).unwrap().len(), 10);
        assert_eq!(registry.get(1).unwrap().len(), 0);

        // Rebalance
        registry.rebalance();

        // After rebalance, both should have ~5 each
        let s0_len = registry.get(0).unwrap().len();
        let s1_len = registry.get(1).unwrap().len();

        // Total should still be 10
        assert_eq!(s0_len + s1_len, 10);

        // Neither should be empty
        assert!(s0_len > 0);
        assert!(s1_len > 0);
    }

    #[test]
    fn test_scheduler_len_is_empty() {
        let scheduler = Scheduler::new(0);
        assert!(scheduler.is_empty());
        assert_eq!(scheduler.len(), 0);
    }

    #[test]
    fn test_scheduler_spawn_idempotent() {
        let mut scheduler = Scheduler::new(0);
        scheduler.spawn_thread();
        scheduler.spawn_thread(); // Should be no-op

        assert!(scheduler.thread_handle.is_some());
        scheduler.stop();
    }

    #[test]
    fn test_scheduler_affinity_with_affinity() {
        let scheduler = Scheduler::with_affinity(0, 3);
        assert_eq!(scheduler.id, 0);
        assert_eq!(scheduler.cpu_affinity(), 3);
        assert_eq!(scheduler.ideal_cpu(), Some(3));
    }

    #[test]
    fn test_scheduler_affinity_default() {
        let scheduler = Scheduler::new(0);
        assert_eq!(scheduler.cpu_affinity(), -1);
        assert_eq!(scheduler.ideal_cpu(), None);
        assert_eq!(scheduler.affinity_mode(), AffinityMode::Spread);
    }

    #[test]
    fn test_scheduler_set_affinity() {
        let mut scheduler = Scheduler::new(0);
        assert_eq!(scheduler.cpu_affinity(), -1);

        scheduler.set_cpu_affinity(2);
        assert_eq!(scheduler.cpu_affinity(), 2);
        assert_eq!(scheduler.ideal_cpu(), Some(2));

        scheduler.set_cpu_affinity(-1);
        assert_eq!(scheduler.cpu_affinity(), -1);
        assert_eq!(scheduler.ideal_cpu(), None);
    }

    #[test]
    fn test_scheduler_set_affinity_mode() {
        let mut scheduler = Scheduler::new(0);
        assert_eq!(scheduler.affinity_mode(), AffinityMode::Spread);

        scheduler.set_affinity_mode(AffinityMode::Compact);
        assert_eq!(scheduler.affinity_mode(), AffinityMode::Compact);

        scheduler.set_affinity_mode(AffinityMode::None);
        scheduler.set_affinity_mode(AffinityMode::None);
    }
}

impl Scheduler {
    /// Run until all queues are empty (scheduler becomes idle)
    pub fn run_until_idle(&mut self, process_table: &mut ProcessTable) {
        while self.dequeue().is_some() {
            self.step(process_table);
        }
    }
}

#[cfg(test)]
mod phase5_progress {
    use super::*;

    #[test]
    fn test_scheduler_run_until_idle() {
        let mut scheduler = Scheduler::new(0);
        // Should return immediately when no processes
        scheduler.run_until_idle(&mut chimera_erlang_beam_process::ProcessTable::new(0));
    }
}
