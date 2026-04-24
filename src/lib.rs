#![no_std]

/// Crate version taken from Cargo metadata at compile time.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ktimer;
mod rbtree;
mod sched;
mod thread;

/// Re-exports of core scheduler primitives for convenient use in application code.
pub use thread::{AlignedStack, Thread, ThreadState, forkyi};

pub use ktimer::{
    KTimerEntity, KTimerQueue, enqueue_ktimer, init_ktimer_queue, next_ktimer_deadline,
    next_ktimer_systick_reload, systick_reload_from_tick_count,
};

pub use sched::{
    SchedEntity, dequeue_thread, enqueue_thread, handle_systick, init_current, init_rq,
    spawn_main_thread, traverse_run_queue,
};
