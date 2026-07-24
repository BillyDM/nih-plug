//! An [`Editor`] implementation for egui.

use crate::EguiState;
use egui::Context;
use egui::ViewportCommand;
use egui_baseview::EguiWindowSettings;
use egui_baseview::ExtraOutputCommands;
use egui_baseview::baseview::{WindowHandle, WindowScalePolicy};
use egui_baseview::{EguiWindow, GraphicsConfig};
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::context::gui::ParamSetter;
use nice_plug_core::editor::Editor;
use nice_plug_core::editor::ParentWindowHandle;
use nice_plug_core::editor::dpi::LogicalSize;
use nice_plug_core::editor::dpi::Size;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[derive(Default)]
pub struct EguiNiceSettings {
    pub title: String,
    pub graphics: GraphicsConfig,
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
}

/// An [`Editor`] implementation that calls an egui draw loop.
pub(crate) struct EguiEditor<T> {
    pub(crate) egui_state: Arc<EguiState>,
    pub(crate) user_state: Arc<Mutex<T>>,

    pub(crate) settings: Arc<EguiNiceSettings>,

    /// The user's build function. Applied once at the start of the application.
    pub(crate) build:
        Arc<dyn Fn(&Context, &mut ExtraOutputCommands, &mut T) + 'static + Send + Sync>,
    /// The user's update function.
    pub(crate) update: Arc<
        dyn Fn(&mut egui::Ui, &ParamSetter, &mut ExtraOutputCommands, &mut T)
            + 'static
            + Send
            + Sync,
    >,
}

impl<T> Editor for EguiEditor<T>
where
    T: 'static + Send,
{
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any> {
        let build = self.build.clone();
        let update = self.update.clone();
        let state = self.user_state.clone();
        let egui_state = self.egui_state.clone();
        let logical_size = self.egui_state.size();
        let host_scale_factor = self.egui_state.host_scale_factor();
        let context_1 = context.clone();

        let scale_policy = host_scale_factor
            .map(|factor| WindowScalePolicy::ScaleFactor(factor as f64))
            .unwrap_or(WindowScalePolicy::SystemScaleFactor);

        let window = EguiWindow::open_parented(
            &parent,
            EguiWindowSettings {
                title: self.settings.title.clone(),
                size: logical_size.into(),
                scale_policy,
                graphics: self.settings.graphics.clone(),
            },
            state,
            move |egui_ctx, extra_commands, state| {
                build(egui_ctx, extra_commands, &mut state.lock());
            },
            move |_output, viewport_output, _state| {
                for command in viewport_output.commands.iter() {
                    if let ViewportCommand::InnerSize(size) = command {
                        let old_size = egui_state.size();
                        egui_state.size.store(LogicalSize::new(size.x, size.y));
                        if !context.request_resize() {
                            egui_state.size.store(old_size);
                        }
                    }
                }
            },
            move |egui_ctx, extra_commands, state| {
                let setter = ParamSetter::new(context_1.as_ref());
                (update)(egui_ctx, &setter, extra_commands, &mut state.lock());
            },
        );

        self.egui_state.open.store(true, Ordering::Release);
        Box::new(EguiEditorHandle {
            egui_state: self.egui_state.clone(),
            window,
        })
    }

    /// Size of the editor window
    fn size(&self) -> Size {
        self.egui_state.size().into()
    }

    fn set_scale_factor(&self, factor: f64) -> bool {
        // If the editor is currently open then the host must not change the current HiDPI scale as
        // we don't have a way to handle that. Ableton Live does this.
        if self.egui_state.is_open() {
            return false;
        }

        self.egui_state.host_scale_factor.store(Some(factor as f32));

        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        // As mentioned above, for now we'll always force a redraw to allow meter widgets to work
        // correctly. In the future we can use an `Arc<AtomicBool>` and only force a redraw when
        // that boolean is set.
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {}

    fn param_values_changed(&self) {
        // Same
    }
}

/// The window handle used for [`EguiEditor`].
struct EguiEditorHandle {
    egui_state: Arc<EguiState>,
    window: WindowHandle,
}

impl Drop for EguiEditorHandle {
    fn drop(&mut self) {
        self.egui_state.open.store(false, Ordering::Release);
        // XXX: This should automatically happen when the handle gets dropped, but apparently not
        self.window.close();
    }
}
