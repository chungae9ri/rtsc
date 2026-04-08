// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

use core::arch::global_asm;
use core::ptr;

use cortex_m::peripheral::SCB;
use cortex_m_rt::exception;

use crate::task::Task;
//use rtt_target::rprintln;

static mut TICK_COUNT: u32 = 0;
#[unsafe(no_mangle)]
pub static mut current: *mut Task = ptr::null_mut();

pub unsafe fn init_current(task: *mut Task) {
    unsafe { current = task };
}

global_asm!(
    ".section .text.PendSV,\"ax\",%progbits",
    ".global PendSV",
    ".type PendSV,%function",
    "PendSV:",
    "push {{lr}}", // Preserve EXC_RETURN across function calls.
    "tst lr, #4",  // Was the interrupted thread using PSP or MSP
    "ite eq",
    "mrseq r0, msp",         // Thread used MSP.
    "mrsne r0, psp",         // Thread used PSP.
    "stmdb r0!, {{r4-r11}}", // Save callee-saved registers on the task stack.
    "ldr r1, =current",      // R1 = &current
    "ldr r2, [r1]",          // R2 = current task pointer
    "str r0, [r2]",          // Save updated stack pointer into the task control block.
    "bl schedule",           // Pick the next task and update current.
    "ldr r1, =current",      // R1 = &current
    "ldr r2, [r1]",          // R2 = next task pointer
    "ldr r0, [r2]",          // R0 = next task's saved SP
    "ldmia r0!, {{r4-r11}}", // Restore callee-saved registers for the selected task.
    "msr psp, r0",
    "pop {{lr}}", // Restore EXC_RETURN and return from exception to the next task.
    "bx lr",
);

#[unsafe(no_mangle)]
extern "C" fn schedule() {}

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
