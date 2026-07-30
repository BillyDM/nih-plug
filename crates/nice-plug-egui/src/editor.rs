//! An [`Editor`] implementation for egui.

use crate::EguiState;
use crate::NiceEguiApp;
use egui_baseview::EguiWindowSettings;
use egui_baseview::ResizeMode;
use egui_baseview::baseview;
use egui_baseview::baseview::HandlerError;
use egui_baseview::baseview::Window;
use egui_baseview::{EguiWindow, GraphicsConfig};
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::editor::Editor;
use nice_plug_core::editor::EditorInstance;
use nice_plug_core::editor::EditorWindow;
use nice_plug_core::editor::Modifiers;
use nice_plug_core::editor::ParentWindowHandle;
use nice_plug_core::editor::ResizeHint;
use nice_plug_core::editor::VirtualKeyCode;
use nice_plug_core::editor::dpi::PhysicalSize;
use nice_plug_core::nice_error;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Default)]
pub struct EguiNiceSettings {
    pub title: String,
    pub graphics: GraphicsConfig,
    pub resize_mode: ResizeMode,
    pub resize_hint: ResizeHint,
}

impl EguiNiceSettings {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_tile(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    #[inline]
    pub fn with_graphics_config(mut self, config: GraphicsConfig) -> Self {
        self.graphics = config;
        self
    }

    #[inline]
    pub fn with_resize_mode(mut self, resize_mode: ResizeMode) -> Self {
        self.resize_mode = resize_mode;
        self
    }

    #[inline]
    pub fn with_resize_hint(mut self, resize_hint: ResizeHint) -> Self {
        self.resize_hint = resize_hint;
        self
    }
}

struct UserAppWrapper<A: NiceEguiApp> {
    user_app: Arc<Mutex<A>>,
    gui_context: GuiContext,
    egui_ctx: Arc<Mutex<Option<egui::Context>>>,
}

impl<A: NiceEguiApp> egui_baseview::App for UserAppWrapper<A> {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        frame: &mut egui_baseview::Frame,
    ) -> Result<(), baseview::HandlerError> {
        *self.egui_ctx.lock() = Some(egui_ctx.clone());

        self.user_app
            .lock()
            .build(egui_ctx, self.gui_context.clone(), frame)
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut egui_baseview::Frame) {
        self.user_app.lock().ui(ui, frame);
    }
}

/// An [`Editor`] implementation that calls an egui draw loop.
pub(crate) struct EguiEditor<A: NiceEguiApp> {
    pub(crate) egui_state: Arc<EguiState>,
    pub(crate) user_app: Arc<Mutex<A>>,

    pub(crate) settings: Arc<EguiNiceSettings>,
}

impl<A: NiceEguiApp> Editor for EguiEditor<A> {
    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        suggested_scale_factor: Option<f64>,
        gui_context: GuiContext,
        host: Option<baseview::host::Host>,
    ) -> Result<EditorWindow, HandlerError> {
        let egui_state = self.egui_state.clone();
        let user_app = self.user_app.clone();
        let zoom_factor = egui_state.zoom_factor.load();
        let logical_size = egui_state.logical_size();

        let settings = EguiWindowSettings::new()
            .with_title(self.settings.title.clone())
            .with_size(logical_size)
            .with_resize_mode(self.settings.resize_mode)
            .with_zoom_factor(zoom_factor)
            .with_graphics_config(self.settings.graphics.clone())
            .with_parent(parent.as_ref());

        let egui_ctx = Arc::new(Mutex::new(None));

        let window = EguiWindow::create_with_host(
            settings,
            UserAppWrapper {
                user_app,
                gui_context,
                egui_ctx: egui_ctx.clone(),
            },
            host,
        )?;

        if let Some(scale_factor) = suggested_scale_factor {
            let _ = window.suggest_fallback_scale_factor(scale_factor)?;
        }

        self.egui_state.open.store(true, Ordering::Release);

        Ok(EditorWindow {
            editor: Box::new(EguiEditorInstance {
                egui_state: self.egui_state.clone(),
                resize_hint: self.settings.resize_hint,
                egui_ctx,
            }),
            window,
        })
    }

    fn size(&self) -> PhysicalSize<u32> {
        self.egui_state.physical_size()
    }

    fn resize_hint(&self) -> nice_plug_core::editor::ResizeHint {
        self.settings.resize_hint
    }
}

/// The window handle used for [`EguiEditor`].
struct EguiEditorInstance {
    egui_state: Arc<EguiState>,
    egui_ctx: Arc<Mutex<Option<egui::Context>>>,
    resize_hint: ResizeHint,
}

impl EditorInstance for EguiEditorInstance {
    fn set_size(&mut self, new_size: PhysicalSize<u32>, window: &mut Window) -> bool {
        window.resize(new_size.into()).is_ok()
    }

    fn set_suggested_scale_factor(&mut self, scale_factor: f64, window: &mut Window) -> bool {
        window.suggest_fallback_scale_factor(scale_factor).is_ok()
    }

    /// Return the closest supported size.
    fn adjust_size(
        &self,
        new_size: PhysicalSize<u32>,
        window: &Window,
    ) -> Option<PhysicalSize<u32>> {
        let current_size = window.size();
        Some(self.resize_hint.adjust_size(
            new_size,
            current_size.physical,
            current_size.scale_factor,
        ))
    }

    fn on_virtual_key_from_host(
        &mut self,
        _key_code: VirtualKeyCode,
        _is_down: bool,
        _modifiers: Modifiers,
    ) -> bool {
        // TODO
        false
    }

    fn state_changed(&mut self, window: Option<&mut Window>) {
        if let Some(egui_ctx) = self.egui_ctx.lock().as_ref() {
            egui_ctx.request_repaint();
        }
        if let Some(window) = window {
            let size = self.egui_state.physical_size();
            if window.size().physical != size {
                if let Err(e) = window.resize(size.into()) {
                    nice_error!("Failed to resize window after state change: {}", e);
                }
            }

            // TODO
        }
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        if let Some(egui_ctx) = self.egui_ctx.lock().as_ref() {
            egui_ctx.request_repaint();
        }
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        if let Some(egui_ctx) = self.egui_ctx.lock().as_ref() {
            egui_ctx.request_repaint();
        }
    }
}

impl Drop for EguiEditorInstance {
    fn drop(&mut self) {
        self.egui_state.open.store(false, Ordering::Release);
    }
}
