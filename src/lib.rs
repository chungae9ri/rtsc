// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

//! Core task and exception primitives for the runtime scheduler.
//!
//! This module defines the scheduler-visible task state and the exception
//! entry points commonly used to drive preemptive scheduling on Cortex-M.

#![no_std]

use cortex_m_rt::exception;
use rtt_target::rprintln;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[exception]
fn SysTick() {
    rprintln!("systick");
}
