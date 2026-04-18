// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

use core::arch::global_asm;
use core::cell::UnsafeCell;
use core::mem::offset_of;
use core::ptr;

use core::arch::asm;
use cortex_m::peripheral::SCB;
use cortex_m_rt::exception;

use crate::rbtree::{RBTree, rb_node};
use crate::task::{Task, TaskState};
//use rtt_target::rprintln;

static mut TICK_COUNT: u32 = 0;
static RUN_QUEUE: RunQueue = RunQueue::new();
#[unsafe(no_mangle)]
pub static mut current: *mut Task = ptr::null_mut();
#[unsafe(no_mangle)]
static mut START_TASK_PTR: *mut Task = ptr::null_mut();

struct RunQueue(UnsafeCell<RBTree>);

impl RunQueue {
    const fn new() -> Self {
        Self(UnsafeCell::new(RBTree::new()))
    }

    fn get(&self) -> *mut RBTree {
        self.0.get()
    }
}

unsafe impl Sync for RunQueue {}

/// Scheduler entity used as the tree node and ordering key.
///
/// `vrun_time` is the primary key. When two entities have the same
/// `vrun_time`, their addresses are used as a stable tie-breaker so insertion
/// order remains deterministic and the tree keeps a strict total ordering.
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct sched_entity {
    /// Scheduler virtual runtime metric used as the red-black tree key.
    pub vrun_time: u64,
    /// Scheduling priority, where the exact ordering is defined by the scheduler.
    pub priority: u8,
    pub(crate) rb_node: rb_node,
}

impl sched_entity {
    /// Create a detached scheduler entity that can be inserted into a tree.
    pub const fn new(vrun_time: u64, priority: u8) -> Self {
        Self {
            vrun_time,
            priority,
            rb_node: rb_node::new(),
        }
    }

    /// Reset linkage so the entity can be reused or inserted into another tree.
    pub fn reset_links(&mut self) {
        self.rb_node.reset_links();
    }

    /// Return `true` if the entity is currently linked under another node.
    pub fn is_linked(&self) -> bool {
        self.rb_node.is_linked()
    }
}

pub unsafe fn init_current(task: *mut Task) {
    unsafe { current = task };
}

/// Spawn main task by restoring its prepared stack frame.
///
/// This does not return. The task must already have been initialized with the
/// same synthetic frame layout produced by `forkyi`. The actual exception
/// return happens in `SVCall`, because `EXC_RETURN` is only valid from
/// handler mode.
pub unsafe fn spawn_main_task(task: *mut Task) -> ! {
    unsafe {
        (*RUN_QUEUE.get()).remove(ptr::addr_of_mut!((*task).sched_entity));
        (*task).state = TaskState::Running;
        START_TASK_PTR = task;
        asm!("svc 0", options(noreturn));
    }
}

/// Reset the scheduler run queue to an empty state.
pub unsafe fn init_rq() {
    unsafe {
        *RUN_QUEUE.get() = RBTree::new();
    }
}

/// Enqueue a task into the scheduler run queue.
///
/// The task's `sched_entity.vrun_time` field is used as the red-black tree key.
pub unsafe fn enqueue_task(task: *mut Task) {
    unsafe {
        (*task).state = TaskState::Ready;
        (*task).sched_entity.reset_links();
        (*RUN_QUEUE.get()).insert(ptr::addr_of_mut!((*task).sched_entity));
    }
}

/// Remove a task from the scheduler run queue if it is currently queued.
pub unsafe fn dequeue_task(task: *mut Task) {
    unsafe {
        if (*task).state == TaskState::Ready {
            (*RUN_QUEUE.get()).remove(ptr::addr_of_mut!((*task).sched_entity));
        }
    }
}

// Switch to the first task which was set up by `forkyi`.
// This is typically called at the end of `main`.
global_asm!(
    ".section .text.SVCall,\"ax\",%progbits",
    ".global SVCall",
    ".type SVCall,%function",
    "SVCall:",
    "ldr r0, =START_TASK_PTR",
    "ldr r0, [r0]", // r0 = task
    "ldr r3, =current",
    "str r0, [r3]",          // current = task
    "ldr r1, [r0]",          // r1 = task->sp
    "ldr lr, [r0, #4]",      // lr = task->exc_return
    "ldmia r1!, {{r4-r11}}", // restore callee-saved registers
    "tst lr, #4",
    "ite eq",
    "msreq msp, r1",
    "msrne psp, r1",
    "bx lr", // exception return into the task entry frame
    ".size SVCall, .-SVCall",
);

// PendSV handler used for context switching between tasks.
// The actual context switch happens in the assembly code, but the scheduler is
// called from here to select the next task to run and update `current`.
// Tasks are expected to have their stack frames (PSP) prepared by `forkyi` so that the
// assembly code can save and restore them without needing to understand the layout.
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
        if !current.is_null() && (*current).state == TaskState::Running {
            (*current).sched_entity.vrun_time += 1;
            (*current).state = TaskState::Ready;
            (*RUN_QUEUE.get()).insert(ptr::addr_of_mut!((*current).sched_entity));
        }

        if let Some(next_entity) = (*RUN_QUEUE.get()).pop_first() {
            let next_task = task_from_sched_entity(next_entity as *mut sched_entity);
            (*next_task).state = TaskState::Running;
            current = next_task;
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

unsafe fn task_from_sched_entity(entity: *mut sched_entity) -> *mut Task {
    unsafe {
        (entity as *mut u8)
            .sub(offset_of!(Task, sched_entity))
            .cast::<Task>()
    }
}
