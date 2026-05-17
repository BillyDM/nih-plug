# nice-plug: bundler and other utilities

This is nice-plug's `cargo xtask` command, as a library. This way you can use it
in your own projects without having to either fork this repo or vendor the
binary into your own repo. This is necessary until Cargo supports [running
binaries from dependencies
directly](https://github.com/rust-lang/rfcs/pull/3168).

To use this, add an `xtask` binary to your project using `cargo new --bin xtask`. Then add that binary to the Cargo workspace in your repository's main
`Cargo.toml` file like so:

```toml
# Cargo.toml

[workspace]
members = ["xtask"]
```

Add `nice-plug-xtask` to the new xtask package's dependencies, and call its main
function from the new xtask binary:

```toml
# xtask/Cargo.toml

[dependencies]
nice-plug-xtask = { git = "https://codeberg.org/BillyDM/nice-plug.git" }
```

```rust
// xtask/src/main.rs

fn main() -> nice_plug_xtask::Result<()> {
    nice_plug_xtask::main()
}
```

Lastly, create a `.cargo/config` file in your repository and add a Cargo alias.
This allows you to run the binary using `cargo xtask`:

```toml
# .cargo/config

[alias]
xtask = "run --package xtask --release --"
```
