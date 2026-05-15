// SPDX-License-Identifier: MIT
// Copyright (c) 2026 kwangdo.yi

static mut SYS_CLK_FREQ: u32 = 0;

pub fn sys_clk_freq() -> u32 {
    unsafe { core::ptr::read_volatile(&raw const SYS_CLK_FREQ) }
}

pub fn update_sys_clk_freq(freq: u32) {
    cortex_m::interrupt::free(|_| unsafe {
        core::ptr::write_volatile(&raw mut SYS_CLK_FREQ, freq);
    });
}

pub fn ticks_per_ms() -> u32 {
    sys_clk_freq() / 1000
}
