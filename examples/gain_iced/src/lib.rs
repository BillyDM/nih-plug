use nih_plug::prelude::*;
use nih_plug_iced::iced::{
    self, Center, PollSubNotifier, Theme,
    widget::{Column, ProgressBar, button, column, slider, text},
};
use nih_plug_iced::{EditorState, NihGuiContext, WindowState, application, create_iced_editor};
use std::sync::{Arc, atomic::Ordering};

const MIN_GAIN_DB: f32 = -30.0;
const MAX_GAIN_DB: f32 = 30.0;

const WINDOW_WIDTH: u32 = 300;
const WINDOW_HEIGHT: u32 = 300;

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 150.0;

pub struct Gain {
    params: Arc<GainParams>,

    /// Needed to normalize the peak meter's response based on the sample rate.
    peak_meter_decay_weight: f32,

    /// The current data for the peak meter. This is stored as an [`Arc`] so we can share it between
    /// the GUI and the audio processing parts. If you have more state to share, then it's a good
    /// idea to put all of that in a struct behind a single `Arc`.
    ///
    /// This is stored as voltage gain.
    peak_meter: Arc<AtomicF32>,

    /// An atomic flag used to notify the program when it should poll for new updates
    /// and redraw (i.e. as a result of the host updating parameters or the audio thread
    /// updating the state of meters). This flag is polled every frame right before
    /// drawing. If the flag is set then the [`poll_events`] subscription will be called, and
    /// the program will update and redraw.
    notifier: PollSubNotifier,
}

#[derive(Params)]
pub struct GainParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "window-state"]
    window_state: Arc<WindowState>,

    #[id = "gain"]
    pub gain: FloatParam,

    // TODO: Remove this parameter when we're done implementing the widgets
    #[id = "foobar"]
    pub some_int: IntParam,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            params: Arc::new(GainParams::default()),

            peak_meter_decay_weight: 1.0,
            peak_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            notifier: PollSubNotifier::new(),
        }
    }
}

impl Default for GainParams {
    fn default() -> Self {
        Self {
            window_state: WindowState::from_logical_size(WINDOW_WIDTH, WINDOW_HEIGHT),

            // See the main gain example for more details
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(MIN_GAIN_DB),
                    max: util::db_to_gain(MAX_GAIN_DB),
                    factor: FloatRange::gain_skew_factor(MIN_GAIN_DB, MAX_GAIN_DB),
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

impl Plugin for Gain {
    const NAME: &'static str = "Gain (nih_plug_iced)";
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
        create_iced_editor(
            self.params.window_state.clone(),
            MyEditorState {
                params: self.params.clone(),
                peak_meter: self.peak_meter.clone(),
            },
            self.notifier.clone(),
            Default::default(),
            |editor_state, nih_ctx| {
                application(
                    editor_state,
                    nih_ctx,
                    MyGui::new,
                    MyGui::update,
                    MyGui::view,
                )
                .theme(MyGui::theme)
                // Subscribe to the poller which detects when the application should poll
                // parameters/meters and redraw.
                .subscription(|_| iced::poll_events().map(|_| Message::Poll))
                .run()
            },
        )
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
            if self.params.window_state.is_open() {
                amplitude = (amplitude / num_samples as f32).abs();
                let current_peak_meter = self.peak_meter.load(Ordering::Relaxed);
                let mut new_peak_meter = if amplitude > current_peak_meter {
                    amplitude
                } else {
                    current_peak_meter * self.peak_meter_decay_weight
                        + amplitude * (1.0 - self.peak_meter_decay_weight)
                };
                if new_peak_meter < 0.0001 {
                    new_peak_meter = 0.0;
                }

                if current_peak_meter != new_peak_meter {
                    self.peak_meter.store(new_peak_meter, Ordering::Relaxed);

                    // Notify the GUI that it should redraw.
                    self.notifier.notify();
                }
            }
        }

        ProcessStatus::Normal
    }
}

#[derive(Debug, Clone, Copy)]
enum Message {
    /// Sent when the application should poll parameters/meters and redraw.
    Poll,
    Increment,
    Decrement,
    GainChanged(f32),
}

/// State relating to the editor itself (not necessarly the GUI). Put any
/// state that should persist between editor opens here.
struct MyEditorState {
    params: Arc<GainParams>,
    peak_meter: Arc<AtomicF32>,
}

struct MyGui {
    /// The editor state is stored inside of a wrapper which allows the
    /// state to persist across editor opens.
    editor_state: EditorState<MyEditorState>,

    /// A handle that can be used to request operations from nih-plug, like
    /// resizing the window.
    #[allow(unused)]
    nih_ctx: NihGuiContext,

    value: i64,
    peak_meter_db: f32,
}

impl MyGui {
    pub fn new(editor_state: EditorState<MyEditorState>, nih_ctx: NihGuiContext) -> Self {
        Self {
            editor_state,
            nih_ctx,
            value: 0,
            peak_meter_db: nih_plug::util::gain_to_db(0.0),
        }
    }

    pub fn theme(&self) -> Option<Theme> {
        Some(Theme::Dark)
    }

    pub fn update(&mut self, message: Message) {
        let setter = self.nih_ctx.param_setter();
        let params = &self.editor_state.params;

        match message {
            Message::Poll => {
                self.peak_meter_db = nih_plug::util::gain_to_db(
                    self.editor_state.peak_meter.load(Ordering::Relaxed),
                );
            }
            Message::Increment => {
                self.value += 1;
            }
            Message::Decrement => {
                self.value -= 1;
            }
            Message::GainChanged(value) => {
                // TODO: Add generic slider widget
                setter.begin_set_parameter(&params.gain);
                setter.set_parameter_normalized(&params.gain, value);
                setter.end_set_parameter(&params.gain);
            }
        }
    }

    pub fn view(&self) -> Column<'_, Message> {
        let params = &self.editor_state.params;

        column![
            button("Increment").on_press(Message::Increment),
            text(self.value).size(30),
            button("Decrement").on_press(Message::Decrement),
            // TODO: Add generic slider widget
            slider(
                0.0..=1.0,
                params.gain.modulated_normalized_value(),
                Message::GainChanged
            )
            .step(0.001),
            text(
                params
                    .gain
                    .normalized_value_to_string(params.gain.modulated_normalized_value(), true)
            ),
            ProgressBar::new(-80.0..=0.0, self.peak_meter_db),
        ]
        .padding(20)
        .spacing(12.0)
        .align_x(Center)
    }
}

impl ClapPlugin for Gain {
    const CLAP_ID: &'static str = "com.moist-plugins-gmbh-egui.nih-plug-gain-iced";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A smoothed gain parameter example plugin with Iced GUI");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

impl Vst3Plugin for Gain {
    const VST3_CLASS_ID: [u8; 16] = *b"GainGuiYeahBoyy1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nih_export_clap!(Gain);
nih_export_vst3!(Gain);
