#![no_std]

use cortex_m_rt::exception;
use rtt_target::rprintln;

// Expose the version from Cargo.toml directly using the env! macro
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[exception]
fn SysTick() {
    rprintln!("systick");
}
