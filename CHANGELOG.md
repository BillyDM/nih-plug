# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic
Versioning](https://semver.org/spec/v2.0.0.html).

Since there is no stable release yet, the main purpose of this document in its current
state is to list breaking changes.

> Since nice-plug is now its own project, and the first release has been published
> to crates.io, this changelog has been reset. To see the old changelog, go to
> https://codeberg.org/RustAudio/nice-plug/src/commit/aefe2eac919aae5ad43f626d0fbd51748c7371ba/CHANGELOG.md

# nice-plug 0.3.0

## Breaking Changes
* All crates updated to use baseview version `0.3.1`
* `nice-plug-core`, `nice-plug`, and `nice-plug-iced` bumped to version 0.3
* `nice-plug-egui` bumped to version 0.5
* `nice-plug-egui` updated to use `egui` version `0.36`
* A new `editor` cargo feature was added to `nice-plug` and `nice-plug-core`. Disabling this will disable all GUI-related
code entirely, saving some processing overhead and reducing binary size.
* `Plugin::initialize()` and `InitContext` renamed to `Plugin::activate()` and `ActivateContext`
* A new `Editor` type was added to the `Plugin` trait. Plugins without an editor can set this to `()`.
* `Plugin::editor()` now returns `Option<Self::Editor>` instead of `Option<Box<dyn Editor>>`
* The `Editor` trait was completely overhauled to support better handling of window resizing and scaling. See
[editor.rs](https://codeberg.org/RustAudio/nice-plug/src/commit/1f496ddd098b5ce050f4617322ec4c8718b94a62/crates/nice-plug-core/src/editor.rs)
for more details.
* Added `resize_hint` field to `EguiNiceSettings`
* The build and update closures in `create_egui_editor` were replaced with a new `NiceEguiApp` trait
* `EguiState` was renamed to `EguiEditorState`
* Renamed `EditorState` in `nice-plug-iced` to `PersistentState`
* Renamed `EditorSettings` in `nice-plug-iced` to `IcedNiceSettings` and added `resize_hint` field
* `nice-plug-egui` no longer automatically repaints every frame. Use the new `RepaintNotifier` to notify the GUI when it
should repaint.

## Added
* Better integration for window resizing and scaling in CLAP, VST3, and standalone targets

## Changed
* Window size is no longer saved/loaded with the plugin's preset state

# nice-plug 0.2.3

## Fixed
* Actually invoke logging setup in CLAP and VST3 targets ([#66](https://codeberg.org/RustAudio/nice-plug/pulls/66))

# nice-plug 0.2.2

## Fixed
* Fixed compiling on Windows

## Changed
* There is no longer a compiler error when using the `assert_process_allocs` feature on the x86_64-pc-windows-gnu target.

# nice-plug 0.2.0

## Breaking Changes
* all crates updated to use baseview version `0.2`
* `nice-plug-core` and `nice-plug` bumped to version 0.2
* `nice-log` bumped to version 0.3
* `nice-plug-egui` bumped to version 0.4
* `nice-plug-iced` bumped to version 0.2
* `Editor::size()` now returns the `Size` type from the `dpi` crate
* `Editor::set_size()` now uses `PhysicalSize<u32>` from the `dpi` crate
* `ParentWindowHandle` enum has been reworked to be more in-line with `raw-window-handle` 0.6
* `create_egui_editor` has been updated for the new version of `egui-baseview`
* `EguiState` in `nice-plug-egui` has been overhauled (window resizing can now be done with the standard `ui.send_viewport_command()` method in egui.)
* The `Queue` struct in `nice-plug-egui` was renamed to `ExtraOutputCommands`
* Some methods removed in `ExtraOutputCommands` in favor of `ui.send_viewport_command()`.
* reworked methods in `NiceGuiContext` from `nice-plug-iced`
* added `window_title` field to `EditorSettings` in `nice-plug-iced`
* `WrapperConfig::dpi_scale` in the standalone target now uses `f64` instead of `f32`
* `nice_log` was overhauled to support the `tracing` crate

## Added
* `Editor::set_scale_factor()` (called when the host requests to set the scaling factor)
* `Plugin::setup_logger()` (can be used to set up logging filters, called when the program first initializes)
* Added a new `tracing-subscriber` feature to `nice-plug` that when enabled, automatically sets up a default logger using `tracing- subscriber`.

## Changed
* `Editor::spawn()` now returns `Box<dyn Any>` instead of `Box<dyn Any + Send>`
* nice-plug now uses `tracing` and `tracing-subscriber` instead of `log`

# nice-plug 0.1.9

* Fixed "unsafe" compiler warning when using the `nice_export_vst3!` macro.

# nice-plug 0.1.8

* use `objc2` dependency in place of the old unmaintained `objc` crate
* re-exported `AtomicF32` and `AtomicF64` in the `util` module

# nice-plug 0.1.7

* bumped `core-foundation` dependency to latest version

# nice-plug 0.1.6

* updated the standalone backend to use `cpal` 0.18

# nice-plug 0.1.5

* improved non-linux/macos unix support
* updated windows dependencies

# nice-plug 0.1.3

nice-plug has been published to crates.io! 🎉

(versions 0.1.0, 0.1.1, and 0.1.2 are identical to 0.1.3, the only changes were fixing
issues with the readme)
