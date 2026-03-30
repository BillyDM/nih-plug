use crossbeam_utils::atomic::AtomicCell;
use iced_baseview::baseview::{Size, WindowOpenOptions, WindowScalePolicy};
use iced_baseview::{
    IcedBaseviewSettings, PollSubNotifier, Program, message, shell::window::WindowHandle,
};
use nih_plug::{
    editor::{Editor, ParentWindowHandle},
    params::persist::PersistentField,
    prelude::{GuiContext, ParamSetter},
};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use crate::{EditorSettings, application::EditorState};

pub(crate) struct IcedEditor<P: Program + 'static, EState: Send + 'static>
where
    <P as Program>::Message: message::MaybeDebug + message::MaybeClone,
{
    pub(crate) window_state: Arc<WindowState>,
    pub(crate) editor_state: Arc<Mutex<Option<EState>>>,

    /// The user's build function. Applied once at the start of the application.
    pub(crate) build: Arc<dyn Fn(EditorState<EState>, NihGuiContext) -> P + 'static + Send + Sync>,
    pub(crate) notifier: PollSubNotifier,

    pub(crate) settings: Arc<EditorSettings>,

    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    pub(crate) scaling_factor: AtomicCell<Option<f32>>,
}

impl<P: Program + 'static, State: Send + 'static> Editor for IcedEditor<P, State> {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any + Send> {
        let nih_ctx = NihGuiContext {
            context: context.clone(),
            window_state: self.window_state.clone(),
        };

        let build = self.build.clone();
        let editor_state = EditorState::from_shared(&self.editor_state);

        let (unscaled_width, unscaled_height) = self.window_state.logical_size();
        let scaling_factor = self.scaling_factor.load();

        let window = iced_baseview::open_parented(
            &ParentWindowHandleAdapter(parent),
            IcedBaseviewSettings {
                window: WindowOpenOptions {
                    title: String::from("iced window"),
                    // Baseview should be doing the DPI scaling for us
                    size: Size::new(unscaled_width as f64, unscaled_height as f64),
                    // NOTE: For some reason passing 1.0 here causes the UI to be scaled on macOS but
                    //       not the mouse events.
                    scale: scaling_factor
                        .map(|factor| WindowScalePolicy::ScaleFactor(factor as f64))
                        .unwrap_or(WindowScalePolicy::SystemScaleFactor),
                    #[cfg(feature = "rustanalyzer_opengl_workaround")]
                    gl_config: None,
                },
                ignore_non_modifier_keys: self.settings.ignore_non_modifier_keys,
                always_redraw: self.settings.always_redraw,
            },
            self.notifier.clone(),
            move || (build)(editor_state, nih_ctx),
        );

        self.window_state.open.store(true, Ordering::Release);

        Box::new(IcedEditorHandle {
            iced_state: self.window_state.clone(),
            _window: window,
        })
    }

    /// Size of the editor window
    fn size(&self) -> (u32, u32) {
        let new_size = self.window_state.requested_logical_size.load();
        // This method will be used to ask the host for new size.
        // If the editor is currently being resized and new size hasn't been consumed and set yet, return new requested size.
        if let Some(new_size) = new_size {
            new_size
        } else {
            self.window_state.logical_size()
        }
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        // If the editor is currently open then the host must not change the current HiDPI scale as
        // we don't have a way to handle that. Ableton Live does this.
        if self.window_state.is_open() {
            return false;
        }

        self.scaling_factor.store(Some(factor));
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        self.notifier.notify();
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        self.notifier.notify();
    }

    fn param_values_changed(&self) {
        self.notifier.notify();
    }
}

/// The window handle used for [`IcedEditor`].
struct IcedEditorHandle<Message: 'static + Send> {
    iced_state: Arc<WindowState>,
    _window: WindowHandle<Message>,
}

/// The window handle enum stored within 'WindowHandle' contains raw pointers. Is there a way around
/// having this requirement?
unsafe impl<Message: 'static + Send> Send for IcedEditorHandle<Message> {}

impl<Message: 'static + Send> Drop for IcedEditorHandle<Message> {
    fn drop(&mut self) {
        self.iced_state.open.store(false, Ordering::Release);
    }
}

/// State for an `nih_plug_iced` editor window.
#[derive(Debug, Serialize, Deserialize)]
pub struct WindowState {
    /// The window's size in logical pixels before applying `scale_factor`.
    #[serde(with = "nih_plug::params::persist::serialize_atomic_cell")]
    pub(crate) logical_size: AtomicCell<(u32, u32)>,

    /// The new size of the window, if it was requested to resize by the GUI.
    #[serde(skip)]
    pub(crate) requested_logical_size: AtomicCell<Option<(u32, u32)>>,

    /// Whether the editor's window is currently open.
    #[serde(skip)]
    pub(crate) open: AtomicBool,
}

impl<'a> PersistentField<'a, WindowState> for Arc<WindowState> {
    fn set(&self, new_value: WindowState) {
        self.logical_size.store(new_value.logical_size.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&WindowState) -> R,
    {
        f(self)
    }
}

impl WindowState {
    /// Initialize the GUI's state. This value can be passed to [`create_iced_editor()`]. The window
    /// size is in logical pixels, so before it is multiplied by the DPI scaling factor.
    pub fn from_logical_size(width: u32, height: u32) -> Arc<WindowState> {
        Arc::new(WindowState {
            logical_size: AtomicCell::new((width, height)),
            requested_logical_size: Default::default(),
            open: AtomicBool::new(false),
        })
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn logical_size(&self) -> (u32, u32) {
        self.logical_size.load()
    }

    /// Whether the GUI is currently visible.
    // Called `is_open()` instead of `open()` to avoid the ambiguity.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    /// Set the new size that will be used to resize the window if the host allows.
    pub fn set_requested_logical_size(&self, new_size: (u32, u32)) {
        self.requested_logical_size.store(Some(new_size));
    }
}

#[derive(Clone)]
pub struct NihGuiContext {
    pub context: Arc<dyn GuiContext>,
    window_state: Arc<WindowState>,
}

impl NihGuiContext {
    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn logical_size(&self) -> (u32, u32) {
        self.window_state.logical_size()
    }

    /// Whether the GUI is currently visible.
    // Called `is_open()` instead of `open()` to avoid the ambiguity.
    pub fn is_open(&self) -> bool {
        self.window_state.is_open()
    }

    /// Set the new size that will be used to resize the window if the host allows.
    pub fn set_requested_logical_size(&self, new_size: (u32, u32)) {
        self.window_state.set_requested_logical_size(new_size);

        // Ask the plugin host to resize to self.size()
        if self.context.request_resize() {
            self.window_state.logical_size.store(new_size);

            // TODO: Resize Iced content?
        }
    }

    pub fn param_setter<'a>(&'a self) -> ParamSetter<'a> {
        ParamSetter {
            raw_context: &*self.context,
        }
    }
}

/// This version of `baseview` uses a different version of `raw_window_handle than NIH-plug, so we
/// need to adapt it ourselves.
struct ParentWindowHandleAdapter(ParentWindowHandle);

unsafe impl HasRawWindowHandle for ParentWindowHandleAdapter {
    fn raw_window_handle(&self) -> RawWindowHandle {
        match self.0 {
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
