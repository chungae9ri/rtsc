// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

//! Core task definitions for the runtime scheduler.

use crate::rbtree::sched_entity;
use crate::sched::enqueue_task;

/// Execution state for a scheduled task.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    /// The task is eligible to run when selected by the scheduler.
    Ready,
    /// The task is currently executing on the CPU.
    Running,
    /// The task cannot run until an external event or resource becomes ready.
    Blocked,
    /// The task has been paused explicitly and will not be scheduled.
    Suspended,
}

/// Registers that must be preserved across a context switch on Cortex-M.
///
/// These are the callee-saved general-purpose registers under the ARM ABI.
/// A PendSV context switch routine typically stores and restores this set
/// around task transitions.
#[repr(C)]
pub struct CalleeSavedRegisters {
    /// Saved value of register r4.
    pub r4: u32,
    /// Saved value of register r5.
    pub r5: u32,
    /// Saved value of register r6.
    pub r6: u32,
    /// Saved value of register r7.
    pub r7: u32,
    /// Saved value of register r8.
    pub r8: u32,
    /// Saved value of register r9.
    pub r9: u32,
    /// Saved value of register r10.
    pub r10: u32,
    /// Saved value of register r11.
    pub r11: u32,
}

/// 8-byte aligned stack storage for Cortex-M thread contexts.
#[repr(align(8))]
pub struct AlignedStack<const N: usize>(pub [u32; N]);

/// Scheduler-visible task control block.
///
/// `sp` points at the saved stack frame used when restoring the task. When
/// `exc_return` records whether that saved frame belongs to MSP or PSP, and
/// real context switching is added, the layout implied by `sp`,
/// `exc_return`, and
/// `callee_saved_regs` should be documented alongside the save/restore code.
#[repr(C)]
pub struct Task {
    /// Stack pointer captured for the next restore of this task.
    /// Stack pointer should be always placed in the first field.
    pub sp: u32,
    /// Saved EXC_RETURN value used to restore the correct stack pointer.
    /// exc_return should be always placed in the second field.
    pub exc_return: u32,
    /// Scheduler-assigned task identifier.
    pub id: u32,
    /// Human-readable task name for logs and diagnostics.
    pub name: &'static str,
    /// Scheduling priority, where the exact ordering is defined by the scheduler.
    pub priority: u8,
    /// Current lifecycle state used by the scheduler.
    pub state: TaskState,
    /// Scheduler entity used for run-queue ordering.
    pub sched_entity: sched_entity,
    /// Software view of the callee-saved register set for this task.
    pub callee_saved_regs: CalleeSavedRegisters,
}

pub unsafe fn forkyi(
    task: *mut Task,
    mut sp: *mut u32,
    entry: extern "C" fn(*mut core::ffi::c_void) -> !,
    arg: *mut core::ffi::c_void,
    id: u32,
    name: &'static str,
    priority: u8,
) {
    // Build the initial stack so that, after PendSV restores r4-r11 and sets
    // PSP, exception return consumes a standard hardware frame:
    // r0, r1, r2, r3, r12, lr, pc, xpsr.
    unsafe {
        // Exception return requires an 8-byte aligned stack.
        sp = ((sp as usize) & !0x7) as *mut u32;

        sp = sp.sub(1);
        *sp = 0x0100_0000; // xPSR: Thumb state

        sp = sp.sub(1);
        *sp = entry as usize as u32; // PC: task entry point

        sp = sp.sub(1);
        *sp = 0xFFFF_FFFD; // LR: return to Thread mode using PSP

        sp = sp.sub(1);
        *sp = 0x0000_0000; // R12

        for _ in 0..3 {
            sp = sp.sub(1);
            *sp = 0x0000_0000; // R3, R2, R1
        }

        sp = sp.sub(1);
        *sp = arg as u32; // R0: argument to the task entry function

        for _ in 0..8 {
            sp = sp.sub(1);
            *sp = 0x0000_0000; // R4-R11: initial values
        }
        *task = Task {
            sp: sp as u32,
            exc_return: 0xFFFF_FFFD,
            id,
            name,
            priority,
            state: TaskState::Ready,
            sched_entity: sched_entity::new(0),
            callee_saved_regs: CalleeSavedRegisters {
                r4: 0,
                r5: 0,
                r6: 0,
                r7: 0,
                r8: 0,
                r9: 0,
                r10: 0,
                r11: 0,
            },
        };
        enqueue_task(task);
    }
}
