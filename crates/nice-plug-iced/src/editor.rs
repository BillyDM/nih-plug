use crossbeam_utils::atomic::AtomicCell;
use iced_baseview::baseview::{WindowOpenOptions, WindowScalePolicy};
use iced_baseview::shell::window::IcedWindowHandle;
use iced_baseview::{IcedBaseviewSettings, PollSubNotifier, Program, message};
use nice_plug_core::context::gui::{GuiContext, ParamSetter};
use nice_plug_core::editor::dpi::LogicalSize;
use nice_plug_core::{
    editor::{Editor, ParentWindowHandle},
    params::persist::PersistentField,
};
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
    pub(crate) build: Arc<dyn Fn(EditorState<EState>, NiceGuiContext) -> P + 'static + Send + Sync>,
    pub(crate) notifier: PollSubNotifier,

    pub(crate) settings: Arc<EditorSettings>,
}

impl<P: Program + 'static, State: Send + 'static> Editor for IcedEditor<P, State> {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any> {
        let nice_ctx = NiceGuiContext {
            context: context.clone(),
            window_state: self.window_state.clone(),
        };

        let build = self.build.clone();
        let editor_state = EditorState::from_shared(&self.editor_state);
        let host_scale_factor = self.window_state.host_scale_factor();
        let logical_size = self.window_state.size();

        let scale_policy = host_scale_factor
            .map(|factor| WindowScalePolicy::ScaleFactor(factor as f64))
            .unwrap_or(WindowScalePolicy::SystemScaleFactor);

        let window = iced_baseview::open_parented(
            &parent,
            IcedBaseviewSettings {
                window: WindowOpenOptions::new()
                    .with_title(self.settings.window_title.clone())
                    .with_size(logical_size)
                    .with_scale_policy(scale_policy),
                ignore_non_modifier_keys: self.settings.ignore_non_modifier_keys,
                always_redraw: self.settings.always_redraw,
            },
            self.notifier.clone(),
            move || (build)(editor_state, nice_ctx),
        );

        self.window_state.open.store(true, Ordering::Release);

        Box::new(IcedEditorHandle {
            iced_state: self.window_state.clone(),
            _window: window,
        })
    }

    /// Size of the editor window
    fn size(&self) -> nice_plug_core::editor::dpi::Size {
        self.window_state.size().into()
    }

    fn set_scale_factor(&self, factor: f64) -> bool {
        // If the editor is currently open then the host must not change the current HiDPI scale as
        // we don't have a way to handle that. Ableton Live does this.
        if self.window_state.is_open() {
            return false;
        }

        self.window_state
            .host_scale_factor
            .store(Some(factor as f32));

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
    _window: IcedWindowHandle<Message>,
}

impl<Message: 'static + Send> Drop for IcedEditorHandle<Message> {
    fn drop(&mut self) {
        self.iced_state.open.store(false, Ordering::Release);
    }
}

/// State for an `nice-plug-iced` editor window.
#[derive(Debug, Serialize, Deserialize)]
pub struct WindowState {
    /// The window's size in logical pixels before applying `scale_factor`.
    #[serde(with = "nice_plug_core::params::persist::serialize_atomic_cell")]
    pub(crate) size: AtomicCell<LogicalSize<f32>>,

    #[serde(skip)]
    pub(crate) window_scale_factor: AtomicCell<f32>,
    #[serde(skip)]
    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    pub(crate) host_scale_factor: AtomicCell<Option<f32>>,

    /// Whether the editor's window is currently open.
    #[serde(skip)]
    pub(crate) open: AtomicBool,
}

impl<'a> PersistentField<'a, WindowState> for Arc<WindowState> {
    fn set(&self, new_value: WindowState) {
        self.size.store(new_value.size.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&WindowState) -> R,
    {
        f(self)
    }
}

impl WindowState {
    /// Initialize the GUI's state. This value can be passed to
    /// [`create_iced_editor()`](crate::create_iced_editor). The window size is in logical
    /// pixels, so before it is multiplied by the DPI scaling factor.
    pub fn from_size(size: LogicalSize<f32>) -> Arc<WindowState> {
        Arc::new(WindowState {
            size: AtomicCell::new(size),
            window_scale_factor: AtomicCell::new(1.0),
            host_scale_factor: AtomicCell::new(None),
            open: AtomicBool::new(false),
        })
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn size(&self) -> LogicalSize<f32> {
        self.size.load()
    }

    pub fn window_scale_factor(&self) -> f32 {
        self.window_scale_factor.load()
    }

    pub fn host_scale_factor(&self) -> Option<f32> {
        self.host_scale_factor.load()
    }

    /// Whether the GUI is currently visible.
    // Called `is_open()` instead of `open()` to avoid the ambiguity.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct NiceGuiContext {
    pub context: Arc<dyn GuiContext>,
    window_state: Arc<WindowState>,
}

impl NiceGuiContext {
    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn size(&self) -> LogicalSize<f32> {
        self.window_state.size()
    }

    /// Whether the GUI is currently visible.
    // Called `is_open()` instead of `open()` to avoid the ambiguity.
    pub fn is_open(&self) -> bool {
        self.window_state.is_open()
    }

    /// Set the new size that will be used to resize the window if the host allows.
    pub fn request_resize(&self, new_size: LogicalSize<f32>) {
        assert_ne!(new_size, LogicalSize::new(0.0, 0.0));

        let old_size = self.window_state.size();

        if new_size == old_size {
            return;
        }

        self.window_state.size.store(new_size);

        // Ask the plugin host to resize to self.size()
        if !self.context.request_resize() {
            self.window_state.size.store(old_size);
        }
    }

    pub fn param_setter<'a>(&'a self) -> ParamSetter<'a> {
        ParamSetter {
            raw_context: &*self.context,
        }
    }
}
