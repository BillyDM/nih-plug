//! This plugin demonstrates how to "bring your own GUI toolkit" using a raw Softbuffer rendering context.

use baseview::{
    HandlerError, Window, WindowContext, WindowSettings,
    dpi::{LogicalSize, PhysicalSize},
};
use crossbeam::atomic::AtomicCell;
use nice_plug::{context::gui::GuiContext, editor::EditorInstance, prelude::*};
use nice_plug::{editor::EditorWindow, params::persist::PersistentField};
use serde::{Deserialize, Serialize};
use softbuffer::SoftBufferError;
use std::{
    cell::{Cell, RefCell},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MIN_SIZE: LogicalSize<f32> = LogicalSize::new(200.0, 150.0);
const RESIZE_HINT: ResizeHint = ResizeHint::with_min_size(MIN_SIZE);

pub struct CustomSoftbufferWindow {
    _gui_context: GuiContext,
    _window: WindowContext,

    surface: RefCell<Surface>,

    #[allow(unused)]
    params: Arc<MyPluginParams>,
    redraw_requested: Arc<AtomicBool>,
    is_first_frame: Cell<bool>,
}

struct Surface {
    _sb_context: softbuffer::Context<WindowContext>,
    sb_surface: softbuffer::Surface<WindowContext, WindowContext>,
}

impl CustomSoftbufferWindow {
    fn new(
        window: WindowContext,
        gui_context: GuiContext,
        params: Arc<MyPluginParams>,
        redraw_requested: Arc<AtomicBool>,
    ) -> Result<Self, SoftBufferError> {
        let size = window.size();

        let sb_context = softbuffer::Context::new(window.clone())?;
        let mut sb_surface = softbuffer::Surface::new(&sb_context, window.clone())?;

        sb_surface.resize(
            NonZeroU32::new(size.physical.width).unwrap(),
            NonZeroU32::new(size.physical.height).unwrap(),
        )?;

        Ok(Self {
            _gui_context: gui_context,
            _window: window,
            surface: RefCell::new(Surface {
                _sb_context: sb_context,
                sb_surface,
            }),
            params,
            redraw_requested,
            is_first_frame: Cell::new(true),
        })
    }
}

impl baseview::WindowHandler for CustomSoftbufferWindow {
    fn on_frame(&self) -> Result<(), HandlerError> {
        if self.is_first_frame.get() {
            // For some reason, softbuffer doesn't show anything on the first paint.
            self.is_first_frame.set(false);
        } else if !self.redraw_requested.swap(false, Ordering::Relaxed) {
            return Ok(());
        }

        // Do rendering here.

        let mut surface = self.surface.borrow_mut();
        let Surface {
            _sb_context,
            sb_surface,
        } = &mut *surface;

        let mut buffer = sb_surface.buffer_mut()?;

        let width = buffer.width().get();
        let height = buffer.height().get();

        for y in 0..height {
            for x in 0..width {
                let red = x % 255;
                let green = y % 255;
                let blue = (x * y) % 255;
                let alpha = 255;

                let index = (y as usize * width as usize) + x as usize;
                buffer[index] = blue | (green << 8) | (red << 16) | (alpha << 24);
            }
        }

        if let Err(e) = buffer.present() {
            nice_plug::nice_error!("{}", e);
        }

        Ok(())
    }

    fn on_event(&self, event: baseview::Event) -> baseview::EventStatus {
        // Do event processing here.
        #[allow(clippy::match_single_binding)]
        match &event {
            _ => {}
        }

        baseview::EventStatus::Captured
    }

    fn resized(&self, new_size: baseview::WindowSize) -> Result<(), HandlerError> {
        self.surface.borrow_mut().sb_surface.resize(
            NonZeroU32::new(new_size.physical.width).unwrap(),
            NonZeroU32::new(new_size.physical.height).unwrap(),
        )?;

        self.params
            .editor_state
            .scale_factor
            .store(new_size.scale_factor);

        let size: LogicalSize<f32> = new_size.logical.cast();
        self.params
            .editor_state
            .logical_size
            .store((size.width, size.height));

        self.redraw_requested.store(true, Ordering::Relaxed);

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomSoftbufferEditorState {
    #[serde(with = "nice_plug::params::persist::serialize_atomic_cell")]
    logical_size: AtomicCell<(f32, f32)>,
    /// Whether the editor's window is currently open.
    #[serde(skip)]
    open: AtomicBool,
    #[serde(skip)]
    scale_factor: AtomicCell<f64>,
}

impl CustomSoftbufferEditorState {
    pub fn from_size(size: LogicalSize<f32>) -> Arc<Self> {
        Arc::new(Self {
            logical_size: AtomicCell::new((size.width, size.height)),
            open: AtomicBool::new(false),
            scale_factor: AtomicCell::new(1.0),
        })
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in logical pixels.
    pub fn logical_size(&self) -> LogicalSize<f32> {
        let (width, height) = self.logical_size.load();
        LogicalSize::new(width, height)
    }

    /// Returns a `(width, height)` pair for the current size of the GUI in physical pixels.
    pub fn physical_size(&self) -> PhysicalSize<u32> {
        let (width, height) = self.logical_size.load();
        let scale_factor = self.scale_factor();
        LogicalSize::new(width, height).to_physical(scale_factor)
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor.load()
    }

    /// Whether the GUI is currently visible.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }
}

impl<'a> PersistentField<'a, CustomSoftbufferEditorState> for Arc<CustomSoftbufferEditorState> {
    fn set(&self, new_value: CustomSoftbufferEditorState) {
        self.logical_size.store(new_value.logical_size.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&CustomSoftbufferEditorState) -> R,
    {
        f(self)
    }
}

pub struct CustomSoftbufferEditor {
    params: Arc<MyPluginParams>,
}

impl Editor for CustomSoftbufferEditor {
    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        suggested_scale_factor: Option<f64>,
        gui_context: GuiContext,
        host: Option<baseview::host::Host>,
    ) -> Result<EditorWindow, HandlerError> {
        let params = Arc::clone(&self.params);

        let redraw_requested = Arc::new(AtomicBool::new(true));
        let redraw_requested_2 = Arc::clone(&redraw_requested);

        let window = baseview::Window::create_with_host(
            WindowSettings::new()
                .with_title("Softbuffer Window")
                .with_size(self.params.editor_state.logical_size())
                .with_parent(parent.as_ref()),
            move |window: WindowContext| -> Result<CustomSoftbufferWindow, HandlerError> {
                params
                    .editor_state
                    .scale_factor
                    .store(window.size().scale_factor);

                CustomSoftbufferWindow::new(window, gui_context, params, redraw_requested_2)
                    .map_err(|e| e.into())
            },
            host,
        )?;

        if let Some(scale_factor) = suggested_scale_factor {
            let _ = window.suggest_fallback_scale_factor(scale_factor);
        }

        self.params.editor_state.open.store(true, Ordering::Release);

        Ok(EditorWindow {
            editor: Box::new(CustomSoftbufferEditorInstance {
                state: self.params.editor_state.clone(),
                redraw_requested,
            }),
            window,
        })
    }

    fn size(&self) -> PhysicalSize<u32> {
        let scale_factor = self.params.editor_state.scale_factor();
        self.params
            .editor_state
            .logical_size()
            .to_physical(scale_factor)
    }

    fn resize_hint(&self) -> ResizeHint {
        RESIZE_HINT
    }
}

struct CustomSoftbufferEditorInstance {
    state: Arc<CustomSoftbufferEditorState>,
    redraw_requested: Arc<AtomicBool>,
}

impl EditorInstance for CustomSoftbufferEditorInstance {
    fn set_size(&mut self, new_size: PhysicalSize<u32>, window: &mut Window) -> bool {
        let current_size = window.size();
        if !RESIZE_HINT.is_size_valid(new_size, current_size.physical, current_size.scale_factor) {
            return false;
        }

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
        Some(RESIZE_HINT.adjust_size(new_size, current_size.physical, current_size.scale_factor))
    }

    fn on_virtual_key_from_host(
        &mut self,
        _key_code: VirtualKeyCode,
        _is_down: bool,
        _modifiers: Modifiers,
    ) -> bool {
        false
    }

    fn state_changed(&mut self, window: Option<&mut Window>) {
        self.redraw_requested.store(true, Ordering::Relaxed);

        if let Some(window) = window {
            let scale_factor = self.state.scale_factor();
            let new_size: PhysicalSize<u32> = self.state.logical_size().to_physical(scale_factor);

            if window.size().physical != new_size {
                if let Err(e) = window.resize(new_size.into()) {
                    nice_error!("Failed to resize window after state change: {}", e);
                }
            }
        }
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {
        // The UI should generally be redrawn when a param is changed.
        self.redraw_requested.store(true, Ordering::Relaxed);
    }

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {
        // The UI should generally be redrawn when a param is changed.
        self.redraw_requested.store(true, Ordering::Relaxed);
    }
}

impl Drop for CustomSoftbufferEditorInstance {
    fn drop(&mut self) {
        self.state.open.store(false, Ordering::Release);
    }
}

// ---------------------------------------------------------------------------------------------------

/// This is mostly identical to the gain example, minus some fluff, and with a GUI.
pub struct MyPlugin {
    params: Arc<MyPluginParams>,
}

#[derive(Params)]
pub struct MyPluginParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "editor-state"]
    editor_state: Arc<CustomSoftbufferEditorState>,

    #[id = "gain"]
    pub gain: FloatParam,

    #[id = "foobar"]
    pub some_int: IntParam,
}

impl Default for MyPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(MyPluginParams::default()),
        }
    }
}

impl Default for MyPluginParams {
    fn default() -> Self {
        Self {
            editor_state: CustomSoftbufferEditorState::from_size(MIN_SIZE),

            // See the main gain example for more details
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
            some_int: IntParam::new("Something", 3, IntRange::Linear { min: 0, max: 3 }),
        }
    }
}

impl Plugin for MyPlugin {
    const NAME: &'static str = "BYO GUI Example (Softbuffer)";
    const VENDOR: &'static str = "Moist Plugins GmbH";
    const URL: &'static str = "https://youtu.be/dQw4w9WgXcQ";
    const EMAIL: &'static str = "info@example.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        Some(Box::new(CustomSoftbufferEditor {
            params: Arc::clone(&self.params),
        }))
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        _buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let gain = self.params.gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
            }

            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.editor_state.is_open() {
                // Do things
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for MyPlugin {
    const CLAP_ID: &'static str = "com.moist-plugins-gmbh.byo-gui-softbuffer";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A simple example plugin with a raw Softbuffer context for rendering");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for MyPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"ByoGuiSoftbuffer";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nice_export_clap!(MyPlugin);
nice_export_vst3!(MyPlugin);
