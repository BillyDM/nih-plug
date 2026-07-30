//! [egui](https://github.com/emilk/egui) editor support for nice-plug.
//!
//! TODO: Proper usage example, for now check out the gain_gui example

// See the comment in the main `nice-plug` crate
#![allow(clippy::type_complexity)]

use crossbeam::atomic::AtomicCell;
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::editor::Editor;
use nice_plug_core::editor::dpi::{LogicalSize, PhysicalSize};
use nice_plug_core::params::persist::PersistentField;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(any(feature = "opengl", feature = "wgpu")))]
compile_error!("There's currently no software rendering support for egui");

/// Re-export for convenience.
pub use egui_baseview::*;

#[cfg(all(feature = "opengl", not(feature = "wgpu")))]
pub use baseview::gl::{GlConfig, Profile};

pub use crate::editor::EguiNiceSettings;

mod editor;
pub mod resizable_window;
pub mod widgets;

/// Create an [`Editor`] instance using an [`egui`] GUI. Using the user state parameter is
/// optional, but it can be useful for keeping track of some temporary GUI-only settings. See the
/// `nice-plug_gain_egui` example for more information on how to use this. The [`EguiState`] passed
/// to this function contains the GUI's intitial size, and this is kept in sync whenever the GUI gets
/// resized. You can also use this to know if the GUI is open, so you can avoid performing
/// potentially expensive calculations while the GUI is not open. If you want this size to be
/// persisted when restoring a plugin instance, then you can store it in a `#[persist = "key"]`
/// field on your parameters struct.
///
/// See [`EguiState::from_size()`].
pub fn create_egui_editor<A: NiceEguiApp>(
    egui_state: Arc<EguiState>,
    settings: EguiNiceSettings,
    app: A,
) -> Option<Box<dyn Editor>> {
    Some(Box::new(editor::EguiEditor {
        egui_state,
        user_app: Arc::new(Mutex::new(app)),
        settings: Arc::new(settings),
    }))
}

/// Implement this trait to run an app with nice-plug-egui.
pub trait NiceEguiApp: Send + 'static {
    /// Called when a new editor is opened. Setup code such as `egui_ctx.set_fonts()`
    /// can be done here.
    ///
    /// This may be called again after a call to [`NiceEguiApp::editor_closed()`].
    ///
    /// If an error is returned, then the window will be closed.
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        frame: &mut Frame,
    ) -> Result<(), baseview::HandlerError> {
        let _ = egui_ctx;
        let _ = nice_gui_ctx;
        let _ = frame;
        Ok(())
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    ///
    /// This will only ever be called while an editor window is open.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame);

    /// Called when the editor is closed. This is needed because the plugin editor window
    /// can be opened and closed multiple times.
    ///
    /// If your app holds onto an `egui::Context` object, then it should be dropped here so that
    /// egui can probably be cleaned up.
    fn editor_closed(&mut self) {}
}

/// State for an `nice-plug-egui` editor.
#[derive(Debug, Serialize, Deserialize)]
pub struct EguiState {
    /// The window's size in logical pixels before applying `scale_factor`.
    #[serde(with = "nice_plug_core::params::persist::serialize_atomic_cell")]
    logical_size: AtomicCell<(f32, f32)>,

    /// It would be annoying if the zoom factor was saved along with presets. The plugin should
    /// load this from a config file instead.
    #[serde(skip)]
    pub(crate) zoom_factor: AtomicCell<f32>,

    #[serde(skip)]
    pub(crate) host_scale_factor: AtomicCell<Option<f32>>,

    #[serde(skip)]
    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    pub(crate) system_scale_factor: AtomicCell<f64>,

    /// Whether the editor's window is currently open.
    #[serde(skip)]
    open: AtomicBool,
}

impl<'a> PersistentField<'a, EguiState> for Arc<EguiState> {
    fn set(&self, new_value: EguiState) {
        self.logical_size.store(new_value.logical_size.load());
        self.zoom_factor.store(new_value.zoom_factor.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&EguiState) -> R,
    {
        f(self)
    }
}

impl EguiState {
    pub fn from_size(size: LogicalSize<f32>, zoom_factor: f32) -> Arc<Self> {
        Arc::new(Self {
            logical_size: AtomicCell::new((size.width, size.height)),
            zoom_factor: AtomicCell::new(zoom_factor),
            open: AtomicBool::new(false),
            host_scale_factor: AtomicCell::new(None),
            system_scale_factor: AtomicCell::new(1.0),
        })
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn logical_size(&self) -> LogicalSize<f32> {
        let (width, height) = self.logical_size.load();
        LogicalSize::new(width, height)
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        let logical_size = self.logical_size();
        let zoom_factor = self.zoom_factor.load();
        let host_scale_factor = self.host_scale_factor.load();
        let system_scale_factor = self.system_scale_factor.load();

        let scale_factor = zoom_factor as f64
            * host_scale_factor
                .map(|s| s as f64)
                .unwrap_or(system_scale_factor);

        logical_size.to_physical(scale_factor)
    }

    /// Whether the GUI is currently visible.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}
