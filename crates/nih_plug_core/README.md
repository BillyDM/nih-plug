# nih_plug_core

[![Documentation](https://docs.rs/nih_plug_core/badge.svg)](https://docs.rs/nih_plug_core)
[![Crates.io](https://img.shields.io/crates/v/nih_plug_core.svg)](https://crates.io/crates/nih_plug_core)
[![License](https://img.shields.io/crates/l/nih_plug_core.svg)](https://codeberg.org/BillyDM/nih-plug/src/branch/main/LICENSE)

Core types and traits for plugins made with the [NIH-plug](https://codeberg.org/BillyDM/nih-plug) plugin framework.

3rd party GUI libraries can use this to implement an adapter without needing to depend on the NIH-plug GitHub repository.

## Cargo features

* `assert_process_allocs` (default) - Enabling this feature will cause the plugin to terminate when allocations occur in the processing function during debug builds. Keep in mind that panics may also allocate if they use string formatting, so temporarily disabling this feature may be necessary when debugging panics in DSP code.
* `simd` - Add adapters to the Buffer object for reading the channel data to and from `std::simd` vectors. Requires a nightly compiler.
