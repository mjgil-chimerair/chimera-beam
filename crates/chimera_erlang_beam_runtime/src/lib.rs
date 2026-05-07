//! RustZigBeam runtime library.
//!
//! Provides the runtime services for the RustZigBeam VM.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_core::RuntimeConfig;
use chimera_erlang_beam_scheduler::SchedulerRegistry;
use chimera_erlang_beam_vm::{boot::BootConfig, VirtualMachine};
use std::ffi::CStr;
use std::os::raw::c_char;

/// Initialize runtime with configuration
pub fn init(config: &RuntimeConfig) -> RuntimeHandle {
    RuntimeHandle {
        config: config.clone(),
        started: false,
    }
}

/// Runtime handle
#[derive(Debug)]
pub struct RuntimeHandle {
    config: RuntimeConfig,
    started: bool,
}

impl RuntimeHandle {
    pub fn start(&mut self) {
        self.started = true;
    }

    pub fn stop(&mut self) {
        self.started = false;
    }

    pub fn is_running(&self) -> bool {
        self.started
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }
}

/// Print usage information
pub fn print_usage() {
    eprintln!("RustZigBeam - BEAM-like runtime in Rust");
    eprintln!();
    eprintln!("Usage: rustzigbeam [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -n <node>      Node name (default: rustzigbeam@localhost)");
    eprintln!("  -s <n>         Number of schedulers (default: 1)");
    eprintln!("  -h <size>      Heap size in words (default: 8192)");
    eprintln!("  -boot <path>   Boot script path (default: minimal)");
    eprintln!("  -pa <path>     Add path to module search path");
    eprintln!("  --help         Show this help message");
}

/// Module search paths
#[derive(Debug, Clone)]
pub struct ModulePaths {
    paths: Vec<String>,
}

impl ModulePaths {
    pub fn new() -> Self {
        ModulePaths { paths: Vec::new() }
    }

    pub fn add(&mut self, path: &str) {
        self.paths.push(path.to_string());
    }
}

impl Default for ModulePaths {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse command line arguments
pub fn parse_args_from<I>(args: I) -> (RuntimeConfig, BootConfig, ModulePaths)
where
    I: IntoIterator<Item = String>,
{
    let mut config = RuntimeConfig::default();
    let boot_config = BootConfig::minimal();
    let mut module_paths = ModulePaths::new();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-n" => {
                if let Some(node) = args.next() {
                    config.node_name = node;
                }
            }
            "-s" => {
                if let Some(schedulers) = args.next() {
                    if let Ok(n) = schedulers.parse() {
                        config.schedulers = n;
                    }
                }
            }
            "-h" => {
                if let Some(size) = args.next() {
                    if let Ok(n) = size.parse() {
                        config.heap_size = n;
                    }
                }
            }
            "-boot" => {
                if let Some(path) = args.next() {
                    eprintln!(
                        "Note: -boot {} specified (boot script loading is E-3)",
                        path
                    );
                }
            }
            "-pa" => {
                if let Some(path) = args.next() {
                    module_paths.add(&path);
                    eprintln!("Added module path: {}", path);
                }
            }
            "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown option: {}", arg);
                print_usage();
                std::process::exit(1);
            }
        }
    }

    (config, boot_config, module_paths)
}

/// Run the VM
pub fn run_vm(config: &RuntimeConfig, boot_config: &BootConfig) -> Result<(), String> {
    println!("Starting RustZigBeam node: {}", config.node_name);
    println!("Schedulers: {}", config.schedulers);
    println!("Heap size: {} words", config.heap_size);
    println!();

    let scheduler_registry = SchedulerRegistry::new(config.schedulers);
    let _vm = VirtualMachine::new(0);

    println!("Boot phase: {:?}", boot_config.phase());
    println!("Loading modules...");
    println!("VM initialized successfully");
    println!("Schedulers running: {}", scheduler_registry.count());

    let scheduler = scheduler_registry.get(0);
    if let Some(s) = scheduler {
        println!("Scheduler 0: {} processes in queue", s.len());
    }

    Ok(())
}

pub fn cli_main_from<I>(args: I) -> i32
where
    I: IntoIterator<Item = String>,
{
    let (config, boot_config, _module_paths) = parse_args_from(args);

    match run_vm(&config, &boot_config) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}

pub fn cli_main() -> i32 {
    cli_main_from(std::env::args().skip(1))
}

#[unsafe(no_mangle)]
pub extern "C" fn chimera_beam_runtime_entry(argc: i32, argv: *const *const c_char) -> i32 {
    let argc = argc.max(0) as usize;
    let mut args = Vec::with_capacity(argc);

    if !argv.is_null() {
        for idx in 0..argc {
            let ptr = unsafe { *argv.add(idx) };
            if ptr.is_null() {
                continue;
            }
            let value = unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned();
            args.push(value);
        }
    }

    cli_main_from(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_init() {
        let config = RuntimeConfig::default();
        let handle = init(&config);
        assert!(!handle.is_running());
    }

    #[test]
    fn test_runtime_start_stop() {
        let config = RuntimeConfig::default();
        let mut handle = init(&config);

        handle.start();
        assert!(handle.is_running());

        handle.stop();
        assert!(!handle.is_running());
    }

    #[test]
    fn test_cli_entry_defaults() {
        assert_eq!(cli_main_from(Vec::<String>::new()), 0);
    }
}
