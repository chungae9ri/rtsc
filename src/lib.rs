#![no_std]

// Expose the version from Cargo.toml directly using the env! macro
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
