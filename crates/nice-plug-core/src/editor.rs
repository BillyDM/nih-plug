//! Traits for working with plugin editors.

use bitflags::bitflags;
use dpi::{PhysicalSize, Size};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::any::Any;
use std::ffi::{c_ulong, c_void};
use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use std::sync::Arc;

use crate::context::gui::GuiContext;

pub use dpi;

/// An editor for a [`Plugin`][crate::plugin::Plugin].
pub trait Editor: Send {
    /// Create an instance of the plugin's editor and embed it in the parent window. As explained in
    /// [`Plugin::editor()`][crate::plugin::Plugin::editor()], you can then read the parameter
    /// values directly from your [`Params`][crate::params::Params] object, and modifying the
    /// values can be done using the functions on the [`ParamSetter`][crate::context::gui::ParamSetter].
    /// When you change a parameter value that way it will be broadcasted to the host and also
    /// updated in your [`Params`][crate::params::Params] struct.
    ///
    /// This function should return a handle to the editor, which will be dropped when the editor
    /// gets closed. Implement the [`Drop`] trait on the returned handle if you need to explicitly
    /// handle the editor's closing behavior.
    ///
    /// If [`set_scale_factor()`][Self::set_scale_factor()] has been called, then any created
    /// windows should have their sizes multiplied by that factor.
    ///
    /// The wrapper guarantees that a previous handle has been dropped before this function is
    /// called again.
    //
    // TODO: Think of how this would work with the event loop. On Linux the wrapper must provide a
    //       timer using VST3's `IRunLoop` interface, but on Window and macOS the window would
    //       normally register its own timer. Right now we just ignore this because it would
    //       otherwise be basically impossible to have this still be GUI-framework agnostic. Any
    //       callback that deos involve actual GUI operations will still be spooled to the IRunLoop
    //       instance.
    // TODO: This function should return an `Option` instead. Right now window opening failures are
    //       always fatal. This would need to be fixed in baseview first.
    fn spawn(&self, parent: ParentWindowHandle, context: Arc<dyn GuiContext>) -> Box<dyn Any>;

    /// Returns the (current) size of the editor.
    fn size(&self) -> Size;

    /// Called whenever a specific parameter's value has changed while the editor is open. You don't
    /// need to do anything with this, but this can be used to force a redraw when the host sends a
    /// new value for a parameter or when a parameter change sent to the host gets processed.
    fn param_value_changed(&self, id: &str, normalized_value: f32);

    /// Called whenever a specific parameter's monophonic modulation value has changed while the
    /// editor is open.
    fn param_modulation_changed(&self, id: &str, modulation_offset: f32);

    /// Called whenever one or more parameter values or modulations have changed while the editor is
    /// open. This may be called in place of [`param_value_changed()`][Self::param_value_changed()]
    /// when multiple parameter values hcange at the same time. For example, when a preset is
    /// loaded.
    fn param_values_changed(&self);

    /// Called when the host delivers a virtual-key event to the plugin's
    /// view. Return `true` if the editor consumed the key (the wrapper
    /// will tell the host to skip its own accelerator handling); return
    /// `false` to let the host process the key normally.
    ///
    /// The wrapper only invokes this for non-character "virtual" keys
    /// ([`VirtualKeyCode::Backspace`], the arrow keys, function keys,
    /// modifier presses, etc.). Plain printable characters arrive through
    /// the plugin window's native keyboard path (on macOS, AppKit
    /// `keyDown:` + NSTextInputContext) and are not routed here; consuming
    /// them through this hook would double-dispatch text input.
    ///
    /// Both key-down and key-up events are delivered; `is_down` is
    /// `true` for press, `false` for release. Plug-ins that consume a
    /// key on press should generally also return `true` for the
    /// matching release so the host doesn't pick up the release as a
    /// separate accelerator.
    ///
    /// This is primarily for text-input routing in hosts (notably
    /// REAPER) that intercept certain keys (Space, Backspace, arrows,
    /// Cmd-shortcuts) before they reach the plugin's native view. The
    /// editor should only return `true` if a text input in the editor
    /// currently has focus and can consume the key.
    ///
    /// # Parameters
    ///
    /// - `key_code`: the virtual key the host reports.
    /// - `is_down`: `true` for key-down, `false` for key-up.
    /// - `modifiers`: which modifier keys were held when the event was
    ///   generated.
    fn on_virtual_key_from_host(
        &self,
        _key_code: VirtualKeyCode,
        _is_down: bool,
        _modifiers: Modifiers,
    ) -> bool {
        false
    }

    /// Called by the wrapper when the host has resized the plugin's view (either
    /// because the host accepted an earlier [`GuiContext::request_resize()`], or
    /// because the user dragged a host-provided resize handle). The editor should
    /// resize its own window and contents to match these dimensions.
    ///
    /// Return `true` if the editor applied the new size, `false` if it rejected
    /// it (e.g. the size is outside what the GUI supports). The default
    /// implementation is a no-op that returns `false`, so editors that don't
    /// support being resized by the host keep their previous fixed-size
    /// behavior without any changes.
    ///
    /// This is the counterpart to [`size()`][Self::size()]: after a successful
    /// `set_size`, `size()` should report the new dimensions.
    fn set_size(&self, physical_size: PhysicalSize<u32>) -> bool {
        let _ = physical_size;
        false
    }

    /// Set the DPI scaling factor, if supported. The plugin APIs don't make any guarantees on when
    /// this is called, but for now just assume it will be the first function that gets called
    /// before creating the editor. If this is set, then any windows created by this editor should
    /// have their sizes multiplied by this scaling factor on Windows and Linux.
    ///
    /// Right now this is never called on macOS since DPI scaling is built into the operating system
    /// there.
    fn set_scale_factor(&self, factor: f64) -> bool;

    /// Describes whether and how the host may resize this editor. The wrapper
    /// reads this to answer the host's resize-capability queries (CLAP's
    /// `gui.can_resize` / `gui.get_resize_hints`, VST3's `canResize`).
    ///
    /// The default is [`ResizeHint::default()`], which is **not** resizable, so
    /// editors keep their fixed-size behavior unless they opt in. An editor that
    /// supports host resizing should return a hint with `can_resize: true` (and
    /// usually also implement [`set_size()`][Self::set_size()] to apply the new
    /// size). See [`ResizeHint`] for the per-axis and aspect-ratio options.
    fn resize_hint(&self) -> ResizeHint {
        ResizeHint::default()
    }

    // TODO: Reconsider adding a tick function here for the Linux `IRunLoop`. To keep this platform
    //       and API agnostic, add a way to ask the GuiContext if the wrapper already provides a
    //       tick function. If it does not, then the Editor implementation must handle this by
    //       itself. This would also need an associated `PREFERRED_FRAME_RATE` constant.
}

/// Describes whether and how a host may resize an [`Editor`], returned from
/// [`Editor::resize_hint()`].
///
/// The default is non-resizable (`can_resize: false`), matching the previous
/// fixed-size behavior. To make an editor resizable, return a hint with
/// `can_resize: true`; the per-axis flags and aspect-ratio fields refine how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeHint {
    /// Whether the host may resize the editor at all. Drives CLAP's
    /// `gui.can_resize` and VST3's `canResize`. When `false`, the other fields
    /// are ignored.
    pub can_resize: bool,
    /// Whether the width may change. Only meaningful when `can_resize` is `true`.
    pub can_resize_horizontally: bool,
    /// Whether the height may change. Only meaningful when `can_resize` is `true`.
    pub can_resize_vertically: bool,
    /// If `true`, the host should keep the editor's aspect ratio fixed at
    /// `aspect_ratio_width : aspect_ratio_height` while resizing.
    pub preserve_aspect_ratio: bool,
    /// Aspect-ratio numerator (only used when `preserve_aspect_ratio` is `true`).
    pub aspect_ratio_width: u32,
    /// Aspect-ratio denominator (only used when `preserve_aspect_ratio` is `true`).
    pub aspect_ratio_height: u32,
}

impl Default for ResizeHint {
    fn default() -> Self {
        // Not resizable by default, so editors keep their fixed-size behavior
        // unless they explicitly opt in.
        Self {
            can_resize: false,
            can_resize_horizontally: true,
            can_resize_vertically: true,
            preserve_aspect_ratio: false,
            aspect_ratio_width: 1,
            aspect_ratio_height: 1,
        }
    }
}

impl ResizeHint {
    /// A freely resizable editor: both axes, no aspect-ratio lock. Convenience
    /// for the common case.
    pub fn resizable() -> Self {
        Self {
            can_resize: true,
            ..Self::default()
        }
    }
}

/// A raw window handle for platform and GUI framework agnostic editors. This implements
/// [`HasRawWindowHandle`] so it can be used directly with GUI libraries that use the same
/// [`raw_window_handle`] version. If the library links against a different version of
/// `raw_window_handle`, then you'll need to wrap around this type and implement the trait yourself.
#[derive(Debug, Clone, Copy)]
pub enum ParentWindowHandle {
    /// The ID of the host's parent window. Used with X11.
    XlibWindow(c_ulong),
    /// The ID of the host's parent window. Used with X11.
    XcbWindow(NonZeroU32),
    /// A handle to the host's parent window. Used only on macOS.
    AppKitNsView(NonNull<c_void>),
    /// A handle to the host's parent window. Used only on Windows.
    Win32Hwnd(NonZeroIsize),
}

impl HasWindowHandle for ParentWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let raw = match *self {
            ParentWindowHandle::XlibWindow(window) => {
                RawWindowHandle::Xlib(raw_window_handle::XlibWindowHandle::new(window))
            }
            ParentWindowHandle::XcbWindow(window) => {
                RawWindowHandle::Xcb(raw_window_handle::XcbWindowHandle::new(window))
            }
            ParentWindowHandle::AppKitNsView(ns_view) => {
                RawWindowHandle::AppKit(raw_window_handle::AppKitWindowHandle::new(ns_view))
            }
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(hwnd))
            }
        };

        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(raw) })
    }
}

/// A non-character key delivered to
/// [`Editor::on_virtual_key_from_host`]. Variant names mirror standard
/// keyboard nomenclature; printable ASCII characters never appear here
/// because they flow through the plugin window's native keyboard path
/// instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VirtualKeyCode {
    Backspace,
    Tab,
    Clear,
    Return,
    Pause,
    Escape,
    Space,
    Next,
    End,
    Home,
    ArrowLeft,
    ArrowUp,
    ArrowRight,
    ArrowDown,
    PageUp,
    PageDown,
    Select,
    Print,
    /// Numpad enter (distinct from [`VirtualKeyCode::Return`]).
    NumpadEnter,
    Snapshot,
    Insert,
    Delete,
    Help,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadMultiply,
    NumpadAdd,
    NumpadSeparator,
    NumpadSubtract,
    NumpadDecimal,
    NumpadDivide,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    NumLock,
    ScrollLock,
    /// Shift key, delivered as a press/release on the modifier itself.
    /// For most text-input purposes you want
    /// [`Modifiers::SHIFT`] on the event's modifier set instead; the
    /// dedicated press is useful only for editors that react to
    /// modifier-only gestures.
    Shift,
    /// Control key (macOS Ctrl, platform-Ctrl elsewhere). See the note
    /// on [`VirtualKeyCode::Shift`].
    Control,
    /// Alt / Option key. See the note on [`VirtualKeyCode::Shift`].
    Alt,
    Equals,
    ContextMenu,
    MediaPlay,
    MediaStop,
    MediaPrevTrack,
    MediaNextTrack,
    VolumeUp,
    VolumeDown,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    /// Super / Command / Windows key. See the note on
    /// [`VirtualKeyCode::Shift`].
    Super,
}

bitflags! {
    /// Modifier keys held while a keyboard event was generated, as
    /// reported by [`Editor::on_virtual_key_from_host`]. Use the
    /// standard `bitflags` API (`contains`, `intersects`, `is_empty`,
    /// etc.) to query individual modifiers.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct Modifiers: u8 {
        /// Shift key.
        const SHIFT = 1 << 0;
        /// Alt / Option key.
        const ALT = 1 << 1;
        /// Command key. On Windows / Linux this is typically the Ctrl
        /// key. See [`Modifiers::CONTROL`] for the macOS Control key
        /// specifically.
        const COMMAND = 1 << 2;
        /// Control key (macOS Ctrl, distinct from
        /// [`Modifiers::COMMAND`]).
        const CONTROL = 1 << 3;
    }
}
