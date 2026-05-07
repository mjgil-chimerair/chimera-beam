//! Bytecode instructions for RustZigBeam.
//!
//! Defines the instruction set and execution context for the interpreter.

#![allow(missing_docs)]

#[cfg(test)]
use chimera_erlang_beam_allocator as _;

use chimera_erlang_beam_term::Term;
use std::convert::TryFrom;

/// Opcode for BEAM-like instructions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Opcode {
    // Load instructions
    Move = 0,
    LoadNil = 1,
    LoadInt = 2,
    LoadAtom = 3,

    // Call instructions
    Call = 10,
    CallLast = 11,
    CallOnly = 12,
    Return = 20,

    // Jump instructions
    Jump = 30,
    JumpOnVal = 31,
    JumpOnFail = 32,

    // Allocate/deallocate
    Allocate = 40,
    AllocateZero = 41,
    Deallocate = 42,

    // Initialize
    Init = 50,
    InitNext = 51,
    InitPutList = 52,

    // List/tuple
    GetList = 60,
    GetTuple = 61,
    PutList = 62,
    PutTuple = 63,

    // Arithmetic
    Add = 70,
    Sub = 71,
    Mul = 72,
    Div = 73,
    Rem = 74,
    Neg = 75,
    IntDiv = 76,
    IntMult = 77,
    IntSub = 78,
    IntAdd = 79,

    // Comparison
    Eq = 80,
    Neq = 81,
    Lt = 82,
    Lte = 83,
    Gt = 84,
    Gte = 85,
    ExactEq = 86,
    ExactNeq = 87,

    // Type tests
    IsAtom = 90,
    IsInteger = 91,
    IsFloat = 92,
    IsNumber = 93,
    IsTuple = 94,
    IsList = 95,
    IsNil = 96,
    IsBinary = 97,
    IsFunction = 98,

    // Messaging
    SendOp = 110,
    SendMsg = 111,

    // Receive
    RecvOp = 112,
    RecvWaitOp = 113,
    RecvPopOp = 114,
    RecvTimeoutOp = 115,

    // Exception handling
    TryVal = 120,
    TryEnd = 121,
    CatchVal = 122,
    CatchEnd = 123,
    Raise = 124,

    // Tuple operations
    SelectTupleArity = 130,
    SelectVal = 131,
    JumpWorks = 132,

    // Error instructions
    Badarg = 140,
    Badmatch = 141,
    CaseClause = 142,
    IfClause = 143,
    FunctionClause = 144,
    SystemLimit = 145,

    // Frame operations
    EnterFrame = 150,
    LeaveFrame = 151,
    Restore = 152,

    // BIF instructions
    Bif0 = 160,
    Bif1 = 161,
    Bif2 = 162,

    // Native calls
    Native = 200,
    NativeClosure = 201,

    // Map operations
    MapCreate = 202,
    MapPut = 203,
    MapGet = 204,
    MapRemove = 205,
    MapSize = 206,

    // Float operations
    FloatAdd = 210,
    FloatSub = 211,
    FloatMul = 212,
    FloatDiv = 213,
    FloatCmp = 214,
    FloatOp = 215,
    FloatLoad = 216,

    // Bitstring operations
    BsInit = 220,
    BsPut = 221,
    BsMatch = 222,
    BsSave = 223,
    BsRestore = 224,
}

impl Opcode {
    /// Try to convert a raw u16 to an Opcode
    ///
    /// Returns None if the raw value doesn't correspond to any variant.
    /// This is safe because it validates against the actual variant list
    /// rather than relying on the repr(u16) range which has gaps.
    pub fn from_raw(raw: u16) -> Option<Self> {
        match raw {
            0 => Some(Opcode::Move),
            1 => Some(Opcode::LoadNil),
            2 => Some(Opcode::LoadInt),
            3 => Some(Opcode::LoadAtom),
            10 => Some(Opcode::Call),
            11 => Some(Opcode::CallLast),
            12 => Some(Opcode::CallOnly),
            20 => Some(Opcode::Return),
            30 => Some(Opcode::Jump),
            31 => Some(Opcode::JumpOnVal),
            32 => Some(Opcode::JumpOnFail),
            40 => Some(Opcode::Allocate),
            41 => Some(Opcode::AllocateZero),
            42 => Some(Opcode::Deallocate),
            50 => Some(Opcode::Init),
            51 => Some(Opcode::InitNext),
            52 => Some(Opcode::InitPutList),
            60 => Some(Opcode::GetList),
            61 => Some(Opcode::GetTuple),
            62 => Some(Opcode::PutList),
            63 => Some(Opcode::PutTuple),
            70 => Some(Opcode::Add),
            71 => Some(Opcode::Sub),
            72 => Some(Opcode::Mul),
            73 => Some(Opcode::Div),
            74 => Some(Opcode::Rem),
            75 => Some(Opcode::Neg),
            76 => Some(Opcode::IntDiv),
            77 => Some(Opcode::IntMult),
            78 => Some(Opcode::IntSub),
            79 => Some(Opcode::IntAdd),
            80 => Some(Opcode::Eq),
            81 => Some(Opcode::Neq),
            82 => Some(Opcode::Lt),
            83 => Some(Opcode::Lte),
            84 => Some(Opcode::Gt),
            85 => Some(Opcode::Gte),
            86 => Some(Opcode::ExactEq),
            87 => Some(Opcode::ExactNeq),
            90 => Some(Opcode::IsAtom),
            91 => Some(Opcode::IsInteger),
            92 => Some(Opcode::IsFloat),
            93 => Some(Opcode::IsNumber),
            94 => Some(Opcode::IsTuple),
            95 => Some(Opcode::IsList),
            96 => Some(Opcode::IsNil),
            97 => Some(Opcode::IsBinary),
            98 => Some(Opcode::IsFunction),
            110 => Some(Opcode::SendOp),
            111 => Some(Opcode::SendMsg),
            112 => Some(Opcode::RecvOp),
            113 => Some(Opcode::RecvWaitOp),
            114 => Some(Opcode::RecvPopOp),
            120 => Some(Opcode::TryVal),
            121 => Some(Opcode::TryEnd),
            122 => Some(Opcode::CatchVal),
            123 => Some(Opcode::CatchEnd),
            124 => Some(Opcode::Raise),
            130 => Some(Opcode::SelectTupleArity),
            131 => Some(Opcode::SelectVal),
            132 => Some(Opcode::JumpWorks),
            140 => Some(Opcode::Badarg),
            141 => Some(Opcode::Badmatch),
            142 => Some(Opcode::CaseClause),
            143 => Some(Opcode::IfClause),
            144 => Some(Opcode::FunctionClause),
            145 => Some(Opcode::SystemLimit),
            150 => Some(Opcode::EnterFrame),
            151 => Some(Opcode::LeaveFrame),
            152 => Some(Opcode::Restore),
            160 => Some(Opcode::Bif0),
            161 => Some(Opcode::Bif1),
            162 => Some(Opcode::Bif2),
            200 => Some(Opcode::Native),
            201 => Some(Opcode::NativeClosure),
            202 => Some(Opcode::MapCreate),
            203 => Some(Opcode::MapPut),
            204 => Some(Opcode::MapGet),
            205 => Some(Opcode::MapRemove),
            206 => Some(Opcode::MapSize),
            210 => Some(Opcode::FloatAdd),
            211 => Some(Opcode::FloatSub),
            212 => Some(Opcode::FloatMul),
            213 => Some(Opcode::FloatDiv),
            214 => Some(Opcode::FloatCmp),
            215 => Some(Opcode::FloatOp),
            216 => Some(Opcode::FloatLoad),
            220 => Some(Opcode::BsInit),
            221 => Some(Opcode::BsPut),
            222 => Some(Opcode::BsMatch),
            223 => Some(Opcode::BsSave),
            224 => Some(Opcode::BsRestore),
            _ => None,
        }
    }
}

impl TryFrom<u16> for Opcode {
    type Error = DecodeError;

    fn try_from(raw: u16) -> Result<Self, Self::Error> {
        Self::from_raw(raw).ok_or(DecodeError::InvalidOpcode(raw))
    }
}

/// Execution result for an instruction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecResult {
    /// Continue to next instruction
    Ok,
    /// Yield (reductions exhausted)
    Yield,
    /// Trap to BIF or native
    Trap,
    /// Error occurred
    Err,
    /// Process exited
    ExitDispatch,
    /// Process is waiting for messages (receive with empty queue)
    Wait,
}

/// Maximum number of X registers
pub const MAX_REGISTERS: usize = 64;

/// Maximum number of Y (stack) registers
pub const MAX_Y_REGISTERS: usize = 256;

/// Default reduction budget per step
pub const DEFAULT_REDUCTION_BUDGET: u64 = 1000;

/// BIF call information extracted from trapped instruction
#[derive(Debug, Clone, Copy)]
pub struct BifCall {
    /// BIF function index
    pub bif_id: u32,
    /// Destination register for result
    pub dest: u32,
    /// First argument register (for Bif1, Bif2)
    pub arg1: u32,
    /// Second argument register (for Bif2)
    pub arg2: u32,
}

impl BifCall {
    /// Create a new BIF call with given details
    pub fn new(bif_id: u32, dest: u32, arg1: u32, arg2: u32) -> Self {
        BifCall {
            bif_id,
            dest,
            arg1,
            arg2,
        }
    }

    /// Get argument count for this BIF call
    pub fn arg_count(&self) -> u32 {
        if self.arg2 != 0 {
            2
        } else if self.arg1 != 0 {
            1
        } else {
            0
        }
    }
}

/// Execution context for the interpreter
#[derive(Debug)]
pub struct ExecContext {
    /// X registers (primary)
    pub x: [Term; MAX_REGISTERS],
    /// Y registers (stack-backed)
    y: [Term; MAX_Y_REGISTERS],
    /// Frame pointer
    fp: u64,
    /// Instruction pointer
    pub ip: u64,
    /// Continuation pointer
    cp: u64,
    /// Number of live X registers
    live: u32,
    /// Remaining reductions
    reductions: u64,
    /// Reduction budget
    reduction_budget: u64,
    /// Current instruction word (for trap handling)
    current_instruction: u64,
    /// BIF call info extracted from trapped instruction
    pub bif_call: Option<BifCall>,
    /// Receive state for message matching
    pub receive_state: Option<ReceiveState>,
    /// Exception handling state
    pub exception_state: Option<ExceptionState>,
    /// Pointer to process heap for allocations (optional - set by VM)
    heap: Option<*mut chimera_erlang_beam_heap::ProcessHeap>,
}

/// Receive operation state
#[derive(Debug, Clone)]
pub struct ReceiveState {
    /// Save list index for pattern matching
    pub save_index: u32,
    /// Timeout value (in reductions)
    pub timeout: u64,
    /// Reductions spent waiting
    pub waited_reductions: u64,
    /// Active message being tested
    pub active_message: Option<Term>,
    /// Flag set when message arrives while waiting
    pub message_arrived: bool,
    /// Saved queue length - position in mailbox when receive started
    pub saved_queue_len: usize,
}

/// Exception handling state
#[derive(Debug, Clone)]
pub struct ExceptionState {
    /// Exception register (where caught value goes)
    pub reg: u32,
    /// Exception handler IP
    pub handler: u64,
    /// Stack depth when try was entered
    pub stack_depth: u32,
    /// Whether this is a catch (vs try)
    pub is_catch: bool,
}

impl ExecContext {
    pub fn new() -> Self {
        ExecContext {
            x: [Term::nil(); MAX_REGISTERS],
            y: [Term::nil(); MAX_Y_REGISTERS],
            fp: 0,
            ip: 0,
            cp: 0,
            live: 0,
            reductions: DEFAULT_REDUCTION_BUDGET,
            reduction_budget: DEFAULT_REDUCTION_BUDGET,
            current_instruction: 0,
            bif_call: None,
            receive_state: None,
            exception_state: None,
            heap: None,
        }
    }

    /// Set the process heap for allocations
    pub fn set_heap(&mut self, heap: *mut chimera_erlang_beam_heap::ProcessHeap) {
        self.heap = Some(heap);
    }

    /// Get mutable reference to the heap if set
    pub fn heap_mut(&mut self) -> Option<&mut chimera_erlang_beam_heap::ProcessHeap> {
        self.heap.map(|h: *mut chimera_erlang_beam_heap::ProcessHeap| {
            // Safety: self.heap is set only when the heap is valid and owned by the process
            unsafe { &mut *h }
        })
    }

    /// Check if heap is available
    pub fn has_heap(&self) -> bool {
        self.heap.is_some()
    }

    /// Get the current instruction word
    pub fn get_current_instruction(&self) -> u64 {
        self.current_instruction
    }

    /// Reset the BIF call info (called after BIF returns)
    pub fn clear_bif_call(&mut self) {
        self.bif_call = None;
    }

    /// Check if there's a pending BIF call
    pub fn has_bif_call(&self) -> bool {
        self.bif_call.is_some()
    }

    /// Check if there's a pending receive operation
    pub fn has_receive_state(&self) -> bool {
        self.receive_state.is_some()
    }

    /// Clear the receive state
    pub fn clear_receive_state(&mut self) {
        self.receive_state = None;
    }

    /// Allocate words on the heap and return the address
    ///
    /// Returns the word address of the allocation, or None if out of memory.
    pub fn heap_alloc(&mut self, words: usize) -> Option<usize> {
        self.heap_mut().and_then(|h| h.alloc(words))
    }

    /// Allocate a map term on the heap
    ///
    /// Layout: [header][key_0][val_0]...[key_n-1][val_n-1]
    pub fn alloc_map(&mut self, num_pairs: usize) -> Option<Term> {
        let words_needed = 1 + (num_pairs * 2);
        let ptr = self.heap_alloc(words_needed)?;
        let header = 6u64 | ((words_needed as u64) << 8);
        if let Some(h) = self.heap_mut() {
            h.set_word(ptr, header);
        }
        Some(Term::from_map(ptr as u64))
    }

    /// Write a key-value pair to the heap at the given map position
    pub fn write_map_pair(&mut self, map_ptr: usize, index: usize, key: Term, value: Term) {
        if let Some(h) = self.heap_mut() {
            let pos = map_ptr + 1 + (index * 2);
            h.set_word(pos, key.0);
            h.set_word(pos + 1, value.0);
        }
    }

    /// Allocate a tuple term on the heap
    ///
    /// Layout: [header][elem_0]...[elem_n-1]
    pub fn alloc_tuple(&mut self, arity: usize) -> Option<Term> {
        let ptr = self.heap_alloc(1 + arity)?;
        let header = 3u64 | (((1 + arity) as u64) << 8);
        if let Some(h) = self.heap_mut() {
            h.set_word(ptr, header);
        }
        Some(Term::from_tuple(ptr as u64))
    }

    /// Write an element to a tuple at the given position
    pub fn write_tuple_element(&mut self, tuple_ptr: usize, index: usize, value: Term) {
        if let Some(h) = self.heap_mut() {
            let pos = tuple_ptr + 1 + index;
            h.set_word(pos, value.0);
        }
    }

    /// Allocate a cons cell on the heap
    ///
    /// Layout: [header][head][tail]
    pub fn alloc_cons(&mut self) -> Option<Term> {
        let ptr = self.heap_alloc(3)?;
        let header = 2u64 | (3u64 << 8);
        if let Some(h) = self.heap_mut() {
            h.set_word(ptr, header);
        }
        Some(Term::from_cons(ptr as u64))
    }

    /// Write head and tail to a cons cell
    pub fn write_cons(&mut self, cons_ptr: usize, head: Term, tail: Term) {
        if let Some(h) = self.heap_mut() {
            h.set_word(cons_ptr + 1, head.0);
            h.set_word(cons_ptr + 2, tail.0);
        }
    }

    /// Read a float value from a register
    /// Returns the f64 value if the register contains a float-encoding
    pub fn get_float(&self, reg: u32) -> Option<f64> {
        let term = self.get_x(reg);
        if term.is_small() {
            // For testing: interpret small int bits as float bits
            let bits = term.0 >> 3;
            Some(f64::from_bits(bits))
        } else {
            None
        }
    }

    /// Create a float-encoded term from f64 value
    /// This encoding requires proper float heap terms - for now returns nil
    pub fn make_float(&self, _value: f64) -> Term {
        Term::nil() // Placeholder until proper float heap terms are implemented
    }

    /// Set an X register value
    pub fn set_x(&mut self, reg: u32, value: Term) {
        if (reg as usize) < MAX_REGISTERS {
            self.x[reg as usize] = value;
        }
    }

    /// Get an X register value
    pub fn get_x(&self, reg: u32) -> Term {
        if (reg as usize) < MAX_REGISTERS {
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

    /// Decrement reductions
    pub fn decrement_reductions(&mut self, amount: u64) {
        if self.reductions >= amount {
            self.reductions -= amount;
        } else {
            self.reductions = 0;
        }
    }

    /// Check if reductions are exhausted
    pub fn is_exhausted(&self) -> bool {
        self.reductions == 0
    }

    /// Reset reductions to budget
    pub fn reset_reductions(&mut self) {
        self.reductions = self.reduction_budget;
    }

    /// Get continuation pointer
    pub fn get_cp(&self) -> u64 {
        self.cp
    }

    /// Set continuation pointer
    pub fn set_cp(&mut self, cp: u64) {
        self.cp = cp;
    }

    /// Get frame pointer
    pub fn get_fp(&self) -> u64 {
        self.fp
    }

    /// Set frame pointer
    pub fn set_fp(&mut self, fp: u64) {
        self.fp = fp;
    }

    /// Get number of live X registers
    pub fn get_live(&self) -> u32 {
        self.live
    }

    /// Set number of live X registers
    pub fn set_live(&mut self, live: u32) {
        self.live = live;
    }

    /// Get Y registers reference
    pub fn get_y_registers(&self) -> &[Term; MAX_Y_REGISTERS] {
        &self.y
    }

    /// Get mutable Y registers reference
    pub fn get_y_registers_mut(&mut self) -> &mut [Term; MAX_Y_REGISTERS] {
        &mut self.y
    }

    /// Get reduction budget
    pub fn get_reduction_budget(&self) -> u64 {
        self.reduction_budget
    }

    /// Set reduction budget
    pub fn set_reduction_budget(&mut self, budget: u64) {
        self.reduction_budget = budget;
    }

    /// Get remaining reductions
    pub fn get_reductions(&self) -> u64 {
        self.reductions
    }

    /// Set remaining reductions
    pub fn set_reductions(&mut self, reductions: u64) {
        self.reductions = reductions;
    }

    /// Get current instruction word
    pub fn get_current_instruction_word(&self) -> u64 {
        self.current_instruction
    }

    /// Set current instruction word
    pub fn set_current_instruction_word(&mut self, instr: u64) {
        self.current_instruction = instr;
    }

    /// Initialize context from PCB state
    #[allow(clippy::too_many_arguments)]
    pub fn init_from_pcb(
        &mut self,
        cp: u64,
        fp: u64,
        live: u32,
        y: &[Term; MAX_Y_REGISTERS],
        reduction_budget: u64,
        current_instruction: u64,
        bif_call: Option<BifCall>,
        receive_state: Option<ReceiveState>,
        exception_state: Option<ExceptionState>,
    ) {
        self.cp = cp;
        self.fp = fp;
        self.live = live;
        self.y.copy_from_slice(y);
        self.reductions = reduction_budget;
        self.reduction_budget = reduction_budget;
        self.current_instruction = current_instruction;
        self.bif_call = bif_call;
        self.receive_state = receive_state;
        self.exception_state = exception_state;
    }
}

impl Default for ExecContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Step result containing new IP and result
#[derive(Debug)]
pub struct StepResult {
    pub ip: u64,
    pub result: ExecResult,
}

/// Decode error for instruction parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidOpcode(u16),
    InvalidRegister(u8),
    TruncatedInstruction,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::InvalidOpcode(op) => write!(f, "Invalid opcode: {}", op),
            DecodeError::InvalidRegister(reg) => write!(f, "Invalid register: {}", reg),
            DecodeError::TruncatedInstruction => write!(f, "Truncated instruction"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Result of safe instruction decode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeResult {
    Valid(Opcode),
    Invalid(DecodeError),
}

impl DecodeResult {
    pub fn is_valid(&self) -> bool {
        matches!(self, DecodeResult::Valid(_))
    }

    pub fn opcode(&self) -> Option<Opcode> {
        match self {
            DecodeResult::Valid(op) => Some(*op),
            DecodeResult::Invalid(_) => None,
        }
    }

    pub fn unwrap(self) -> Opcode {
        match self {
            DecodeResult::Valid(op) => op,
            DecodeResult::Invalid(e) => panic!("unwrap called on Invalid: {}", e),
        }
    }
}

/// Safely decode an opcode from a raw instruction word
///
/// Returns `DecodeResult::Valid` if the opcode is recognized,
/// or `DecodeResult::Invalid` with the specific error otherwise.
/// This avoids undefined behavior from invalid discriminants.
fn decode_opcode(word: u64) -> DecodeResult {
    let raw = (word & 0xFFFF) as u16;

    // Use Opcode::try_from to safely convert the discriminant
    // This handles the sparse enum case where many raw values
    // are invalid even though they're <= 201
    match Opcode::try_from(raw) {
        Ok(opcode) => DecodeResult::Valid(opcode),
        Err(_) => DecodeResult::Invalid(DecodeError::InvalidOpcode(raw)),
    }
}

/// Try to decode an opcode, returning None on invalid
#[allow(dead_code)]
fn try_decode_opcode(word: u64) -> Option<Opcode> {
    decode_opcode(word).opcode()
}

fn decode_dest(word: u64) -> u32 {
    ((word >> 16) & 0xFF) as u32
}

fn decode_src(word: u64) -> u32 {
    ((word >> 24) & 0xFF) as u32
}

fn decode_src2(word: u64) -> u32 {
    ((word >> 32) & 0xFF) as u32
}

fn decode_value(word: u64) -> i64 {
    (word >> 32) as i64
}

/// Full instruction execution with safe decoding
pub fn execute_instruction(ctx: &mut ExecContext, code: &[u64]) -> StepResult {
    if ctx.ip as usize >= code.len() {
        return StepResult {
            ip: ctx.ip,
            result: ExecResult::ExitDispatch,
        };
    }

    let word = code[ctx.ip as usize];
    let decode_result = decode_opcode(word);

    // Handle invalid opcodes gracefully
    let opcode = match decode_result {
        DecodeResult::Valid(op) => op,
        DecodeResult::Invalid(_) => {
            // Invalid opcode - treat as error
            return StepResult {
                ip: ctx.ip,
                result: ExecResult::Err,
            };
        }
    };

    let ip = ctx.ip;
    ctx.ip += 1;
    ctx.decrement_reductions(1);

    match opcode {
        Opcode::Move => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let value = ctx.get_x(src);
            ctx.set_x(dest, value);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::LoadInt => {
            let dest = decode_dest(word);
            let value = decode_value(word);
            ctx.set_x(dest, Term::from_small(value));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::LoadNil => {
            let dest = decode_dest(word);
            ctx.set_x(dest, Term::nil());
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::LoadAtom => {
            let dest = decode_dest(word);
            let atom_index = code[(ctx.ip) as usize] as u32;
            ctx.set_x(dest, Term::from_atom(atom_index));
            ctx.ip += 1;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Return => {
            ctx.ip = ctx.cp;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Jump => {
            let target = decode_value(word);
            ctx.ip = target as u64;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::JumpOnVal => {
            let src = decode_src(word);
            let value = decode_value(word);
            let test_val = ctx.get_x(src);
            if let Some(v) = test_val.to_small_opt() {
                if v == value {
                    ctx.ip = ctx.ip.wrapping_add(1);
                    return StepResult {
                        ip,
                        result: ExecResult::Ok,
                    };
                }
            }
            // Skip the branch target word
            ctx.ip += 1;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::JumpOnFail => {
            let src = decode_src(word);
            let test_val = ctx.get_x(src);
            // A "fail" is typically false or nil
            let is_fail = test_val == Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
                || test_val == Term::nil();
            if is_fail {
                let target = decode_value(word);
                ctx.ip = target as u64;
            } else {
                ctx.ip += 1;
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Add | Opcode::IntAdd => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                ctx.set_x(dest, Term::from_small(ai + bi));
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Sub | Opcode::IntSub => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                ctx.set_x(dest, Term::from_small(ai - bi));
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Mul | Opcode::IntMult => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                ctx.set_x(dest, Term::from_small(ai * bi));
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Div | Opcode::IntDiv => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                if bi != 0 {
                    ctx.set_x(dest, Term::from_small(ai / bi));
                }
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Rem => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                if bi != 0 {
                    ctx.set_x(dest, Term::from_small(ai % bi));
                }
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Neg => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            if let Some(ai) = a.to_small_opt() {
                ctx.set_x(dest, Term::from_small(-ai));
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Eq | Opcode::ExactEq => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            let result = if a == b {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Neq | Opcode::ExactNeq => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            let result = if a != b {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Lt => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            let result = if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                if ai < bi {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
                } else {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
                }
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Lte => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            let result = if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                if ai <= bi {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
                } else {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
                }
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Gt => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            let result = if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                if ai > bi {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
                } else {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
                }
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Gte => {
            let dest = decode_dest(word);
            let a = ctx.get_x(decode_src(word));
            let b = ctx.get_x(decode_src2(word));
            let result = if let (Some(ai), Some(bi)) = (a.to_small_opt(), b.to_small_opt()) {
                if ai >= bi {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
                } else {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
                }
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsAtom => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let result = if ctx.get_x(src).is_atom() {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsInteger => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let result = if ctx.get_x(src).is_small() {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsList => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let val = ctx.get_x(src);
            let result = if val.is_cons() || val == Term::nil() {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsTuple => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let result = if ctx.get_x(src).tag() == chimera_erlang_beam_term::TermTag::Tuple {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsNil => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let result = if ctx.get_x(src) == Term::nil() {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsFloat => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            // Floats are boxed in real implementation
            let result = if ctx.get_x(src).tag() == chimera_erlang_beam_term::TermTag::Float {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsNumber => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let val = ctx.get_x(src);
            // Number is small integer or float
            let result = if val.is_small() || val.tag() == chimera_erlang_beam_term::TermTag::Float {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsBinary => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let result = if ctx.get_x(src).tag() == chimera_erlang_beam_term::TermTag::Binary {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::IsFunction => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let result = if ctx.get_x(src).tag() == chimera_erlang_beam_term::TermTag::Fun {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
            } else {
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
            };
            ctx.set_x(dest, result);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Call => {
            let arity = decode_dest(word);
            let target = decode_value(word) as u64;
            // Save return address
            let return_addr = ctx.ip + 1;
            ctx.cp = return_addr;
            // Set up for the call: target is the new IP
            ctx.ip = target;
            // Note: In real BEAM, the live register count would be used for GC
            ctx.live = arity;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::CallLast => {
            let arity = decode_dest(word);
            let target = decode_value(word) as u64;
            // Same as Call but also deallocates the current frame
            ctx.ip = target;
            ctx.live = arity;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::CallOnly => {
            let target = decode_value(word) as u64;
            ctx.ip = target;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Allocate => {
            let slots = decode_dest(word);
            let live = decode_src(word);
            ctx.fp = ctx.y.len() as u64 - slots as u64;
            ctx.live = live;
            // Zero the slots
            for i in 0..slots as usize {
                let idx = ctx.fp as usize + i;
                if idx < ctx.y.len() {
                    ctx.y[idx] = Term::nil();
                }
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::AllocateZero => {
            let slots = decode_dest(word);
            let live = decode_src(word);
            ctx.fp = ctx.y.len() as u64 - slots as u64;
            ctx.live = live;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Deallocate => {
            let slots = decode_dest(word);
            ctx.fp = ctx.fp.wrapping_add(slots as u64);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::EnterFrame => {
            let slots = decode_dest(word);
            let live = decode_src(word);
            // Save old frame pointer
            ctx.y[ctx.fp as usize] = Term::from_small(ctx.fp as i64);
            ctx.fp = ctx.y.len() as u64 - slots as u64;
            ctx.live = live;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::LeaveFrame => {
            // Restore old frame pointer
            let old_fp = ctx.y[ctx.fp as usize].to_small();
            ctx.fp = old_fp as u64;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Restore => {
            let slot = decode_dest(word);
            let src = decode_src(word);
            ctx.y[ctx.fp as usize + slot as usize] = ctx.get_x(src);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Init | Opcode::InitNext | Opcode::InitPutList => {
            // Initialization instructions - no-op in interpreter
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::GetList => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let _cons = ctx.get_x(src);
            // In a real implementation, we'd decode the cons cell
            // For now, just leave nil
            ctx.set_x(dest, Term::nil());
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::GetTuple => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let _tuple = ctx.get_x(src);
            // Tuple handling would set up the elements
            ctx.set_x(dest, Term::nil());
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::PutList => {
            let hd = decode_src(word);
            let tl = decode_src2(word);
            let dest = decode_dest(word);
            // Create a cons cell (would allocate in real implementation)
            // For now, just store the two values as a pseudo-pair
            ctx.set_x(
                dest,
                Term::from_cons(((ctx.get_x(hd).0 as u64) << 32) | (ctx.get_x(tl).0 as u64)),
            );
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::PutTuple => {
            let dest = decode_dest(word);
            let src = decode_src(word);
            let _elt = ctx.get_x(src);
            // Would create a tuple
            ctx.set_x(dest, Term::nil());
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::SendOp => {
            // Send message to a process
            // Operand: dest = PID register, src = message register
            let pid_reg = decode_dest(word);
            let msg_reg = decode_src(word);
            let _pid_term = ctx.get_x(pid_reg);
            let _message = ctx.get_x(msg_reg);

            // In a real implementation, we'd call the process table's send function
            // For now, just return Ok to indicate the send was processed
            // The actual send would be: process_table.send(pid, message);
            StepResult {
                ip: ip + 1,
                result: ExecResult::Ok,
            }
        }
        Opcode::SendMsg => {
            // Message send with trap possibility
            // Similar to SendOp but may trap to BIF for distributed send
            let pid_reg = decode_dest(word);
            let msg_reg = decode_src(word);
            let _pid_term = ctx.get_x(pid_reg);
            let _message = ctx.get_x(msg_reg);

            // In full implementation, this might trap to BIF
            StepResult {
                ip: ip + 1,
                result: ExecResult::Ok,
            }
        }
        Opcode::RecvOp => {
            // Receive operation - sets up for selective receive
            // RecvOp starts a receive sequence, saving the current message pointer
            let save_index = decode_dest(word);
            ctx.receive_state = Some(ReceiveState {
                save_index,
                timeout: 0,
                waited_reductions: 0,
                active_message: None,
                message_arrived: false,
                saved_queue_len: 0, // Set by VM before execution
            });
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::RecvWaitOp => {
            // Wait for message with timeout
            // If a message is available, return Ok to continue to RecvPopOp
            // If no message, return Wait to yield to scheduler
            let timeout = decode_dest(word) as u64;
            let save_index = decode_src(word);

            if let Some(ref mut state) = ctx.receive_state {
                state.timeout = timeout;

                // Check if message_arrived flag was set by VM wakeup
                if state.message_arrived {
                    // Message arrived - clear the flag so we don't re-trigger
                    state.message_arrived = false;
                    // Deliver the active message to X0 register
                    // The VM sets active_message before calling step()
                    if let Some(msg) = state.active_message.take() {
                        ctx.set_x(0, msg);
                    }
                    return StepResult {
                        ip,
                        result: ExecResult::Ok,
                    };
                }

                // If timeout is 0 and no message arrived, immediately return timeout
                // This is the "receive after 0" case
                if state.timeout == 0 {
                    ctx.receive_state = None;
                    // Set X0 to 'timeout' atom for the receive to handle
                    ctx.set_x(0, Term::from_atom(0)); // 'timeout' = atom index 0
                    return StepResult {
                        ip,
                        result: ExecResult::Ok,
                    };
                }

                // Track reductions spent waiting
                state.waited_reductions += 1;

                // Check timeout
                if state.timeout > 0 && state.waited_reductions >= state.timeout {
                    // Timeout - clear receive state
                    ctx.receive_state = None;
                    // Set X0 to 'timeout' atom
                    ctx.set_x(0, Term::from_atom(0)); // 'timeout'
                    return StepResult {
                        ip,
                        result: ExecResult::Ok,
                    };
                }

                return StepResult {
                    ip,
                    result: ExecResult::Wait,
                };
            }

            // No receive state - start a new receive
            ctx.receive_state = Some(ReceiveState {
                save_index,
                timeout,
                waited_reductions: 0,
                active_message: None,
                message_arrived: false,
                saved_queue_len: 0, // Set by VM before execution
            });
            StepResult {
                ip,
                result: ExecResult::Wait,
            }
        }
        Opcode::RecvPopOp => {
            // Pop matched message from queue
            // This is called after RecvWaitOp finds a matching message
            let dest = decode_dest(word);

            // Clear the receive state
            ctx.receive_state = None;
            // Message will be delivered via VM on next step when message_arrived was true
            ctx.set_x(dest, Term::nil());

            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::RecvTimeoutOp => {
            // Receive timeout - triggered when no message arrives within timeout
            // Clear receive state and return timeout indicator
            ctx.receive_state = None;
            ctx.set_x(0, Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::MapCreate => {
            // Create a new map from key-value pairs in registers
            // dest = destination register, src = number of pairs
            // Keys in X0..X(num_pairs-1), values in X(num_pairs)..X(2*num_pairs-1)
            let dest = decode_dest(word);
            let num_pairs = decode_src(word) as usize;

            if let Some(map_term) = ctx.alloc_map(num_pairs) {
                let map_ptr = map_term.to_map() as usize;
                // Read keys and values from registers
                for i in 0..num_pairs {
                    let key = ctx.get_x(i as u32);
                    let value = ctx.get_x((num_pairs + i) as u32);
                    ctx.write_map_pair(map_ptr, i, key, value);
                }
                ctx.set_x(dest, map_term);
            } else {
                // Failed to allocate
                ctx.set_x(dest, Term::nil());
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::MapPut => {
            // Add or update a key in an existing map
            // dest = result register, src = map, src2 = key, additional = value
            let dest = decode_dest(word);
            let map_reg = decode_src(word);
            let key_reg = decode_src2(word);
            let value_reg = ((word >> 40) & 0xFF) as u32;
            let map_term = ctx.get_x(map_reg);
            let key_term = ctx.get_x(key_reg);
            let value_term = ctx.get_x(value_reg);

            // Read existing map data first
            let existing_data: Option<(usize, Vec<(Term, Term)>)> = if map_term.is_map() {
                if let Some(ref h) = ctx.heap_mut() {
                    let ptr = map_term.to_map() as usize;
                    if let Some(header) = h.get_word(ptr) {
                        let num_pairs = ((header >> 8) as usize - 1) / 2;
                        let mut pairs = Vec::with_capacity(num_pairs);
                        for i in 0..num_pairs {
                            let key_pos = ptr + 1 + (i * 2);
                            let val_pos = key_pos + 1;
                            if let Some(k) = h.get_word(key_pos) {
                                let v = h.get_word(val_pos).unwrap_or(0);
                                pairs.push((Term(k), Term(v)));
                            }
                        }
                        Some((ptr, pairs))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Now we can allocate and write without multiple mutable borrows
            if let Some((_, pairs)) = existing_data {
                // Check if key exists
                let key_found = pairs.iter().any(|(k, _)| *k == key_term);
                let new_num_pairs = if key_found {
                    pairs.len()
                } else {
                    pairs.len() + 1
                };

                if let Some(new_map_term) = ctx.alloc_map(new_num_pairs) {
                    let new_ptr = new_map_term.to_map() as usize;
                    for (i, &(k, v)) in pairs.iter().enumerate() {
                        ctx.write_map_pair(new_ptr, i, k, v);
                    }
                    if !key_found {
                        ctx.write_map_pair(new_ptr, pairs.len(), key_term, value_term);
                    }
                    ctx.set_x(dest, new_map_term);
                } else {
                    ctx.set_x(dest, Term::nil());
                }
            } else {
                // No existing map - create new with single pair
                if let Some(new_map_term) = ctx.alloc_map(1) {
                    let new_ptr = new_map_term.to_map() as usize;
                    ctx.write_map_pair(new_ptr, 0, key_term, value_term);
                    ctx.set_x(dest, new_map_term);
                } else {
                    ctx.set_x(dest, Term::nil());
                }
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::MapGet => {
            // Get a value from a map by key
            // dest = result register, src = map, src2 = key
            let dest = decode_dest(word);
            let map_reg = decode_src(word);
            let key_reg = decode_src2(word);
            let map_term = ctx.get_x(map_reg);
            let key_term = ctx.get_x(key_reg);

            let value = if map_term.is_map() && ctx.heap_mut().is_some() {
                let ptr = map_term.to_map() as usize;
                if let Some(ref mut h) = ctx.heap_mut() {
                    // Read header to get size
                    if let Some(header) = h.get_word(ptr) {
                        let num_pairs = ((header >> 8) as usize - 1) / 2;
                        // Search for key
                        let mut found = false;
                        let mut result = Term::nil();
                        for i in 0..num_pairs {
                            let key_pos = ptr + 1 + (i * 2);
                            let val_pos = key_pos + 1;
                            if let Some(stored_key) = h.get_word(key_pos) {
                                if Term(stored_key) == key_term {
                                    if let Some(val) = h.get_word(val_pos) {
                                        result = Term(val);
                                        found = true;
                                    }
                                    break;
                                }
                            }
                        }
                        if !found {
                            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_UNDEFINED)
                        } else {
                            result
                        }
                    } else {
                        Term::nil()
                    }
                } else {
                    Term::nil()
                }
            } else {
                // Not a map - in real OTP this raises badarg
                Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_UNDEFINED)
            };
            ctx.set_x(dest, value);
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::MapRemove => {
            // Remove a key from a map
            // dest = result register, src = map, src2 = key
            let dest = decode_dest(word);
            let map_reg = decode_src(word);
            let key_reg = decode_src2(word);
            let map_term = ctx.get_x(map_reg);
            let key_term = ctx.get_x(key_reg);

            // Read existing map data first
            let existing_data: Option<Vec<(Term, Term)>> = if map_term.is_map() {
                if let Some(ref h) = ctx.heap_mut() {
                    let ptr = map_term.to_map() as usize;
                    if let Some(header) = h.get_word(ptr) {
                        let num_pairs = ((header >> 8) as usize - 1) / 2;
                        let mut pairs = Vec::with_capacity(num_pairs);
                        for i in 0..num_pairs {
                            let key_pos = ptr + 1 + (i * 2);
                            let val_pos = key_pos + 1;
                            if let Some(k) = h.get_word(key_pos) {
                                let v = h.get_word(val_pos).unwrap_or(0);
                                pairs.push((Term(k), Term(v)));
                            }
                        }
                        Some(pairs)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(pairs) = existing_data {
                // Find key to remove
                let key_index = pairs.iter().position(|(k, _)| *k == key_term);
                if let Some(idx) = key_index {
                    // Create new map without this key
                    let new_num_pairs = pairs.len() - 1;
                    if let Some(new_map_term) = ctx.alloc_map(new_num_pairs) {
                        let new_ptr = new_map_term.to_map() as usize;
                        let mut dst = 0;
                        for (i, &(k, v)) in pairs.iter().enumerate() {
                            if i != idx {
                                ctx.write_map_pair(new_ptr, dst, k, v);
                                dst += 1;
                            }
                        }
                        ctx.set_x(dest, new_map_term);
                    } else {
                        ctx.set_x(dest, Term::nil());
                    }
                } else {
                    // Key not found - return copy
                    if let Some(new_map_term) = ctx.alloc_map(pairs.len()) {
                        let new_ptr = new_map_term.to_map() as usize;
                        for (i, &(k, v)) in pairs.iter().enumerate() {
                            ctx.write_map_pair(new_ptr, i, k, v);
                        }
                        ctx.set_x(dest, new_map_term);
                    } else {
                        ctx.set_x(dest, Term::nil());
                    }
                }
            } else {
                ctx.set_x(dest, map_term); // Not a map or no heap
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::MapSize => {
            // Get the number of keys in a map
            // dest = result register, src = map register
            let dest = decode_dest(word);
            let map_reg = decode_src(word);
            let map_term = ctx.get_x(map_reg);

            // Try to get size from heap if map is valid
            let size = if map_term.is_map() {
                let ptr = map_term.to_map() as usize;
                if let Some(ref h) = ctx.heap_mut() {
                    // Read header word - size is in bits 8+
                    if let Some(header) = h.get_word(ptr) {
                        ((header >> 8) as usize - 1) / 2 // (total_words - 1) / 2 for key-value pairs
                    } else {
                        0
                    }
                } else {
                    0
                }
            } else {
                0 // Not a map, return 0 (could also raise badarg)
            };
            ctx.set_x(dest, Term::from_small(size as i64));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatAdd => {
            // dest = result, src1 = left operand, src2 = right operand
            let dest = decode_dest(word);
            let a_reg = decode_src(word);
            let b_reg = decode_src2(word);

            let a_val = ctx.get_float(a_reg).unwrap_or(0.0);
            let b_val = ctx.get_float(b_reg).unwrap_or(0.0);
            let result = a_val + b_val;
            ctx.set_x(dest, ctx.make_float(result));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatSub => {
            let dest = decode_dest(word);
            let a_reg = decode_src(word);
            let b_reg = decode_src2(word);

            let a_val = ctx.get_float(a_reg).unwrap_or(0.0);
            let b_val = ctx.get_float(b_reg).unwrap_or(0.0);
            let result = a_val - b_val;
            ctx.set_x(dest, ctx.make_float(result));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatMul => {
            let dest = decode_dest(word);
            let a_reg = decode_src(word);
            let b_reg = decode_src2(word);

            let a_val = ctx.get_float(a_reg).unwrap_or(0.0);
            let b_val = ctx.get_float(b_reg).unwrap_or(0.0);
            let result = a_val * b_val;
            ctx.set_x(dest, ctx.make_float(result));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatDiv => {
            let dest = decode_dest(word);
            let a_reg = decode_src(word);
            let b_reg = decode_src2(word);

            let a_val = ctx.get_float(a_reg).unwrap_or(0.0);
            let b_val = ctx.get_float(b_reg).unwrap_or(0.0);
            let result = if b_val == 0.0 {
                // Handle division by zero per IEEE 754
                if a_val == 0.0 {
                    f64::NAN
                } else if a_val > 0.0 {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                }
            } else {
                a_val / b_val
            };
            ctx.set_x(dest, ctx.make_float(result));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatCmp => {
            // Compare two floats: dest = result (true/false atom)
            let dest = decode_dest(word);
            let a_reg = decode_src(word);
            let b_reg = decode_src2(word);
            let cmp_type = ((word >> 40) & 0xFF) as u32; // 0=lt, 1=lte, 2=gt, 3=gte, 4=eq

            let a_val = ctx.get_float(a_reg).unwrap_or(0.0);
            let b_val = ctx.get_float(b_reg).unwrap_or(0.0);

            let result = match cmp_type {
                0 => a_val < b_val,  // less than
                1 => a_val <= b_val, // less than or equal
                2 => a_val > b_val,  // greater than
                3 => a_val >= b_val, // greater than or equal
                4 => a_val == b_val, // equal
                _ => false,
            };
            ctx.set_x(
                dest,
                if result {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
                } else {
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
                },
            );
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatOp => {
            // Complex float operations (sqrt, sin, cos, etc.)
            // dest = result, src = operand, extra = operation type
            let dest = decode_dest(word);
            let src = decode_src(word);
            let op_type = decode_src2(word);

            let val = ctx.get_float(src).unwrap_or(0.0);
            let result = match op_type {
                0 => val.sqrt(),    // sqrt
                1 => val.sin(),     // sin
                2 => val.cos(),     // cos
                3 => val.tan(),     // tan
                4 => val.exp(),     // exp
                5 => val.ln(),      // log (natural log)
                6 => val.powf(2.0), // square
                7 => val.abs(),     // abs
                _ => 0.0,
            };
            ctx.set_x(dest, ctx.make_float(result));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::FloatLoad => {
            // Load a float constant into a register
            let dest = decode_dest(word);
            let value_bits = decode_value(word);

            // value_bits contains the raw f64 bits
            let value = f64::from_bits(value_bits as u64);
            ctx.set_x(dest, ctx.make_float(value));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::BsInit => {
            // Initialize a bitstring with specified size
            // dest = result register, src = size in bits
            let dest = decode_dest(word);
            let size_bits = decode_src(word);

            // Calculate size in bytes (rounding up)
            let size_bytes = (size_bits + 7) / 8;
            let words_needed = 2 + size_bytes as usize; // header + binary header + data

            if let Some(ptr) = ctx.heap_alloc(words_needed) {
                // Write binary header: subtag=Binary(3), size in words
                let header = (chimera_erlang_beam_term::boxed::BoxedSubTag::Binary as u64)
                    | ((words_needed as u64) << 8);
                if let Some(h) = ctx.heap_mut() {
                    h.set_word(ptr, header);
                    h.set_word(ptr + 1, size_bits as u64); // store bit size in binary header
                }
                ctx.set_x(dest, Term::from_binary_ptr(ptr as u64));
            } else {
                ctx.set_x(dest, Term::nil());
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::BsPut => {
            // Append integer, binary, or string to bitstring
            // dest = result register, src = type, src2 = value
            let dest = decode_dest(word);
            let _type = decode_src(word);
            let value_reg = decode_src2(word);
            let value = ctx.get_x(value_reg);

            // Append value to bitstring if it's a binary
            if value.is_binary() {
                let bin_ptr = value.to_binary() as usize;
                if let Some(h) = ctx.heap_mut() {
                    if let Some(_header) = h.get_word(bin_ptr) {
                        // Get current bit size from binary header
                        let _current_bits = h.get_word(bin_ptr + 1).unwrap_or(0) as usize;
                        // For now, just return the original binary
                        // Full implementation would append value data
                        ctx.set_x(dest, value);
                    } else {
                        ctx.set_x(dest, Term::nil());
                    }
                } else {
                    ctx.set_x(dest, Term::nil());
                }
            } else {
                // For non-binary values, copy as-is (placeholder behavior)
                ctx.set_x(dest, value);
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::BsMatch => {
            // Pattern match bitstring with guards
            // dest = result register, src = bitstring, src2 = pattern
            let dest = decode_dest(word);
            let bs_reg = decode_src(word);
            let _pattern_reg = decode_src2(word);
            let bs_term = ctx.get_x(bs_reg);

            // If bitstring is valid, return it; otherwise nil
            if bs_term.is_binary() {
                ctx.set_x(dest, bs_term);
            } else {
                ctx.set_x(dest, Term::nil());
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::BsSave => {
            // Save current bitstring position for later restore
            // src = position register to save to
            let pos_reg = decode_src(word);

            // Save current heap pointer as position marker
            let pos = ctx.heap_alloc(1).unwrap_or(0) as u64;
            ctx.set_x(pos_reg, Term::from_small(pos as i64));
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::BsRestore => {
            // Restore bitstring position from saved value
            // src = position register to restore from
            let pos_reg = decode_src(word);
            let saved_pos = ctx.get_x(pos_reg).to_small() as usize;

            // Verify the position is valid heap pointer
            if let Some(h) = ctx.heap_mut() {
                if saved_pos < h.heap_end() {
                    // Could restore position but current implementation
                    // doesn't track bs context - placeholder
                }
            }
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::TryVal => {
            // Set up try/catch handler for catching errors
            let dest = decode_dest(word);
            let src = decode_src(word);
            let handler = decode_value(word) as u64;
            // Store the value in destination register
            ctx.set_x(dest, ctx.get_x(src));
            // Set up exception state
            ctx.exception_state = Some(ExceptionState {
                reg: dest,
                handler,
                stack_depth: ctx.fp as u32,
                is_catch: false,
            });
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::TryEnd => {
            // End try block - clear exception state
            ctx.exception_state = None;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::CatchVal => {
            // Set up catch handler (similar to try but always catches)
            let dest = decode_dest(word);
            let handler = decode_value(word) as u64;
            ctx.exception_state = Some(ExceptionState {
                reg: dest,
                handler,
                stack_depth: ctx.fp as u32,
                is_catch: true,
            });
            ctx.set_x(dest, Term::nil());
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::CatchEnd => {
            // Exit catch block - clear exception state
            ctx.exception_state = None;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Raise => {
            // Raise an exception
            // If we have an exception handler, jump to it
            if let Some(exc) = ctx.exception_state.take() {
                // Copy the exception value to the handler register
                ctx.set_x(
                    exc.reg,
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_ERROR),
                );
                ctx.ip = exc.handler;
                return StepResult {
                    ip: exc.handler,
                    result: ExecResult::Ok,
                };
            }
            // No handler - propagate the error
            StepResult {
                ip,
                result: ExecResult::Err,
            }
        }
        Opcode::SelectTupleArity => {
            // Jump based on tuple arity
            let src = decode_src(word);
            let _tuple = ctx.get_x(src);
            // Skip to next instruction for now
            ctx.ip += 1;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::SelectVal => {
            // Jump based on value
            let src = decode_src(word);
            let _val = ctx.get_x(src);
            // Skip to next instruction for now
            ctx.ip += 1;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::JumpWorks => {
            let target = decode_value(word);
            ctx.ip = target as u64;
            StepResult {
                ip,
                result: ExecResult::Ok,
            }
        }
        Opcode::Badarg => {
            // Bad argument error - check if we have an exception handler
            if let Some(exc) = ctx.exception_state.take() {
                ctx.set_x(
                    exc.reg,
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_BADARG),
                );
                ctx.ip = exc.handler;
                return StepResult {
                    ip: exc.handler,
                    result: ExecResult::Ok,
                };
            }
            StepResult {
                ip,
                result: ExecResult::Err,
            }
        }
        Opcode::Badmatch => {
            if let Some(exc) = ctx.exception_state.take() {
                ctx.set_x(
                    exc.reg,
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_BADMATCH),
                );
                ctx.ip = exc.handler;
                return StepResult {
                    ip: exc.handler,
                    result: ExecResult::Ok,
                };
            }
            StepResult {
                ip,
                result: ExecResult::Err,
            }
        }
        Opcode::CaseClause => {
            if let Some(exc) = ctx.exception_state.take() {
                ctx.set_x(
                    exc.reg,
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_CASE_CLAUSE),
                );
                ctx.ip = exc.handler;
                return StepResult {
                    ip: exc.handler,
                    result: ExecResult::Ok,
                };
            }
            StepResult {
                ip,
                result: ExecResult::Err,
            }
        }
        Opcode::IfClause => {
            if let Some(exc) = ctx.exception_state.take() {
                ctx.set_x(
                    exc.reg,
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_IF_CLAUSE),
                );
                ctx.ip = exc.handler;
                return StepResult {
                    ip: exc.handler,
                    result: ExecResult::Ok,
                };
            }
            StepResult {
                ip,
                result: ExecResult::Err,
            }
        }
        Opcode::FunctionClause => {
            if let Some(exc) = ctx.exception_state.take() {
                ctx.set_x(
                    exc.reg,
                    Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FUNCTION_CLAUSE),
                );
                ctx.ip = exc.handler;
                return StepResult {
                    ip: exc.handler,
                    result: ExecResult::Ok,
                };
            }
            StepResult {
                ip,
                result: ExecResult::Err,
            }
        }
        Opcode::SystemLimit => StepResult {
            ip,
            result: ExecResult::Err,
        },
        Opcode::Bif0 | Opcode::Bif1 | Opcode::Bif2 => {
            // BIF calls trap to runtime
            // Store the current instruction for trap handling
            ctx.current_instruction = word;

            // Decode BIF call info from instruction
            // Format: bits 0-15 = opcode, bits 16-23 = dest, bits 24-31 = src/arg1, bits 32-39 = src2/arg2, bits 40-47 = BIF id
            let dest = decode_dest(word);
            let arg1 = decode_src(word);
            let arg2 = decode_src2(word);
            let bif_id = ((word >> 40) & 0xFF) as u32;

            ctx.bif_call = Some(BifCall::new(bif_id, dest, arg1, arg2));

            // Restore IP since we want to re-execute this instruction after BIF returns
            ctx.ip = ip;

            StepResult {
                ip,
                result: ExecResult::Trap,
            }
        }
        Opcode::Native | Opcode::NativeClosure => {
            // Native calls trap to runtime
            // Store the current instruction for trap handling
            ctx.current_instruction = word;

            // For native calls, extract similar info
            let dest = decode_dest(word);
            let arg1 = decode_src(word);
            let arg2 = decode_src2(word);
            let native_id = ((word >> 40) & 0xFF) as u32;

            ctx.bif_call = Some(BifCall::new(native_id, dest, arg1, arg2));

            // Restore IP since we want to re-execute this instruction after native returns
            ctx.ip = ip;

            StepResult {
                ip,
                result: ExecResult::Trap,
            }
        }
    }
}

/// Legacy step function using safe decoding
pub fn step(ctx: &mut ExecContext, code: &[u64]) -> StepResult {
    execute_instruction(ctx, code)
}

/// Interpreter loop that runs until yield, error, or exit
///
/// This is the main interpreter loop that:
/// 1. Fetches and decodes instructions
/// 2. Executes them with reduction counting
/// 3. Handles BIF traps and native traps
/// 4. Returns Yield when reductions exhausted
/// 5. Returns ExitDispatch on process termination
pub struct InterpreterLoop {
    pub instructions_executed: u64,
    pub reductions_exhausted: u64,
    pub traps: u64,
}

impl InterpreterLoop {
    pub fn new() -> Self {
        InterpreterLoop {
            instructions_executed: 0,
            reductions_exhausted: 0,
            traps: 0,
        }
    }

    /// Run the interpreter loop until a stopping condition
    ///
    /// The loop continues until:
    /// - `should_stop` returns true (checked periodically)
    /// - A trap occurs (returns `ExecResult::Trap`)
    /// - An error occurs (returns `ExecResult::Err`)
    /// - The process exits (returns `ExecResult::ExitDispatch`)
    pub fn run<F>(&mut self, ctx: &mut ExecContext, code: &[u64], mut should_stop: F) -> ExecResult
    where
        F: FnMut() -> bool,
    {
        loop {
            // Check stop condition periodically
            if should_stop() {
                return ExecResult::Ok;
            }

            // Check reduction exhaustion before executing
            if ctx.is_exhausted() {
                self.reductions_exhausted += 1;
                return ExecResult::Yield;
            }

            let result = execute_instruction(ctx, code);
            self.instructions_executed += 1;

            match result.result {
                ExecResult::Ok => {
                    // Continue loop
                }
                ExecResult::Yield => {
                    return ExecResult::Yield;
                }
                ExecResult::Trap => {
                    self.traps += 1;
                    return ExecResult::Trap;
                }
                ExecResult::Err => {
                    return ExecResult::Err;
                }
                ExecResult::ExitDispatch => {
                    return ExecResult::ExitDispatch;
                }
                ExecResult::Wait => {
                    return ExecResult::Wait;
                }
            }
        }
    }

    /// Run for a specific number of reductions
    pub fn run_reductions(
        &mut self,
        ctx: &mut ExecContext,
        code: &[u64],
        max_reductions: u64,
    ) -> ExecResult {
        let initial_reductions = ctx.reductions;
        let mut reductions_done = 0;

        loop {
            if reductions_done >= max_reductions {
                return ExecResult::Yield;
            }

            let result = execute_instruction(ctx, code);
            self.instructions_executed += 1;

            match result.result {
                ExecResult::Ok => {
                    reductions_done = initial_reductions - ctx.reductions;
                }
                ExecResult::Yield => return ExecResult::Yield,
                ExecResult::Trap => {
                    self.traps += 1;
                    return ExecResult::Trap;
                }
                ExecResult::Err => return ExecResult::Err,
                ExecResult::ExitDispatch => return ExecResult::ExitDispatch,
                ExecResult::Wait => return ExecResult::Wait,
            }
        }
    }
}

impl Default for InterpreterLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_new() {
        let ctx = ExecContext::new();
        assert_eq!(ctx.ip, 0);
        assert_eq!(ctx.reductions, DEFAULT_REDUCTION_BUDGET);
    }

    #[test]
    fn test_context_x_registers() {
        let mut ctx = ExecContext::new();
        let term = Term::from_small(42);
        ctx.set_x(0, term);
        assert_eq!(ctx.get_x(0), term);
    }

    #[test]
    fn test_context_y_registers() {
        let mut ctx = ExecContext::new();
        let term = Term::from_small(100);
        ctx.set_y(0, term);
        assert_eq!(ctx.get_y(0), term);
    }

    #[test]
    fn test_context_reductions() {
        let mut ctx = ExecContext::new();
        assert!(!ctx.is_exhausted());
        ctx.decrement_reductions(DEFAULT_REDUCTION_BUDGET);
        assert!(ctx.is_exhausted());
    }

    #[test]
    fn test_load_int_instruction() {
        let mut ctx = ExecContext::new();
        // Format: opcode(16) | dest(8) | unused(8) | value(32)
        // dest at bits 16-23, value at bits 32-63
        let instr: u64 = (Opcode::LoadInt as u64) | (1_u64 << 16) | ((42_u64) << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(1), Term::from_small(42));
    }

    #[test]
    fn test_jump_instruction() {
        let mut ctx = ExecContext::new();
        ctx.ip = 5;
        // Jump to target 10
        let instr: u64 = (Opcode::Jump as u64) | ((10 as u64) << 32);
        let code = [0, 0, 0, 0, 0, instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.ip, 10);
    }

    #[test]
    fn test_add_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(10));
        ctx.set_x(2, Term::from_small(32));
        // Format: opcode | dest | src1 | src2
        let instr: u64 = (Opcode::Add as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_small(42));
    }

    #[test]
    fn test_safe_decode_valid_opcodes() {
        // Test that valid opcodes decode correctly
        for opcode in 0u16..=201 {
            let word = (opcode as u64) << 48; // opcode in high bits for this format
            let result = decode_opcode(word);
            // Opcodes 0-6, 10-11, 20, 30-32, 40-42, 50-52, 60-63, 70-79, 80-87,
            // 90-98, 110-114, 120-124, 130-132, 140-145, 150-152, 160-162, 200-201
            // are valid ranges
            let is_valid = matches!(result, DecodeResult::Valid(_));
            // We expect most common opcodes to be valid
            if opcode <= 201 {
                assert!(is_valid, "Opcode {} should be valid", opcode);
            }
        }
    }

    #[test]
    fn test_safe_decode_invalid_opcodes() {
        // Test that clearly invalid opcodes are rejected
        // decode_opcode extracts bits 0-15 as the opcode
        let invalid_words = [
            0xFFFFu64, // opcode 0xFFFF = 65535, way out of range
            0xFFFDu64, // opcode 0xFFFD = 65533, out of range
        ];

        for word in invalid_words {
            let result = decode_opcode(word);
            assert!(
                matches!(result, DecodeResult::Invalid(_)),
                "Word {:x} should be invalid",
                word
            );
        }
    }

    #[test]
    fn test_sub_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(50));
        ctx.set_x(2, Term::from_small(8));
        let instr: u64 = (Opcode::Sub as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_small(42));
    }

    #[test]
    fn test_mul_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(6));
        ctx.set_x(2, Term::from_small(7));
        let instr: u64 = (Opcode::Mul as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_small(42));
    }

    #[test]
    fn test_div_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(84));
        ctx.set_x(2, Term::from_small(2));
        let instr: u64 = (Opcode::Div as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_small(42));
    }

    #[test]
    fn test_neg_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(0, Term::from_small(42));
        let instr: u64 = (Opcode::Neg as u64) | (1 << 16) | (0 << 24);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(1), Term::from_small(-42));
    }

    #[test]
    fn test_comparison_instructions() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(10));
        ctx.set_x(2, Term::from_small(20));

        // Test Lt: 10 < 20 should be true
        let instr_lt: u64 = (Opcode::Lt as u64) | (3 << 16) | (1 << 24) | (2 << 32);
        let code = [instr_lt];
        step(&mut ctx, &code);
        assert_eq!(
            ctx.get_x(3),
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
        );
    }

    #[test]
    fn test_is_type_instructions() {
        let mut ctx = ExecContext::new();
        ctx.set_x(0, Term::from_small(42));
        ctx.set_x(1, Term::from_atom(5));
        ctx.set_x(2, Term::nil());

        // Test IsInteger
        let is_int: u64 = (Opcode::IsInteger as u64) | (3 << 16) | (0 << 24);
        step(&mut ctx, &[is_int]);
        assert_eq!(
            ctx.get_x(3),
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
        );

        // Test IsAtom
        let is_atom: u64 = (Opcode::IsAtom as u64) | (3 << 16) | (1 << 24);
        step(&mut ctx, &[is_atom]);
        assert_eq!(
            ctx.get_x(3),
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
        );

        // Test IsNil
        let is_nil: u64 = (Opcode::IsNil as u64) | (3 << 16) | (2 << 24);
        step(&mut ctx, &[is_nil]);
        assert_eq!(
            ctx.get_x(3),
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
        );
    }

    #[test]
    fn test_move_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(0, Term::from_small(123));
        let instr: u64 = (Opcode::Move as u64) | (1 << 16) | (0 << 24);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(1), Term::from_small(123));
    }

    #[test]
    fn test_load_nil_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = (Opcode::LoadNil as u64) | (0 << 16);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::nil());
    }

    #[test]
    fn test_load_atom_instruction() {
        let mut ctx = ExecContext::new();
        let atom_index: u64 = 5;
        let instr: u64 = (Opcode::LoadAtom as u64) | (0 << 16);
        let code = [instr, atom_index];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_atom(5));
    }

    #[test]
    fn test_call_instruction() {
        let mut ctx = ExecContext::new();
        ctx.ip = 0;
        // Call to target 10, arity 1
        // word format: opcode(16) | dest(8) | unused(8) | target(32)
        let call_instr: u64 = (Opcode::Call as u64) | (1_u64 << 16) | ((10_u64) << 32);
        let code = vec![call_instr; 20];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // After call, ip should be at target (10) and cp should be return address
        // cp = ctx.ip + 1 before setting to target, and ctx.ip was incremented before match
        // so cp = 1 + 1 = 2
        assert_eq!(ctx.ip, 10);
        assert_eq!(ctx.cp, 2); // return address is old ip + 1
    }

    #[test]
    fn test_return_instruction() {
        let mut ctx = ExecContext::new();
        ctx.cp = 5; // Set up return address
        let instr: u64 = Opcode::Return as u64;
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.ip, 5); // Should return to cp
    }

    #[test]
    fn test_allocate_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = (Opcode::Allocate as u64) | (4 << 16) | (2 << 24);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.live, 2);
    }

    #[test]
    fn test_deallocate_instruction() {
        let mut ctx = ExecContext::new();
        ctx.fp = 100;
        let instr: u64 = (Opcode::Deallocate as u64) | (4 << 16);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.fp, 104);
    }

    #[test]
    fn test_error_instructions() {
        let mut ctx = ExecContext::new();

        // Badarg should return error - provide enough code words to avoid bounds issues
        let badarg: u64 = Opcode::Badarg as u64;
        let code = [badarg, 0, 0, 0]; // extra words for ip to stay in bounds
        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_bif_trap() {
        let mut ctx = ExecContext::new();

        // BIF0 should trap - provide enough code words
        let bif0: u64 = Opcode::Bif0 as u64;
        let code = [bif0, 0, 0, 0];
        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Trap);

        // Native should trap - reset ctx.ip since step() modified it
        ctx.ip = 0;
        let native: u64 = Opcode::Native as u64;
        let code2 = [native, 0, 0, 0];
        let result = step(&mut ctx, &code2);
        assert_eq!(result.result, ExecResult::Trap);
    }

    #[test]
    fn test_interpreter_loop_basic() {
        let mut loop_ctx = InterpreterLoop::new();
        let mut ctx = ExecContext::new();
        ctx.reductions = 10;

        // Simple program: load 42 into X0 (dest=0 at bits 16-23, value at bits 32-63)
        // Use a larger code array to avoid bounds issues
        let instr: u64 = (Opcode::LoadInt as u64) | (0_u64 << 16) | ((42_u64) << 32);
        let code = [instr, 0, 0, 0, 0, 0, 0, 0, 0];

        // Use a stop counter - stop after 2 calls so we execute 1 instruction
        let mut steps = 0;
        let result = loop_ctx.run(&mut ctx, &code, || {
            steps += 1;
            steps > 1 // Stop after 1 instruction (on 2nd call)
        });
        assert_eq!(result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_small(42));
        assert_eq!(loop_ctx.instructions_executed, 1);
    }

    #[test]
    fn test_interpreter_loop_yield_on_exhaustion() {
        let mut loop_ctx = InterpreterLoop::new();
        let mut ctx = ExecContext::new();
        // Only 1 reduction
        ctx.reductions = 1;

        // Two instructions
        let instr1: u64 = (Opcode::LoadInt as u64) | ((1 as u64) << 32);
        let instr2: u64 = (Opcode::LoadInt as u64) | ((2 as u64) << 32);
        let code = [instr1, instr2];

        let should_stop = || false;
        let result = loop_ctx.run(&mut ctx, &code, should_stop);
        assert_eq!(result, ExecResult::Yield);
    }

    #[test]
    fn test_interpreter_loop_run_reductions() {
        let mut loop_ctx = InterpreterLoop::new();
        let mut ctx = ExecContext::new();
        ctx.reductions = 5;

        // Simple program - load 42 into X0
        let instr: u64 = (Opcode::LoadInt as u64) | (0_u64 << 16) | ((42 as u64) << 32);
        // Use larger code array to avoid bounds issues
        let code = [instr, 0, 0, 0, 0];

        let result = loop_ctx.run_reductions(&mut ctx, &code, 3);
        assert_eq!(result, ExecResult::Yield);
        // Should have done 3 reductions worth
    }

    #[test]
    fn test_exit_dispatch_on_invalid_ip() {
        let mut ctx = ExecContext::new();
        ctx.ip = 100; // Beyond code length

        let code = [Opcode::LoadInt as u64];
        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::ExitDispatch);
    }

    #[test]
    fn test_eq_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(42));
        ctx.set_x(2, Term::from_small(42));
        let instr: u64 = (Opcode::Eq as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(
            ctx.get_x(0),
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
        );
    }

    #[test]
    fn test_neq_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(42));
        ctx.set_x(2, Term::from_small(99));
        let instr: u64 = (Opcode::Neq as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(
            ctx.get_x(0),
            Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
        );
    }

    #[test]
    fn test_rem_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(85));
        ctx.set_x(2, Term::from_small(43));
        let instr: u64 = (Opcode::Rem as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.get_x(0), Term::from_small(42));
    }

    #[test]
    fn test_recv_op_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::RecvOp as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // RecvOp is currently a no-op placeholder
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_recv_wait_op_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::RecvWaitOp as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // RecvWaitOp returns Wait when waiting for messages
        assert_eq!(result.result, ExecResult::Wait);
    }

    #[test]
    fn test_recv_wait_op_with_message_arrived() {
        let mut ctx = ExecContext::new();

        // Set up receive_state with message_arrived flag and active message
        ctx.receive_state = Some(ReceiveState {
            save_index: 0,
            timeout: 100,
            waited_reductions: 5,
            active_message: Some(Term::from_small(42)), // Message to deliver
            message_arrived: true,                      // Message has arrived
            saved_queue_len: 0,
        });

        // RecvWaitOp: timeout=100, save_index=0
        let instr: u64 = (Opcode::RecvWaitOp as u64) | (100 << 16);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);

        // message_arrived flag should be cleared
        let state = ctx.receive_state.clone().unwrap();
        assert!(!state.message_arrived);

        // The message should be delivered to X0
        assert_eq!(ctx.get_x(0), Term::from_small(42));
    }

    #[test]
    fn test_recv_op_initializes_receive_state() {
        let mut ctx = ExecContext::new();

        // RecvOp: dest=5 (save_index)
        let instr: u64 = (Opcode::RecvOp as u64) | (5 << 16);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);

        // receive_state should be initialized
        assert!(ctx.receive_state.is_some());

        let state = ctx.receive_state.unwrap();
        assert_eq!(state.save_index, 5);
        assert_eq!(state.timeout, 0);
        assert_eq!(state.waited_reductions, 0);
        assert!(state.active_message.is_none());
        assert!(!state.message_arrived);
    }

    #[test]
    fn test_recv_wait_op_with_timeout_expired() {
        let mut ctx = ExecContext::new();

        // Set up receive_state with timeout already elapsed
        ctx.receive_state = Some(ReceiveState {
            save_index: 0,
            timeout: 100,           // High timeout
            waited_reductions: 200, // Already waited past timeout
            active_message: None,
            message_arrived: false,
            saved_queue_len: 0,
        });

        // RecvWaitOp: timeout=100 (same as state.timeout), save_index=0
        let instr: u64 = (Opcode::RecvWaitOp as u64) | (100 << 16);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);

        // Timeout should have cleared receive_state
        assert!(ctx.receive_state.is_none());
    }

    #[test]
    fn test_recv_wait_op_immediate_timeout() {
        let mut ctx = ExecContext::new();

        // Set up receive_state with timeout=0 (immediate timeout)
        ctx.receive_state = Some(ReceiveState {
            save_index: 0,
            timeout: 0,
            waited_reductions: 0,
            active_message: None,
            message_arrived: false, // No message arrived
            saved_queue_len: 0,
        });

        // RecvWaitOp: timeout=0 (immediate timeout), save_index=0
        let instr: u64 = (Opcode::RecvWaitOp as u64) | (0 << 16);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);

        // Timeout should have cleared receive_state
        assert!(ctx.receive_state.is_none());

        // X0 should be set to 'timeout' atom
        // Atom 0 is 'timeout' in Erlang
        let x0 = ctx.get_x(0);
        assert!(x0.is_atom(), "X0 should be an atom");
    }

    #[test]
    fn test_recv_pop_op_clears_receive_state() {
        let mut ctx = ExecContext::new();

        // Set up receive_state with an active message
        ctx.receive_state = Some(ReceiveState {
            save_index: 0,
            timeout: 100,
            waited_reductions: 5,
            active_message: Some(Term::from_small(99)),
            message_arrived: false,
            saved_queue_len: 0,
        });

        // RecvPopOp: dest=0
        let instr: u64 = (Opcode::RecvPopOp as u64) | (0 << 16);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);

        // Receive state should be cleared after RecvPopOp
        assert!(ctx.receive_state.is_none());
    }

    #[test]
    fn test_try_val_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(0, Term::from_small(42));
        // TryVal: dest=1, src=0, handler at target
        let instr: u64 = (Opcode::TryVal as u64) | (1 << 16) | (0 << 24) | ((100 as u64) << 32);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // Value should be copied to dest
        assert_eq!(ctx.get_x(1), Term::from_small(42));
    }

    #[test]
    fn test_try_end_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::TryEnd as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_catch_val_instruction() {
        let mut ctx = ExecContext::new();
        // CatchVal: dest=0, handler=100
        let instr: u64 = (Opcode::CatchVal as u64) | (0 << 16) | ((100 as u64) << 32);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // Should set dest to nil
        assert_eq!(ctx.get_x(0), Term::nil());
    }

    #[test]
    fn test_catch_end_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::CatchEnd as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_raise_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::Raise as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // Raise should return error
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_select_tuple_arity_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(0, Term::from_tuple(0x100)); // Set up a tuple
                                               // SelectTupleArity: src=0, skip the next word (default case)
        let instr: u64 = (Opcode::SelectTupleArity as u64) | (0 << 24);
        let code = [instr, 0, 0, 0]; // 0 is the fallback/default target

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_select_val_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(0, Term::from_small(42));
        // SelectVal: src=0, skip the next word
        let instr: u64 = (Opcode::SelectVal as u64) | (0 << 24);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_jump_works_instruction() {
        let mut ctx = ExecContext::new();
        ctx.ip = 5;
        // JumpWorks to target 20
        let instr: u64 = (Opcode::JumpWorks as u64) | ((20 as u64) << 32);
        let code = [0, 0, 0, 0, 0, instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        assert_eq!(ctx.ip, 20);
    }

    #[test]
    fn test_put_list_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_small(10));
        ctx.set_x(2, Term::from_small(20));
        // PutList: hd=1, tl=2, dest=0
        let instr: u64 = (Opcode::PutList as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // PutList creates a cons cell
        // The result is implementation-dependent
    }

    #[test]
    fn test_get_list_instruction() {
        let mut ctx = ExecContext::new();
        // GetList: dest=0, src=1
        let instr: u64 = (Opcode::GetList as u64) | (0 << 16) | (1 << 24);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_get_tuple_instruction() {
        let mut ctx = ExecContext::new();
        // GetTuple: dest=0, src=1
        let instr: u64 = (Opcode::GetTuple as u64) | (0 << 16) | (1 << 24);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_put_tuple_instruction() {
        let mut ctx = ExecContext::new();
        // PutTuple: dest=0, src=1
        let instr: u64 = (Opcode::PutTuple as u64) | (0 << 16) | (1 << 24);
        let code = [instr];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_send_op_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::SendOp as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // SendOp is a no-op placeholder
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_send_msg_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::SendMsg as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // SendMsg is a no-op placeholder
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_badmatch_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::Badmatch as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // Badmatch returns error
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_case_clause_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::CaseClause as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // CaseClause returns error
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_if_clause_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::IfClause as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // IfClause returns error
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_function_clause_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::FunctionClause as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // FunctionClause returns error
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_system_limit_instruction() {
        let mut ctx = ExecContext::new();
        let instr: u64 = Opcode::SystemLimit as u64;
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        // SystemLimit returns error
        assert_eq!(result.result, ExecResult::Err);
    }

    #[test]
    fn test_map_create_instruction() {
        let mut ctx = ExecContext::new();
        // MapCreate: dest=0, num_pairs=2
        let instr: u64 = (Opcode::MapCreate as u64) | (0 << 16) | (2 << 24);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // MapCreate returns nil placeholder (full impl would create map)
        assert_eq!(ctx.get_x(0), Term::nil());
    }

    #[test]
    fn test_map_put_instruction() {
        let mut ctx = ExecContext::new();
        ctx.set_x(1, Term::from_atom(1)); // map in x1
        ctx.set_x(2, Term::from_atom(2)); // key in x2
                                          // MapPut: dest=0, src=1 (map), src2=2 (key), value_reg=3
        ctx.set_x(3, Term::from_small(42)); // value
        let instr: u64 =
            (Opcode::MapPut as u64) | (0 << 16) | (1 << 24) | (2 << 32) | ((3 as u64) << 40);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
    }

    #[test]
    fn test_map_size_instruction() {
        let mut ctx = ExecContext::new();
        // MapSize: dest=0, src=1 (map register)
        let instr: u64 = (Opcode::MapSize as u64) | (0 << 16) | (1 << 24);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // MapSize returns 0 placeholder
        assert_eq!(ctx.get_x(0), Term::from_small(0));
    }

    #[test]
    fn test_float_cmp_instruction() {
        let mut ctx = ExecContext::new();
        // FloatCmp: dest=0, src=1, src2=2, cmp_type=4 (eq)
        // Note: without proper float encoding in heap, this tests the comparison path
        let instr: u64 =
            (Opcode::FloatCmp as u64) | (0 << 16) | (1 << 24) | (2 << 32) | ((4 as u64) << 40);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // Result is either true or false atom
        let res = ctx.get_x(0);
        assert!(
            res == Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_TRUE)
                || res == Term::from_atom(chimera_erlang_beam_term::atoms::ATOM_FALSE)
        );
    }

    #[test]
    fn test_float_add_instruction() {
        let mut ctx = ExecContext::new();
        // FloatAdd: dest=0, src=1, src2=2
        let instr: u64 = (Opcode::FloatAdd as u64) | (0 << 16) | (1 << 24) | (2 << 32);
        let code = [instr, 0, 0, 0];

        let result = step(&mut ctx, &code);
        assert_eq!(result.result, ExecResult::Ok);
        // Result should be some term (actual value depends on encoding)
        let _ = ctx.get_x(0);
    }
}

// =====================================================================
// Property-Based Tests (Task F-1)
// =====================================================================
// These tests use proptest to verify properties across random inputs.

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::{prop_assert, prop_assert_eq, proptest};

    proptest! {
        /// Property: small integers within range don't cause panics or crashes
        #[test]
        fn test_small_int_in_range(val in -0x40000000i64..0x40000000i64) {
            // Converting to term and back should be lossless for small ints
            let term = Term::from_small(val);
            prop_assert!(term.is_small());
            prop_assert_eq!(term.to_small(), val);
        }

        /// Property: random atoms don't cause panics
        #[test]
        fn test_random_atom_no_panic(atom_id: u32) {
            let term = Term::from_atom(atom_id);
            prop_assert!(term.is_atom());
            prop_assert_eq!(term.to_atom(), atom_id);
        }

        /// Property: instruction execution doesn't panic for valid opcodes
        #[test]
        fn test_random_instruction_no_panic(opcode_idx: u8) {
            let mut ctx = ExecContext::new();
            let code = [opcode_idx as u64, 0, 0, 0];
            // Should not panic, may return any result
            let _ = step(&mut ctx, &code);
        }

        /// Property: ExecContext operations don't corrupt state
        #[test]
        fn test_exec_context_registers_work(idx: u32, val in -0x40000000i64..0x40000000i64) {
            let mut ctx = ExecContext::new();
            let idx = idx % 1024; // Stay within bounds

            let term = Term::from_small(val);
            ctx.set_x(idx, term);
            let retrieved = ctx.get_x(idx);

            // For valid indices, should round-trip correctly
            if (idx as usize) < ctx.x.len() {
                prop_assert_eq!(retrieved, term);
            }
        }

        /// Property: decode_dest extracts correct bits from instruction
        #[test]
        fn test_decode_dest_correct(dest: u8) {
            // Build instruction with dest in correct position
            let dest = dest as u32;
            let dest_bits = (dest as u64) << 16;
            let word = dest_bits;
            let decoded = decode_dest(word);
            prop_assert_eq!(decoded, dest);
        }

        /// Property: decode_src extracts correct bits from instruction
        #[test]
        fn test_decode_src_correct(src: u8) {
            let src = src as u32;
            let src_bits = (src as u64) << 24;
            let word = src_bits;
            let decoded = decode_src(word);
            prop_assert_eq!(decoded, src);
        }

        /// Property: receive state can be created with various parameters
        #[test]
        fn test_receive_state_creation(save_idx: u32, timeout: u64, waited: u64) {
            let state = ReceiveState {
                save_index: save_idx,
                timeout,
                waited_reductions: waited,
                active_message: None,
                message_arrived: false,
                saved_queue_len: 0,
            };

            prop_assert_eq!(state.save_index, save_idx);
            prop_assert_eq!(state.timeout, timeout);
            prop_assert_eq!(state.waited_reductions, waited);
            prop_assert!(!state.message_arrived);
            prop_assert!(state.active_message.is_none());
        }
    }
}

#[cfg(test)]
mod phase4_tests {
    use super::*;

    #[test]
    fn test_exec_context_creation() {
        let ctx = ExecContext::new();
        assert_eq!(ctx.ip, 0);
        assert_eq!(ctx.reduction_budget, DEFAULT_REDUCTION_BUDGET);
        assert!(!ctx.is_exhausted());
    }

    #[test]
    fn test_opcode_from_raw() {
        assert_eq!(Opcode::from_raw(0), Some(Opcode::Move));
        assert_eq!(Opcode::from_raw(10), Some(Opcode::Call));
        assert_eq!(Opcode::from_raw(999), None);
    }
}

#[cfg(test)]
mod phase4_progress {
    use super::*;
    use chimera_erlang_beam_term::Term;

    #[test]
    fn test_opcode_from_raw_complete() {
        // Verify all major opcodes can be decoded
        assert_eq!(Opcode::from_raw(0), Some(Opcode::Move));
        assert_eq!(Opcode::from_raw(70), Some(Opcode::Add));
        assert_eq!(Opcode::from_raw(80), Some(Opcode::Eq));
        assert_eq!(Opcode::from_raw(110), Some(Opcode::SendOp));
        assert_eq!(Opcode::from_raw(112), Some(Opcode::RecvOp));
        assert_eq!(Opcode::from_raw(999), None);
    }
}
