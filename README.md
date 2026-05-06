# Rust runtime scheduler (rtsched) for Cortex-M

`rtsched` is a runtime scheduler crate for Cortex-M systems. It provides
the core pieces needed to create and switch between application threads on a
single microcontroller core.

The crate currently includes:

- Earliest scheduled thread first scheduling through the `KTimer` framework.
- A kernel timer queue built on an intrusive red-black tree.
- CFS (Completely Fair Scheduler) style scheduled threads `CfsThread` through the `RunQueue` red-black tree.
- `RtThread` associated with a dedicated `KTimer` entry for `EDF` (Earliest Deadline First) style scheduling.
- Synthetic Cortex-M thread stack setup with `forkyi`.
- CPU resource yielding (`yieldyi`) in `RtThread` to the next earliest scheduled thread.
- PendSV/SVCall based preemptive context switching support.
- SysTick integration for advancing timers and requesting scheduler dispatch.

`rtsched` is intended to be used by a board crate that owns hardware setup,
clock configuration, SysTick configuration, and concrete thread storage. The
board initializes the ktimer queue and CFS scheduler, creates threads with
dedicated stacks, then starts the first thread with `spawn_main_thread`.

## CFS (Completely Fair Scheduler) Scheduler
CFS scheduler assigns the CFS time slot to all CFS tasks based on the priority-based virtual runtime (`vruntime`).
`vruntime` of each CFS thread is defined as:
vruntime = (ticks_consumed * priority) / priority_sum_of_all_CFS_threads

This means a high-priority thread (lower value of priority, must be > 0) has a slower `vruntime` compared to
a low-priority thread. CFS scheduler does nothing but make this vruntime fair among the CFS tasks.

CFS scheduler doesn't starve lower-priority threads because even the lowest-priority thread gets a minimum CPU resource slot while running.

CFS threads are enqueued to or dequeued from the `RunQueue` (runq) rbtree by using `RbNode` in the `SchedEntity`.

## Soft Realtime Scheduler for RtThread
Each `RtThread` has its own entry with duration and deadline in the `KTimer` red-black tree (rbtree).

`RtThread` should complete its job before the deadline and yield CPU ownership to the next thread with the left-most entry in the
`KTimer` rbtree. When the current `RtThread` yields, the deadline of the current `RtThread` is updated as
(duration - elapsed_time) and enqueued to the timer rbtree.

## KTimer framework
`KTimer` framework is the foundation for the CFS scheduler and RT scheduler. `KTimer` framework builds a
red-black tree with `KTimerEntity` defined as:
```
pub struct KTimerEntity {
    duration: u32,
    deadline: u32,
    timer_type: KTimerType,
    thread: *mut Thread,
    node: RbNode,
}
```
The `duration` field is a periodic timer interrupt interval. CFS threads in the CFS runq share one sched_tick duration (10ms by default).
`RtThread` has its own duration value for its periodic scheduling. `deadline` is the next timer expiration value and
is updated whenever a SysTick timer interrupt occurs. `KTimerType` is either a `Cfs` or `Rt` timer.
The `thread` field assigns a dedicated `RtThread` to the timer, and when an RT timer SysTick interrupt occurs,
the linked `RtThread` is switched in and run. If a CFS timer SysTick interrupt occurs, CFS scheduler picks
the least fair CFS thread from the runq and runs it. `RbNode` is the entry to the `KTimer` rbtree.

## Caveat
This crate is experimental scheduler/runtime code and is actively under development.

Most public APIs that accept raw pointers are unsafe because callers must provide valid static thread storage, valid stacks, and correctly initialized timer entities.

How an application can use the rtsched crate is demonstrated in https://github.com/chungae9ri/slos-m.
