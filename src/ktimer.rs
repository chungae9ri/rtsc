// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

//! Kernel timer queue keyed by ktimer deadline.
//!
//! The queue is intrusive: each `KTimerEntity` embeds its own `RbNode`, so
//! inserting ktimers does not allocate.

use core::cell::UnsafeCell;
use core::mem::offset_of;
use core::ptr;

use crate::rbtree::{RBTree, RBTreeNode, RbNode};

pub const SYSTICK_RELOAD_MAX: u32 = 0x00FF_FFFF;
static KTIMER_QUEUE: GlobalKTimerQueue = GlobalKTimerQueue::new();

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
pub struct KTimerEntity {
    duration: u32,
    deadline: u32,
    node: RbNode,
}

impl KTimerEntity {
    pub const fn new(duration: u32, deadline: u32) -> Self {
        Self {
            duration,
            deadline,
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

    pub fn reset_links(&mut self) {
        self.node.reset_links();
    }

    pub fn is_linked(&self) -> bool {
        self.node.is_linked()
    }
}

/// Convert a raw SysTick tick interval into a reload register value.
///
/// SysTick reload stores `tick_count - 1`, and the register is 24 bits wide.
pub fn systick_reload_from_tick_count(tick_count: u32) -> Option<u32> {
    tick_count
        .checked_sub(1)
        .filter(|&reload| reload <= SYSTICK_RELOAD_MAX)
}

pub unsafe fn init_ktimer_queue() {
    unsafe {
        *KTIMER_QUEUE.get() = KTimerQueue::new();
    }
}

pub unsafe fn enqueue_ktimer(entity: *mut KTimerEntity) {
    unsafe {
        (*entity).reset_links();
        (*KTIMER_QUEUE.get()).insert(entity);
    }
}

pub fn next_ktimer_deadline() -> Option<u32> {
    unsafe { (*KTIMER_QUEUE.get()).next_deadline() }
}

pub fn next_ktimer_systick_reload() -> Option<u32> {
    unsafe { (*KTIMER_QUEUE.get()).next_systick_reload() }
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

    pub fn next_systick_reload(&self) -> Option<u32> {
        self.next_deadline()
            .and_then(systick_reload_from_tick_count)
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
