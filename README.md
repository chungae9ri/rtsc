Rust runtime scheduler for cortex-m

Note: This crate sets a default compilation target using `.cargo/config.toml`:

	[build]
	target = "thumbv8m.main-none-eabihf"

If you build this crate directly, ensure the target is installed:

```bash
rustup target add thumbv8m.main-none-eabihf
cargo build
```

When this crate is used as a dependency in a parent project, the parent's Cargo configuration (or workspace root) controls the build target, so this file will not force a target on the parent project.
