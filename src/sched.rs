// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

use core::arch::global_asm;
use core::cell::UnsafeCell;
use core::mem::offset_of;
use core::ptr;

use core::arch::asm;
use cortex_m::peripheral::SCB;

use crate::ktimer::{
    KTimerEntity, KTimerType, advance_ktimers, dispatch_expired_ktimer,
    elapsed_ticks_since_last_interrupt, enqueue_ktimer, next_ktimer, program_next_systick,
    update_next_ktimer,
};
use crate::rbtree::{RBTree, RBTreeNode, RbNode};
use crate::thread::{
    Thread, ThreadControlBlock, ThreadState, cfs_sched_entity, thread_from_cfs_sched_entity,
};
//use rtt_target::rprintln;

pub(crate) static mut CFS_TIMER_ENTITY: KTimerEntity =
    KTimerEntity::new(0, 0, KTimerType::Cfs, ptr::null_mut::<Thread>());
pub(crate) static CFS_RUN_QUEUE: RunQueue = RunQueue::new();
#[unsafe(no_mangle)]
pub static mut CURRENT_THREAD: *mut Thread = ptr::null_mut();
pub(crate) static mut CURRENT_THREAD_IS_CFS: bool = false;
#[unsafe(no_mangle)]
static mut START_THREAD_PTR: *mut Thread = ptr::null_mut();

pub(crate) struct RunQueue {
    tree: UnsafeCell<RBTree<SchedEntity>>,
    priority_sum: UnsafeCell<u32>,
}

impl RunQueue {
    const fn new() -> Self {
        Self {
            tree: UnsafeCell::new(RBTree::new()),
            priority_sum: UnsafeCell::new(0),
        }
    }

    pub(crate) fn get(&self) -> *mut RBTree<SchedEntity> {
        self.tree.get()
    }

    fn priority_sum(&self) -> *mut u32 {
        self.priority_sum.get()
    }
}

unsafe impl Sync for RunQueue {}

/// Scheduler entity used as the tree node and ordering key.
///
/// `vruntime` is the primary key. When two entities have the same
/// `vruntime`, their addresses are used as a stable tie-breaker so insertion
/// order remains deterministic and the tree keeps a strict total ordering.
#[repr(C)]
pub struct SchedEntity {
    pub(crate) sched_tick_cnt: u64,
    /// Scheduler virtual runtime metric used as the red-black tree key.
    pub(crate) vruntime: u64,
    /// Scheduling priority, where the exact ordering is defined by the scheduler.
    pub priority: u32,
    pub(crate) rb_node: RbNode,
}

impl SchedEntity {
    /// Create a detached scheduler entity that can be inserted into a tree.
    pub const fn new(priority: u32) -> Self {
        Self {
            sched_tick_cnt: 0,
            vruntime: 0,
            priority,
            rb_node: RbNode::new(),
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

    /// Return the scheduler virtual runtime used for run-queue ordering.
    pub fn vruntime(&self) -> u64 {
        self.vruntime
    }

    /// Return the scheduler tick count accumulated for this entity.
    pub fn sched_tick_cnt(&self) -> u64 {
        self.sched_tick_cnt
    }
}

unsafe impl RBTreeNode for SchedEntity {
    fn node(entity: *mut Self) -> *mut RbNode {
        if entity.is_null() {
            ptr::null_mut()
        } else {
            unsafe { ptr::addr_of_mut!((*entity).rb_node) }
        }
    }

    fn entity_of(node: *mut RbNode) -> *mut Self {
        if node.is_null() {
            ptr::null_mut()
        } else {
            unsafe {
                (node as *mut u8)
                    .sub(offset_of!(SchedEntity, rb_node))
                    .cast::<SchedEntity>()
            }
        }
    }

    fn entity_of_const(node: *const RbNode) -> *const Self {
        if node.is_null() {
            ptr::null()
        } else {
            unsafe {
                (node as *const u8)
                    .sub(offset_of!(SchedEntity, rb_node))
                    .cast::<SchedEntity>()
            }
        }
    }

    unsafe fn cmp(a: *const Self, b: *const Self) -> core::cmp::Ordering {
        unsafe {
            match (*a).vruntime.cmp(&(*b).vruntime) {
                core::cmp::Ordering::Equal => (a as usize).cmp(&(b as usize)),
                other => other,
            }
        }
    }
}

pub unsafe fn init_current<T: ThreadControlBlock>(thread: *mut T) {
    unsafe {
        CURRENT_THREAD = thread.cast::<Thread>();
        CURRENT_THREAD_IS_CFS = T::IS_CFS;
    }
}

pub(crate) fn thread_is_cfs(thread: *const Thread) -> bool {
    if thread.is_null() {
        return false;
    }

    unsafe {
        if thread == CURRENT_THREAD.cast_const() {
            return CURRENT_THREAD_IS_CFS;
        }

        let tree = &*CFS_RUN_QUEUE.get();
        let mut entity = tree.first();
        while !entity.is_null() {
            if thread_from_cfs_sched_entity(entity).cast_const() == thread {
                return true;
            }
            entity = tree.next(entity);
        }
    }

    false
}

/// Traverse the scheduler-visible threads, including the running thread.
///
/// Pass `None` to get the CURRENT_THREAD running thread when one exists; otherwise
/// this returns the first queued thread. Pass the previously returned thread to
/// get the next entry. After the running thread, traversal continues through
/// the run queue in ascending vruntime order. Returns `None` after the last
/// queued thread.
///
/// # Safety
///
/// The caller must ensure that any provided thread pointer still refers to a
/// valid thread control block and that the run queue is not concurrently
/// mutated in a way that invalidates the traversal step.
pub unsafe fn traverse_run_queue(cursor: Option<*mut Thread>) -> Option<*mut Thread> {
    unsafe {
        let tree = &*CFS_RUN_QUEUE.get();
        match cursor {
            None => {
                if !CURRENT_THREAD.is_null() {
                    Some(CURRENT_THREAD)
                } else {
                    let first = tree.first();
                    if first.is_null() {
                        None
                    } else {
                        Some(thread_from_cfs_sched_entity(first))
                    }
                }
            }
            Some(thread) if thread == CURRENT_THREAD => {
                let first = tree.first();
                if first.is_null() {
                    None
                } else {
                    Some(thread_from_cfs_sched_entity(first))
                }
            }
            Some(thread) => {
                let next = tree.next(cfs_sched_entity(thread));
                if next.is_null() {
                    None
                } else {
                    Some(thread_from_cfs_sched_entity(next))
                }
            }
        }
    }
}

/// Spawn main thread by restoring its prepared stack frame.
///
/// This does not return. The thread must already have been initialized with the
/// same synthetic frame layout produced by `forkyi`. The actual exception
/// return happens in `SVCall`, because `EXC_RETURN` is only valid from
/// handler mode.
pub unsafe fn spawn_main_thread(thread: *mut Thread) -> ! {
    unsafe {
        (*CFS_RUN_QUEUE.get()).remove(cfs_sched_entity(thread));
        (*thread).state = ThreadState::Running;
        START_THREAD_PTR = thread;
        CURRENT_THREAD_IS_CFS = true;
        asm!("svc 0", options(noreturn));
    }
}

/// Reset the scheduler run queue to an empty state.
unsafe fn init_cfs_rq() {
    unsafe {
        *CFS_RUN_QUEUE.get() = RBTree::new();
        *CFS_RUN_QUEUE.priority_sum() = 0;
    }
}

/// Initialize the CFS scheduler state and enqueue its scheduler timer.
///
/// `ticks` is expressed in raw timer ticks because the board owns the clock
/// configuration.
pub unsafe fn init_cfs(ticks: u32) {
    unsafe {
        init_cfs_rq();
        CFS_TIMER_ENTITY =
            KTimerEntity::new(ticks, ticks, KTimerType::Cfs, ptr::null_mut::<Thread>());
        enqueue_ktimer(ptr::addr_of_mut!(CFS_TIMER_ENTITY));
    }
}

/// Enqueue a thread into the scheduler run queue.
///
/// The thread's scheduler entity vruntime field is used as the red-black tree key.
pub unsafe fn enqueue_thread(thread: *mut Thread) {
    unsafe {
        (*thread).state = ThreadState::Ready;
        let entity = cfs_sched_entity(thread);
        (*entity).reset_links();
        (*CFS_RUN_QUEUE.get()).insert(entity);
        *CFS_RUN_QUEUE.priority_sum() += (*entity).priority;
    }
}

/// Remove a thread from the scheduler run queue if it is currently queued.
pub unsafe fn dequeue_thread(thread: *mut Thread) {
    unsafe {
        if (*thread).state == ThreadState::Ready {
            let entity = cfs_sched_entity(thread);
            (*CFS_RUN_QUEUE.get()).remove(entity);
            *CFS_RUN_QUEUE.priority_sum() -= (*entity).priority;
        }
    }
}

// Switch to the first thread which was set up by `forkyi`.
// This is typically called at the end of `main`.
// NOTE: Assembly below relies on the `Thread` layout defined in
// `rtsched/src/thread.rs` where `Thread.sp` is the first field (offset 0)
// and `Thread.exc_return` is the second field (offset 4). The save/restore
// sequence performed by PendSV/SVCall does NOT copy callee-saved registers
// into the `Thread.callee_saved_regs` struct field; instead it pushes r4-r11
// onto the thread's stack and stores the stack pointer into `Thread.sp`.
//
// Stack frame expectations produced by `forkyi`:
// - The synthetic thread entry frame left for exception return contains
//   (from low to high addresses): r4..r11 (pushed by PendSV), then the
//   standard hardware frame consumed by EXC_RETURN: r0, r1, r2, r3, r12, lr,
//   pc, xPSR. `Thread.sp` points at the saved r4..r11 block (the full saved
//   context begins at this pointer when restoring).
//
// Offsets used by the assembly:
// - `str r0, [r2]`   -> stores saved SP into `Thread.sp` (offset 0)
// - `str lr, [r2, #4]`-> stores EXC_RETURN into `Thread.exc_return` (offset 4)
//
// If you intend `Thread.callee_saved_regs` to mirror the saved r4..r11
// words, either update the assembly to store into that struct region or
// remove/ignore the `callee_saved_regs` field to avoid confusion.
global_asm!(
    ".section .text.SVCall,\"ax\",%progbits",
    ".global SVCall",
    ".type SVCall,%function",
    "SVCall:",
    "ldr r0, =START_THREAD_PTR",
    "ldr r0, [r0]", // r0 = thread
    "ldr r3, =CURRENT_THREAD",
    "str r0, [r3]",          // CURRENT_THREAD = thread
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
// called from here to select the next thread to run and update `CURRENT_THREAD`.
// Threads are expected to have their stack frames (PSP) prepared by `forkyi` so that the
// assembly code can save and restore them without needing to understand the layout.
global_asm!(
    ".section .text.PendSV,\"ax\",%progbits",
    ".global PendSV",
    ".type PendSV,%function",
    "PendSV:",
    "tst lr, #4", // Was the interrupted thread using PSP or MSP
    "ite eq",
    "mrseq r0, msp",           // Thread used MSP.
    "mrsne r0, psp",           // Thread used PSP.
    "stmdb r0!, {{r4-r11}}",   // Save callee-saved registers on the thread stack.
    "ldr r1, =CURRENT_THREAD", // R1 = &CURRENT_THREAD
    "ldr r2, [r1]",            // R2 = CURRENT_THREAD thread pointer
    "str r0, [r2]",            // Save updated stack pointer into the thread control block.
    "str lr, [r2, #4]",        // Save EXC_RETURN so the next restore uses MSP or PSP correctly.
    "bl schedule",             // Run the CURRENT_THREAD ktimer handler and update CURRENT_THREAD.
    "ldr r1, =CURRENT_THREAD", // R1 = &CURRENT_THREAD
    "ldr r2, [r1]",            // R2 = next thread pointer
    "ldr r0, [r2]",            // R0 = next thread's saved SP
    "ldr lr, [r2, #4]",        // LR = next thread's saved EXC_RETURN
    "ldmia r0!, {{r4-r11}}",   // Restore callee-saved registers for the selected thread.
    "tst lr, #4",              // Does the next thread return using MSP or PSP?
    "ite eq",
    "msreq msp, r0", // Restore MSP-backed context.
    "msrne psp, r0", // Restore PSP-backed context.
    "bx lr",
);

#[unsafe(no_mangle)]
extern "C" fn schedule() {
    unsafe {
        let next_ktimer = next_ktimer();
        if next_ktimer.is_null() {
            program_next_systick();
            return;
        }

        // The scheduler logic is as follows:
        // - If the CURRENT_THREAD is CFS, update its vruntime based on the elapsed
        //   ticks and its priority.
        // - If the next expired ktimer is for a CFS thread and current thread is
        //   CFS thread, compare its vruntime with the CURRENT_THREAD's vruntime
        //   to decide whether to preempt.
        // - If the next expired ktimer is for a CFS thread and current thread is
        //   RT thread, switch to the left-most CFS thread.
        // - If the next expired ktimer is for an RT thread and current thread is
        //   CFS thread, insert current to CFS runq and switch to next RT thread.
        // - If the next expired ktimer is for an RT thread and current thread is
        //   RT thread,preempt the CURRENT_THREAD with next RT thread.
        if !CURRENT_THREAD.is_null() && (*CURRENT_THREAD).state == ThreadState::Running {
            if CURRENT_THREAD_IS_CFS {
                let current_entity = cfs_sched_entity(CURRENT_THREAD);
                (*current_entity).sched_tick_cnt += u64::from(elapsed_ticks_since_last_interrupt());
                let priority_sum = *CFS_RUN_QUEUE.priority_sum();
                if priority_sum == 0 {
                    return;
                }
                let sched_tick_cnt = (*current_entity).sched_tick_cnt;
                let priority = u64::from((*current_entity).priority);
                let priority_sum = u64::from(priority_sum);

                (*current_entity).vruntime = sched_tick_cnt * priority / priority_sum;
            }

            if (*next_ktimer).timer_type() == KTimerType::Cfs {
                if let Some(next_entity) = (*CFS_RUN_QUEUE.get()).pop_first() {
                    let next_thread = thread_from_cfs_sched_entity(next_entity as *mut SchedEntity);

                    if CURRENT_THREAD_IS_CFS {
                        let current_entity = cfs_sched_entity(CURRENT_THREAD);
                        debug_assert!(
                            CURRENT_THREAD != next_thread,
                            "CFS_RUN_QUEUE.pop_first() returned the CURRENT_THREAD running thread"
                        );
                        if (*current_entity).vruntime > (*next_entity).vruntime {
                            (*CURRENT_THREAD).state = ThreadState::Ready;
                            (*CFS_RUN_QUEUE.get()).insert(current_entity);
                            (*next_thread).state = ThreadState::Running;
                            CURRENT_THREAD = next_thread;
                            CURRENT_THREAD_IS_CFS = true;
                        } else {
                            (*CFS_RUN_QUEUE.get()).insert(next_entity as *mut SchedEntity);
                        }
                    } else {
                        (*CURRENT_THREAD).state = ThreadState::Ready;
                        (*next_thread).state = ThreadState::Running;
                        CURRENT_THREAD = next_thread;
                        CURRENT_THREAD_IS_CFS = true;
                    }
                }
            } else if (*next_ktimer).timer_type() == KTimerType::Rt {
                let next_thread = (*next_ktimer).thread();
                if next_thread.is_null() {
                    return;
                }

                if CURRENT_THREAD_IS_CFS {
                    (*CURRENT_THREAD).state = ThreadState::Ready;
                    (*CFS_RUN_QUEUE.get()).insert(cfs_sched_entity(CURRENT_THREAD));
                    (*next_thread).state = ThreadState::Running;
                    CURRENT_THREAD = next_thread;
                    CURRENT_THREAD_IS_CFS = false;
                } else {
                    (*CURRENT_THREAD).state = ThreadState::Ready;
                    (*next_thread).state = ThreadState::Running;
                    CURRENT_THREAD = next_thread;
                    CURRENT_THREAD_IS_CFS = false;
                }
            }
        }

        program_next_systick();
    }
}

/// Handle one SysTick event and request ktimer dispatch.
pub fn handle_systick() {
    let elapsed = elapsed_ticks_since_last_interrupt();

    let next_ktimer = unsafe {
        advance_ktimers(elapsed);
        dispatch_expired_ktimer()
    };

    unsafe {
        update_next_ktimer(next_ktimer);
    }

    if !next_ktimer.is_null() {
        SCB::set_pendsv();
    }
}
