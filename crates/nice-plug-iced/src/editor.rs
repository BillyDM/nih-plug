use crossbeam_utils::atomic::AtomicCell;
use iced_baseview::baseview::HandlerError;
use iced_baseview::shell::SharedWindowSize;
use iced_baseview::{IcedWindowSettings, PollSubNotifier, Program, baseview, message};
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::editor::dpi::{PhysicalSize, Size};
use nice_plug_core::editor::{
    Editor, EditorHandle, EditorWindow, Modifiers, ParentWindowHandle, ResizeHint, VirtualKeyCode,
};
use nice_plug_core::nice_error;
use std::error::Error;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use crate::{IcedNiceSettings, application::PersistentState};

pub(crate) struct IcedEditorInner<P: Program + 'static, PState: Send + 'static>
where
    <P as Program>::Message: message::MaybeDebug + message::MaybeClone,
{
    pub(crate) editor_state: Arc<IcedEditorState>,
    pub(crate) persistent_state: Arc<Mutex<Option<PState>>>,

    /// The user's build function. Applied once at the start of the application.
    pub(crate) build: Arc<
        dyn Fn(PersistentState<PState>, IcedNiceContext) -> Result<P, HandlerError>
            + 'static
            + Send
            + Sync,
    >,
    pub(crate) notifier: PollSubNotifier,

    pub(crate) settings: Arc<IcedNiceSettings>,
}

impl<P: Program + 'static, State: Send + 'static> Editor for IcedEditorInner<P, State> {
    type Handle = IcedEditorHandle;

    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        wait_for_parent: bool,
        suggested_scale_factor: Option<f64>,
        gui_context: GuiContext,
        host: Option<Box<dyn nice_plug_core::editor::HostCallbacks>>,
    ) -> Result<EditorWindow<Self::Handle>, Box<dyn Error>> {
        let build = self.build.clone();
        let persistent_state = PersistentState::from_shared(&self.persistent_state);
        let size = self.editor_state.size.load();
        let zoom_factor = self.editor_state.zoom_factor.load();
        let nice_context = gui_context.clone();
        let editor_state = self.editor_state.clone();

        let settings = IcedWindowSettings::new()
            .with_title(self.settings.title.clone())
            .with_size(size)
            .with_scale_factor(zoom_factor)
            .with_parent(parent.as_ref())
            .with_wait_for_parent(wait_for_parent)
            .with_fallback_scale_factor(suggested_scale_factor)
            .with_ignore_non_modifier_keys(self.settings.ignore_non_modifier_keys)
            .with_always_redraw(self.settings.always_redraw);

        // The host is a re-implementation of
        // [`baseview::host::HostCallbacks`](https://docs.rs/baseview/latest/baseview/host/trait.HostCallbacks.html)
        // to avoid `nice-plug-core` from depending on `baseview` until it is stabilized.
        //
        // Create a small wrapper to adapt it to baseview's HostCallbacks trait.
        struct HostAdapter {
            host: Box<dyn nice_plug_core::editor::HostCallbacks>,
        }
        impl baseview::host::HostCallbacks for HostAdapter {
            fn request_resize(
                &mut self,
                new_size: baseview::WindowSize,
            ) -> Result<(), HandlerError> {
                self.host
                    .request_resize(new_size.physical.into(), new_size.scale_factor)
                    .map_err(HandlerError::from_boxed)
            }

            fn destroyed(&mut self) {
                self.host.destroyed();
            }
        }
        let host =
            host.map(|host| baseview::host::Host::new().with_callbacks(HostAdapter { host }));

        let (window, _message_sender) = iced_baseview::create_window(
            settings,
            self.notifier.clone(),
            move |window_size| {
                let nice_ctx = IcedNiceContext {
                    nice_context,
                    window_size,
                    editor_state,
                };

                (build)(persistent_state, nice_ctx)
            },
            host,
        )?;

        self.editor_state.open.store(true, Ordering::Release);

        Ok(EditorWindow {
            handle: IcedEditorHandle {
                editor_state: Arc::clone(&self.editor_state),
                notifier: self.notifier.clone(),
                resize_hint: self.settings.resize_hint,
            },
            window,
        })
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.editor_state.physical_size()
    }

    fn resize_hint(&self) -> nice_plug_core::editor::ResizeHint {
        self.settings.resize_hint
    }
}

/// The window handle used for [`IcedEditor`](crate::IcedEditor).
pub struct IcedEditorHandle {
    editor_state: Arc<IcedEditorState>,
    notifier: PollSubNotifier,
    resize_hint: ResizeHint,
}

impl EditorHandle for IcedEditorHandle {
    type Window = iced_baseview::baseview::Window;
    type Error = iced_baseview::baseview::Error;

    fn run_until_closed(window: Self::Window) -> Result<(), Self::Error> {
        window.run_until_closed()
    }

    fn set_parent(
        &self,
        parent: ParentWindowHandle,
        window: &Self::Window,
    ) -> Result<(), Self::Error> {
        window.set_parent(&parent)
    }

    fn show(&self, window: &Self::Window) -> Result<(), Self::Error> {
        window.show()
    }

    fn hide(&self, window: &Self::Window) -> Result<(), Self::Error> {
        window.hide()
    }

    fn set_size(&self, new_size: PhysicalSize<u32>, window: &Self::Window) -> bool {
        let current_size = window.size();
        if !self.resize_hint.is_size_valid(
            new_size,
            current_size.physical,
            current_size.scale_factor,
        ) {
            return false;
        }

        if let Err(e) = window.resize(new_size) {
            nice_error!("Failed to resize window to {:?}: {}", new_size, e);
            false
        } else {
            true
        }
    }

    fn set_suggested_scale_factor(
        &self,
        scale_factor: f64,
        window: &Self::Window,
    ) -> Result<(), Self::Error> {
        window.suggest_fallback_scale_factor(scale_factor)
    }

    /// Return the closest supported size.
    fn adjust_size(
        &self,
        new_size: PhysicalSize<u32>,
        window: &Self::Window,
    ) -> Option<PhysicalSize<u32>> {
        let current_size = window.size();
        Some(self.resize_hint.adjust_size(
            new_size,
            current_size.physical,
            current_size.scale_factor,
        ))
    }

    fn on_virtual_key_from_host(
        &self,
        _key_code: VirtualKeyCode,
        _is_down: bool,
        _modifiers: Modifiers,
    ) -> bool {
        // TODO
        false
    }

    fn state_changed(&self) {
        self.notifier.notify();
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        self.notifier.notify();
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        self.notifier.notify();
    }
}

impl Drop for IcedEditorHandle {
    fn drop(&mut self) {
        self.editor_state.open.store(false, Ordering::Release);
    }
}

/// State for an `nice-plug-iced` editor window.
#[derive(Debug)]
pub struct IcedEditorState {
    size: AtomicCell<Size>,

    pub(crate) zoom_factor: AtomicCell<f32>,

    pub(crate) host_scale_factor: AtomicCell<Option<f32>>,

    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    pub(crate) system_scale_factor: AtomicCell<f64>,

    /// Whether the editor's window is currently open.
    open: AtomicBool,
}

impl IcedEditorState {
    /// Create a new state for iced's editor.
    pub fn from_size(size: impl Into<Size>, scale_factor: f32) -> Arc<Self> {
        assert!(scale_factor > 0.0);

        Arc::new(Self {
            size: AtomicCell::new(size.into()),
            zoom_factor: AtomicCell::new(scale_factor),
            open: AtomicBool::new(false),
            host_scale_factor: AtomicCell::new(None),
            system_scale_factor: AtomicCell::new(1.0),
        })
    }

    pub fn size(&self) -> Size {
        self.size.load()
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        let size = self.size.load();

        match size {
            Size::Logical(logical_size) => {
                let zoom_factor = self.zoom_factor.load();
                let host_scale_factor = self.host_scale_factor.load();
                let system_scale_factor = self.system_scale_factor.load();

                let scale_factor = zoom_factor as f64
                    * host_scale_factor
                        .map(|s| s as f64)
                        .unwrap_or(system_scale_factor);

                logical_size.to_physical(scale_factor)
            }
            Size::Physical(physical_size) => physical_size,
        }
    }

    /// The current user zoom (scale) factor. This is applied on top of the
    /// system's scale factor.
    pub fn user_scale_factor(&self) -> f32 {
        self.zoom_factor.load()
    }

    /// Set the user zoom (scale) factor. This is applied on top of the
    /// system's scale factor.
    pub fn set_user_scale_factor(&self, scale_factor: f32) {
        assert!(scale_factor > 0.0);

        self.zoom_factor.store(scale_factor);
    }

    /// Whether the GUI is currently visible.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}

pub struct IcedNiceContext {
    pub nice_context: GuiContext,
    window_size: SharedWindowSize,
    editor_state: Arc<IcedEditorState>,
}

impl IcedNiceContext {
    /// Whether the GUI is currently visible.
    pub fn is_open(&self) -> bool {
        self.editor_state.is_open()
    }

    pub fn window_size(&self) -> baseview::WindowSize {
        self.window_size.get()
    }

    /// The current user zoom (scale) factor. This is applied on top of the
    /// system's scale factor.
    pub fn user_scale_factor(&self) -> f32 {
        self.editor_state.user_scale_factor()
    }

    /// Set the user zoom (scale) factor. This is applied on top of the
    /// system's scale factor.
    ///
    /// Note, this must be paired with returning the scale factor in
    /// [`Program::scale_factor`] in order for the window to resize.
    pub fn set_user_scale_factor(&self, scale_factor: f32) {
        self.editor_state.set_user_scale_factor(scale_factor);
    }

    /// Sync the editor state with the current window size and zoom factor.
    pub fn sync_window_size(&self) {
        let size = self.window_size();

        let old_size = self.editor_state.size.load();
        let new_size = match old_size {
            Size::Logical(_) => Size::Logical(size.logical),
            Size::Physical(_) => Size::Physical(size.physical),
        };
        self.editor_state.size.store(new_size);
    }
}

impl Clone for IcedNiceContext {
    fn clone(&self) -> Self {
        Self {
            nice_context: self.nice_context.clone(),
            window_size: self.window_size.clone(),
            editor_state: Arc::clone(&self.editor_state),
        }
    }
}
