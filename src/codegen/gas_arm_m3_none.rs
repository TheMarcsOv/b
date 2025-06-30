use crate::crust::libc::*;
use crate::nob::*;
use crate::{align_bytes, missingf, Arg, Binop, Compiler, Func, Loc, Op, OpWithLocation};
use core::ffi::*;

pub unsafe fn call_arg(arg: Arg, loc: Loc, stack_size: usize, output: *mut String_Builder) {
    match arg {
        Arg::RefExternal(name) | Arg::External(name) => sb_appendf(output, c!("    bl %s\n"), name),
        arg => {
            load_arg_to_reg(arg, c!("r4"), output, loc, stack_size);
            sb_appendf(output, c!("    blx r4\n"))
        },
    };
}

pub unsafe fn return_func(stack_size: usize, output: *mut String_Builder) {
    sb_appendf(output, c!("    add r7, r7, #%zu\n"), stack_size); //Free stack
    sb_appendf(output, c!("    mov sp, r7\n"));
    sb_appendf(output, c!("    pop {r7, pc}\n")); // pop frame pointer and return
}

pub unsafe fn load_arg_to_reg(arg: Arg, reg: *const c_char, output: *mut String_Builder, loc: Loc, stack_size: usize) {
    match arg {
        Arg::Bogus => unreachable!("bogus-amogus"),
        Arg::External(name) => {
            sb_appendf(output, c!("    adr %s, =%s\n"), reg, name);
            sb_appendf(output, c!("    ldr %s, [%s]\n"), reg, reg);
        }
        Arg::Deref(index) => {
            sb_appendf(output, c!("    ldr %s, [r7, #%zu]\n"), reg, stk_off(stack_size, index));
            sb_appendf(output, c!("    ldr %s, [%s]\n"), reg, reg);
        }
        Arg::RefAutoVar(index) => {
            sb_appendf(output, c!("    add %s, r7, %zu\n"), reg, stk_off(stack_size, index));
        }
        Arg::RefExternal(name) => {
            sb_appendf(output, c!("    adr %s, =%s\n"), reg, name);
        }
        Arg::AutoVar(index) => {
            sb_appendf(output, c!("    ldr %s, [r7, #%zu]\n"), reg, stk_off(stack_size, index));
        }
        Arg::Literal(value) => {
            let value = value as u32;
            if value <= 0xff {
                sb_appendf(output, c!("    mov %s, #%d\n"), reg, value);
            } else if value <= 0xffff {
                sb_appendf(output, c!("    movw %s, #%d\n"), reg, value);
            } else {
                sb_appendf(output, c!("    movw %s, #%d\n"), reg, value & 0xFFFF);
                sb_appendf(output, c!("    movt %s, #%d\n"), reg, value >> 16);
            }
        }
        Arg::DataOffset(offset) => {
            todo!();
            // sb_appendf(output, c!("    adrp %s, .dat\n"), reg);
            // sb_appendf(output, c!("    add  %s, %s, :lo12:.dat\n"), reg, reg);
            // if offset >= 4095 {
            // missingf!(loc, c!("Data offsets bigger than 4095 are not supported yet\n"));
            // } else if offset > 0 {
            //     sb_appendf(output, c!("    add %s, %s, %zu\n"), reg, reg, offset);
            // }
        }
    };
}

pub unsafe fn stk_off(stack_size: usize, idx: usize) -> usize {
    //NOTE: idx is one based
    assert!(idx > 0);
    if (idx - 1) * 4 >= stack_size {
        printf(c!("Stack overflow, index %zu vs stack %zu\n"), idx, stack_size);
        panic!();
    }
    stack_size - idx * 4
}

pub unsafe fn generate_function(name: *const c_char, name_loc: Loc, params_count: usize, auto_vars_count: usize, body: *const [OpWithLocation], output: *mut String_Builder) {
    
    let stack_size = align_bytes(auto_vars_count * 4, 8);
    sb_appendf(output, c!(".global %s\n"), name);
    sb_appendf(output, c!(".type %s, %%function\n"), name);
    sb_appendf(output, c!(".thumb_func\n"));
    sb_appendf(output, c!("%s:\n"), name);

    sb_appendf(output, c!("    push {r7, lr}\n")); // Push frame pointer and link register
    sb_appendf(output, c!("    sub sp, sp, #%zu\n"), stack_size); // Reserve stack for autovars
    sb_appendf(output, c!("    mov r7, sp\n")); // Set frame pointer
    assert!(auto_vars_count >= params_count);

    const REGISTERS: *const [*const c_char] = &[c!("r0"), c!("r1"), c!("r2"), c!("r3")];
    if params_count > REGISTERS.len() {
        missingf!(name_loc, c!("Too many parameters in function definition. We support only %zu but %zu were provided\n"), REGISTERS.len(), params_count);
    }

    for i in 0..params_count {
        let reg = (*REGISTERS)[i];
        // In Thumb, compilers seem to use positive offsets in order to use the more compact
        // 16-bit encoding. If you use a negative value, it uses the 32-bit encoding.
        let stack_offset = stk_off(stack_size, i+1);
        sb_appendf(output, c!("    str %s, [r7, #%zu]\n"), reg, stack_offset);
    }

    for i in 0..body.len() {
        sb_appendf(output, c!("%s.op_%zu:\n"), name, i);
        let op = (*body)[i];
        match op.opcode {
            Op::Return { arg } => {
                if let Some(arg) = arg {
                    load_arg_to_reg(arg, c!("x0"), output, op.loc, stack_size);
                }
                return_func(stack_size, output);
            }
            Op::Negate { result, arg } => {
                load_arg_to_reg(arg, c!("r0"), output, op.loc, stack_size);
                sb_appendf(output, c!("    neg r0, r0\n"));
                sb_appendf(output, c!("    str r0, [r7, #%zu]\n"), stk_off(stack_size, result));
            }
            Op::UnaryNot { result, arg } => {
                load_arg_to_reg(arg, c!("r0"), output, op.loc, stack_size);
                sb_appendf(output, c!("    clz r0, r0\n"));
                sb_appendf(output, c!("    lsr r0, r0, #5\n")); // 1 if there are 32 zeros 0 otherwise
                sb_appendf(output, c!("    str r0, [r7, #%zu]\n"), stk_off(stack_size, result));
            }
            Op::Binop { binop, index, lhs, rhs } => {
                match binop {
                    Binop::BitOr => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    orr r0, r0, r1\n"));
                    }
                    Binop::BitAnd => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    and r0, r0, r1\n"));
                    }
                    Binop::BitShl => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    lsl r0, r0, r1\n"));
                    }
                    Binop::BitShr => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    lsr r0, r0, r1\n"));
                    }
                    Binop::Plus => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    add r0, r0, r1\n"));
                    }
                    Binop::Minus => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    sub r0, r0, r1\n"));
                    }
                    Binop::Mod => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    sdiv r2, r0, r1\n"));    // a / b = d;
                        sb_appendf(output, c!("    mls r0, r2, r1, r0\n")); // a % b = a - d * b 
                    }
                    Binop::Div => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    sdiv r2, r0, r1\n"));    // a / b = d;
                    }
                    Binop::Mult => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    mul r0, r0, r1\n"));
                    }
                    Binop::Less => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    cmp r0, r1\n"));
                        sb_appendf(output, c!("    ite lt\n"));
                        sb_appendf(output, c!("    movlt r0, #1\n"));
                        sb_appendf(output, c!("    movge r0, #0\n"));
                    }
                    Binop::Greater => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    cmp r0, r1\n"));
                        sb_appendf(output, c!("    ite gt\n"));
                        sb_appendf(output, c!("    movgt r0, #1\n"));
                        sb_appendf(output, c!("    movle r0, #0\n"));
                    }
                    Binop::Equal => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    cmp r0, r1\n"));
                        sb_appendf(output, c!("    ite eq\n"));
                        sb_appendf(output, c!("    moveq r0, #1\n"));
                        sb_appendf(output, c!("    movne r0, #0\n"));
                    }
                    Binop::NotEqual => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    cmp r0, r1\n"));
                        sb_appendf(output, c!("    ite ne\n"));
                        sb_appendf(output, c!("    movne r0, #1\n"));
                        sb_appendf(output, c!("    moveq r0, #0\n"));
                    }
                    Binop::GreaterEqual => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    cmp r0, r1\n"));
                        sb_appendf(output, c!("    ite ge\n"));
                        sb_appendf(output, c!("    movge r0, #1\n"));
                        sb_appendf(output, c!("    movlt r0, #0\n"));
                    }
                    Binop::LessEqual => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    cmp r0, r1\n"));
                        sb_appendf(output, c!("    ite le\n"));
                        sb_appendf(output, c!("    movle r0, #1\n"));
                        sb_appendf(output, c!("    movgt r0, #0\n"));
                    }
                }
                sb_appendf(output, c!("    str r0, [r7, #%zu]\n"), stk_off(stack_size, index));
            }
            Op::ExternalAssign { name, arg } => {
                load_arg_to_reg(arg, c!("r0"), output, op.loc, stack_size);
                sb_appendf(output, c!("    str r0, [r1]\n"));
            }
            Op::AutoAssign { index, arg } => {
                load_arg_to_reg(arg, c!("r0"), output, op.loc, stack_size);
                sb_appendf(output, c!("    str r0, [r7, #%zu]\n"), stk_off(stack_size, index));
            }
            Op::Store { index, arg } => {
                sb_appendf(output, c!("    ldr r0, [r7, #%zu]\n"), stk_off(stack_size, index));
                load_arg_to_reg(arg, c!("r1"), output, op.loc, stack_size);
                sb_appendf(output, c!("    str r1, [r0]\n"));
            }
            Op::Funcall { result, fun, args } => {
                let reg_args_count = args.count;
                // TODO: allocate stack space for extra args
                if reg_args_count > REGISTERS.len() {
                    missingf!(op.loc, c!("Too many function call arguments. We support only %zu but %zu were provided\n"), REGISTERS.len(), args.count);
                }
                for i in 0..reg_args_count {
                    let reg = (*REGISTERS)[i];
                    load_arg_to_reg(*args.items.add(i), reg, output, op.loc, stack_size);
                }
                
                call_arg(fun, op.loc, stack_size, output);
                sb_appendf(output, c!("    str r0, [r7, #%zu]\n"), stk_off(stack_size, result));
                // TODO: deallocate stack space
            }
            Op::Asm { stmts } => {
                for i in 0..stmts.count {
                    let stmt = *stmts.items.add(i);
                    sb_appendf(output, c!("    %s\n"), stmt.line);
                }
            }
            Op::Label {label} => {
                sb_appendf(output, c!("%s.label_%zu:\n"), name, label);
            }
            Op::JmpLabel { label } => {
                sb_appendf(output, c!("    b %s.label_%zu\n"), name, label);
            }
            Op::JmpIfNotLabel { label, arg } => {
                load_arg_to_reg(arg, c!("r0"), output, op.loc, stack_size);
                sb_appendf(output, c!("    cmp r0, 0\n"));
                sb_appendf(output, c!("    beq %s.label_%zu\n"), name, label);
            }
            Op::Bogus => unreachable!("bogus-amogus"),
        }
    }
    sb_appendf(output, c!("%s.op_%zu:\n"), name, body.len());
    return_func(stack_size, output);
    sb_appendf(output, c!("    .size %s, .-%s\n"), name, name); //TODO: check why/if needed
}
pub unsafe fn generate_funcs(output: *mut String_Builder, funcs: *const [Func]) {
    sb_appendf(output, c!(".text\n"));
    for i in 0..funcs.len() {
        generate_function(
            (*funcs)[i].name,
            (*funcs)[i].name_loc,
            (*funcs)[i].params_count,
            (*funcs)[i].auto_vars_count,
            da_slice((*funcs)[i].body),
            output,
        );
    }
}

pub unsafe fn generate_program(output: *mut String_Builder, c: *const Compiler) {
    // File ASM preamble
    sb_appendf(output, c!(".syntax unified\n"));
    sb_appendf(output, c!(".cpu cortex-m3\n"));
    sb_appendf(output, c!(".fpu softvfp\n"));
    sb_appendf(output, c!(".thumb\n\n"));

    // Rest of the translation unit
    generate_funcs(output, da_slice((*c).funcs));
    // TODO: asm functions
    // TODO: globals
    // TODO: data section
}
