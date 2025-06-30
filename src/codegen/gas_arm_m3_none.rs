use crate::crust::libc::*;
use crate::nob::*;
use crate::{align_bytes, missingf, Arg, Binop, Compiler, Func, Loc, Op, OpWithLocation};
use core::ffi::*;

pub unsafe fn load_arg_to_reg(arg: Arg, reg: *const c_char, output: *mut String_Builder, loc: Loc, stack_size: usize,
) {
    match arg {
        Arg::Bogus => unreachable!("argbogus"),
        Arg::External(name) => {
            todo!();
            // sb_appendf(output, c!("    adrp %s, %s\n"), reg, name);
            // sb_appendf(output, c!("    add  %s, %s, :lo12:%s\n"), reg, reg, name);
            // sb_appendf(output, c!("    ldr %s, [%s]\n"), reg, reg);
        }
        Arg::Deref(index) => {
            sb_appendf(
                output,
                c!("    ldr %s, [r7, #%zu]\n"),
                reg,
                stk_off(stack_size, index),
            );
            sb_appendf(output, c!("    ldr %s, [%s]\n"), reg, reg);
        }
        Arg::RefAutoVar(index) => {
            todo!();
            // sb_appendf(output, c!("    sub %s, x29, %zu\n"), reg, index*8);
        }
        Arg::RefExternal(name) => {
            todo!();
            // sb_appendf(output, c!("    adrp %s, %s\n"), reg, name);
            // sb_appendf(output, c!("    add  %s, %s, :lo12:%s\n"), reg, reg, name);
        }
        Arg::AutoVar(index) => {
            sb_appendf(
                output,
                c!("    ldr %s, [r7, #%zu]\n"),
                reg,
                stk_off(stack_size, index),
            );
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
    if idx * 4 >= stack_size {
        printf(c!("Stack overflow, index %zu vs stack %zu\n"), idx, stack_size);
        panic!();
    }
    stack_size - (idx + 1) * 4
}

pub unsafe fn generate_function(name: *const c_char, name_loc: Loc, params_count: usize, auto_vars_count: usize, body: *const [OpWithLocation], output: *mut String_Builder) {
    
    let stack_size = align_bytes(auto_vars_count * 4, 8) + 8; //TODO: remove +8
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
        let stack_offset = stk_off(stack_size, i);
        sb_appendf(output, c!("    str %s, [r7, #%zu]\n"), reg, stack_offset);
    }

    for i in 0..body.len() {
        sb_appendf(output, c!("%s.op_%zu:\n"), name, i);
        let op = (*body)[i];
        match op.opcode {
            Op::Return { arg } => {
                todo!();
            }
            Op::Negate { result, arg } => {
                todo!();
            }
            Op::UnaryNot { result, arg } => {
                todo!();
            }
            Op::Binop {
                binop,
                index,
                lhs,
                rhs,
            } => {
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
                        todo!();
                    }
                    Binop::BitShr => {
                        todo!();
                    }
                    Binop::Plus => {
                        todo!();
                    }
                    Binop::Minus => {
                        load_arg_to_reg(lhs, c!("r0"), output, op.loc, stack_size);
                        load_arg_to_reg(rhs, c!("r1"), output, op.loc, stack_size);
                        sb_appendf(output, c!("    sub r0, r0, r1\n"));
                    }
                    Binop::Mod => {
                        todo!();
                    }
                    Binop::Div => {
                        todo!();
                    }
                    Binop::Mult => {
                        todo!();
                    }
                    Binop::Less => {
                        todo!();
                    }
                    Binop::Greater => {
                        todo!();
                    }
                    Binop::Equal => {
                        todo!();
                    }
                    Binop::NotEqual => {
                        todo!();
                    }
                    Binop::GreaterEqual => {
                        todo!();
                    }
                    Binop::LessEqual => {
                        todo!();
                    }
                }
                sb_appendf(
                    output,
                    c!("    str r0, [r7, #%zu]\n"),
                    stk_off(stack_size, index),
                );
            }
            Op::ExternalAssign { name, arg } => {
                todo!();
            }
            Op::AutoAssign { index, arg } => {
                load_arg_to_reg(arg, c!("r0"), output, op.loc, stack_size);
                sb_appendf(
                    output,
                    c!("    str r0, [r7, #%zu]\n"),
                    stk_off(stack_size, index),
                );
            }
            Op::Store { index, arg } => {
                sb_appendf(
                    output,
                    c!("    ldr r0, [r7, #%zu]\n"),
                    stk_off(stack_size, index),
                );
                load_arg_to_reg(arg, c!("r1"), output, op.loc, stack_size);
                sb_appendf(output, c!("    str r1, [r0]\n"));
            }
            Op::Funcall { result, fun, args } => {
                todo!();
                if args.count > REGISTERS.len() {
                    missingf!(op.loc, c!("Too many function call arguments. We support only %zu but %zu were provided\n"), REGISTERS.len(), args.count);
                }
                // for i in 0..args.count {
                //     let reg = (*REGISTERS)[i];
                //     load_arg_to_reg(*args.items.add(i), reg, output, op.loc);
                // }
                // call_arg(fun, op.loc, output);

                // sb_appendf(output, c!("    str x0, [x29, -%zu]\n"), result*8);
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
            Op::Bogus => unreachable!("amogus"),
        }
    }
    sb_appendf(output, c!("%s.op_%zu:\n"), name, body.len());
    sb_appendf(output, c!("    add r7, r7, #%zu\n"), stack_size); //Free stack
    sb_appendf(output, c!("    mov sp, r7\n"));
    sb_appendf(output, c!("    pop {r7, pc}\n")); // pop frame pointer and return
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
