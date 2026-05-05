//! IR - Intermediate Representation for JIT
//!
//! Converts BEAM bytecode to an intermediate representation suitable
//! for optimization and code generation.

#pragma once

#include <cstdint>
#include <vector>
#include <string>
#include <variant>
#include <memory>

namespace ir {

// x86-64 register IDs
enum class Reg : uint8_t {
    rax = 0, rcx = 1, rdx = 2, rbx = 3,
    rsp = 4, rbp = 5, rsi = 6, rdi = 7,
    r8 = 8, r9 = 9, r10 = 10, r11 = 11,
    r12 = 12, r13 = 13, r14 = 14, r15 = 15
};

// Immediate value
struct Imm {
    int64_t value;
    explicit Imm(int64_t v) : value(v) {}
};

// Memory operand
struct Mem {
    Reg base;
    int32_t offset;
    Mem(Reg b, int32_t o = 0) : base(b), offset(o) {}
};

// Operand types for instructions
using Operand = std::variant<Reg, Imm, Mem>;

// Instruction opcodes
enum class Opcode : uint8_t {
    MOV,
    ADD,
    SUB,
    IMUL,
    CMP,
    JMP,
    JE,
    JNE,
    JL,
    JLE,
    JG,
    JGE,
    CALL,
    RET,
    PUSH,
    POP,
    LABEL,
    LOAD,
    STORE,
    NOP
};

// Value types for SSA
enum class ValueType : uint8_t {
    I64,
    I32,
    POINTER,
    FLAG
};

// SSA value
struct Value {
    ValueType type;
    std::variant<Reg, int64_t> representation;
    Value(ValueType t, Reg r) : type(t), representation(r) {}
    Value(ValueType t, int64_t v) : type(t), representation(v) {}
};

// Basic block
struct BasicBlock {
    std::string label;
    std::vector<Opcode> ops;
    std::vector<std::vector<Value>> operands;
};

// Function representation
struct Function {
    uint32_t arity;
    std::vector<BasicBlock> blocks;
    BasicBlock* current_block;
    Function(uint32_t a) : arity(a), current_block(nullptr) {}
};

// BEAM instruction types
enum class BeamOpcode : uint8_t {
    CALL,
    CALL_EXT,
    MOVE,
    ADD,
    SUB,
    MUL,
    DIV,
    CMP,
    JMP,
    JEQ,
    JNE,
    HALT,
    NOP
};

// Instruction decoder
class Decoder {
public:
    static bool decode_bytecode(const uint8_t* bytecode, size_t len, Function& func);
};

// IR builder
class IRBuilder {
    std::unique_ptr<Function> current_func;
    std::vector<uint8_t> bytecode;

public:
    IRBuilder();
    ~IRBuilder();

    bool decode_bytecode(const uint8_t* bytecode, size_t bytecode_len, uint32_t arity);
    void emit_code(class CodeEmitter& emitter);

    Function* get_function() { return current_func.get(); }
};

// Emit code to x86-64
class CodeEmitter {
    std::vector<uint8_t> code;

public:
    void emit_byte(uint8_t b) { code.push_back(b); }

    void emit_rex(bool W, bool R, bool X, bool B) {
        uint8_t rex = 0x40 | (W ? 8 : 0) | (R ? 4 : 0) | (X ? 2 : 0) | (B ? 1 : 0);
        emit_byte(rex);
    }

    void emit_modrm(uint8_t mod, uint8_t reg, uint8_t rm) {
        emit_byte(static_cast<uint8_t>((mod << 6) | ((reg & 7) << 3) | (rm & 7)));
    }

    void emit_imm64(uint64_t val) {
        for (int i = 0; i < 8; i++) {
            emit_byte(static_cast<uint8_t>((val >> (i * 8)) & 0xFF));
        }
    }

    void emit_imm32(uint32_t val) {
        for (int i = 0; i < 4; i++) {
            emit_byte(static_cast<uint8_t>((val >> (i * 8)) & 0xFF));
        }
    }

    void emit_prologue() {
        emit_byte(0x55);  // push rbp
        emit_byte(0x48);  // rex
        emit_byte(0x89);  // mov rbp, rsp
        emit_byte(0xE5);
    }

    void emit_epilogue() {
        emit_byte(0x5D);  // pop rbp
        emit_byte(0xC3);  // ret
    }

    void emit_mov_reg_imm32(Reg dst, uint32_t imm) {
        emit_rex(true, false, false, false);
        emit_byte(0xB8 | static_cast<uint8_t>(dst));
        emit_imm32(imm);
    }

    void emit_mov_reg_reg(Reg dst, Reg src) {
        emit_rex(true, false, false, false);
        emit_byte(0x89);
        emit_modrm(3, static_cast<uint8_t>(dst), static_cast<uint8_t>(src));
    }

    void emit_add_reg_reg(Reg dst, Reg src) {
        emit_rex(true, false, false, false);
        emit_byte(0x01);
        emit_modrm(3, static_cast<uint8_t>(dst), static_cast<uint8_t>(src));
    }

    void emit_sub_reg_reg(Reg dst, Reg src) {
        emit_rex(true, false, false, false);
        emit_byte(0x29);
        emit_modrm(3, static_cast<uint8_t>(dst), static_cast<uint8_t>(src));
    }

    void emit_imul_reg(Reg reg) {
        emit_rex(true, false, false, false);
        emit_byte(0xF7);
        emit_modrm(3, 5, static_cast<uint8_t>(reg));
    }

    void emit_cmp_reg_reg(Reg dst, Reg src) {
        emit_rex(true, false, false, false);
        emit_byte(0x39);
        emit_modrm(3, static_cast<uint8_t>(dst), static_cast<uint8_t>(src));
    }

    void emit_call_reg(Reg reg) {
        emit_rex(false, false, false, true);
        emit_byte(0xFF);
        emit_modrm(3, 2, static_cast<uint8_t>(reg));
    }

    void emit_jmp_offset(int32_t offset) {
        emit_byte(0xE9);
        emit_imm32(static_cast<uint32_t>(offset));
    }

    void emit_je_offset(int32_t offset) {
        emit_byte(0x74);
        emit_byte(static_cast<uint8_t>(offset & 0xFF));
    }

    std::vector<uint8_t> get_code() const { return code; }
    void clear() { code.clear(); }
};

// Helper to convert register to string
inline const char* reg_to_string(Reg r) {
    switch (r) {
        case Reg::rax: return "rax";
        case Reg::rcx: return "rcx";
        case Reg::rdx: return "rdx";
        case Reg::rbx: return "rbx";
        case Reg::rsp: return "rsp";
        case Reg::rbp: return "rbp";
        case Reg::rsi: return "rsi";
        case Reg::rdi: return "rdi";
        case Reg::r8: return "r8";
        case Reg::r9: return "r9";
        case Reg::r10: return "r10";
        case Reg::r11: return "r11";
        case Reg::r12: return "r12";
        case Reg::r13: return "r13";
        case Reg::r14: return "r14";
        case Reg::r15: return "r15";
        default: return "unknown";
    }
}

} // namespace ir

// Make CodeEmitter available in codegen namespace for compatibility
namespace codegen {
    using ::ir::CodeEmitter;
}