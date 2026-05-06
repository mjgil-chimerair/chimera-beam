//! BIF dispatch helpers for VM
//!
//! Separated from lib.rs to avoid borrow checker issues when calling
//! from within impl VirtualMachine method that holds &mut self.

use chimera_erlang_beam_bif::BifContext;
use chimera_erlang_beam_process::{BifCall, ProcessControlBlock, ProcessTable};
use chimera_erlang_beam_term::Term;

/// Dispatch a BIF call and return the result
///
/// Uses raw pointers to break the borrow chain at the call site,
/// allowing us to pass &mut ProcessControlBlock and &mut ProcessTable
/// separately even though they refer to overlapping data.
///
/// # Safety
///
/// The caller must ensure that:
/// - `process_table` points to a valid, properly initialized `ProcessTable`
/// - `pcb` points to a valid, properly initialized `ProcessControlBlock`
/// - The `ProcessControlBlock` must be valid for the duration of this call
/// - The pointers must be properly aligned and not dangling
pub unsafe fn dispatch_bif_impl(
    process_table: *mut ProcessTable,
    pcb: *mut ProcessControlBlock,
    bif_call: &BifCall,
    node_name: &str,
) -> Term {
    // Collect arguments based on arg count
    let arg1 = if bif_call.arg1 > 0 {
        (*pcb).get_x(bif_call.arg1)
    } else {
        Term::nil()
    };
    let arg2 = if bif_call.arg2 > 0 {
        (*pcb).get_x(bif_call.arg2)
    } else {
        Term::nil()
    };

    // Build args slice
    let args: &[Term] = if bif_call.arg2 > 0 {
        &[arg1, arg2]
    } else if bif_call.arg1 > 0 {
        &[arg1]
    } else {
        &[]
    };

    // Extract PID before borrowing PCB for BifContext
    let pid = (*pcb).pid;

    // Create BIF context using raw pointers
    let mut bif_ctx = BifContext {
        process: &mut *pcb,
        process_table: &mut *process_table,
        pid,
        node_name,
    };

    // Dispatch based on BIF ID
    match bif_call.bif_id {
        0 => chimera_erlang_beam_bif::bif_self(&mut bif_ctx, args),
        1 => chimera_erlang_beam_bif::bif_spawn(&mut bif_ctx, args),
        2 => chimera_erlang_beam_bif::bif_send(&mut bif_ctx, args),
        3 => chimera_erlang_beam_bif::bif_exit(&mut bif_ctx, args),
        4 => chimera_erlang_beam_bif::bif_link(&mut bif_ctx, args),
        5 => chimera_erlang_beam_bif::bif_unlink(&mut bif_ctx, args),
        6 => chimera_erlang_beam_bif::bif_monitor(&mut bif_ctx, args),
        7 => chimera_erlang_beam_bif::bif_demonitor(&mut bif_ctx, args),
        8 => chimera_erlang_beam_bif::bif_make_ref(&mut bif_ctx, args),
        9 => chimera_erlang_beam_bif::bif_get(&mut bif_ctx, args),
        10 => chimera_erlang_beam_bif::bif_put(&mut bif_ctx, args),
        11 => chimera_erlang_beam_bif::bif_registered(&mut bif_ctx, args),
        12 => chimera_erlang_beam_bif::bif_whereis(&mut bif_ctx, args),
        13 => chimera_erlang_beam_bif::bif_register(&mut bif_ctx, args),
        14 => chimera_erlang_beam_bif::bif_process_info(&mut bif_ctx, args),
        15 => chimera_erlang_beam_bif::bif_spawn_link(&mut bif_ctx, args),
        16 => chimera_erlang_beam_bif::bif_spawn_monitor(&mut bif_ctx, args),
        17 => chimera_erlang_beam_bif::bif_exit_reason(&mut bif_ctx, args),
        18 => chimera_erlang_beam_bif::bif_kill(&mut bif_ctx, args),
        19 => chimera_erlang_beam_bif::bif_trace(&mut bif_ctx, args),
        20 => chimera_erlang_beam_bif::bif_trace_pattern(&mut bif_ctx, args),
        21 => chimera_erlang_beam_bif::bif_now(&mut bif_ctx, args),
        22 => chimera_erlang_beam_bif::bif_node(&mut bif_ctx, args),
        23 => chimera_erlang_beam_bif::bif_nodes(&mut bif_ctx, args),
        _ => Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_UNDEFINED),
    }
}
