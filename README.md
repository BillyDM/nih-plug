# NIH-plug

> This is a hard fork of https://github.com/robbert-vdh/nih-plug, since the
> original author is no longer maintaining it.
>
> This fork does NOT contain the original collection of plugins. If you are looking
> for those, go to the original repository linked above. Please do NOT post any
> issues about the original plugins here, this is for the development framework only!
>
> This repository has moved to Codeberg https://codeberg.org/BillyDM/nih-plug. Please
> only post issues and pull requests there. A read-only mirror is available at
> https://github.com/BillyDM/nih-plug for users who prefer that.

NIH-plug is an API-agnostic audio plugin framework written in Rust.

The idea is to have a stateful yet simple plugin API that gets rid of as much
unnecessary ceremony wherever possible, while also keeping the amount of magic to
a minimum and making it easy to experiment with different approaches to things. See
the [current features](#current-features) section for more information on the
project's current status.

<!--- TODO
Check out the [documentation](https://nih-plug.robbertvanderhelm.nl/), or use
the [cookiecutter template](https://github.com/robbert-vdh/nih-plug-template) to
quickly get started with NIH-plug.
-->

### Table of contents

- [Baseview adapters](#baseview-adapters)
- [Framework](#framework)
  - [Current features](#current-features)
  - [Building](#building)
  - [Plugin formats](#plugin-formats)
  - [Example plugins](#example-plugins)
- [Licensing](#licensing)

## Baseview adapters

This repository contains [baseview](https://github.com/RustAudio/baseview) adapters
for popular Rust GUI frameworks. These can be used on their own without the rest of
the NIH-plug framework.

- [egui-baseview](baseview-adapters/egui-baseview/) - adapter for
[egui](https://github.com/emilk/egui)
- [iced_baseview](baseview-adapters/iced-baseview/) - adapter for
[Iced](https://iced.rs/)

## Framework

For a list of available crate flags, see
[crates/nih_plug/Cargo.toml](crates/nih_plug/Cargo.toml).

### Current features

- Supports both VST3 and [CLAP](https://github.com/free-audio/clap) by simply
  adding the corresponding `nih_export_<api>!(Foo)` macro to your plugin's
  library.
- Standalone binaries can be made by calling `nih_export_standalone(Foo)` from
  your `main()` function. Standalones come with a CLI for configuration and full
  JACK audio, MIDI, and transport support.
- Rich declarative parameter system without any boilerplate.
  - Define parameters for your plugin by adding `FloatParam`, `IntParam`,
    `BoolParam`, and `EnumParam<T>` fields to your parameter struct, assign
    stable IDs to them with the `#[id = "foobar"]`, and a `#[derive(Params)]`
    does all of the boring work for you.
  - Parameters can have complex value distributions and the parameter objects
    come with built-in smoothers and callbacks.
  - Use simple enums deriving the `Enum` trait with the `EnumParam<T>` parameter
    type for parameters that allow the user to choose between multiple discrete
    options. That way you can use regular Rust pattern matching when working
    with these values without having to do any conversions yourself.
  - Store additional non-parameter state for your plugin by adding any field
    that can be serialized with [Serde](https://serde.rs/) to your plugin's
    `Params` object and annotating them with `#[persist = "key"]`.
  - Optional support for state migrations, for handling breaking changes in
    plugin parameters.
  - Group your parameters into logical groups by nesting `Params` objects using
    the `#[nested(group = "...")]`attribute.
  - The `#[nested]` attribute also enables you to use multiple copies of the
    same parameter, either as regular object fields or through arrays.
  - When needed, you can also provide your own implementation for the `Params`
    trait to enable compile time generated parameters and other bespoke
    functionality.
- Stateful. Behaves mostly like JUCE, just without all of the boilerplate.
- Comes with a simple yet powerful way to asynchronously run background tasks
  from a plugin that's both type-safe and realtime-safe.
- Does not make any assumptions on how you want to process audio, but does come
  with utilities and adapters to help with common access patterns.
  - Efficiently iterate over an audio buffer either per-sample per-channel,
    per-block per-channel, or even per-block per-sample-per-channel with the
    option to manually index the buffer or get access to a channel slice at any
    time.
  - Easily leverage per-channel SIMD using the SIMD adapters on the buffer and
    block iterators.
  - Comes with bring-your-own-FFT adapters for common (inverse) short-time
    Fourier Transform operations. More to come.
- Optional sample accurate automation support for VST3 and CLAP that can be
  enabled by setting the `Plugin::SAMPLE_ACCURATE_AUTOMATION` constant to
  `true`.
- Optional support for compressing the human readable JSON state files using
  [Zstandard](https://en.wikipedia.org/wiki/Zstd).
- Comes with adapters for popular Rust GUI frameworks as well as some basic
  widgets for them that integrate with NIH-plug's parameter system:
    - [nih_plug_egui](crates/nih_plug_egui) - Adapter for [egui](https://github.com/emilk/egui).
    See the [egui-baseview](baseview-adapters/egui-baseview/) crate for prerequisites.
    - [nih_plug_iced](crates/nih_plug_iced) - Adapter for [Iced](https://iced.rs/).
    See the [iced_baseview](baseview-adapters/iced-baseview/) crate for prerequisites.
- Full support for receiving and outputting both modern polyphonic note
  expression events as well as MIDI CCs, channel pressure, and pitch bend for
  CLAP and VST3.
  - MIDI SysEx is also supported. Plugins can define their own structs or sum
    types to wrap around those messages so they don't need to interact with raw
    byte buffers in the process function.
- Support for flexible dynamic buffer configurations, including variable numbers
  of input and output ports.
- First-class support several more exotic CLAP features:
  - Both monophonic and polyphonic parameter modulation are supported.
  - Plugins can declaratively define pages of remote controls that DAWs can bind
    to hardware controllers.
- A plugin bundler accessible through the
  `cargo xtask bundle <package> <build_arguments>` command that automatically
  detects which plugin targets your plugin exposes and creates the correct
  plugin bundles for your target operating system and architecture, with
  cross-compilation support. The cargo subcommand can easily be added to [your
  own project](https://github.com/robbert-vdh/nih-plug/tree/main/nih_plug_xtask)
  as an alias or [globally](https://github.com/robbert-vdh/nih-plug/tree/main/cargo_nih_plug)
  as a regular cargo subcommand.
- Tested on Linux and Windows, with limited testing on macOS. Windows support
  has mostly been tested through Wine with
  [yabridge](https://github.com/robbert-vdh/yabridge).
- See the [`Plugin`](src/plugin.rs) trait's documentation for an incomplete list
  of the functionality that has currently not yet been implemented.

### Building

NIH-plug works with the latest stable Rust compiler.

After installing [Rust](https://rustup.rs/), you can compile any of the plugins
in the `plugins` directory in the following way, replacing `gain` with the name
of the plugin:

```shell
cargo xtask bundle gain --release
```

### Plugin formats

NIH-plug can currently export VST3 and
[CLAP](https://github.com/free-audio/clap) plugins. Exporting a specific plugin
format for a plugin is as simple as calling the `nih_export_<format>!(Foo);`
macro. The `cargo xtask bundle` command will detect which plugin formats your
plugin supports and create the appropriate bundles accordingly, even when cross
compiling.

### Example plugins

The best way to get an idea for what the API looks like is to look at the
examples.

- [**gain**](examples/gain) is a simple smoothed gain plugin that shows
  off a couple other parts of the API, like support for storing arbitrary
  serializable state.
- **gain_\<gui\>** are the same plugins as gain, but with a GUI to control the
  parameter and a digital peak meter.
    - [**gain_egui**](examples/gain_egui) - See the
    [egui-baseview](baseview-adapters/egui-baseview/) crate for prerequisites.
    - [**gain_iced**](examples/gain_iced) - See the
    [iced_baseview](baseview-adapters/iced-baseview/) crate for prerequisites.
- Examples for adding your own custom GUI framework on top of raw rendering APIs:
  - [**byo_gui_gl**](examples/byo_gui_gl) - for rendering with OpenGL
  - [**byo_gui_wgpu**](examples/byo_gui_wgpu) - for rendering with [wgpu](wgpu.rs)
  - [**byo_gui_softbuffer**](examples/byo_gui_softbuffer) - for rendering with
  [softbuffer](https://github.com/rust-windowing/softbuffer) (software rendering)
- [**midi_inverter**](examples/midi_inverter) takes note/MIDI events and
  flips around the note, channel, expression, pressure, and CC values. This
  example demonstrates how to receive and output those events.
- [**poly_mod_synth**](examples/poly_mod_synth) is a simple polyphonic
  synthesizer with support for polyphonic modulation in supported CLAP hosts.
  This demonstrates how polyphonic modulation can be used in NIH-plug.
- [**sine**](examples/sine) is a simple test tone generator plugin with
  frequency smoothing that can also make use of MIDI input instead of generating
  a static signal based on the plugin's parameters.
- [**stft**](examples/stft) shows off some of NIH-plug's other optional
  higher level helper features, such as an adapter to process audio with a
  short-term Fourier transform using the overlap-add method, all using the
  compositional `Buffer` interfaces.
- [**sysex**](examples/sysex) is a simple example of how to send and
  receive SysEx messages by defining custom message types.

## Contributing

Please refer to the Rust Audio community's policy an AI usage:
https://rust.audio/community/ai/

Nih-plug uses optional nightly features. To make rust analyzer happy, you can
enable the nightly compiler for your local repository with:
```
rustup override add nightly
```

## Licensing

> Check each crate's Cargo.toml file for more information.

The framework, all of the crates in `crates/`, and the example plugins in
`examples/` are all licensed under the [ISC license](https://www.isc.org/licenses/).

All of the crates in `baseview-adapters/` are licensed under "MIT or Apache-2.0". 

However, the [VST3 bindings](https://github.com/RustAudio/vst3-sys) used by
`nih_export_vst3!()` are licensed under the GPLv3 license. This means that
unless you replace these bindings with your own bindings made from scratch, any
VST3 plugins built with NIH-plug need to be able to comply with the terms of the
GPLv3 license.
