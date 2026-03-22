// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

//! Core task and exception primitives for the runtime scheduler.
//!
//! This module defines the scheduler-visible task state and the exception
//! entry points commonly used to drive preemptive scheduling on Cortex-M.

#![no_std]

use cortex_m_rt::exception;
use rtt_target::rprintln;

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Execution state for a scheduled task.
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

/// Scheduler-visible task control block.
///
/// `sp` points at the saved stack frame used when restoring the task. When
/// real context switching is added, the layout implied by `sp` and
/// `callee_saved_regs` should be documented alongside the save/restore code.
pub struct Task {
    /// Scheduler-assigned task identifier.
    pub id: u32,
    /// Human-readable task name for logs and diagnostics.
    pub name: &'static str,
    /// Scheduling priority, where the exact ordering is defined by the scheduler.
    pub priority: u8,
    /// Current lifecycle state used by the scheduler.
    pub state: TaskState,
    /// Stack pointer captured for the next restore of this task.
    pub sp: u32,
    /// Software view of the callee-saved register set for this task.
    pub callee_saved_regs: CalleeSavedRegisters,
}

/// PendSV handler used for deferred context switching work.
///
/// On Cortex-M, PendSV is commonly assigned the lowest practical priority so
/// context switching happens after higher-priority interrupt work completes.
#[exception]
fn PendSV() {
    rprintln!("pendsv");
}

/// SysTick handler used for scheduler tick processing.
///
/// A real scheduler would typically update time-based accounting here and may
/// pend PendSV when the current task should yield.
#[exception]
fn SysTick() {
    rprintln!("systick");
}
