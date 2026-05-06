//! Traits for working with plugin editors.

use bitflags::bitflags;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::any::Any;
use std::ffi::c_void;
use std::sync::Arc;

use crate::context::gui::GuiContext;

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
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn Any + Send>;

    /// Returns the (current) size of the editor in pixels as a `(width, height)` pair. This size
    /// must be reported in _logical pixels_, i.e. the size before being multiplied by the DPI
    /// scaling factor to get the actual physical screen pixels.
    fn size(&self) -> (u32, u32);

    /// Set the DPI scaling factor, if supported. The plugin APIs don't make any guarantees on when
    /// this is called, but for now just assume it will be the first function that gets called
    /// before creating the editor. If this is set, then any windows created by this editor should
    /// have their sizes multiplied by this scaling factor on Windows and Linux.
    ///
    /// Right now this is never called on macOS since DPI scaling is built into the operating system
    /// there.
    fn set_scale_factor(&self, factor: f32) -> bool;

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

    // TODO: Reconsider adding a tick function here for the Linux `IRunLoop`. To keep this platform
    //       and API agnostic, add a way to ask the GuiContext if the wrapper already provides a
    //       tick function. If it does not, then the Editor implementation must handle this by
    //       itself. This would also need an associated `PREFERRED_FRAME_RATE` constant.
    // TODO: Host->Plugin resizing
}

/// A raw window handle for platform and GUI framework agnostic editors. This implements
/// [`HasRawWindowHandle`] so it can be used directly with GUI libraries that use the same
/// [`raw_window_handle`] version. If the library links against a different version of
/// `raw_window_handle`, then you'll need to wrap around this type and implement the trait yourself.
#[derive(Debug, Clone, Copy)]
pub enum ParentWindowHandle {
    /// The ID of the host's parent window. Used with X11.
    X11Window(u32),
    /// A handle to the host's parent window. Used only on macOS.
    AppKitNsView(*mut c_void),
    /// A handle to the host's parent window. Used only on Windows.
    Win32Hwnd(*mut c_void),
}

unsafe impl HasRawWindowHandle for ParentWindowHandle {
    fn raw_window_handle(&self) -> RawWindowHandle {
        match *self {
            ParentWindowHandle::X11Window(window) => {
                let mut handle = raw_window_handle::XcbWindowHandle::empty();
                handle.window = window;
                RawWindowHandle::Xcb(handle)
            }
            ParentWindowHandle::AppKitNsView(ns_view) => {
                let mut handle = raw_window_handle::AppKitWindowHandle::empty();
                handle.ns_view = ns_view;
                RawWindowHandle::AppKit(handle)
            }
            ParentWindowHandle::Win32Hwnd(hwnd) => {
                let mut handle = raw_window_handle::Win32WindowHandle::empty();
                handle.hwnd = hwnd;
                RawWindowHandle::Win32(handle)
            }
        }
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
