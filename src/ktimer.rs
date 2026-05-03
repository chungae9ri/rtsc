// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

//! Kernel timer queue keyed by ktimer deadline.
//!
//! The queue is intrusive: each `KTimerEntity` embeds its own `RbNode`, so
//! inserting ktimers does not allocate.

use core::cell::UnsafeCell;
use core::mem::offset_of;
use core::ptr;

use cortex_m::{interrupt, peripheral::SYST};

use crate::rbtree::{RBTree, RBTreeNode, RbNode};
use crate::thread::Thread;

pub const SYSTICK_RELOAD_MAX: u32 = 0x00FF_FFFF;
static KTIMER_QUEUE: GlobalKTimerQueue = GlobalKTimerQueue::new();
static mut NEXT_KTIMER: *mut KTimerEntity = ptr::null_mut();

struct GlobalKTimerQueue {
    queue: UnsafeCell<KTimerQueue>,
}

impl GlobalKTimerQueue {
    const fn new() -> Self {
        Self {
            queue: UnsafeCell::new(KTimerQueue::new()),
        }
    }

    fn get(&self) -> *mut KTimerQueue {
        self.queue.get()
    }
}

unsafe impl Sync for GlobalKTimerQueue {}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KTimerType {
    Cfs,
    Rt,
}

#[repr(C)]
pub struct KTimerEntity {
    duration: u32,
    deadline: u32,
    timer_type: KTimerType,
    thread: *mut Thread,
    node: RbNode,
}

impl KTimerEntity {
    pub const fn new(
        duration: u32,
        deadline: u32,
        timer_type: KTimerType,
        thread: *mut Thread,
    ) -> Self {
        Self {
            duration,
            deadline,
            timer_type,
            thread,
            node: RbNode::new(),
        }
    }

    pub fn duration(&self) -> u32 {
        self.duration
    }

    pub fn deadline(&self) -> u32 {
        self.deadline
    }

    pub fn set_deadline(&mut self, deadline: u32) {
        self.deadline = deadline;
    }

    pub fn timer_type(&self) -> KTimerType {
        self.timer_type
    }

    pub fn set_timer_type(&mut self, timer_type: KTimerType) {
        self.timer_type = timer_type;
    }

    pub fn thread(&self) -> *mut Thread {
        self.thread
    }

    pub fn init_thread(&mut self, thread: *mut Thread) {
        self.thread = thread;
    }

    pub fn set_thread(&mut self, thread: *mut Thread) {
        self.init_thread(thread);
    }

    pub fn reset_links(&mut self) {
        self.node.reset_links();
    }

    pub fn is_linked(&self) -> bool {
        self.node.is_linked()
    }
}

/// Convert a raw tick interval into a SysTick reload register value.
///
/// SysTick reload stores `ticks - 1`, and the register is 24 bits wide.
pub fn reload_from_ticks(ticks: u32) -> Option<u32> {
    ticks
        .checked_sub(1)
        .filter(|&reload| reload <= SYSTICK_RELOAD_MAX)
}

pub unsafe fn init_ktimer_queue() {
    interrupt::free(|_| unsafe {
        *KTIMER_QUEUE.get() = KTimerQueue::new();
    });
}

pub unsafe fn enqueue_ktimer(entity: *mut KTimerEntity) {
    interrupt::free(|_| unsafe {
        (*entity).reset_links();
        (*KTIMER_QUEUE.get()).insert(entity);
    });
}

pub fn next_ktimer_deadline() -> Option<u32> {
    interrupt::free(|_| unsafe { (*KTIMER_QUEUE.get()).next_deadline() })
}

pub fn next_ktimer_reload() -> Option<u32> {
    interrupt::free(|_| unsafe { (*KTIMER_QUEUE.get()).next_reload() })
}

pub(crate) fn elapsed_ticks_since_last_interrupt() -> u32 {
    SYST::get_reload().saturating_add(1)
}

pub(crate) fn elapsed_ticks_since_current_reload() -> u32 {
    SYST::get_reload().saturating_sub(SYST::get_current())
}

pub(crate) unsafe fn advance_ktimers(elapsed: u32) {
    interrupt::free(|_| unsafe {
        (*KTIMER_QUEUE.get()).advance(elapsed);
    });
}

pub(crate) unsafe fn dispatch_expired_ktimer() -> *mut KTimerEntity {
    interrupt::free(|_| unsafe { (*KTIMER_QUEUE.get()).dispatch_expired() })
}

pub(crate) unsafe fn update_next_ktimer(entity: *mut KTimerEntity) {
    interrupt::free(|_| unsafe {
        NEXT_KTIMER = entity;
    });
}

pub(crate) fn next_ktimer() -> *mut KTimerEntity {
    interrupt::free(|_| unsafe { NEXT_KTIMER })
}

pub(crate) fn update_next_ktimer_to_first() {
    interrupt::free(|_| unsafe {
        NEXT_KTIMER = (*KTIMER_QUEUE.get()).first();
    });
}

pub(crate) unsafe fn reset_rt_ktimer_deadline(thread: *mut Thread) -> bool {
    interrupt::free(|_| unsafe {
        let queue = &mut *KTIMER_QUEUE.get();
        let mut entity = queue.first();

        while !entity.is_null() {
            let next = queue.next(entity);
            if (*entity).timer_type() == KTimerType::Rt && (*entity).thread() == thread {
                queue.remove(entity);
                (*entity).set_deadline((*entity).duration());
                queue.insert(entity);
                return true;
            }
            entity = next;
        }

        false
    })
}

pub(crate) unsafe fn yield_rt_ktimer(thread: *mut Thread, elapsed: u32) -> bool {
    interrupt::free(|_| unsafe {
        let queue = &mut *KTIMER_QUEUE.get();
        let Some(entity) = queue.pop_first() else {
            return false;
        };
        let entity = entity as *mut KTimerEntity;

        //debug_assert!((*entity).timer_type() == KTimerType::Rt);
        //debug_assert!((*entity).thread() == thread);

        if (*entity).timer_type() != KTimerType::Rt || (*entity).thread() != thread {
            queue.insert(entity);
            return false;
        }

        (*entity).set_deadline((*entity).duration() - elapsed);
        queue.advance(elapsed);
        queue.insert(entity);
        true
    })
}

pub(crate) fn program_next_systick() -> Option<u32> {
    interrupt::free(|_| unsafe {
        let queue = &mut *KTIMER_QUEUE.get();
        let entity = queue.first();
        if entity.is_null() {
            return None;
        }

        let reload = (*entity).duration();
        (*entity).set_deadline(reload);

        (*SYST::PTR).rvr.write(reload);
        (*SYST::PTR).cvr.write(0);

        Some(reload)
    })
}

unsafe impl RBTreeNode for KTimerEntity {
    fn node(entity: *mut Self) -> *mut RbNode {
        if entity.is_null() {
            ptr::null_mut()
        } else {
            unsafe { ptr::addr_of_mut!((*entity).node) }
        }
    }

    fn entity_of(node: *mut RbNode) -> *mut Self {
        if node.is_null() {
            ptr::null_mut()
        } else {
            unsafe {
                (node as *mut u8)
                    .sub(offset_of!(KTimerEntity, node))
                    .cast::<KTimerEntity>()
            }
        }
    }

    fn entity_of_const(node: *const RbNode) -> *const Self {
        if node.is_null() {
            ptr::null()
        } else {
            unsafe {
                (node as *const u8)
                    .sub(offset_of!(KTimerEntity, node))
                    .cast::<KTimerEntity>()
            }
        }
    }

    unsafe fn cmp(a: *const Self, b: *const Self) -> core::cmp::Ordering {
        unsafe {
            match (*a).deadline.cmp(&(*b).deadline) {
                core::cmp::Ordering::Equal => (a as usize).cmp(&(b as usize)),
                other => other,
            }
        }
    }
}

pub struct KTimerQueue {
    tree: RBTree<KTimerEntity>,
}

impl KTimerQueue {
    pub const fn new() -> Self {
        Self {
            tree: RBTree::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tree.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tree.len()
    }

    pub fn root(&self) -> *mut KTimerEntity {
        self.tree.root()
    }

    pub fn first(&self) -> *mut KTimerEntity {
        self.tree.first()
    }

    pub fn last(&self) -> *mut KTimerEntity {
        self.tree.last()
    }

    pub fn next(&self, entity: *mut KTimerEntity) -> *mut KTimerEntity {
        self.tree.next(entity)
    }

    pub fn next_deadline(&self) -> Option<u32> {
        let first = self.first();
        if first.is_null() {
            None
        } else {
            Some(unsafe { (*first).deadline() })
        }
    }

    pub fn next_reload(&self) -> Option<u32> {
        self.next_deadline().and_then(reload_from_ticks)
    }

    pub unsafe fn advance(&mut self, elapsed: u32) {
        unsafe {
            let mut entity = self.first();
            while !entity.is_null() {
                let next = self.next(entity);
                (*entity).deadline = (*entity).deadline.saturating_sub(elapsed);
                entity = next;
            }
        }
    }

    pub unsafe fn dispatch_expired(&mut self) -> *mut KTimerEntity {
        unsafe {
            let Some(expired) = self.pop_first() else {
                return ptr::null_mut();
            };

            let expired = expired as *mut KTimerEntity;
            (*expired).deadline = (*expired).duration();
            self.insert(expired);
            self.first()
        }
    }

    /// Insert a detached ktimer entity into the queue.
    ///
    /// # Safety
    ///
    /// The caller must ensure `entity` is valid for mutation and is not already
    /// linked into a queue.
    pub unsafe fn insert(&mut self, entity: *mut KTimerEntity) {
        unsafe { self.tree.insert(entity) }
    }

    /// Remove a ktimer entity from the queue.
    ///
    /// # Safety
    ///
    /// The caller must ensure `entity` currently belongs to this queue.
    pub unsafe fn remove(&mut self, entity: *mut KTimerEntity) -> *mut KTimerEntity {
        unsafe { self.tree.remove(entity) }
    }

    /// Remove and return the earliest ktimer entity in the queue.
    pub unsafe fn pop_first(&mut self) -> Option<&mut KTimerEntity> {
        unsafe { self.tree.pop_first() }
    }
}

impl Default for KTimerQueue {
    fn default() -> Self {
        Self::new()
    }
}
