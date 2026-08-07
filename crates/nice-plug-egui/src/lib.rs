//! [egui](https://github.com/emilk/egui) editor support for nice-plug.
//!
//! TODO: Proper usage example, for now check out the gain_gui example

// See the comment in the main `nice-plug` crate
#![allow(clippy::type_complexity)]

use crossbeam::atomic::AtomicCell;
use egui_baseview::baseview::WindowSize;
use nice_plug_core::context::gui::GuiContext;
use nice_plug_core::editor::dpi::{PhysicalSize, Size};
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(not(any(feature = "opengl", feature = "wgpu")))]
compile_error!("There's currently no software rendering support for egui");

/// Re-export for convenience.
pub use egui_baseview::*;

#[cfg(all(feature = "opengl", not(feature = "wgpu")))]
pub use baseview::gl::{GlConfig, Profile};

pub use crate::editor::{EguiEditor, EguiEditorHandle, EguiNiceSettings};

mod editor;
pub mod resizable_window;
pub mod widgets;

/// Create an [`Editor`](nice_plug_core::editor::Editor) instance using an [`egui`] GUI. Using the
/// user state parameter is optional, but it can be useful for keeping track of some temporary
/// GUI-only settings. See the `nice-plug_gain_egui` example for more information on how to use this.
/// The [`EguiState`] passed to this function contains the GUI's intitial size, and this is kept in
/// sync whenever the GUI gets resized. You can also use this to know if the GUI is open, so you can
/// avoid performing potentially expensive calculations while the GUI is not open.
///
/// See [`EguiState::from_size()`].
pub fn create_egui_editor<A: NiceEguiApp>(
    egui_state: Arc<EguiState>,
    settings: EguiNiceSettings,
    app: A,
) -> Option<EguiEditor<A>> {
    Some(EguiEditor {
        egui_state,
        user_app: Arc::new(Mutex::new(app)),
        settings: Arc::new(settings),
    })
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

    /// Called when the window has been resized.
    fn resized(&mut self, new_size: WindowSize) {
        let _ = new_size;
    }

    /// Called when the zoom factor has changed.
    fn zoom_factor_changed(&mut self, zoom_factor: f32) {
        let _ = zoom_factor;
    }

    /// Called when the editor is closed. This is needed because the plugin editor window
    /// can be opened and closed multiple times.
    ///
    /// If your app holds onto an `egui::Context` object, then it should be dropped here so that
    /// egui can probably be cleaned up.
    fn editor_closed(&mut self) {}
}

/// State for an `nice-plug-egui` editor.
#[derive(Debug)]
pub struct EguiState {
    size: AtomicCell<Size>,
    zoom_factor: AtomicCell<f32>,

    pub(crate) host_scale_factor: AtomicCell<Option<f32>>,

    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    pub(crate) system_scale_factor: AtomicCell<f64>,

    /// Whether the editor's window is currently open.
    open: AtomicBool,
}

impl EguiState {
    /// Create a new state for egui's editor.
    pub fn from_size(size: impl Into<Size>, zoom_factor: f32) -> Arc<Self> {
        assert!(zoom_factor > 0.0);

        Arc::new(Self {
            size: AtomicCell::new(size.into()),
            zoom_factor: AtomicCell::new(zoom_factor),
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

    /// Whether the GUI is currently visible.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}
