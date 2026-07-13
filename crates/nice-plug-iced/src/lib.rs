use std::sync::{Arc, Mutex};

use iced_baseview::{PollSubNotifier, Program};
use nice_plug_core::editor::Editor;
use serde::{Deserialize, Serialize};

pub use iced_baseview as iced;

pub mod application;
#[doc(inline)]
pub use application::application;
pub use application::{Application, EditorState};

mod editor;
pub use editor::{NiceGuiContext, WindowState};

/// Create a new `Editor` using the Iced GUI framework.
///
/// * `window_state` - The initial window state.
/// * `editor_state` - Custom state which persists between editor opens.
/// * `notifier` - An atomic flag used to notify the program when it should
///   poll for new updates and redraw (i.e. as a result of the host updating
///   parameters or the audio thread updating the state of meters). This flag
///   is polled every frame right before drawing. If the flag is set then the
///   `poll_events` subscription will be called.
/// * `settings` - Additional settings for the editor.
/// * `build` - The function which builds the Iced program.
pub fn create_iced_editor<P, B, EState>(
    window_state: Arc<WindowState>,
    editor_state: EState,
    notifier: PollSubNotifier,
    settings: EditorSettings,
    build: B,
) -> Option<Box<dyn Editor>>
where
    P: Program + 'static,
    B: Fn(EditorState<EState>, NiceGuiContext) -> P + 'static + Send + Sync,
    EState: Send + 'static,
{
    Some(Box::new(editor::IcedEditor {
        window_state,
        editor_state: Arc::new(Mutex::new(Some(editor_state))),
        settings: Arc::new(settings),
        build: Arc::new(build),
        notifier,
    }))
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorSettings {
    pub window_title: String,

    /// Ignore key inputs, except for modifier keys such as SHIFT and ALT
    ///
    /// By default this is set to `false`.
    pub ignore_non_modifier_keys: bool,

    /// Always redraw whenever the baseview window updates instead of only when iced wants to update
    /// the window. This works around a current baseview limitation where it does not support
    /// trigger a redraw on window visibility change (which may cause blank windows when opening or
    /// reopening the editor) and an iced limitation where it's not possible to have animations
    /// without using an asynchronous timer stream to send redraw messages to the application.
    ///
    /// By default this is set to `false`.
    pub always_redraw: bool,
}
