# iced_baseview

[![License](https://img.shields.io/crates/l/iced_baseview.svg)](https://github.com/BillyDM/nih-plug/blob/main/crates/iced-baseview/LICENSE-MIT)

A [`baseview`](https://github.com/RustAudio/baseview) backend for [`Iced`](https://github.com/hecrj/iced)

<div align="center">
    <img src="screenshot.png">
</div>

## How to use with your own custom plugin framework

Add the following to your `Cargo.toml`:

```toml
iced-baseview = { git = "https://github.com/BillyDM/nih-plug", branch = "main" }
```

or if you want to use a specific revision:

```toml
iced-baseview = { git = "https://github.com/BillyDM/nih-plug", rev = "8225a2d94a7a27a212c64aa814467801faa9585c" }
```

## Prerequisites

### Linux

Install dependencies, e.g.,

```sh
sudo apt-get install libx11-dev libxcursor-dev libxcb-dri2-0-dev libxcb-icccm4-dev libx11-xcb-dev mesa-common-dev libgl1-mesa-dev libglu1-mesa-dev
```
