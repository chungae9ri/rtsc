# Rust runtime scheduler (rtsched) for Cortex-M

`rtsched` is a runtime scheduler crate for Cortex-M systems. It provides
the core pieces needed to create and switch between application threads on a
single microcontroller core.

The crate currently includes:

- `Earliest Deadline First (EDF)` scheduling through the `KTimer` framework.
- A kernel timer queue built on an intrusive red-black tree.
- CFS (Completely Fair Scheduler) style scheduled threads `CfsThread` through the `RunQueue` red-black tree.
- `RtThread` associated with a dedicated `KTimer` entry for `EDF` scheduling.
- Synthetic Cortex-M thread stack setup with `forkyi`.
- CPU resource yielding (`yieldyi`) to the next `active` thread.
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

CFS scheduler doesn't starve lower-priority threads because even the lowest-priority thread gets a minimum CPU resource slot for running.

CFS threads are enqueued to or dequeued from the `RunQueue` (runq) rbtree by using `RbNode` in the `SchedEntity`.

CFS has its dedicated CFS KTimer having `execution time` and `duration (period)`. CFS timer also runs `yieldyi` to yield CPU time to other threads when its `runtime` hits the execution time.

## Soft Realtime Scheduler for RtThread
Each `RtThread` has its own entry with duration and deadline in the `KTimer` red-black tree (rbtree).

`RtThread` should complete its job before the deadline and yield CPU ownership to the next thread with the left-most entry in the
`KTimer` rbtree. When the current `RtThread` yields, current `RtThread` is set to `inactive` and the deadline is updated as
(duration - elapsed_time) and enqueued to the timer rbtree. The inactive thread is back to `active` when it gets contex-switch-in from the systick interrupt.

## KTimer framework
`KTimer` framework is the foundation for the CFS scheduler and RT scheduler. `KTimer` framework builds a
red-black tree with `KTimerEntity` defined as:
```
pub struct KTimerEntity {
    duration: u32,
    deadline: u32,
    node: RbNode,
    active: bool,
}
```
The `duration` field is a periodic timer interrupt interval for `RtTimer`. Systick interrupt for `CfsKTimer` works differently.
When `CfsKTimer` switches to active, it sets the `systick interrupt` with its `execution time`. When `CfsKTimer` is switched out, it
sets next `systick interrupt` with its duration time and is set to `inactive`.

`RtThread` has its own duration value for its periodic scheduling.

`deadline` is the next timer expiration value and is updated whenever a SysTick timer interrupt occurs.

`RbNode` is the entry to the `KTimer` rbtree.

`active` flag shows current timer isn't served yet until its deadline, so this KTimer is a candidated to be picked by the scheduler.

## Example of scheduling

C: needed runtime to finish its work
D: periodic time (duration) of the thread (initial deadline)

example 1:

```text
Ta: C=2, D=5
Tb: c=3, D=10
0     1     2     3     4     5     6     7     8     9     10
|-----Ta----|--------Tb-------|-----Ta----|-------idle------|
```


example 2:
```text
Ta: C=2, D=5
Tb: C=3, D=9
Tc: C=1, D=6
0     1     2     3     4     5     6     7     8     9
|-----Ta----|-Tc--|-----Tb----|-Ta--|-Tc--|-Tb--|-Ta--|
```


## Caveat
This crate is experimental scheduler/runtime code and is actively under development.

Most public APIs that accept raw pointers are unsafe because callers must provide valid static thread storage, valid stacks, and correctly initialized timer entities.

How an application can use the rtsched crate is demonstrated in https://github.com/chungae9ri/slos-m.
