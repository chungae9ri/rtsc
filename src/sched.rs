// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

use core::arch::global_asm;
use core::ptr;

use core::arch::asm;
use cortex_m::peripheral::SCB;
use cortex_m_rt::exception;

use crate::task::Task;
//use rtt_target::rprintln;

static mut TICK_COUNT: u32 = 0;
#[unsafe(no_mangle)]
pub static mut current: *mut Task = ptr::null_mut();
#[unsafe(no_mangle)]
static mut START_TASK_PTR: *mut Task = ptr::null_mut();

pub unsafe fn init_current(task: *mut Task) {
    unsafe { current = task };
}

/// Start the first scheduled task by restoring its prepared stack frame.
///
/// This does not return. The task must already have been initialized with the
/// same synthetic frame layout produced by `forkyi`. The actual exception
/// return happens in `SVCall`, because `EXC_RETURN` is only valid from handler
/// mode.
pub unsafe fn start_first_task(task: *mut Task) -> ! {
    unsafe {
        START_TASK_PTR = task;
        asm!("svc 0", options(noreturn));
    }
}

static mut MAIN_THREAD_PTR: *mut Task = ptr::null_mut();
static mut DUMMY_THREAD_PTR: *mut Task = ptr::null_mut();

pub unsafe fn init_rq(main: *mut Task, dummy: *mut Task) {
    unsafe {
        MAIN_THREAD_PTR = main;
        DUMMY_THREAD_PTR = dummy;
    }
}

global_asm!(
    ".section .text.SVCall,\"ax\",%progbits",
    ".global SVCall",
    ".type SVCall,%function",
    "SVCall:",
    "ldr r0, =START_TASK_PTR",
    "ldr r0, [r0]",          // r0 = task
    "ldr r3, =current",
    "str r0, [r3]",          // current = task
    "ldr r1, [r0]",          // r1 = task->sp
    "ldr lr, [r0, #4]",      // lr = task->exc_return
    "ldmia r1!, {{r4-r11}}", // restore callee-saved registers
    "tst lr, #4",
    "ite eq",
    "msreq msp, r1",
    "msrne psp, r1",
    "bx lr",                 // exception return into the task entry frame
    ".size SVCall, .-SVCall",
);

global_asm!(
    ".section .text.PendSV,\"ax\",%progbits",
    ".global PendSV",
    ".type PendSV,%function",
    "PendSV:",
    "tst lr, #4", // Was the interrupted thread using PSP or MSP
    "ite eq",
    "mrseq r0, msp",         // Thread used MSP.
    "mrsne r0, psp",         // Thread used PSP.
    "stmdb r0!, {{r4-r11}}", // Save callee-saved registers on the task stack.
    "ldr r1, =current",      // R1 = &current
    "ldr r2, [r1]",          // R2 = current task pointer
    "str r0, [r2]",          // Save updated stack pointer into the task control block.
    "str lr, [r2, #4]",      // Save EXC_RETURN so the next restore uses MSP or PSP correctly.
    "bl schedule",           // Pick the next task and update current.
    "ldr r1, =current",      // R1 = &current
    "ldr r2, [r1]",          // R2 = next task pointer
    "ldr r0, [r2]",          // R0 = next task's saved SP
    "ldr lr, [r2, #4]",      // LR = next task's saved EXC_RETURN
    "ldmia r0!, {{r4-r11}}", // Restore callee-saved registers for the selected task.
    "tst lr, #4",            // Does the next task return using MSP or PSP?
    "ite eq",
    "msreq msp, r0", // Restore MSP-backed context.
    "msrne psp, r0", // Restore PSP-backed context.
    "bx lr",
);

#[unsafe(no_mangle)]
extern "C" fn schedule() {
   unsafe {
       if TICK_COUNT % 2 == 0 {
           current = MAIN_THREAD_PTR;
       } else {
           current = DUMMY_THREAD_PTR;
       }
   }
}

/// SysTick handler used for scheduler tick processing.
///
/// A real scheduler would typically update time-based accounting here and may
/// pend PendSV when the current task should yield.
#[exception]
fn SysTick() {
    unsafe {
        TICK_COUNT += 1;
    }
    SCB::set_pendsv();
}

pub unsafe fn switch_ctx(_cur: *mut Task, _next: *const Task) {}
