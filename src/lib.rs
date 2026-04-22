#![no_std]

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod rbtree;
pub mod sched;
pub mod thread;

/// Re-exports of core scheduler primitives for convenient use in application code.
pub use thread::{AlignedStack, CalleeSavedRegisters, Thread, ThreadState, forkyi};

pub use sched::{
    dequeue_thread, enqueue_thread, init_current, init_rq, sched_entity, spawn_main_thread,
    traverse_run_queue,
};
