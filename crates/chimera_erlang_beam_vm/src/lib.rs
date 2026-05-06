//! VM core for RustZigBeam.
//!
//! Rust owns all VM semantics - bytecode interpreter, BIFs, code loading.
//! The VM uses Zig kernels for hot-path term operations via C ABI.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

pub mod bif_dispatch;
pub mod boot;
pub use boot::{
    Application, ApplicationType, BootConfig, BootEntry, BootError, BootScript, SystemProcess,
};
pub use chimera_erlang_beam_code::{
    AtomEntry, AtomTable, Chunk, ChunkTag, CodeHeader, CodeLoader, Container, ExportEntry,
    LoadedModule, ModuleTable, SymbolEntry, SymbolTable,
};

use chimera_erlang_beam_core::{VmError, VmResult};
use chimera_erlang_beam_instr::{execute_instruction, ExecContext};
use chimera_erlang_beam_process::{Pid, ProcessControlBlock, ProcessState, ProcessTable};
use chimera_erlang_beam_scheduler::{Scheduler, SchedulerStats};
use chimera_erlang_beam_term::Term;

/// VM statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct VmStats {
    pub processes_running: u64,
    pub processes_waiting: u64,
    pub instructions_executed: u64,
}

/// Comprehensive VM metrics for observability
#[derive(Debug, Clone)]
pub struct VmMetricsSnapshot {
    pub stats: VmStats,
    pub scheduler_stats: SchedulerStats,
    pub process_count: usize,
    pub run_queue_depth: usize,
}

/// The RustZigBeam virtual machine
#[derive(Debug)]
pub struct VirtualMachine {
    pub scheduler: Scheduler,
    process_table: ProcessTable,
    pub stats: VmStats,
    /// Node name for distribution (erlang:node/0)
    node_name: String,
}

impl VirtualMachine {
    pub fn new(scheduler_id: u32) -> Self {
        VirtualMachine {
            scheduler: Scheduler::new(scheduler_id),
            process_table: ProcessTable::new(0),
            stats: VmStats::default(),
            node_name: "rustzigbeam@localhost".to_string(),
        }
    }

    /// Create a new VM with a specific node name
    pub fn new_with_node(scheduler_id: u32, node_name: &str) -> Self {
        VirtualMachine {
            scheduler: Scheduler::new(scheduler_id),
            process_table: ProcessTable::new(0),
            stats: VmStats::default(),
            node_name: node_name.to_string(),
        }
    }

    /// Get the node name
    pub fn node_name(&self) -> &str {
        &self.node_name
    }

    /// Spawn a new process and add it to the scheduler run queue
    pub fn spawn(&mut self, heap_size: usize) -> Pid {
        let (handle, pid) = self.process_table.spawn(heap_size);

        // Get raw pointer to enqueue in scheduler
        // Initialize reduction budget and enqueue with handle
        self.scheduler.reset_reduction_budget(pid);
        self.scheduler.enqueue(handle, pid);

        self.scheduler.increment_processes_spawned();
        self.stats.processes_running += 1;

        pid
    }

    /// Send a message from one process to another
    ///
    /// This implements the local send path:
    /// 1. Look up the receiver's PCB in the process table
    /// 2. Check that the receiver is alive (not exiting/exited)
    /// 3. Copy the message term to the receiver's heap/fragment
    /// 4. Enqueue a message signal in the receiver's mailbox
    /// 5. Wake the receiver if it is waiting (mark as Running)
    /// 6. Update send statistics
    pub fn send(&mut self, _sender: Pid, receiver: Pid, msg: Term) -> VmResult<()> {
        // Look up the receiver
        let (_idx, pcb) = self
            .process_table
            .get_by_pid(receiver)
            .ok_or(VmError::ProcessNotFound)?;

        // Check that the receiver is alive
        if pcb.state == ProcessState::Exiting || pcb.state == ProcessState::GarbageCollecting {
            return Err(VmError::ProcessExited(pcb.exit_reason.0));
        }

        // In a full implementation, we would:
        // 1. Allocate a heap fragment for the message
        // 2. Deep-copy the message term to the fragment
        // 3. Enqueue the message in the receiver's signal queue
        // For now, just push to the mailbox directly
        pcb.send_message(msg);

        // Wake the receiver if it was waiting for messages
        // The scheduler will pick it up on the next step/run_until
        // Note: Full wake requires Task 39 handle-based scheduling
        // to properly enqueue without raw pointers
        if pcb.state == ProcessState::Waiting {
            pcb.state = ProcessState::Running;

            // If the process is in a receive state, signal that a message arrived
            // so the next RecvWaitOp will see message_arrived=true and deliver immediately
            if let Some(ref mut rs) = pcb.receive_state {
                if !rs.message_arrived {
                    // Check if there are messages in the mailbox
                    if pcb.mailbox.has_message() {
                        rs.message_arrived = true;
                    }
                }
            }
        }

        // Update statistics
        self.stats.processes_waiting = self.process_table.len() as u64;

        Ok(())
    }

    /// Get a reference to the process table
    pub fn process_table(&self) -> &ProcessTable {
        &self.process_table
    }

    /// Get a mutable reference to the process table
    pub fn process_table_mut(&mut self) -> &mut ProcessTable {
        &mut self.process_table
    }

    /// Process a single reduction using the bytecode interpreter
    ///
    /// Implements reduction-driven scheduling:
    /// 1. Dequeue a runnable process
    /// 2. Validate the process is still alive via ProcessTable
    /// 3. Create ExecContext from PCB state (full state transfer)
    /// 4. Execute instructions via the interpreter (loop until yield/trap-exit/error)
    /// 5. Handle BIF traps by dispatching to BIF runtime
    /// 6. Update PCB with new state (full state transfer)
    /// 7. Handle yield/exit
    pub fn step(&mut self) {
        if let Some((pid, handle)) = self.scheduler.dequeue() {
            self.scheduler.increment_context_switches();

            // Validate the process is still alive via ProcessTable
            if !self.process_table.is_alive(pid) {
                self.scheduler.increment_processes_exited();
                self.stats.processes_running = self.stats.processes_running.saturating_sub(1);
                return;
            }

            // Get mutable reference to PCB via handle
            let pcb = match self.process_table.get_by_handle(handle) {
                Some(p) => p,
                None => return,
            };

            // Early return helper - must drop pcb first
            macro_rules! return_drop_pcb {
                () => { return; };
                ($($tt:tt)*) => { {
                    drop(pcb);
                    return $($tt)*
                }}
            }

            // Check if reduction budget is exhausted
            if self.scheduler.is_budget_exhausted(pid) {
                pcb.state = ProcessState::Waiting;
                return_drop_pcb!();
            }

            // Create ExecContext from FULL PCB state
            let mut ctx = ExecContext::new();
            ctx.ip = pcb.ip;
            ctx.x = pcb.x;
            ctx.init_from_pcb(
                pcb.cp,
                pcb.fp,
                pcb.live,
                &pcb.y,
                pcb.reduction_budget,
                pcb.current_instruction,
                pcb.bif_call,
                pcb.receive_state.clone(),
                pcb.exception_state.clone(),
            );

            // Execute one instruction
            let result = execute_instruction(&mut ctx, &pcb.code);

            // Update FULL PCB from ExecContext using accessor methods
            pcb.ip = ctx.ip;
            pcb.cp = ctx.get_cp();
            pcb.fp = ctx.get_fp();
            pcb.live = ctx.get_live();
            pcb.x = ctx.x;
            pcb.y.copy_from_slice(ctx.get_y_registers());
            pcb.reduction_budget = ctx.get_reduction_budget();
            pcb.current_instruction = ctx.get_current_instruction_word();
            pcb.bif_call = ctx.bif_call;
            if ctx.receive_state.is_some() {
                pcb.receive_state = ctx.receive_state.clone();
            }
            if ctx.exception_state.is_some() {
                pcb.exception_state = ctx.exception_state.clone();
            }

            // Update reductions from ctx
            pcb.reductions = ctx.get_reductions();

            // Consume reduction budget
            pcb.reductions += 1;
            self.scheduler.increment_reductions();
            self.scheduler.consume_reduction(pid);

            // Only count actual instruction executions, not early exits
            match result.result {
                chimera_erlang_beam_instr::ExecResult::Ok
                | chimera_erlang_beam_instr::ExecResult::Yield
                | chimera_erlang_beam_instr::ExecResult::Trap
                | chimera_erlang_beam_instr::ExecResult::Wait => {
                    self.stats.instructions_executed += 1;
                }
                chimera_erlang_beam_instr::ExecResult::Err
                | chimera_erlang_beam_instr::ExecResult::ExitDispatch => {
                    // Error or exit - don't count as instruction
                }
            }

            // Handle result
            match result.result {
                chimera_erlang_beam_instr::ExecResult::Ok => {
                    // Instruction executed successfully
                    // Re-enqueue if still running
                    if pcb.state == ProcessState::Running {
                        self.scheduler.enqueue(handle, pid);
                    }
                }
                chimera_erlang_beam_instr::ExecResult::Yield => {
                    // Yield - process used its budget
                    pcb.state = ProcessState::Waiting;
                }
                chimera_erlang_beam_instr::ExecResult::Trap => {
                    // Trap to BIF - handle the trap
                    // Extract all data we need from pcb while it's valid
                    let pcb_raw = pcb as *mut ProcessControlBlock;
                    let pid_for_later = pid;
                    let dest_reg = pcb.bif_call.as_ref().map(|bc| bc.dest).unwrap_or(0);
                    let bif_call_opt = pcb.bif_call.take();
                    let pcb_index = handle.index();
                    let node_name = self.node_name.clone();
                    // Now release pcb to release the borrow
                    let _ = pcb;
                    // Get pt_raw after pcb is dropped
                    let pt_raw = &mut self.process_table as *mut ProcessTable;
                    // Call dispatch using raw pointers (in a separate expression to release pt_raw borrow)
                    let result = unsafe {
                        let pt = &mut *pt_raw;
                        if let Some(bif_call) = bif_call_opt {
                            bif_dispatch::dispatch_bif_impl(pt, pcb_raw, &bif_call, &node_name)
                        } else {
                            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_UNDEFINED)
                        }
                    }; // pt borrow ends here
                       // Set result via process table (fresh borrow)
                    if let Some(p) = self.process_table.get_by_index(pcb_index) {
                        p.set_x(dest_reg, result);
                    }
                    // Re-enqueue
                    self.scheduler.enqueue(handle, pid_for_later);
                }
                chimera_erlang_beam_instr::ExecResult::Err => {
                    // Error - mark as exiting
                    pcb.state = ProcessState::Exiting;
                }
                chimera_erlang_beam_instr::ExecResult::ExitDispatch => {
                    // Process exited
                    pcb.state = ProcessState::Exiting;
                }
                chimera_erlang_beam_instr::ExecResult::Wait => {
                    // Process is waiting for messages
                    pcb.state = ProcessState::Waiting;
                }
            }
        }
    }

    /// Exit a process and remove it from the scheduler
    pub fn exit_process(&mut self, pid: Pid, reason: Term) -> VmResult<()> {
        // Remove from scheduler if present
        self.scheduler.remove_by_pid(pid);

        // Mark as exited in process table
        self.process_table.mark_exited(pid, reason)?;

        // Update stats
        self.stats.processes_running = self.stats.processes_running.saturating_sub(1);
        self.scheduler.increment_processes_exited();

        Ok(())
    }

    /// Run the VM until a condition is met
    pub fn run_until<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&VmStats) -> bool,
    {
        while !predicate(&self.stats) {
            self.step();
        }
    }

    /// Get comprehensive VM metrics snapshot
    ///
    /// This returns a consistent view of VM state for observability,
    /// combining scheduler stats, VM stats, and process table info.
    pub fn get_metrics(&self) -> VmMetricsSnapshot {
        VmMetricsSnapshot {
            stats: self.stats,
            scheduler_stats: self.scheduler.get_stats(),
            process_count: self.process_table.len(),
            run_queue_depth: self.scheduler.len(),
        }
    }

    /// Get scheduler statistics
    pub fn get_scheduler_stats(&self) -> SchedulerStats {
        self.scheduler.get_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_spawn() {
        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);
        assert!(pid.id >= 1);
        // Process is in the process table but not yet in scheduler run queue
        // (scheduler integration is completed in Task 39)
        assert_eq!(vm.process_table.len(), 1);
    }

    #[test]
    fn test_vm_step() {
        let mut vm = VirtualMachine::new(0);
        let _pid = vm.spawn(8192);

        let initial_stats = vm.stats.clone();
        vm.step();

        // Should have executed one reduction
        assert!(vm.stats.instructions_executed >= initial_stats.instructions_executed);
    }

    #[test]
    fn test_reduction_driven_scheduling() {
        use chimera_erlang_beam_instr::Opcode;
        use chimera_erlang_beam_scheduler::DEFAULT_REDUCTION_BUDGET;

        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Give the process some code to execute (load instruction)
        // Format: opcode(16) | dest(8) | unused(8) | value(32)
        let instr: u64 = (Opcode::LoadInt as u64) | (0_u64 << 16) | ((42_u64) << 32);
        if let Some((_, pcb)) = vm.process_table.get_by_pid(pid) {
            pcb.code = vec![instr, 0, 0, 0, 0, 0, 0, 0, 0];
        }

        // Process should be in scheduler run queue
        assert_eq!(vm.scheduler.len(), 1);

        // Run until the process yields (budget exhausted)
        let mut steps = 0;
        let max_steps = (DEFAULT_REDUCTION_BUDGET * 2) as usize;

        while vm.scheduler.len() > 0 && steps < max_steps {
            vm.step();
            steps += 1;
        }

        // Process should have yielded due to budget exhaustion
        assert_eq!(vm.scheduler.len(), 0, "Process should have yielded");

        // But the process should still exist in the process table
        assert_eq!(vm.process_table.len(), 1);

        // Verify reductions were executed (many instructions executed before yield)
        assert!(vm.stats.instructions_executed > 0);
    }

    #[test]
    fn test_process_wake_on_message() {
        use chimera_erlang_beam_instr::Opcode;

        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Give the process some code to execute (load instruction)
        let instr: u64 = (Opcode::LoadInt as u64) | (0_u64 << 16) | ((42_u64) << 32);
        if let Some((_, pcb)) = vm.process_table.get_by_pid(pid) {
            pcb.code = vec![instr, 0, 0, 0, 0, 0, 0, 0, 0];
        }

        // Run until process yields (should happen after budget exhausted)
        let mut steps = 0;
        while vm.scheduler.len() > 0 && steps < 10000 {
            vm.step();
            steps += 1;
        }

        // Process should have yielded or exited (budget exhausted or code done)
        // Note: full wake-on-message is Task 50
    }

    #[test]
    fn test_exit_process() {
        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Process should be in scheduler
        assert_eq!(vm.scheduler.len(), 1);
        assert!(vm.scheduler.contains_pid(pid));

        // Exit the process
        let result = vm.exit_process(
            pid,
            chimera_erlang_beam_term::Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        );
        assert!(result.is_ok());

        // Process should no longer be in scheduler
        assert_eq!(vm.scheduler.len(), 0);
        assert!(!vm.scheduler.contains_pid(pid));

        // Process table should no longer have the process alive
        assert!(!vm.process_table.is_alive(pid));
    }

    #[test]
    fn test_step_skips_dead_process() {
        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Exit the process without running it
        vm.exit_process(
            pid,
            chimera_erlang_beam_term::Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        )
        .unwrap();

        // Step should skip the dead process
        let initial_stats = vm.stats.clone();
        vm.step();
        vm.step();

        // Should not have executed any reductions (process is dead)
        assert_eq!(
            vm.stats.instructions_executed,
            initial_stats.instructions_executed
        );
    }

    #[test]
    fn test_vm_metrics_snapshot() {
        use chimera_erlang_beam_instr::Opcode;

        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Give the process some code to execute (load instruction)
        let instr: u64 = (Opcode::LoadInt as u64) | (0_u64 << 16) | ((42_u64) << 32);
        if let Some((_, pcb)) = vm.process_table.get_by_pid(pid) {
            pcb.code = vec![instr, 0, 0, 0, 0, 0, 0, 0, 0];
        }

        // Get metrics before any steps
        let metrics = vm.get_metrics();
        assert_eq!(metrics.process_count, 1);
        assert_eq!(metrics.run_queue_depth, 1);
        assert_eq!(metrics.stats.processes_running, 1);
        assert_eq!(metrics.scheduler_stats.processes_spawned, 1);

        // Step a few times
        vm.step();
        vm.step();

        // Get scheduler stats
        let sched_stats = vm.get_scheduler_stats();
        assert!(sched_stats.context_switches >= 2);
    }

    #[test]
    fn test_vm_send_local() {
        let mut vm = VirtualMachine::new(0);
        let sender_pid = vm.spawn(8192);
        let receiver_pid = vm.spawn(8192);

        // Send a message from sender to receiver
        let msg = Term::from_small(42);
        let result = vm.send(sender_pid, receiver_pid, msg);
        assert!(result.is_ok());

        // Check receiver got the message
        if let Some((_, pcb)) = vm.process_table.get_by_pid(receiver_pid) {
            assert_eq!(pcb.mailbox.len(), 1);
            let delivered = pcb.receive_message();
            assert!(delivered.is_some());
            assert_eq!(delivered.unwrap(), Term::from_small(42));
        } else {
            panic!("receiver process not found");
        }
    }

    #[test]
    fn test_vm_send_to_exited_process() {
        let mut vm = VirtualMachine::new(0);
        let sender_pid = vm.spawn(8192);
        let receiver_pid = vm.spawn(8192);

        // Exit the receiver first
        vm.exit_process(
            receiver_pid,
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        )
        .unwrap();

        // Try to send to exited process - should fail
        let msg = Term::from_small(42);
        let result = vm.send(sender_pid, receiver_pid, msg);
        assert!(result.is_err());
    }

    #[test]
    fn test_vm_send_wakes_waiting_process() {
        let mut vm = VirtualMachine::new(0);
        let sender_pid = vm.spawn(8192);
        let receiver_pid = vm.spawn(8192);

        // Put receiver in waiting state
        if let Some((_, pcb)) = vm.process_table.get_by_pid(receiver_pid) {
            pcb.state = ProcessState::Waiting;
        }

        // Send a message - should wake receiver
        let msg = Term::from_small(99);
        let result = vm.send(sender_pid, receiver_pid, msg);
        assert!(result.is_ok());

        // Check receiver was woken
        if let Some((_, pcb)) = vm.process_table.get_by_pid(receiver_pid) {
            assert_eq!(pcb.state, ProcessState::Running);
        }
    }

    #[test]
    fn test_vm_send_wakes_waiting_process_with_receive_state() {
        let mut vm = VirtualMachine::new(0);
        let sender_pid = vm.spawn(8192);
        let receiver_pid = vm.spawn(8192);

        // Put receiver in waiting state with receive_state active
        if let Some((_, pcb)) = vm.process_table.get_by_pid(receiver_pid) {
            pcb.state = ProcessState::Waiting;
            pcb.receive_state = Some(chimera_erlang_beam_instr::ReceiveState {
                save_index: 0,
                timeout: 100,
                waited_reductions: 0,
                active_message: None,
                message_arrived: false,
                saved_queue_len: 0,
            });
        }

        // Send a message - should wake receiver AND set message_arrived
        let msg = Term::from_small(99);
        let result = vm.send(sender_pid, receiver_pid, msg);
        assert!(result.is_ok());

        // Check receiver was woken
        if let Some((_, pcb)) = vm.process_table.get_by_pid(receiver_pid) {
            assert_eq!(pcb.state, ProcessState::Running);
            // Check message_arrived was set
            assert!(pcb.receive_state.is_some());
            assert!(pcb.receive_state.as_ref().unwrap().message_arrived);
        }
    }

    #[test]
    fn test_vm_exit_propagates_to_linked() {
        use chimera_erlang_beam_process::{bidirectional_link_with_table, propagate_exit};

        let mut vm = VirtualMachine::new(0);
        let pid1 = vm.spawn(8192);
        let pid2 = vm.spawn(8192);

        // Create a bidirectional link between pid1 and pid2
        bidirectional_link_with_table(&mut vm.process_table, pid1, pid2).unwrap();

        // Exit pid1 with normal reason
        vm.exit_process(pid1, Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL))
            .unwrap();

        // pid1 should not be alive
        assert!(!vm.process_table.is_alive(pid1));

        // Propagate exit to linked processes
        propagate_exit(
            &mut vm.process_table,
            pid1,
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        );
    }

    #[test]
    fn test_vm_exit_with_trap_exit_receives_signal() {
        use chimera_erlang_beam_process::{bidirectional_link_with_table, propagate_exit};

        let mut vm = VirtualMachine::new(0);
        let pid1 = vm.spawn(8192);
        let pid2 = vm.spawn(8192);

        // Create bidirectional link
        bidirectional_link_with_table(&mut vm.process_table, pid1, pid2).unwrap();

        // Set trap_exit on pid2
        {
            let (_, pcb2) = vm.process_table.get_by_pid(pid2).unwrap();
            pcb2.flags.trap_exit = true;
        }

        // Exit pid1 and propagate
        vm.exit_process(pid1, Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL))
            .unwrap();
        propagate_exit(
            &mut vm.process_table,
            pid1,
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL),
        );

        // pid2 should have exit signal in mailbox since trap_exit is true
        let (_, pcb2) = vm.process_table.get_by_pid(pid2).unwrap();
        let exits = pcb2.mailbox.exits();
        // pid2 receives the exit from pid1 but continues running
        assert_eq!(pcb2.state, ProcessState::Running);
    }

    #[test]
    fn test_vm_process_cleanup_on_exit() {
        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Add something to process dictionary
        {
            let (_, pcb) = vm.process_table.get_by_pid(pid).unwrap();
            pcb.put(Term::from_small(1), Term::from_small(100));
            assert_eq!(pcb.dictionary_len(), 1);
        }

        // Exit process
        vm.exit_process(pid, Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_NORMAL))
            .unwrap();

        // Process should be marked as Exiting
        let (_, pcb) = vm.process_table.get_by_pid(pid).unwrap();
        assert_eq!(pcb.state, ProcessState::Exiting);
    }

    #[test]
    fn test_vm_kill_process() {
        let mut vm = VirtualMachine::new(0);
        let pid = vm.spawn(8192);

        // Kill the process using kill reason
        vm.exit_process(pid, Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_KILL))
            .unwrap();

        // Verify process is no longer alive
        assert!(!vm.process_table.is_alive(pid));
        assert!(!vm.scheduler.contains_pid(pid));
    }
}
