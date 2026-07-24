//! [egui](https://github.com/emilk/egui) editor support for nice-plug.
//!
//! TODO: Proper usage example, for now check out the gain_gui example

// See the comment in the main `nice-plug` crate
#![allow(clippy::type_complexity)]

use crossbeam::atomic::AtomicCell;
use egui::{Context, Ui};
use nice_plug_core::context::gui::ParamSetter;
use nice_plug_core::editor::Editor;
use nice_plug_core::editor::dpi::LogicalSize;
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
pub fn create_egui_editor<T, B, U>(
    egui_state: Arc<EguiState>,
    user_state: T,
    settings: EguiNiceSettings,
    build: B,
    update: U,
) -> Option<Box<dyn Editor>>
where
    T: 'static + Send,
    B: Fn(&Context, &mut ExtraOutputCommands, &mut T) + 'static + Send + Sync,
    U: Fn(&mut Ui, &ParamSetter, &mut ExtraOutputCommands, &mut T) + 'static + Send + Sync,
{
    Some(Box::new(editor::EguiEditor {
        egui_state,
        user_state: Arc::new(Mutex::new(user_state)),
        settings: Arc::new(settings),
        build: Arc::new(build),
        update: Arc::new(update),
    }))
}

/// State for an `nice-plug-egui` editor.
#[derive(Debug, Serialize, Deserialize)]
pub struct EguiState {
    /// The window's size in logical pixels before applying `scale_factor`.
    #[serde(with = "nice_plug_core::params::persist::serialize_atomic_cell")]
    size: AtomicCell<LogicalSize<f32>>,

    #[serde(skip)]
    window_scale_factor: AtomicCell<f32>,
    #[serde(skip)]
    /// The scaling factor reported by the host, if any. On macOS this will never be set and we
    /// should use the system scaling factor instead.
    host_scale_factor: AtomicCell<Option<f32>>,

    /// Whether the editor's window is currently open.
    #[serde(skip)]
    open: AtomicBool,
}

impl<'a> PersistentField<'a, EguiState> for Arc<EguiState> {
    fn set(&self, new_value: EguiState) {
        self.size.store(new_value.size.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&EguiState) -> R,
    {
        f(self)
    }
}

impl EguiState {
    /// Initialize the GUI's state. This value can be passed to [`create_egui_editor()`]. The window
    /// size is in logical pixels, so before it is multiplied by the DPI scaling factor.
    pub fn from_size(size: LogicalSize<f32>) -> Arc<EguiState> {
        Arc::new(EguiState {
            size: AtomicCell::new(size),
            window_scale_factor: AtomicCell::new(1.0),
            host_scale_factor: AtomicCell::new(None),
            open: AtomicBool::new(false),
        })
    }

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
