#![no_std]

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ktimer;
mod rbtree;
mod sched;
mod thread;

/// Re-exports of core scheduler primitives for convenient use in application code.
pub use thread::{
    AlignedStack, CfsThread, RtThread, Thread, ThreadControlBlock, ThreadState, forkyi, yieldyi,
};

pub use ktimer::{
    KTimerEntity, KTimerQueue, KTimerType, enqueue_ktimer, init_ktimer_queue, next_ktimer_deadline,
    next_ktimer_reload, reload_from_ticks,
};

pub use sched::{
    SchedEntity, dequeue_thread, enqueue_thread, handle_systick, init_cfs, init_current,
    spawn_main_thread, traverse_run_queue,
};
