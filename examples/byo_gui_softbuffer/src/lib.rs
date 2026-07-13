//! This plugin demonstrates how to "bring your own GUI toolkit" using a raw Softbuffer rendering context.

use baseview::{
    WindowContext, WindowHandle, WindowOpenOptions, WindowScalePolicy,
    dpi::{LogicalSize, Size},
};
use crossbeam::atomic::AtomicCell;
use nice_plug::params::persist::PersistentField;
use nice_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 150.0;

pub struct CustomSoftbufferWindow {
    gui_context: Arc<dyn GuiContext>,
    _window: WindowContext,

    surface: RefCell<Surface>,

    #[allow(unused)]
    params: Arc<MyPluginParams>,
    #[allow(unused)]
    peak_meter: Arc<AtomicF32>,
}

struct Surface {
    _sb_context: softbuffer::Context<WindowContext>,
    sb_surface: softbuffer::Surface<WindowContext, WindowContext>,
}

impl CustomSoftbufferWindow {
    fn new(
        window: WindowContext,
        gui_context: Arc<dyn GuiContext>,
        params: Arc<MyPluginParams>,
        peak_meter: Arc<AtomicF32>,
    ) -> Self {
        let size = window.size();

        let sb_context =
            softbuffer::Context::new(window.clone()).expect("could not get softbuffer context");
        let mut sb_surface = softbuffer::Surface::new(&sb_context, window.clone())
            .expect("could not create softbuffer surface");

        sb_surface
            .resize(
                NonZeroU32::new(size.physical.width).unwrap(),
                NonZeroU32::new(size.physical.height).unwrap(),
            )
            .unwrap();

        Self {
            gui_context,
            _window: window,
            surface: RefCell::new(Surface {
                _sb_context: sb_context,
                sb_surface,
            }),
            params,
            peak_meter,
        }
    }
}

impl baseview::WindowHandler for CustomSoftbufferWindow {
    fn on_frame(&self) {
        // Do rendering here.

        let mut surface = self.surface.borrow_mut();
        let Surface {
            _sb_context,
            sb_surface,
        } = &mut *surface;

        let mut buffer = sb_surface.buffer_mut().unwrap();

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

        buffer.present().unwrap();
    }

    fn on_event(&self, event: baseview::Event) -> baseview::EventStatus {
        // Use this to set parameter values.
        let _param_setter = ParamSetter::new(self.gui_context.as_ref());

        // Do event processing here.
        #[allow(clippy::match_single_binding)]
        match &event {
            _ => {}
        }

        baseview::EventStatus::Captured
    }

    fn resized(&self, new_size: baseview::WindowSize) {
        self.params
            .editor_state
            .window_scale_factor
            .store(new_size.scale_factor as f32);
        self.params.editor_state.size.store(new_size.logical.cast());

        self.surface
            .borrow_mut()
            .sb_surface
            .resize(
                NonZeroU32::new(new_size.physical.width).unwrap(),
                NonZeroU32::new(new_size.physical.height).unwrap(),
            )
            .unwrap();
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomSoftbufferEditorState {
    /// The window's size in logical pixels before applying `scale_factor`.
    #[serde(with = "nice_plug::params::persist::serialize_atomic_cell")]
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

impl CustomSoftbufferEditorState {
    pub fn from_size(size: LogicalSize<f32>) -> Arc<Self> {
        Arc::new(Self {
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

impl<'a> PersistentField<'a, CustomSoftbufferEditorState> for Arc<CustomSoftbufferEditorState> {
    fn set(&self, new_value: CustomSoftbufferEditorState) {
        self.size.store(new_value.size.load());
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
    peak_meter: Arc<AtomicF32>,
}

impl Editor for CustomSoftbufferEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn std::any::Any> {
        let host_scale_factor = self.params.editor_state.host_scale_factor();
        let size = self.params.editor_state.size();

        let gui_context = Arc::clone(&context);

        let params = Arc::clone(&self.params);
        let peak_meter = Arc::clone(&self.peak_meter);

        let scale_policy = host_scale_factor
            .map(|factor| WindowScalePolicy::ScaleFactor(factor as f64))
            .unwrap_or(WindowScalePolicy::SystemScaleFactor);

        let window = baseview::Window::open_parented(
            &parent,
            WindowOpenOptions::new()
                .with_title("Softbuffer Window")
                .with_size(size)
                .with_scale_policy(scale_policy),
            move |window: WindowContext| -> CustomSoftbufferWindow {
                CustomSoftbufferWindow::new(window, gui_context, params, peak_meter)
            },
        );

        self.params.editor_state.open.store(true, Ordering::Release);
        Box::new(CustomSoftbufferEditorHandle {
            state: self.params.editor_state.clone(),
            window,
        })
    }

    fn size(&self) -> Size {
        self.params.editor_state.size().into()
    }

    fn set_scale_factor(&self, factor: f64) -> bool {
        // If the editor is currently open then the host must not change the current HiDPI scale as
        // we don't have a way to handle that. Ableton Live does this.
        if self.params.editor_state.is_open() {
            return false;
        }

        self.params
            .editor_state
            .host_scale_factor
            .store(Some(factor as f32));

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

/// The window handle used for [`CustomSoftbufferEditor`].
struct CustomSoftbufferEditorHandle {
    state: Arc<CustomSoftbufferEditorState>,
    window: WindowHandle,
}

impl Drop for CustomSoftbufferEditorHandle {
    fn drop(&mut self) {
        self.state.open.store(false, Ordering::Release);
        // XXX: This should automatically happen when the handle gets dropped, but apparently not
        self.window.close();
    }
}

/// This is mostly identical to the gain example, minus some fluff, and with a GUI.
pub struct MyPlugin {
    params: Arc<MyPluginParams>,

    /// Needed to normalize the peak meter's response based on the sample rate.
    peak_meter_decay_weight: f32,
    /// The current data for the peak meter. This is stored as an [`Arc`] so we can share it between
    /// the GUI and the audio processing parts. If you have more state to share, then it's a good
    /// idea to put all of that in a struct behind a single `Arc`.
    ///
    /// This is stored as voltage gain.
    peak_meter: Arc<AtomicF32>,
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

            peak_meter_decay_weight: 1.0,
            peak_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        }
    }
}

impl Default for MyPluginParams {
    fn default() -> Self {
        Self {
            editor_state: CustomSoftbufferEditorState::from_size(LogicalSize::new(200.0, 150.0)),

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
            peak_meter: Arc::clone(&self.peak_meter),
        }))
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // After `PEAK_METER_DECAY_MS` milliseconds of pure silence, the peak meter's value should
        // have dropped by 12 dB
        self.peak_meter_decay_weight = 0.25f64
            .powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip())
            as f32;

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let mut amplitude = 0.0;
            let num_samples = channel_samples.len();

            let gain = self.params.gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
                amplitude += *sample;
            }

            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.editor_state.is_open() {
                amplitude = (amplitude / num_samples as f32).abs();
                let current_peak_meter = self.peak_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_peak_meter = if amplitude > current_peak_meter {
                    amplitude
                } else {
                    current_peak_meter * self.peak_meter_decay_weight
                        + amplitude * (1.0 - self.peak_meter_decay_weight)
                };

                self.peak_meter
                    .store(new_peak_meter, std::sync::atomic::Ordering::Relaxed)
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
