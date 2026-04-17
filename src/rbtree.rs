// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

//! Intrusive red-black tree keyed by scheduler entity time.
//!
//! The tree stores `sched_entity` nodes directly and does not allocate. This
//! fits scheduler code that manages task metadata in pre-allocated control
//! blocks.

use core::cmp::Ordering;
use core::ptr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

/// Scheduler entity used as the tree node and ordering key.
///
/// `time` is the primary key. When two entities have the same `time`, their
/// addresses are used as a stable tie-breaker so insertion order remains
/// deterministic and the tree keeps a strict total ordering.
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct sched_entity {
    /// Scheduler time metric used as the red-black tree key.
    pub time: u64,
    parent: *mut sched_entity,
    left: *mut sched_entity,
    right: *mut sched_entity,
    color: Color,
}

impl sched_entity {
    /// Create a detached scheduler entity that can be inserted into a tree.
    pub const fn new(time: u64) -> Self {
        Self {
            time,
            parent: ptr::null_mut(),
            left: ptr::null_mut(),
            right: ptr::null_mut(),
            color: Color::Red,
        }
    }

    /// Reset linkage so the entity can be reused or inserted into another tree.
    pub fn reset_links(&mut self) {
        self.parent = ptr::null_mut();
        self.left = ptr::null_mut();
        self.right = ptr::null_mut();
        self.color = Color::Red;
    }

    /// Return `true` if the entity is currently linked under another node.
    pub fn is_linked(&self) -> bool {
        !self.parent.is_null() || !self.left.is_null() || !self.right.is_null()
    }
}

/// Red-black tree of `sched_entity` nodes ordered by `time`.
#[derive(Debug)]
pub struct RBTree {
    root: *mut sched_entity,
    len: usize,
}

impl Default for RBTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RBTree {
    pub const fn new() -> Self {
        Self {
            root: ptr::null_mut(),
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_null()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn root(&self) -> *mut sched_entity {
        self.root
    }

    pub fn first(&self) -> *mut sched_entity {
        Self::minimum(self.root)
    }

    pub fn last(&self) -> *mut sched_entity {
        Self::maximum(self.root)
    }

    /// Insert a detached scheduler entity into the tree.
    ///
    /// # Safety
    ///
    /// The caller must ensure `entity` is valid for mutation and is not
    /// simultaneously linked into another tree.
    pub unsafe fn insert(&mut self, entity: *mut sched_entity) {
        debug_assert!(!entity.is_null());

        unsafe {
            (*entity).left = ptr::null_mut();
            (*entity).right = ptr::null_mut();
            (*entity).parent = ptr::null_mut();
            (*entity).color = Color::Red;

            let mut parent = ptr::null_mut();
            let mut current = self.root;

            while !current.is_null() {
                parent = current;
                match Self::cmp(entity, current) {
                    Ordering::Less => current = (*current).left,
                    Ordering::Greater => current = (*current).right,
                    Ordering::Equal => unreachable!("address tie-break keeps ordering strict"),
                }
            }

            (*entity).parent = parent;
            if parent.is_null() {
                self.root = entity;
            } else if Self::cmp(entity, parent) == Ordering::Less {
                (*parent).left = entity;
            } else {
                (*parent).right = entity;
            }

            self.insert_fixup(entity);
            self.len += 1;
        }
    }

    /// Remove an entity from the tree.
    ///
    /// Returns the removed pointer, or null if `entity` was null.
    ///
    /// # Safety
    ///
    /// The caller must ensure `entity` currently belongs to this tree.
    pub unsafe fn remove(&mut self, entity: *mut sched_entity) -> *mut sched_entity {
        if entity.is_null() {
            return ptr::null_mut();
        }

        unsafe {
            let mut y = entity;
            let mut y_original_color = (*y).color;
            let x;
            let x_parent;

            if (*entity).left.is_null() {
                x = (*entity).right;
                x_parent = (*entity).parent;
                self.transplant(entity, (*entity).right);
            } else if (*entity).right.is_null() {
                x = (*entity).left;
                x_parent = (*entity).parent;
                self.transplant(entity, (*entity).left);
            } else {
                y = Self::minimum((*entity).right);
                y_original_color = (*y).color;
                x = (*y).right;

                if (*y).parent == entity {
                    x_parent = y;
                    if !x.is_null() {
                        (*x).parent = y;
                    }
                } else {
                    x_parent = (*y).parent;
                    self.transplant(y, (*y).right);
                    (*y).right = (*entity).right;
                    (*(*y).right).parent = y;
                }

                self.transplant(entity, y);
                (*y).left = (*entity).left;
                (*(*y).left).parent = y;
                (*y).color = (*entity).color;
            }

            if y_original_color == Color::Black {
                self.remove_fixup(x, x_parent);
            }

            (*entity).parent = ptr::null_mut();
            (*entity).left = ptr::null_mut();
            (*entity).right = ptr::null_mut();
            (*entity).color = Color::Red;
            self.len -= 1;
            entity
        }
    }

    /// Remove and return the left-most entity in the tree.
    ///
    /// # Safety
    ///
    /// Returned reference is valid only as long as the caller maintains the
    /// lifetime of the underlying entity.
    pub unsafe fn pop_first(&mut self) -> Option<&mut sched_entity> {
        let first = self.first();
        if first.is_null() {
            return None;
        }

        unsafe {
            self.remove(first);
            Some(&mut *first)
        }
    }

    fn color_of(node: *mut sched_entity) -> Color {
        if node.is_null() {
            Color::Black
        } else {
            unsafe { (*node).color }
        }
    }

    fn minimum(mut node: *mut sched_entity) -> *mut sched_entity {
        unsafe {
            while !node.is_null() && !(*node).left.is_null() {
                node = (*node).left;
            }
        }
        node
    }

    fn maximum(mut node: *mut sched_entity) -> *mut sched_entity {
        unsafe {
            while !node.is_null() && !(*node).right.is_null() {
                node = (*node).right;
            }
        }
        node
    }

    fn cmp(a: *const sched_entity, b: *const sched_entity) -> Ordering {
        unsafe {
            match (*a).time.cmp(&(*b).time) {
                Ordering::Equal => (a as usize).cmp(&(b as usize)),
                other => other,
            }
        }
    }

    unsafe fn left_rotate(&mut self, x: *mut sched_entity) {
        unsafe {
            let y = (*x).right;
            debug_assert!(!y.is_null());

            (*x).right = (*y).left;
            if !(*y).left.is_null() {
                (*(*y).left).parent = x;
            }

            (*y).parent = (*x).parent;
            if (*x).parent.is_null() {
                self.root = y;
            } else if x == (*(*x).parent).left {
                (*(*x).parent).left = y;
            } else {
                (*(*x).parent).right = y;
            }

            (*y).left = x;
            (*x).parent = y;
        }
    }

    unsafe fn right_rotate(&mut self, y: *mut sched_entity) {
        unsafe {
            let x = (*y).left;
            debug_assert!(!x.is_null());

            (*y).left = (*x).right;
            if !(*x).right.is_null() {
                (*(*x).right).parent = y;
            }

            (*x).parent = (*y).parent;
            if (*y).parent.is_null() {
                self.root = x;
            } else if y == (*(*y).parent).right {
                (*(*y).parent).right = x;
            } else {
                (*(*y).parent).left = x;
            }

            (*x).right = y;
            (*y).parent = x;
        }
    }

    unsafe fn insert_fixup(&mut self, mut z: *mut sched_entity) {
        unsafe {
            while Self::color_of((*z).parent) == Color::Red {
                let parent = (*z).parent;
                let grandparent = (*parent).parent;

                if parent == (*grandparent).left {
                    let uncle = (*grandparent).right;
                    if Self::color_of(uncle) == Color::Red {
                        (*parent).color = Color::Black;
                        (*uncle).color = Color::Black;
                        (*grandparent).color = Color::Red;
                        z = grandparent;
                    } else {
                        if z == (*parent).right {
                            z = parent;
                            self.left_rotate(z);
                        }

                        let parent = (*z).parent;
                        let grandparent = (*parent).parent;
                        (*parent).color = Color::Black;
                        (*grandparent).color = Color::Red;
                        self.right_rotate(grandparent);
                    }
                } else {
                    let uncle = (*grandparent).left;
                    if Self::color_of(uncle) == Color::Red {
                        (*parent).color = Color::Black;
                        (*uncle).color = Color::Black;
                        (*grandparent).color = Color::Red;
                        z = grandparent;
                    } else {
                        if z == (*parent).left {
                            z = parent;
                            self.right_rotate(z);
                        }

                        let parent = (*z).parent;
                        let grandparent = (*parent).parent;
                        (*parent).color = Color::Black;
                        (*grandparent).color = Color::Red;
                        self.left_rotate(grandparent);
                    }
                }
            }

            if !self.root.is_null() {
                (*self.root).color = Color::Black;
            }
        }
    }

    unsafe fn transplant(&mut self, u: *mut sched_entity, v: *mut sched_entity) {
        unsafe {
            if (*u).parent.is_null() {
                self.root = v;
            } else if u == (*(*u).parent).left {
                (*(*u).parent).left = v;
            } else {
                (*(*u).parent).right = v;
            }

            if !v.is_null() {
                (*v).parent = (*u).parent;
            }
        }
    }

    unsafe fn remove_fixup(&mut self, mut x: *mut sched_entity, mut parent: *mut sched_entity) {
        unsafe {
            while x != self.root && Self::color_of(x) == Color::Black {
                if x == parent_left(parent) {
                    let mut w = parent_right(parent);

                    if Self::color_of(w) == Color::Red {
                        (*w).color = Color::Black;
                        (*parent).color = Color::Red;
                        self.left_rotate(parent);
                        w = parent_right(parent);
                    }

                    if Self::color_of(left_of(w)) == Color::Black
                        && Self::color_of(right_of(w)) == Color::Black
                    {
                        if !w.is_null() {
                            (*w).color = Color::Red;
                        }
                        x = parent;
                        parent = parent_of(x);
                    } else {
                        if Self::color_of(right_of(w)) == Color::Black {
                            let left = left_of(w);
                            if !left.is_null() {
                                (*left).color = Color::Black;
                            }
                            if !w.is_null() {
                                (*w).color = Color::Red;
                                self.right_rotate(w);
                            }
                            w = parent_right(parent);
                        }

                        if !w.is_null() {
                            (*w).color = (*parent).color;
                        }
                        (*parent).color = Color::Black;
                        let right = right_of(w);
                        if !right.is_null() {
                            (*right).color = Color::Black;
                        }
                        self.left_rotate(parent);
                        x = self.root;
                        parent = ptr::null_mut();
                    }
                } else {
                    let mut w = parent_left(parent);

                    if Self::color_of(w) == Color::Red {
                        (*w).color = Color::Black;
                        (*parent).color = Color::Red;
                        self.right_rotate(parent);
                        w = parent_left(parent);
                    }

                    if Self::color_of(right_of(w)) == Color::Black
                        && Self::color_of(left_of(w)) == Color::Black
                    {
                        if !w.is_null() {
                            (*w).color = Color::Red;
                        }
                        x = parent;
                        parent = parent_of(x);
                    } else {
                        if Self::color_of(left_of(w)) == Color::Black {
                            let right = right_of(w);
                            if !right.is_null() {
                                (*right).color = Color::Black;
                            }
                            if !w.is_null() {
                                (*w).color = Color::Red;
                                self.left_rotate(w);
                            }
                            w = parent_left(parent);
                        }

                        if !w.is_null() {
                            (*w).color = (*parent).color;
                        }
                        (*parent).color = Color::Black;
                        let left = left_of(w);
                        if !left.is_null() {
                            (*left).color = Color::Black;
                        }
                        self.right_rotate(parent);
                        x = self.root;
                        parent = ptr::null_mut();
                    }
                }
            }

            if !x.is_null() {
                (*x).color = Color::Black;
            }
        }
    }
}

fn parent_of(node: *mut sched_entity) -> *mut sched_entity {
    if node.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*node).parent }
    }
}

fn left_of(node: *mut sched_entity) -> *mut sched_entity {
    if node.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*node).left }
    }
}

fn right_of(node: *mut sched_entity) -> *mut sched_entity {
    if node.is_null() {
        ptr::null_mut()
    } else {
        unsafe { (*node).right }
    }
}

fn parent_left(node: *mut sched_entity) -> *mut sched_entity {
    left_of(node)
}

fn parent_right(node: *mut sched_entity) -> *mut sched_entity {
    right_of(node)
}
