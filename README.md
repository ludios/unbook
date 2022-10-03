This is a template for command-line Rust programs. Clone and run the `rename` script with both a `lowercase` and `UpperCase` name.

In the dev profile, dependencies (but not your own crate) are optimized. This is a good tradeoff because dependencies are recompiled far less frequently.

In the release profile, full [LTO](https://doc.rust-lang.org/cargo/reference/profiles.html#lto) is enabled.

To see your log messages, start your program with `RUST_LOG=trace` or see the [EnvFilter documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) for more filter syntax.

To further reduce the size of your release build, compile with Rust nightly and `RUSTFLAGS="-Z share-generics"` and `cargo build --release -Z build-std --target x86_64-unknown-linux-gnu`
