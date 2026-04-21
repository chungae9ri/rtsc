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
use crate::thread::{Thread, ThreadState};
//use rtt_target::rprintln;

static mut TICK_COUNT: u32 = 0;
static RUN_QUEUE: RunQueue = RunQueue::new();
#[unsafe(no_mangle)]
pub static mut current: *mut Thread = ptr::null_mut();
#[unsafe(no_mangle)]
static mut START_THREAD_PTR: *mut Thread = ptr::null_mut();

struct RunQueue {
    tree: UnsafeCell<RBTree>,
    priority_sum: UnsafeCell<u64>,
}

impl RunQueue {
    const fn new() -> Self {
        Self {
            tree: UnsafeCell::new(RBTree::new()),
            priority_sum: UnsafeCell::new(0),
        }
    }

    fn get(&self) -> *mut RBTree {
        self.tree.get()
    }

    fn priority_sum(&self) -> *mut u64 {
        self.priority_sum.get()
    }
}

unsafe impl Sync for RunQueue {}

/// Scheduler entity used as the tree node and ordering key.
///
/// `vruntime` is the primary key. When two entities have the same
/// `vruntime`, their addresses are used as a stable tie-breaker so insertion
/// order remains deterministic and the tree keeps a strict total ordering.
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct sched_entity {
    pub(crate) sched_tick_cnt: u32,
    /// Scheduler virtual runtime metric used as the red-black tree key.
    pub(crate) vruntime: u64,
    /// Scheduling priority, where the exact ordering is defined by the scheduler.
    pub priority: u32,
    pub(crate) rb_node: rb_node,
}

impl sched_entity {
    /// Create a detached scheduler entity that can be inserted into a tree.
    pub const fn new(priority: u32) -> Self {
        Self {
            sched_tick_cnt: 0,
            vruntime: 0,
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

pub unsafe fn init_current(thread: *mut Thread) {
    unsafe { current = thread };
}

/// Spawn main thread by restoring its prepared stack frame.
///
/// This does not return. The thread must already have been initialized with the
/// same synthetic frame layout produced by `forkyi`. The actual exception
/// return happens in `SVCall`, because `EXC_RETURN` is only valid from
/// handler mode.
pub unsafe fn spawn_main_thread(thread: *mut Thread) -> ! {
    unsafe {
        (*RUN_QUEUE.get()).remove(ptr::addr_of_mut!((*thread).sched_entity));
        (*thread).state = ThreadState::Running;
        START_THREAD_PTR = thread;
        asm!("svc 0", options(noreturn));
    }
}

/// Reset the scheduler run queue to an empty state.
pub unsafe fn init_rq() {
    unsafe {
        *RUN_QUEUE.get() = RBTree::new();
        *RUN_QUEUE.priority_sum() = 0;
    }
}

/// Enqueue a thread into the scheduler run queue.
///
/// The thread's `sched_entity.vruntime` field is used as the red-black tree key.
pub unsafe fn enqueue_thread(thread: *mut Thread) {
    unsafe {
        (*thread).state = ThreadState::Ready;
        (*thread).sched_entity.reset_links();
        (*RUN_QUEUE.get()).insert(ptr::addr_of_mut!((*thread).sched_entity));
        *RUN_QUEUE.priority_sum() += u64::from((*thread).sched_entity.priority);
    }
}

/// Remove a thread from the scheduler run queue if it is currently queued.
pub unsafe fn dequeue_thread(thread: *mut Thread) {
    unsafe {
        if (*thread).state == ThreadState::Ready {
            (*RUN_QUEUE.get()).remove(ptr::addr_of_mut!((*thread).sched_entity));
            *RUN_QUEUE.priority_sum() -= u64::from((*thread).sched_entity.priority);
        }
    }
}

// Switch to the first thread which was set up by `forkyi`.
// This is typically called at the end of `main`.
global_asm!(
    ".section .text.SVCall,\"ax\",%progbits",
    ".global SVCall",
    ".type SVCall,%function",
    "SVCall:",
    "ldr r0, =START_THREAD_PTR",
    "ldr r0, [r0]", // r0 = thread
    "ldr r3, =current",
    "str r0, [r3]",          // current = thread
    "ldr r1, [r0]",          // r1 = thread->sp
    "ldr lr, [r0, #4]",      // lr = thread->exc_return
    "ldmia r1!, {{r4-r11}}", // restore callee-saved registers
    "tst lr, #4",
    "ite eq",
    "msreq msp, r1",
    "msrne psp, r1",
    "bx lr", // exception return into the thread entry frame
    ".size SVCall, .-SVCall",
);

// PendSV handler used for context switching between threads.
// The actual context switch happens in the assembly code, but the scheduler is
// called from here to select the next thread to run and update `current`.
// Threads are expected to have their stack frames (PSP) prepared by `forkyi` so that the
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
    "stmdb r0!, {{r4-r11}}", // Save callee-saved registers on the thread stack.
    "ldr r1, =current",      // R1 = &current
    "ldr r2, [r1]",          // R2 = current thread pointer
    "str r0, [r2]",          // Save updated stack pointer into the thread control block.
    "str lr, [r2, #4]",      // Save EXC_RETURN so the next restore uses MSP or PSP correctly.
    "bl schedule",           // Pick the next thread and update current.
    "ldr r1, =current",      // R1 = &current
    "ldr r2, [r1]",          // R2 = next thread pointer
    "ldr r0, [r2]",          // R0 = next thread's saved SP
    "ldr lr, [r2, #4]",      // LR = next thread's saved EXC_RETURN
    "ldmia r0!, {{r4-r11}}", // Restore callee-saved registers for the selected thread.
    "tst lr, #4",            // Does the next thread return using MSP or PSP?
    "ite eq",
    "msreq msp, r0", // Restore MSP-backed context.
    "msrne psp, r0", // Restore PSP-backed context.
    "bx lr",
);

#[unsafe(no_mangle)]
extern "C" fn schedule() {
    unsafe {
        if !current.is_null() && (*current).state == ThreadState::Running {
            debug_assert!(
                !(*current).sched_entity.is_linked(),
                "running thread is still linked in RUN_QUEUE before scheduling"
            );
            (*current).sched_entity.sched_tick_cnt += 1;
            let priority_sum = *RUN_QUEUE.priority_sum();
            if priority_sum == 0 {
                return;
            }
            let sched_tick_cnt = u64::from((*current).sched_entity.sched_tick_cnt);
            let priority = u64::from((*current).sched_entity.priority);

            (*current).sched_entity.vruntime = sched_tick_cnt * priority / priority_sum;
            if let Some(next_entity) = (*RUN_QUEUE.get()).pop_first() {
                let next_thread = thread_from_sched_entity(next_entity as *mut sched_entity);
                debug_assert!(
                    current != next_thread,
                    "RUN_QUEUE.pop_first() returned the current running thread"
                );
                if (*current).sched_entity.vruntime > (*next_thread).sched_entity.vruntime {
                    (*current).state = ThreadState::Ready;
                    (*RUN_QUEUE.get()).insert(ptr::addr_of_mut!((*current).sched_entity));
                    (*next_thread).state = ThreadState::Running;
                    current = next_thread;
                } else {
                    (*RUN_QUEUE.get()).insert(ptr::addr_of_mut!((*next_thread).sched_entity));
                }
            }
        }
    }
}

/// SysTick handler used for scheduler tick processing.
///
/// A real scheduler would typically update time-based accounting here and may
/// pend PendSV when the current thread should yield.
#[exception]
fn SysTick() {
    unsafe {
        TICK_COUNT += 1;
    }
    SCB::set_pendsv();
}

unsafe fn thread_from_sched_entity(entity: *mut sched_entity) -> *mut Thread {
    unsafe {
        (entity as *mut u8)
            .sub(offset_of!(Thread, sched_entity))
            .cast::<Thread>()
    }
}
