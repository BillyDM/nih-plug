//! Configure your application.
use baseview::{Size, WindowOpenOptions, WindowScalePolicy};

/// Any settings specific to `iced_baseview`.
pub struct IcedBaseviewSettings {
    // /// The identifier of the application.
    // ///
    // /// If provided, this identifier may be used to identify the application or
    // /// communicate with it through the windowing system.
    // pub id: Option<String>,
    /// The window settings.
    pub window: WindowOpenOptions,

    /// Ignore key inputs, except for modifier keys such as SHIFT and ALT
    pub ignore_non_modifier_keys: bool,

    /// Always redraw whenever the baseview window updates instead of only when iced wants to update
    /// the window. This works around a current baseview limitation where it does not support
    /// trigger a redraw on window visibility change (which may cause blank windows when opening or
    /// reopening the editor) and an iced limitation where it's not possible to have animations
    /// without using an asynchronous timer stream to send redraw messages to the application.
    pub always_redraw: bool,
}

impl Default for IcedBaseviewSettings {
    fn default() -> Self {
        Self {
            window: WindowOpenOptions {
                title: String::from("iced_baseview"),
                size: Size::new(500.0, 300.0),
                scale: WindowScalePolicy::SystemScaleFactor,
                #[cfg(feature = "rustanalyzer_opengl_workaround")]
                gl_config: None,
            },
            ignore_non_modifier_keys: false,
            always_redraw: false,
        }
    }
}

impl Clone for IcedBaseviewSettings {
    fn clone(&self) -> Self {
        Self {
            window: WindowOpenOptions {
                title: self.window.title.clone(),
                size: self.window.size,
                scale: self.window.scale,
                #[cfg(feature = "rustanalyzer_opengl_workaround")]
                gl_config: None,
            },
            ignore_non_modifier_keys: self.ignore_non_modifier_keys,
            always_redraw: self.always_redraw,
        }
    }
}
