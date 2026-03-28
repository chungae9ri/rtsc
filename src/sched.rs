// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

use cortex_m_rt::exception;
use rtt_target::rprintln;

/// PendSV handler used for deferred context switching work.
///
/// On Cortex-M, PendSV is commonly assigned the lowest practical priority so
/// context switching happens after higher-priority interrupt work completes.
#[exception]
fn PendSV() {
    rprintln!("pendsv");
}

/// SysTick handler used for scheduler tick processing.
///
/// A real scheduler would typically update time-based accounting here and may
/// pend PendSV when the current task should yield.
#[exception]
fn SysTick() {
    rprintln!("systick");
}
