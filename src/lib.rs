#![no_std]

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod rbtree;
mod sched;
mod thread;

/// Re-exports of core scheduler primitives for convenient use in application code.
pub use thread::{AlignedStack, Thread, ThreadState, forkyi};

pub use sched::{
    SchedEntity, dequeue_thread, enqueue_thread, init_current, init_rq, spawn_main_thread,
    traverse_run_queue,
};
