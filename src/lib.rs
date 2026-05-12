#![no_std]

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ktimer;
mod rbtree;
mod sched;
mod thread;

/// Re-exports of core scheduler primitives for convenient use in application code.
pub use thread::{AlignedStack, CfsThread, RtThread, ThreadCtx, ThreadState, forkyi, yieldyi};

pub use ktimer::{KTimerEntity, RtKTimer, enqueue_ktimer, init_ktimer_queue, next_ktimer_reload};

pub use sched::{
    WaitThreadMapError, dequeue_to_wait_map, enqueue_from_wait_map, handle_systick, init_cfs,
    spawn_main_thread, traverse_run_queue,
};
