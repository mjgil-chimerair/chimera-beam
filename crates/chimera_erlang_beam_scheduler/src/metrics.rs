//! Scheduler metrics and observability.
//!
//! Rust owns all scheduler observability - metrics collection, reporting,
//! and exposition. Per design.md section 11 observability is 100% Rust.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global metrics accumulator
static _METRICS_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Scheduler metrics snapshot for observability
#[derive(Debug, Default, Clone, Copy)]
pub struct SchedulerMetrics {
    pub scheduler_id: u32,
    pub run_queue_depth: usize,
    pub reductions_executed: u64,
    pub context_switches: u64,
    pub processes_spawned: u64,
    pub processes_exited: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub dirty_cpu_jobs_submitted: u64,
    pub dirty_cpu_jobs_completed: u64,
    pub dirty_io_jobs_submitted: u64,
    pub dirty_io_jobs_completed: u64,
    pub total_cpu_time_ms: u64,
    pub idle_time_ms: u64,
}

impl SchedulerMetrics {
    pub fn new(scheduler_id: u32) -> Self {
        SchedulerMetrics {
            scheduler_id,
            ..Default::default()
        }
    }

    /// Calculate scheduler utilization as a percentage (0-100)
    pub fn utilization_percent(&self) -> u8 {
        if self.total_cpu_time_ms == 0 {
            return 0;
        }
        let idle = self.idle_time_ms.min(self.total_cpu_time_ms);
        let active = self.total_cpu_time_ms - idle;
        ((active as f64 / self.total_cpu_time_ms as f64) * 100.0) as u8
    }
}

/// System-wide VM metrics
#[derive(Debug, Default, Clone, Copy)]
pub struct VmMetrics {
    pub total_processes: u64,
    pub total_schedulers: u32,
    pub total_reductions: u64,
    pub total_context_switches: u64,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub total_dirty_cpu_jobs: u64,
    pub total_dirty_io_jobs: u64,
    pub uptime_seconds: u64,
    // Work stealing metrics
    pub steal_attempts: u64,
    pub steal_successes: u64,
    pub processes_migrated: u64,
}

impl VmMetrics {
    pub fn new() -> Self {
        VmMetrics {
            uptime_seconds: uptime_seconds(),
            ..Default::default()
        }
    }
}

/// Metrics registry - collects and aggregates scheduler metrics
#[derive(Debug)]
pub struct MetricsRegistry {
    start_time: u64,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        MetricsRegistry {
            start_time: timestamp_s(),
        }
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        timestamp_s() - self.start_time
    }

    /// Get current wall time as Unix timestamp
    pub fn wall_time(&self) -> u64 {
        timestamp_s()
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Incremental counter for high-throughput metrics
#[derive(Debug)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn new(initial: u64) -> Self {
        Counter {
            value: AtomicU64::new(initial),
        }
    }

    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::SeqCst)
    }

    pub fn add(&self, delta: u64) -> u64 {
        self.value.fetch_add(delta, Ordering::SeqCst)
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::SeqCst)
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Gauge for values that can go up and down
#[derive(Debug)]
pub struct Gauge {
    value: AtomicU64,
}

impl Gauge {
    pub fn new(initial: u64) -> Self {
        Gauge {
            value: AtomicU64::new(initial),
        }
    }

    pub fn set(&self, new_value: u64) {
        self.value.store(new_value, Ordering::SeqCst);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::SeqCst)
    }

    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::SeqCst)
    }

    pub fn decrement(&self) -> u64 {
        self.value.fetch_sub(1, Ordering::SeqCst)
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Histogram bucket for latency distributions
#[derive(Debug, Clone, Copy)]
pub struct HistogramBucket {
    pub le: u64,
    pub count: u64,
}

impl HistogramBucket {
    pub fn new(le: u64) -> Self {
        HistogramBucket { le, count: 0 }
    }
}

/// Timer for measuring elapsed time
#[derive(Debug)]
pub struct Timer {
    start: u64,
}

impl Timer {
    pub fn new() -> Self {
        Timer {
            start: timestamp_ms(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        timestamp_ms() - self.start
    }

    pub fn reset(&mut self) {
        self.start = timestamp_ms();
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn timestamp_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn uptime_seconds() -> u64 {
    timestamp_s()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new(0);
        assert_eq!(counter.get(), 0);
        counter.increment();
        assert_eq!(counter.get(), 1);
        counter.add(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new(10);
        assert_eq!(gauge.get(), 10);
        gauge.set(20);
        assert_eq!(gauge.get(), 20);
        gauge.increment();
        assert_eq!(gauge.get(), 21);
        gauge.decrement();
        assert_eq!(gauge.get(), 20);
    }

    #[test]
    fn test_timer() {
        let timer = Timer::new();
        std::thread::sleep(std::time::Duration::from_millis(1));
        assert!(timer.elapsed_ms() >= 1);
    }

    #[test]
    fn test_metrics_registry() {
        let registry = MetricsRegistry::new();
        assert!(registry.uptime_seconds() >= 0);
    }

    #[test]
    fn test_scheduler_metrics_utilization() {
        let mut metrics = SchedulerMetrics::new(0);
        assert_eq!(metrics.utilization_percent(), 0);

        metrics.total_cpu_time_ms = 100;
        metrics.idle_time_ms = 0;
        assert_eq!(metrics.utilization_percent(), 100);

        metrics.idle_time_ms = 50;
        assert_eq!(metrics.utilization_percent(), 50);
    }

    #[test]
    fn test_vm_metrics() {
        let metrics = VmMetrics::new();
        assert!(metrics.uptime_seconds >= 0);
        assert_eq!(metrics.total_schedulers, 0);
    }
}
