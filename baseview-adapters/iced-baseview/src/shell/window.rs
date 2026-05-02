use std::time::Instant;
use std::{cell::RefCell, pin::Pin, rc::Rc};

use baseview::{Event, EventStatus, Window, WindowHandler};
use iced_core::{mouse, theme};
use iced_program::Program;
use iced_runtime::Task;
pub use iced_runtime::core::window::Id;
use iced_runtime::futures::futures::{
    self,
    channel::mpsc::{self, SendError},
};
pub use iced_runtime::window::{close_events, close_requests, events, open_events, resize_events};
use iced_widget::core::Size;
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

#[cfg(feature = "nih_log")]
use nih_plug_core::log::error;

#[cfg(all(feature = "tracing", not(feature = "nih_log")))]
use tracing::error;

use crate::graphics::Compositor;
use crate::shell::RuntimeEvent;
use crate::shell::conversion::WindowWrapper;

pub(super) mod state;

pub(super) struct InstanceWindow<P, C>
where
    P: Program,
    C: Compositor<Renderer = P::Renderer>,
    P::Theme: theme::Base,
{
    pub state: state::State<P>,
    pub mouse_interaction: mouse::Interaction,
    pub surface: C::Surface,
    pub surface_version: u64,
    pub compositor: C,
    pub renderer: P::Renderer,
    //pub preedit: Option<Preedit<P::Renderer>>,
    pub queue: WindowQueue,
    pub window06: WindowWrapper,
    pub id: iced_core::window::Id,
    #[allow(unused)]
    pub always_redraw: bool,
    pub ignore_non_modifier_keys: bool,
    pub redraw_requested: bool,
    pub redraw_at: Option<Instant>,
    //ime_state: Option<(Rectangle, input_method::Purpose)>,
}

pub(super) struct IcedWindowHandler<P: Program> {
    pub sender: mpsc::UnboundedSender<RuntimeEvent<P::Message>>,
    pub instance: Pin<Box<dyn futures::Future<Output = ()>>>,
    pub runtime_context: futures::task::Context<'static>,
    pub runtime_rx: mpsc::UnboundedReceiver<iced_runtime::Action<P::Message>>,
    pub window_queue_rx: mpsc::UnboundedReceiver<WindowCommand>,
    pub event_status: Rc<RefCell<EventStatus>>,
    pub processed_close_signal: bool,
}

impl<P> IcedWindowHandler<P>
where
    P: Program + 'static,
{
    fn drain_window_commands(&mut self, window: &mut Window<'_>) {
        while let Ok(cmd) = self.window_queue_rx.try_recv() {
            match cmd {
                WindowCommand::CloseWindow => {
                    window.close();
                }
                WindowCommand::ResizeWindow(size) => {
                    window.resize(baseview::Size {
                        width: size.width as f64,
                        height: size.height as f64,
                    });
                }
                WindowCommand::Focus => {
                    window.focus();
                }
                WindowCommand::SetCursorIcon(cursor) => {
                    #[cfg(not(target_os = "macos"))]
                    window.set_mouse_cursor(cursor);

                    #[cfg(target_os = "macos")]
                    let _ = cursor;
                }
            }
        }
    }
}

impl<P: Program + 'static> WindowHandler for IcedWindowHandler<P> {
    fn on_frame(&mut self, window: &mut Window<'_>) {
        if self.processed_close_signal {
            return;
        }

        self.sender
            .start_send(RuntimeEvent::Poll)
            .expect("Send event");

        // Flush all messages. This will block until the instance is finished.
        let _ = self.instance.as_mut().poll(&mut self.runtime_context);

        // Poll subscriptions and send the corresponding messages.
        while let Ok(message) = self.runtime_rx.try_recv() {
            self.sender
                .start_send(RuntimeEvent::UserEvent(message))
                .expect("Send event");
        }

        // Send the event to the instance.
        self.sender
            .start_send(RuntimeEvent::OnFrame)
            .expect("Send event");

        // Flush all messages. This will block until the instance is finished.
        let _ = self.instance.as_mut().poll(&mut self.runtime_context);

        self.drain_window_commands(window);
    }

    fn on_event(&mut self, window: &mut Window<'_>, event: Event) -> EventStatus {
        if self.processed_close_signal {
            return EventStatus::Ignored;
        }

        // Parent/embedded windows do not always gain keyboard focus
        // Automatically on click. Request focus explicitly before forwarding the event.
        #[cfg(not(target_os = "linux"))]
        if matches!(
            event,
            Event::Mouse(baseview::MouseEvent::ButtonPressed { .. })
        ) && !window.has_focus()
        {
            window.focus();
        }

        let status = if requests_exit(&event) {
            self.processed_close_signal = true;

            self.sender
                .start_send(RuntimeEvent::WillClose)
                .expect("Send event");

            // Flush all messages so the application receives the close event. This will block until the instance is finished.
            let _ = self.instance.as_mut().poll(&mut self.runtime_context);

            EventStatus::Ignored
        } else {
            // Send the event to the instance.
            self.sender
                .start_send(RuntimeEvent::Baseview((event, true)))
                .expect("Send event");

            // Flush all messages so the application receives the event. This will block until the instance is finished.
            let _ = self.instance.as_mut().poll(&mut self.runtime_context);

            // TODO: make this Copy
            *self.event_status.borrow()
        };

        if !self.processed_close_signal {
            self.drain_window_commands(window);
        }

        status
    }
}

/// Closes the application window.
pub fn close<T>() -> Task<T> {
    iced_runtime::window::close(Id::unique())
}

/// Resize the application window to the given logical dimensions.
pub fn resize<T>(new_size: Size) -> Task<T> {
    iced_runtime::window::resize(Id::unique(), new_size)
}

/// Brings the application window to the front and sets input focus. Has no effect if the window
/// is already in focus, minimized, or not visible.
///
/// This [`Task`] steals input focus from other applications. Do not use this method unless
/// you are certain that's what the user wants. Focus stealing can cause an extremely disruptive
/// user experience.
pub fn gain_focus<T>() -> Task<T> {
    iced_runtime::window::gain_focus(Id::unique())
}

/// Returns true if the provided event should cause an [`Application`] to
/// exit.
pub fn requests_exit(event: &baseview::Event) -> bool {
    match event {
        baseview::Event::Window(baseview::WindowEvent::WillClose) => true,
        #[cfg(target_os = "macos")]
        baseview::Event::Keyboard(event) => {
            if event.code == keyboard_types::Code::KeyQ
                && event.modifiers == keyboard_types::Modifiers::META
                && event.state == keyboard_types::KeyState::Down
            {
                return true;
            }

            false
        }
        _ => false,
    }
}

/// Use this to send custom events to the iced window.
///
/// Please note this channel is ***not*** realtime-safe and should never be
/// be used to send events from the audio thread. Use a realtime-safe ring
/// buffer instead.
#[allow(missing_debug_implementations)]
pub struct WindowHandle<Message: 'static + Send> {
    bv_handle: baseview::WindowHandle,
    tx: mpsc::UnboundedSender<RuntimeEvent<Message>>,
}

impl<Message: 'static + Send> WindowHandle<Message> {
    pub(crate) fn new(
        bv_handle: baseview::WindowHandle,
        tx: mpsc::UnboundedSender<RuntimeEvent<Message>>,
    ) -> Self {
        Self { bv_handle, tx }
    }

    /// Send a custom `baseview::Event` to the window.
    ///
    /// Please note this channel is ***not*** realtime-safe and should never be
    /// be used to send events from the audio thread. Use a realtime-safe ring
    /// buffer instead.
    pub fn send_baseview_event(&mut self, event: baseview::Event) -> Result<(), SendError> {
        self.tx.start_send(RuntimeEvent::Baseview((event, false)))
    }

    /// Send a custom message to the window.
    ///
    /// Please note this channel is ***not*** realtime-safe and should never be
    /// used to send events from the audio thread. Use a realtime-safe ring
    /// buffer instead.
    pub fn send_message(&mut self, msg: Message) -> Result<(), SendError> {
        self.tx
            .start_send(RuntimeEvent::UserEvent(iced_runtime::Action::Output(msg)))
    }

    /// Signal the window to close.
    pub fn close_window(&mut self) {
        self.bv_handle.close();
    }

    /// Returns `true` if the window is still open, and `false` if the window
    /// was closed/dropped.
    pub fn is_open(&self) -> bool {
        self.bv_handle.is_open()
    }
}

impl<Message: 'static + Send> Drop for WindowHandle<Message> {
    fn drop(&mut self) {
        self.close_window();
    }
}

unsafe impl<Message: 'static + Send> HasRawWindowHandle for WindowHandle<Message> {
    fn raw_window_handle(&self) -> RawWindowHandle {
        self.bv_handle.raw_window_handle()
    }
}

pub enum WindowCommand {
    CloseWindow,
    ResizeWindow(crate::core::Size),
    Focus,
    SetCursorIcon(baseview::MouseCursor),
}

/// Used to request things from the `baseview` window.
pub struct WindowQueue {
    tx: mpsc::UnboundedSender<WindowCommand>,
}

impl WindowQueue {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<WindowCommand>) {
        let (tx, rx) = mpsc::unbounded();

        (Self { tx }, rx)
    }

    pub fn send(&mut self, command: WindowCommand) {
        if let Err(e) = self.tx.start_send(command) {
            error!("Failed to send command to window: {}", e);
        }
    }
}

/*
pub(super) struct Preedit<Renderer>
where
    Renderer: text::Renderer,
{
    position: Point,
    content: Renderer::Paragraph,
    spans: Vec<text::Span<'static, (), Renderer::Font>>,
}

impl<Renderer> Preedit<Renderer>
where
    Renderer: text::Renderer,
{
    fn new() -> Self {
        Self {
            position: Point::ORIGIN,
            spans: Vec::new(),
            content: Renderer::Paragraph::default(),
        }
    }

    fn update(
        &mut self,
        cursor: Rectangle,
        preedit: &input_method::Preedit,
        background: Color,
        renderer: &Renderer,
    ) {
        self.position = cursor.position() + Vector::new(0.0, cursor.height);

        let background = Color {
            a: 1.0,
            ..background
        };

        let spans = match &preedit.selection {
            Some(selection) => {
                vec![
                    text::Span::new(&preedit.content[..selection.start]),
                    text::Span::new(if selection.start == selection.end {
                        "\u{200A}"
                    } else {
                        &preedit.content[selection.start..selection.end]
                    })
                    .color(background),
                    text::Span::new(&preedit.content[selection.end..]),
                ]
            }
            _ => vec![text::Span::new(&preedit.content)],
        };

        if spans != self.spans.as_slice() {
            use text::Paragraph as _;

            self.content = Renderer::Paragraph::with_spans(Text {
                content: &spans,
                bounds: Size::INFINITE,
                size: preedit.text_size.unwrap_or_else(|| renderer.default_size()),
                line_height: text::LineHeight::default(),
                font: renderer.default_font(),
                align_x: text::Alignment::Default,
                align_y: alignment::Vertical::Top,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::None,
            });

            self.spans.clear();
            self.spans
                .extend(spans.into_iter().map(text::Span::to_static));
        }
    }

    pub fn draw(
        &self,
        renderer: &mut Renderer,
        color: Color,
        background: Color,
        viewport: &Rectangle,
    ) {
        use text::Paragraph as _;

        if self.content.min_width() < 1.0 {
            return;
        }

        let mut bounds = Rectangle::new(
            self.position - Vector::new(0.0, self.content.min_height()),
            self.content.min_bounds(),
        );

        bounds.x = bounds
            .x
            .max(viewport.x)
            .min(viewport.x + viewport.width - bounds.width);

        bounds.y = bounds
            .y
            .max(viewport.y)
            .min(viewport.y + viewport.height - bounds.height);

        renderer.with_layer(bounds, |renderer| {
            let background = Color {
                a: 1.0,
                ..background
            };

            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..Default::default()
                },
                background,
            );

            renderer.fill_paragraph(&self.content, bounds.position(), color, bounds);

            const UNDERLINE: f32 = 2.0;

            renderer.fill_quad(
                renderer::Quad {
                    bounds: bounds.shrink(Padding {
                        top: bounds.height - UNDERLINE,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                color,
            );

            for span_bounds in self.content.span_bounds(1) {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: span_bounds + (bounds.position() - Point::ORIGIN),
                        ..Default::default()
                    },
                    color,
                );
            }
        });
    }
}
*/
