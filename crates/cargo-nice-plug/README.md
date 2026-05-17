# nice-plug: cargo subcommand for bundling plugins

This is nice-plug's `cargo xtask` command, but as a `cargo` subcommand. This way
you can use it outside of nice-plug projects. If you're using nice-plug, you'll
want to use the xtask integration directly instead so you don't need to worry
about keeping the command up to date, see:
<https://codeberg.org/BillyDM/nice-plug/src/branch/main/crates/nice-plug-xtask>.

Since this has not yet been published to `crates.io`, you'll need to install
this using:

```shell
cargo install --git https://codeberg.org/BillyDM/nice-plug.git cargo-nice-plug
```
I m
Once that's installed, you can compile and bundle plugins using:

```shell
cargo nice-plug bundle <package_name> --release
```
