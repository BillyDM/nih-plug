//! This plugin demonstrates how to "bring your own GUI toolkit" using a raw WGPU context.

use baseview::dpi::PhysicalSize;
use baseview::{HandlerError, Window, WindowContext, WindowSettings};
use crossbeam::atomic::AtomicCell;
use nice_plug::context::gui::GuiContext;
use nice_plug::editor::{EditorInstance, EditorWindow};
use nice_plug::prelude::*;
use nice_plug::{editor::dpi::LogicalSize, params::persist::PersistentField};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MIN_SIZE: LogicalSize<f32> = LogicalSize::new(200.0, 150.0);
const RESIZE_HINT: ResizeHint = ResizeHint::with_min_size(MIN_SIZE);

pub struct CustomWgpuWindow {
    _gui_context: GuiContext,
    window: WindowContext,

    surface: RefCell<Surface>,

    #[allow(unused)]
    params: Arc<MyPluginParams>,
    redraw_requested: Arc<AtomicBool>,
}

struct Surface {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
}

impl CustomWgpuWindow {
    fn new(
        window: WindowContext,
        gui_context: GuiContext,
        params: Arc<MyPluginParams>,
        redraw: Arc<AtomicBool>,
    ) -> Result<Self, HandlerError> {
        pollster::block_on(Self::create(window, gui_context, params, redraw))
    }

    async fn create(
        window: WindowContext,
        gui_context: GuiContext,
        params: Arc<MyPluginParams>,
        redraw: Arc<AtomicBool>,
    ) -> Result<Self, HandlerError> {
        let size = window.size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        let surface = instance.create_surface(window.platform_handle())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                // Request an adapter which can render to our surface
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;

        // Create the logical device and command queue
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await?;

        const SHADER: &str = "
            const VERTS = array(
                vec2<f32>(0.5, 1.0),
                vec2<f32>(0.0, 0.0),
                vec2<f32>(1.0, 0.0)
            );

            struct VertexOutput {
                @builtin(position) clip_position: vec4<f32>,
                @location(0) position: vec2<f32>,
            };

            @vertex
            fn vs_main(
                @builtin(vertex_index) in_vertex_index: u32,
            ) -> VertexOutput {
                var out: VertexOutput;
                out.position = VERTS[in_vertex_index];
                out.clip_position = vec4<f32>(out.position - 0.5, 0.0, 1.0);
                return out;
            }

            @fragment
            fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                return vec4<f32>(in.position, 0.5, 1.0);
            }
            ";

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(swapchain_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let surface_config = surface
            .get_default_config(&adapter, size.physical.width, size.physical.height)
            .unwrap(); // TODO
        surface.configure(&device, &surface_config);

        Ok(Self {
            _gui_context: gui_context,
            window,
            surface: RefCell::new(Surface {
                device,
                queue,
                pipeline,
                surface,
                surface_config,
            }),
            redraw_requested: redraw,
            params,
        })
    }
}

impl baseview::WindowHandler for CustomWgpuWindow {
    fn on_frame(&self) -> Result<(), HandlerError> {
        if !self.redraw_requested.swap(false, Ordering::Relaxed) {
            return Ok(());
        }

        // Do rendering here.
        let mut surface = self.surface.borrow_mut();
        let Surface {
            device,
            queue,
            pipeline,
            surface,
            surface_config,
        } = &mut *surface;

        let mut recreate_surface = false;
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                unreachable!("No error scope registered, so validation errors will panic")
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                recreate_surface = true;
                None
            }
        };

        let Some(frame) = frame else {
            if recreate_surface {
                let instance =
                    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

                *surface = instance
                    .create_surface(self.window.platform_handle())
                    .unwrap();
            }

            surface.configure(device, surface_config);
            return Ok(());
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            rpass.set_pipeline(pipeline);
            rpass.draw(0..3, 0..1);
        }

        queue.submit(Some(encoder.finish()));
        queue.present(frame);

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
        self.params
            .editor_state
            .scale_factor
            .store(new_size.scale_factor);

        let size: LogicalSize<f32> = new_size.logical.cast();
        self.params
            .editor_state
            .logical_size
            .store((size.width, size.height));

        {
            let mut surface = self.surface.borrow_mut();
            let Surface {
                device,
                queue: _,
                pipeline: _,
                surface,
                surface_config,
            } = &mut *surface;

            surface_config.width = new_size.physical.width;
            surface_config.height = new_size.physical.height;

            surface.configure(device, surface_config);
        }

        self.redraw_requested.store(true, Ordering::Relaxed);

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CustomWgpuEditorState {
    #[serde(with = "nice_plug::params::persist::serialize_atomic_cell")]
    logical_size: AtomicCell<(f32, f32)>,
    /// Whether the editor's window is currently open.
    #[serde(skip)]
    open: AtomicBool,
    #[serde(skip)]
    scale_factor: AtomicCell<f64>,
}

impl CustomWgpuEditorState {
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

impl<'a> PersistentField<'a, CustomWgpuEditorState> for Arc<CustomWgpuEditorState> {
    fn set(&self, new_value: CustomWgpuEditorState) {
        self.logical_size.store(new_value.logical_size.load());
    }

    fn map<F, R>(&self, f: F) -> R
    where
        F: Fn(&CustomWgpuEditorState) -> R,
    {
        f(self)
    }
}

pub struct CustomWgpuEditor {
    params: Arc<MyPluginParams>,
}

impl Editor for CustomWgpuEditor {
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
                .with_title("Wgpu Window")
                .with_size(self.params.editor_state.logical_size())
                .with_parent(parent.as_ref()),
            move |window: WindowContext| -> Result<CustomWgpuWindow, HandlerError> {
                CustomWgpuWindow::new(window, gui_context, params, redraw_requested_2)
                    .map_err(|e| e.into())
            },
            host,
        )?;

        self.params.editor_state.open.store(true, Ordering::Release);

        if let Some(scale_factor) = suggested_scale_factor {
            let _ = window.suggest_fallback_scale_factor(scale_factor);
        }

        Ok(EditorWindow {
            editor: Box::new(CustomWgpuEditorInstance {
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

struct CustomWgpuEditorInstance {
    state: Arc<CustomWgpuEditorState>,
    redraw_requested: Arc<AtomicBool>,
}

impl EditorInstance for CustomWgpuEditorInstance {
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

impl Drop for CustomWgpuEditorInstance {
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
    editor_state: Arc<CustomWgpuEditorState>,

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
            editor_state: CustomWgpuEditorState::from_size(LogicalSize::new(400.0, 300.0)),

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
    const NAME: &'static str = "BYO GUI Example (WGPU)";
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
        Some(Box::new(CustomWgpuEditor {
            params: Arc::clone(&self.params),
        }))
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
                // Do stuff
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for MyPlugin {
    const CLAP_ID: &'static str = "com.moist-plugins-gmbh.byo-gui-wgpu";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A simple example plugin with a raw WGPU context for rendering");
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
    const VST3_CLASS_ID: [u8; 16] = *b"ByoGuiWGPUWooooo";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Tools];
}

nice_export_clap!(MyPlugin);
nice_export_vst3!(MyPlugin);
