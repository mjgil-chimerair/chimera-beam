//! IR - Intermediate Representation for JIT
//!
//! Converts BEAM bytecode to an intermediate representation suitable
//! for optimization and code generation.

#include "ir.h"
#include <cstring>
#include <algorithm>

namespace ir {

IRBuilder::IRBuilder() : current_func(nullptr) {}

IRBuilder::~IRBuilder() {}

bool IRBuilder::decode_bytecode(const uint8_t* bytecode, size_t bytecode_len, uint32_t arity) {
    if (!bytecode || bytecode_len == 0) {
        return false;
    }

    // Create new function
    current_func = std::make_unique<Function>(arity);

    // Create entry block
    BasicBlock entry;
    entry.label = "entry";
    current_func->blocks.push_back(entry);
    current_func->current_block = &current_func->blocks.back();

    // Simple bytecode decoding
    // In a full implementation, this would handle all BEAM opcodes
    size_t offset = 0;

    while (offset < bytecode_len) {
        uint8_t op = bytecode[offset];

        switch (op) {
            case 0x00: // NOP
                current_func->current_block->ops.push_back(Opcode::NOP);
                current_func->current_block->operands.push_back({});
                offset++;
                break;

            case 0x01: // CALL
                if (offset + 5 <= bytecode_len) {
                    current_func->current_block->ops.push_back(Opcode::CALL);
                    current_func->current_block->operands.push_back({});
                    offset += 5;
                } else {
                    return false;
                }
                break;

            case 0x02: // RETURN
                current_func->current_block->ops.push_back(Opcode::RET);
                current_func->current_block->operands.push_back({});
                offset++;
                break;

            case 0x03: // ADD
                current_func->current_block->ops.push_back(Opcode::ADD);
                current_func->current_block->operands.push_back({});
                offset++;
                break;

            case 0x04: // SUB
                current_func->current_block->ops.push_back(Opcode::SUB);
                current_func->current_block->operands.push_back({});
                offset++;
                break;

            case 0x05: // MUL
                current_func->current_block->ops.push_back(Opcode::IMUL);
                current_func->current_block->operands.push_back({});
                offset++;
                break;

            case 0x06: // CMP
                current_func->current_block->ops.push_back(Opcode::CMP);
                current_func->current_block->operands.push_back({});
                offset++;
                break;

            case 0x07: // JMP
                if (offset + 4 <= bytecode_len) {
                    current_func->current_block->ops.push_back(Opcode::JMP);
                    current_func->current_block->operands.push_back({});
                    offset += 5;
                } else {
                    return false;
                }
                break;

            case 0x08: // JEQ
                if (offset + 4 <= bytecode_len) {
                    current_func->current_block->ops.push_back(Opcode::JE);
                    current_func->current_block->operands.push_back({});
                    offset += 5;
                } else {
                    return false;
                }
                break;

            case 0x09: // MOVE
                current_func->current_block->ops.push_back(Opcode::MOV);
                current_func->current_block->operands.push_back({});
                offset += 3;
                break;

            default:
                // Unknown opcode, treat as NOP
                current_func->current_block->ops.push_back(Opcode::NOP);
                current_func->current_block->operands.push_back({});
                offset++;
                break;
        }
    }

    return true;
}

void IRBuilder::emit_code(class CodeEmitter& emitter) {
    if (!current_func) {
        return;
    }

    // Emit function prologue
    emitter.emit_prologue();

    // For simple functions, emit a return with arity-based value
    // In full implementation, this would walk the CFG and emit proper code
    uint32_t arity = current_func->arity;

    // Simple implementation: return arity as immediate value
    emitter.emit_mov_reg_imm32(Reg::rax, arity);

    // Emit epilogue
    emitter.emit_epilogue();
}

} // namespace ir