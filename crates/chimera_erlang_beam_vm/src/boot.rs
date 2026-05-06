//! OTP boot support for RustZigBeam.
//!
//! Rust owns the boot process - loading boot scripts, starting the initial
//! system process, and initializing standard library BIFs.
//!
//! Per design.md section 13 item 15.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chimera_erlang_beam_core::{VmError, VmResult};
use chimera_erlang_beam_process::{Pid, ProcessControlBlock, ProcessState};
use chimera_erlang_beam_term::Term;

/// Application descriptor
#[derive(Debug, Clone)]
pub struct Application {
    pub name: String,
    pub version: String,
    pub description: String,
    pub modules: Vec<String>,
    pub type_: ApplicationType,
}

impl Application {
    pub fn new(name: &str, version: &str) -> Self {
        Application {
            name: name.to_string(),
            version: version.to_string(),
            description: String::new(),
            modules: Vec::new(),
            type_: ApplicationType::default(),
        }
    }

    pub fn with_modules(mut self, modules: Vec<&str>) -> Self {
        self.modules = modules.iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }
}

/// Boot script entry
#[derive(Debug, Clone)]
pub enum BootEntry {
    /// Start an application
    StartApplication { app: String, type_: ApplicationType },
    /// Load a module
    LoadModule { module: String },
    /// Run a command
    Run {
        module: String,
        function: String,
        args: Vec<Term>,
    },
    /// Set process flag
    SetFlag { flag: String, value: Term },
    /// Register a name
    RegisterName { name: String, pid: Pid },
}

impl BootEntry {
    pub fn load_module(module: &str) -> Self {
        BootEntry::LoadModule {
            module: module.to_string(),
        }
    }

    pub fn start_app(app: &str, type_: ApplicationType) -> Self {
        BootEntry::StartApplication {
            app: app.to_string(),
            type_,
        }
    }

    pub fn run(module: &str, function: &str, args: Vec<Term>) -> Self {
        BootEntry::Run {
            module: module.to_string(),
            function: function.to_string(),
            args,
        }
    }
}

/// Application type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApplicationType {
    /// Permanent application - restarts if it terminates
    #[default]
    Permanent,
    /// Transient application - only restarts if it terminates abnormally
    Transient,
    /// Temporary application - never restarts
    Temporary,
    /// Load-only application - loaded but not started
    Load,
}

/// Boot phase tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    /// Initial state
    Starting,
    /// Loading modules
    LoadingModules,
    /// Starting applications
    StartingApplications,
    /// Running final boot sequence
    RunningBoot,
    /// Boot complete
    Ready,
    /// Boot failed
    Failed,
}

/// Boot script
#[derive(Debug, Clone)]
pub struct BootScript {
    pub entries: Vec<BootEntry>,
    pub applications: HashMap<String, Application>,
}

impl BootScript {
    pub fn new() -> Self {
        BootScript {
            entries: Vec::new(),
            applications: HashMap::new(),
        }
    }

    pub fn add_entry(&mut self, entry: BootEntry) {
        self.entries.push(entry);
    }

    pub fn add_application(&mut self, app: Application) {
        self.applications.insert(app.name.clone(), app);
    }

    pub fn get_application(&self, name: &str) -> Option<&Application> {
        self.applications.get(name)
    }

    pub fn load_module(&mut self, module: &str) {
        self.add_entry(BootEntry::load_module(module));
    }

    pub fn start_application(&mut self, app: &str, type_: ApplicationType) {
        self.add_entry(BootEntry::start_app(app, type_));
    }

    pub fn run(&mut self, module: &str, function: &str, args: Vec<Term>) {
        self.add_entry(BootEntry::run(module, function, args));
    }
}

impl Default for BootScript {
    fn default() -> Self {
        Self::new()
    }
}

/// Boot configuration
#[derive(Debug, Clone)]
pub struct BootConfig {
    pub boot_script: BootScript,
    pub system_apps: Vec<String>,
    pub initial_pcb_size: usize,
    pub boot_phase: BootPhase,
}

impl BootConfig {
    pub fn new() -> Self {
        BootConfig {
            boot_script: BootScript::new(),
            system_apps: Vec::new(),
            initial_pcb_size: 8192,
            boot_phase: BootPhase::Starting,
        }
    }

    /// Create the default minimal boot configuration
    pub fn minimal() -> Self {
        let mut config = BootConfig::new();
        config.boot_script.load_module("init");
        config.boot_phase = BootPhase::LoadingModules;
        config
    }

    /// Create the standard OTP boot configuration
    pub fn standard() -> Self {
        let mut config = BootConfig::new();
        // Standard apps that OTP starts
        config.system_apps = vec!["kernel".to_string(), "stdlib".to_string()];
        config.initial_pcb_size = 16384;
        config.boot_phase = BootPhase::Starting;

        // Standard boot sequence
        config.boot_script.load_module("init");
        config.boot_script.load_module("otp_ring0");
        config
            .boot_script
            .start_application("kernel", ApplicationType::Permanent);
        config
            .boot_script
            .start_application("stdlib", ApplicationType::Permanent);

        config
    }

    pub fn with_app(mut self, app: Application) -> Self {
        self.boot_script.add_application(app);
        self
    }

    /// Start a kernel application (minimal system process)
    pub fn with_kernel_app(mut self) -> Self {
        let kernel = Application::new("kernel", "4.0")
            .with_description("ERTS kernel")
            .with_modules(vec![
                "init",
                "kernel",
                "erts",
                "error_handler",
                "file_server",
            ]);
        self.boot_script.add_application(kernel);
        self.system_apps.push("kernel".to_string());
        self
    }

    /// Start stdlib application
    pub fn with_stdlib_app(mut self) -> Self {
        let stdlib = Application::new("stdlib", "4.0")
            .with_description("ERTS standard library")
            .with_modules(vec!["supervisor", "gen_server", "gen_event"]);
        self.boot_script.add_application(stdlib);
        self.system_apps.push("stdlib".to_string());
        self
    }

    pub fn phase(&self) -> BootPhase {
        self.boot_phase
    }

    pub fn set_phase(&mut self, phase: BootPhase) {
        self.boot_phase = phase;
    }
}

impl Default for BootConfig {
    fn default() -> Self {
        Self::minimal()
    }
}

/// Initial system process creation
#[derive(Debug)]
pub struct SystemProcess {
    pub pid: Pid,
    pub pcb: ProcessControlBlock,
    pub applications: Vec<String>,
    pub boot_config: BootConfig,
}

impl SystemProcess {
    pub fn new(pid: Pid, config: &BootConfig) -> Self {
        let pcb = ProcessControlBlock::new(pid, config.initial_pcb_size);
        SystemProcess {
            pid,
            pcb,
            applications: config.system_apps.clone(),
            boot_config: config.clone(),
        }
    }

    /// Execute the boot sequence and return events
    pub fn start_boot_sequence(&mut self) -> VmResult<Vec<BootEvent>> {
        let mut events = Vec::new();

        // Set system process state - it's running boot code
        self.pcb.state = ProcessState::Running;

        for (event_id, entry) in (0_u32..).zip(&self.boot_config.boot_script.entries) {
            let event = match entry {
                BootEntry::LoadModule { module } => {
                    self.pcb.state = ProcessState::Running;
                    BootEvent::LoadModule {
                        id: event_id,
                        module: module.clone(),
                    }
                }
                BootEntry::StartApplication { app, type_ } => BootEvent::StartApplication {
                    id: event_id,
                    application: app.clone(),
                    type_: *type_,
                },
                BootEntry::Run {
                    module,
                    function,
                    args,
                } => BootEvent::RunFunction {
                    id: event_id,
                    module: module.clone(),
                    function: function.clone(),
                    args: args.clone(),
                },
                BootEntry::SetFlag { flag, value } => BootEvent::SetFlag {
                    id: event_id,
                    flag: flag.clone(),
                    value: *value,
                },
                BootEntry::RegisterName { name, pid } => BootEvent::RegisterName {
                    id: event_id,
                    name: name.clone(),
                    pid: *pid,
                },
            };
            events.push(event);
        }

        // Mark boot as complete
        self.boot_config.set_phase(BootPhase::Ready);

        Ok(events)
    }

    /// Mark boot as complete
    pub fn boot_complete(&mut self) {
        self.boot_config.set_phase(BootPhase::Ready);
    }

    /// Get the boot phase
    pub fn phase(&self) -> BootPhase {
        self.boot_config.phase()
    }
}

/// Boot event for observability
#[derive(Debug, Clone)]
pub enum BootEvent {
    LoadModule {
        id: u32,
        module: String,
    },
    StartApplication {
        id: u32,
        application: String,
        type_: ApplicationType,
    },
    RunFunction {
        id: u32,
        module: String,
        function: String,
        args: Vec<Term>,
    },
    SetFlag {
        id: u32,
        flag: String,
        value: Term,
    },
    RegisterName {
        id: u32,
        name: String,
        pid: Pid,
    },
}

impl BootEvent {
    pub fn id(&self) -> u32 {
        match self {
            BootEvent::LoadModule { id, .. } => *id,
            BootEvent::StartApplication { id, .. } => *id,
            BootEvent::RunFunction { id, .. } => *id,
            BootEvent::SetFlag { id, .. } => *id,
            BootEvent::RegisterName { id, .. } => *id,
        }
    }
}

/// Runtime services available during boot
pub trait BootServices {
    fn load_module(&mut self, module: &str) -> VmResult<()>;
    fn register_name(&mut self, name: &str, pid: Pid) -> VmResult<()>;
    fn start_application(&mut self, app: &str, type_: ApplicationType) -> VmResult<()>;
}

/// Boot loader that manages the boot process
pub struct BootLoader {
    pub config: BootConfig,
    pub system_process: Option<SystemProcess>,
    pub modules_loaded: Vec<String>,
}

impl BootLoader {
    pub fn new(config: BootConfig) -> Self {
        BootLoader {
            config,
            system_process: None,
            modules_loaded: Vec::new(),
        }
    }

    /// Create a minimal boot loader
    pub fn minimal() -> Self {
        Self::new(BootConfig::minimal())
    }

    /// Create a standard OTP boot loader
    pub fn standard() -> Self {
        Self::new(BootConfig::standard())
    }

    /// Initialize the system process
    pub fn init_system_process(&mut self, pid: Pid) -> VmResult<()> {
        self.system_process = Some(SystemProcess::new(pid, &self.config));
        Ok(())
    }

    /// Get the system process PID
    pub fn system_pid(&self) -> Option<Pid> {
        self.system_process.as_ref().map(|sp| sp.pid)
    }

    /// Run the boot sequence
    pub fn run_boot(&mut self) -> VmResult<Vec<BootEvent>> {
        if let Some(ref mut sp) = self.system_process {
            sp.start_boot_sequence()
        } else {
            Err(VmError::Generic(
                "system process not initialized".to_string(),
            ))
        }
    }

    /// Check if module is loaded
    pub fn is_module_loaded(&self, module: &str) -> bool {
        self.modules_loaded.iter().any(|m| m == module)
    }

    /// Mark a module as loaded
    pub fn mark_module_loaded(&mut self, module: &str) {
        if !self.is_module_loaded(module) {
            self.modules_loaded.push(module.to_string());
        }
    }
}

impl Default for BootLoader {
    fn default() -> Self {
        Self::minimal()
    }
}

/// Parse a simple .boot file format
/// Format: each line is either:
///   {load, Module}.
///   {start, App, Type}.
///   {run, Module, Function, Args}.
///   {setflag, Flag, Value}.
///   {register, Name, Pid}.
pub fn parse_boot_file<P: AsRef<Path>>(path: P) -> Result<BootScript, BootError> {
    let content = fs::read_to_string(path).map_err(|e| BootError::IoError(e.to_string()))?;

    parse_boot_content(&content)
}

/// Parse boot content from string
pub fn parse_boot_content(content: &str) -> Result<BootScript, BootError> {
    let mut script = BootScript::new();

    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('%') {
            continue;
        }

        let entry = parse_boot_line(line).map_err(|e| BootError::ParseError(e, line_no + 1))?;
        if let Some(entry) = entry {
            script.add_entry(entry);
        }
    }

    Ok(script)
}

fn parse_boot_line(line: &str) -> Result<Option<BootEntry>, String> {
    let line = line.trim().trim_end_matches('.');

    if line.starts_with("{load,") && line.ends_with('}') {
        let inner = &line[6..line.len() - 1];
        let module = inner.trim().to_string();
        return Ok(Some(BootEntry::LoadModule { module }));
    }

    if line.starts_with("{start,") {
        // {start, App, Type}.
        let inner = line[7..line.len() - 1].trim();
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 2 {
            let app = parts[0].trim().to_string();
            let type_str = parts[1].trim();
            let type_ = match type_str {
                "permanent" => ApplicationType::Permanent,
                "transient" => ApplicationType::Transient,
                "temporary" => ApplicationType::Temporary,
                "load" => ApplicationType::Load,
                _ => return Err(format!("unknown application type: {}", type_str)),
            };
            return Ok(Some(BootEntry::StartApplication { app, type_ }));
        }
    }

    if line.starts_with("{run,") {
        // {run, Module, Function, Args}.
        let inner = line[5..line.len() - 1].trim();
        let parts: Vec<&str> = inner.splitn(3, ',').collect();
        if parts.len() >= 2 {
            let module = parts[0].trim().to_string();
            let function = parts[1].trim().to_string();
            return Ok(Some(BootEntry::Run {
                module,
                function,
                args: Vec::new(),
            }));
        }
    }

    if line.starts_with("{setflag,") {
        // {setflag, Flag, Value}.
        let inner = line[9..line.len() - 1].trim();
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() >= 2 {
            let flag = parts[0].trim().to_string();
            // Value would be parsed from term representation
            return Ok(Some(BootEntry::SetFlag {
                flag,
                value: Term::nil(),
            }));
        }
    }

    if line.starts_with("{register,") {
        // {register, Name, Pid}.
        let inner = line[10..line.len() - 1].trim();
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() >= 2 {
            let name = parts[0].trim().to_string();
            let pid_str = parts[1].trim();
            // Parse PID from string representation
            if let Ok(pid_num) = pid_str.parse::<u32>() {
                let pid = Pid::new(pid_num, 0, 0);
                return Ok(Some(BootEntry::RegisterName { name, pid }));
            }
        }
    }

    Ok(None)
}

/// Boot error types
#[derive(Debug)]
pub enum BootError {
    IoError(String),
    ParseError(String, usize),
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::IoError(msg) => write!(f, "IO error: {}", msg),
            BootError::ParseError(msg, line) => write!(f, "Parse error at line {}: {}", line, msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_config_minimal() {
        let config = BootConfig::minimal();
        assert_eq!(config.boot_script.entries.len(), 1);
    }

    #[test]
    fn test_boot_config_standard() {
        let config = BootConfig::standard();
        assert_eq!(config.system_apps.len(), 2);
        assert!(config.system_apps.contains(&"kernel".to_string()));
    }

    #[test]
    fn test_parse_boot_content_load() {
        let content = r#"
{load, init}.
{load, erl_prim_loader}.
"#;
        let script = parse_boot_content(content).unwrap();
        assert_eq!(script.entries.len(), 2);
    }

    #[test]
    fn test_parse_boot_content_start() {
        let content = r#"
{start, kernel, permanent}.
{start, stdlib, transient}.
"#;
        let script = parse_boot_content(content).unwrap();
        assert_eq!(script.entries.len(), 2);
    }

    #[test]
    fn test_parse_boot_content_run() {
        let content = r#"
{run, init, boot, []}.
"#;
        let script = parse_boot_content(content).unwrap();
        assert_eq!(script.entries.len(), 1);
    }

    #[test]
    fn test_system_process_boot_sequence() {
        let config = BootConfig::minimal();
        let pid = Pid::new(0, 0, 0);
        let mut sys = SystemProcess::new(pid, &config);

        let events = sys.start_boot_sequence().unwrap();
        assert!(!events.is_empty());
        assert_eq!(sys.phase(), BootPhase::Ready);
    }

    #[test]
    fn test_application_type_default() {
        assert_eq!(ApplicationType::default(), ApplicationType::Permanent);
    }

    #[test]
    fn test_boot_loader_init() {
        let mut loader = BootLoader::minimal();
        let pid = Pid::new(0, 0, 0);
        loader.init_system_process(pid).unwrap();
        assert!(loader.system_pid().is_some());
    }

    #[test]
    fn test_boot_loader_run_boot() {
        let mut loader = BootLoader::minimal();
        let pid = Pid::new(0, 0, 0);
        loader.init_system_process(pid).unwrap();

        let events = loader.run_boot().unwrap();
        assert!(!events.is_empty());
    }

    #[test]
    fn test_boot_loader_mark_module_loaded() {
        let mut loader = BootLoader::minimal();
        assert!(!loader.is_module_loaded("init"));

        loader.mark_module_loaded("init");
        assert!(loader.is_module_loaded("init"));

        // Idempotent - should not duplicate
        loader.mark_module_loaded("init");
        assert_eq!(loader.modules_loaded.len(), 1);
    }

    #[test]
    fn test_parse_boot_content_setflag() {
        let content = r#"
{setflag, trap_exit, true}.
"#;
        let script = parse_boot_content(content).unwrap();
        assert_eq!(script.entries.len(), 1);
        match &script.entries[0] {
            BootEntry::SetFlag { flag, .. } => assert_eq!(flag, "trap_exit"),
            _ => panic!("expected SetFlag entry"),
        }
    }

    #[test]
    fn test_parse_boot_content_register() {
        let content = r#"
{register, init, 0}.
"#;
        let script = parse_boot_content(content).unwrap();
        assert_eq!(script.entries.len(), 1);
        match &script.entries[0] {
            BootEntry::RegisterName { name, pid } => {
                assert_eq!(name, "init");
                assert_eq!(pid.id, 0);
            }
            _ => panic!("expected RegisterName entry"),
        }
    }

    #[test]
    fn test_boot_config_with_kernel_app() {
        let config = BootConfig::new().with_kernel_app();
        assert!(config.boot_script.get_application("kernel").is_some());
        assert!(config.system_apps.contains(&"kernel".to_string()));
    }

    #[test]
    fn test_boot_config_with_stdlib_app() {
        let config = BootConfig::new().with_stdlib_app();
        assert!(config.boot_script.get_application("stdlib").is_some());
        assert!(config.system_apps.contains(&"stdlib".to_string()));
    }

    #[test]
    fn test_boot_script_convenience_methods() {
        let mut script = BootScript::new();
        script.load_module("test_module");
        script.start_application("test_app", ApplicationType::Permanent);
        script.run("test_mod", "test_func", vec![Term::nil()]);

        assert_eq!(script.entries.len(), 3);
    }

    #[test]
    fn test_boot_phase_transitions() {
        let mut config = BootConfig::minimal();
        assert_eq!(config.phase(), BootPhase::LoadingModules);

        config.set_phase(BootPhase::StartingApplications);
        assert_eq!(config.phase(), BootPhase::StartingApplications);

        config.set_phase(BootPhase::Ready);
        assert_eq!(config.phase(), BootPhase::Ready);
    }

    #[test]
    fn test_boot_event_ids() {
        let content = r#"
{load, init}.
{load, kernel}.
{run, init, boot, []}.
"#;
        let script = parse_boot_content(content).unwrap();
        assert_eq!(script.entries.len(), 3);
    }

    #[test]
    fn test_application_new() {
        let app = Application::new("test_app", "1.0");
        assert_eq!(app.name, "test_app");
        assert_eq!(app.version, "1.0");
        assert_eq!(app.type_, ApplicationType::default());
    }

    #[test]
    fn test_application_with_modules() {
        let app = Application::new("test_app", "1.0")
            .with_modules(vec!["mod1", "mod2", "mod3"])
            .with_description("Test application");
        assert_eq!(app.modules.len(), 3);
        assert_eq!(app.description, "Test application");
    }

    #[test]
    fn test_boot_entry_convenience() {
        let load = BootEntry::load_module("test");
        assert!(matches!(load, BootEntry::LoadModule { .. }));

        let start = BootEntry::start_app("test", ApplicationType::Permanent);
        assert!(matches!(start, BootEntry::StartApplication { .. }));

        let run = BootEntry::run("m", "f", vec![Term::nil()]);
        assert!(matches!(run, BootEntry::Run { .. }));
    }
}
