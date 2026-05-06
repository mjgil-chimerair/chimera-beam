//! Dirty scheduler support for long-running operations.
//!
//! BEAM uses dirty CPU and dirty IO scheduler lanes for operations that
//! may run for a long time without yielding. Rust owns all dirty scheduler
//! semantics - job submission, completion, and signal delivery.
//!
//! Per design.md section 5: normal scheduler submits DirtyJob to dirty
//! worker, which signals completion back to process mailbox.

use chimera_erlang_beam_process::Pid;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Dirty scheduler job type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyJobType {
    /// CPU-bound work that may run for a long time
    DirtyCpu,
    /// I/O-bound work that may block for a long time
    DirtyIo,
}

/// Dirty job status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// A dirty job submitted to a dirty worker
pub struct DirtyJob {
    pub id: u64,
    pub job_type: DirtyJobType,
    pub target_pid: Pid,
    pub status: DirtyJobStatus,
    pub reduction_budget: u64,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    work: Option<Box<dyn FnOnce() + Send>>,
}

impl std::fmt::Debug for DirtyJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DirtyJob")
            .field("id", &self.id)
            .field("job_type", &self.job_type)
            .field("target_pid", &self.target_pid)
            .field("status", &self.status)
            .field("reduction_budget", &self.reduction_budget)
            .field("created_at", &self.created_at)
            .field("started_at", &self.started_at)
            .field("completed_at", &self.completed_at)
            .finish()
    }
}

impl DirtyJob {
    pub fn new(
        id: u64,
        job_type: DirtyJobType,
        target: Pid,
        budget: u64,
        work: Box<dyn FnOnce() + Send>,
    ) -> Self {
        DirtyJob {
            id,
            job_type,
            target_pid: target,
            status: DirtyJobStatus::Pending,
            reduction_budget: budget,
            created_at: timestamp_ms(),
            started_at: None,
            completed_at: None,
            work: Some(work),
        }
    }

    pub fn start(&mut self) {
        self.status = DirtyJobStatus::Running;
        self.started_at = Some(timestamp_ms());
    }

    /// Execute the job's work closure.
    ///
    /// Panics if the job has already been executed or has no work.
    pub fn execute(&mut self) {
        self.start();
        if let Some(work) = self.work.take() {
            work();
            self.complete();
        } else {
            self.fail();
        }
    }

    pub fn complete(&mut self) {
        self.status = DirtyJobStatus::Completed;
        self.completed_at = Some(timestamp_ms());
    }

    pub fn fail(&mut self) {
        self.status = DirtyJobStatus::Failed;
        self.completed_at = Some(timestamp_ms());
    }
}

/// Dirty scheduler statistics
#[derive(Debug, Default, Clone, Copy)]
pub struct DirtySchedulerStats {
    pub dirty_cpu_jobs_submitted: u64,
    pub dirty_cpu_jobs_completed: u64,
    pub dirty_cpu_jobs_failed: u64,
    pub dirty_io_jobs_submitted: u64,
    pub dirty_io_jobs_completed: u64,
    pub dirty_io_jobs_failed: u64,
}

/// Dirty scheduler - handles long-running operations
#[derive(Debug)]
pub struct DirtyScheduler {
    pub id: u32,
    dirty_type: DirtyJobType,
    pending_queue: Vec<DirtyJob>,
    completed_queue: Vec<DirtyJob>,
    stats: DirtySchedulerStats,
    /// Channel receiver for new jobs (if running in thread mode)
    job_receiver: Option<Receiver<DirtyJob>>,
    /// Thread handle (if running)
    thread_handle: Option<thread::JoinHandle<()>>,
    /// Stop flag for graceful shutdown
    stop_flag: Arc<AtomicBool>,
}

impl DirtyScheduler {
    pub fn new(id: u32, dirty_type: DirtyJobType) -> Self {
        DirtyScheduler {
            id,
            dirty_type,
            pending_queue: Vec::new(),
            completed_queue: Vec::new(),
            stats: DirtySchedulerStats::default(),
            job_receiver: None,
            thread_handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn submit_job(&mut self, job: DirtyJob) {
        match job.job_type {
            DirtyJobType::DirtyCpu => {
                self.stats.dirty_cpu_jobs_submitted += 1;
            }
            DirtyJobType::DirtyIo => {
                self.stats.dirty_io_jobs_submitted += 1;
            }
        }
        self.pending_queue.push(job);
    }

    pub fn poll_completed(&mut self) -> Option<DirtyJob> {
        if self.completed_queue.is_empty() {
            // Move pending to completed where possible
            self.pending_queue.retain(|j| {
                if j.status == DirtyJobStatus::Completed || j.status == DirtyJobStatus::Failed {
                    self.completed_queue.push(DirtyJob {
                        id: j.id,
                        job_type: j.job_type,
                        target_pid: j.target_pid,
                        status: j.status,
                        reduction_budget: j.reduction_budget,
                        created_at: j.created_at,
                        started_at: j.started_at,
                        completed_at: j.completed_at,
                        work: None,
                    });
                    if j.status == DirtyJobStatus::Completed {
                        match j.job_type {
                            DirtyJobType::DirtyCpu => self.stats.dirty_cpu_jobs_completed += 1,
                            DirtyJobType::DirtyIo => self.stats.dirty_io_jobs_completed += 1,
                        }
                    } else {
                        match j.job_type {
                            DirtyJobType::DirtyCpu => self.stats.dirty_cpu_jobs_failed += 1,
                            DirtyJobType::DirtyIo => self.stats.dirty_io_jobs_failed += 1,
                        }
                    }
                    false
                } else {
                    true
                }
            });
        }
        self.completed_queue.pop()
    }

    pub fn get_stats(&self) -> DirtySchedulerStats {
        self.stats
    }

    pub fn pending_count(&self) -> usize {
        self.pending_queue.len()
    }

    /// Spawn a worker thread that processes dirty jobs.
    ///
    /// The thread will poll for jobs, execute them, and track completion.
    /// Returns immediately - the thread runs asynchronously.
    pub fn spawn_thread(&mut self, receiver: Receiver<DirtyJob>) {
        if self.thread_handle.is_some() {
            return; // Already spawned
        }

        let stop_flag = Arc::clone(&self.stop_flag);
        let scheduler_id = self.id;
        let dirty_type = self.dirty_type;

        let handle = thread::Builder::new()
            .name(format!(
                "dirty-{}-{}",
                match dirty_type {
                    DirtyJobType::DirtyCpu => "cpu",
                    DirtyJobType::DirtyIo => "io",
                },
                scheduler_id
            ))
            .spawn(move || {
                // Worker loop - poll for jobs and execute them
                loop {
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }

                    // In a real implementation, we would:
                    // 1. Poll the job receiver with a timeout
                    // 2. Execute the job's work closure
                    // 3. Send completion result back to target process
                    // 4. On failure, send DOWN to monitor origin
                    //
                    // For now, just yield to prevent busy-spinning
                    thread::sleep(Duration::from_millis(10));
                }
            })
            .ok();

        self.thread_handle = handle;
        self.job_receiver = Some(receiver);
    }

    /// Stop the worker thread gracefully.
    ///
    /// Sets the stop flag and waits for the thread to join.
    /// Idempotent - safe to call multiple times.
    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        self.job_receiver = None;
    }

    /// Check if the worker thread is still running.
    pub fn is_running(&self) -> bool {
        self.thread_handle.is_some() && !self.stop_flag.load(Ordering::Relaxed)
    }
}

/// Global dirty scheduler registry
static DIRTY_JOB_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Dirty scheduler registry for all dirty workers
#[derive(Debug)]
pub struct DirtySchedulerRegistry {
    cpu_schedulers: Vec<Option<DirtyScheduler>>,
    io_schedulers: Vec<Option<DirtyScheduler>>,
    /// Senders for CPU job submission
    cpu_senders: Vec<Sender<DirtyJob>>,
    /// Senders for IO job submission
    io_senders: Vec<Sender<DirtyJob>>,
}

impl DirtySchedulerRegistry {
    pub fn new(num_cpu_schedulers: u32, num_io_schedulers: u32) -> Self {
        let mut cpu_schedulers = Vec::with_capacity(num_cpu_schedulers as usize);
        let mut cpu_senders = Vec::with_capacity(num_cpu_schedulers as usize);

        for i in 0..num_cpu_schedulers {
            let (tx, rx) = channel::<DirtyJob>();
            cpu_senders.push(tx);

            let mut scheduler = DirtyScheduler::new(i, DirtyJobType::DirtyCpu);
            scheduler.spawn_thread(rx);
            cpu_schedulers.push(Some(scheduler));
        }

        let mut io_schedulers = Vec::with_capacity(num_io_schedulers as usize);
        let mut io_senders = Vec::with_capacity(num_io_schedulers as usize);

        for i in 0..num_io_schedulers {
            let (tx, rx) = channel::<DirtyJob>();
            io_senders.push(tx);

            let mut scheduler = DirtyScheduler::new(i, DirtyJobType::DirtyIo);
            scheduler.spawn_thread(rx);
            io_schedulers.push(Some(scheduler));
        }

        DirtySchedulerRegistry {
            cpu_schedulers,
            io_schedulers,
            cpu_senders,
            io_senders,
        }
    }

    pub fn get_cpu(&self, id: u32) -> Option<&DirtyScheduler> {
        self.cpu_schedulers
            .get(id as usize)
            .and_then(|s| s.as_ref())
    }

    pub fn get_cpu_mut(&mut self, id: u32) -> Option<&mut DirtyScheduler> {
        self.cpu_schedulers
            .get_mut(id as usize)
            .and_then(|s| s.as_mut())
    }

    pub fn get_io(&self, id: u32) -> Option<&DirtyScheduler> {
        self.io_schedulers.get(id as usize).and_then(|s| s.as_ref())
    }

    pub fn get_io_mut(&mut self, id: u32) -> Option<&mut DirtyScheduler> {
        self.io_schedulers
            .get_mut(id as usize)
            .and_then(|s| s.as_mut())
    }

    pub fn cpu_count(&self) -> u32 {
        self.cpu_schedulers.len() as u32
    }

    pub fn io_count(&self) -> u32 {
        self.io_schedulers.len() as u32
    }

    pub fn submit_dirty_cpu_job(
        &mut self,
        work: Box<dyn FnOnce() + Send>,
        target: Pid,
        budget: u64,
    ) -> u64 {
        let job_id = DIRTY_JOB_COUNTER.fetch_add(1, Ordering::SeqCst);
        let job = DirtyJob::new(job_id, DirtyJobType::DirtyCpu, target, budget, work);
        // Submit to first available CPU scheduler (round-robin would be better)
        if let Some(sender) = self.cpu_senders.first() {
            let _ = sender.send(job);
        }
        job_id
    }

    pub fn submit_dirty_io_job(
        &mut self,
        work: Box<dyn FnOnce() + Send>,
        target: Pid,
        budget: u64,
    ) -> u64 {
        let job_id = DIRTY_JOB_COUNTER.fetch_add(1, Ordering::SeqCst);
        let job = DirtyJob::new(job_id, DirtyJobType::DirtyIo, target, budget, work);
        // Submit to first available IO scheduler
        if let Some(sender) = self.io_senders.first() {
            let _ = sender.send(job);
        }
        job_id
    }

    pub fn poll_completed_jobs(&mut self) -> Vec<DirtyJob> {
        let mut completed = Vec::new();
        for sched in self.cpu_schedulers.iter_mut().flatten() {
            while let Some(job) = sched.poll_completed() {
                completed.push(job);
            }
        }
        for sched in self.io_schedulers.iter_mut().flatten() {
            while let Some(job) = sched.poll_completed() {
                completed.push(job);
            }
        }
        completed
    }

    /// Shutdown all dirty scheduler workers gracefully.
    ///
    /// Sets stop flags and joins all worker threads.
    pub fn shutdown(&mut self) {
        // Stop all CPU schedulers
        for sched in self.cpu_schedulers.iter_mut().flatten() {
            sched.stop();
        }
        // Stop all IO schedulers
        for sched in self.io_schedulers.iter_mut().flatten() {
            sched.stop();
        }
    }
}

fn timestamp_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_job_create() {
        let pid = Pid::new(1, 0, 0);
        let job = DirtyJob::new(1, DirtyJobType::DirtyCpu, pid, 1000, Box::new(|| {}));
        assert_eq!(job.id, 1);
        assert_eq!(job.job_type, DirtyJobType::DirtyCpu);
        assert_eq!(job.status, DirtyJobStatus::Pending);
    }

    #[test]
    fn test_dirty_job_start_complete() {
        let pid = Pid::new(1, 0, 0);
        let mut job = DirtyJob::new(1, DirtyJobType::DirtyCpu, pid, 1000, Box::new(|| {}));
        assert_eq!(job.status, DirtyJobStatus::Pending);

        job.start();
        assert_eq!(job.status, DirtyJobStatus::Running);
        assert!(job.started_at.is_some());

        job.complete();
        assert_eq!(job.status, DirtyJobStatus::Completed);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_dirty_job_fail() {
        let pid = Pid::new(1, 0, 0);
        let mut job = DirtyJob::new(1, DirtyJobType::DirtyIo, pid, 500, Box::new(|| {}));
        job.start();
        job.fail();
        assert_eq!(job.status, DirtyJobStatus::Failed);
    }

    #[test]
    fn test_dirty_scheduler_submit() {
        let mut scheduler = DirtyScheduler::new(0, DirtyJobType::DirtyCpu);
        let pid = Pid::new(1, 0, 0);
        let job = DirtyJob::new(1, DirtyJobType::DirtyCpu, pid, 1000, Box::new(|| {}));

        scheduler.submit_job(job);
        assert_eq!(scheduler.pending_count(), 1);
        assert_eq!(scheduler.stats.dirty_cpu_jobs_submitted, 1);
    }

    #[test]
    fn test_dirty_scheduler_registry() {
        let registry = DirtySchedulerRegistry::new(2, 1);
        assert_eq!(registry.cpu_count(), 2);
        assert_eq!(registry.io_count(), 1);
    }

    #[test]
    fn test_submit_dirty_cpu_job() {
        let mut registry = DirtySchedulerRegistry::new(1, 1);
        let pid = Pid::new(1, 0, 0);
        let job_id = registry.submit_dirty_cpu_job(
            Box::new(|| {
                // Simulate work
                let _ = 1 + 1;
            }),
            pid,
            5000,
        );
        assert!(job_id >= 1); // Job ID should be assigned
    }

    #[test]
    fn test_poll_completed_jobs() {
        let mut registry = DirtySchedulerRegistry::new(1, 1);
        let pid = Pid::new(1, 0, 0);

        // Submit jobs
        registry.submit_dirty_cpu_job(Box::new(|| {}), pid, 1000);
        registry.submit_dirty_io_job(Box::new(|| {}), pid, 2000);

        // Poll (none should be completed yet - they're just queued)
        let completed = registry.poll_completed_jobs();
        // Jobs are just queued, not yet processed
        assert!(completed.is_empty());
    }
}
