#![no_std]

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod rbtree;
pub mod sched;
pub mod task;

/// Re-exports of core scheduler primitives for convenient use in application code.
pub use task::{AlignedStack, CalleeSavedRegisters, Task, TaskState, forkyi};

pub use sched::{dequeue_task, enqueue_task, init_current, init_rq, start_first_task};
