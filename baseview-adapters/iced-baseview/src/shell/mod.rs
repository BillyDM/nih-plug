use baseview::{EventStatus, WindowOpenOptions};
use iced_core::widget::operation;
use iced_core::window::RedrawRequest;
use iced_core::{Point, Size, mouse, renderer, theme};
use iced_futures::futures::{StreamExt, task};
use iced_futures::{Executor, Subscription};
use iced_futures::{Runtime, futures::channel::mpsc, subscription};
use iced_program::Program;
use iced_runtime::{Action, UserInterface, user_interface};
use iced_widget::graphics::Viewport;
use raw_window_handle::HasRawWindowHandle;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::{cell::RefCell, rc::Rc};

#[cfg(feature = "nih_log")]
use nih_plug_core::log::{debug, error, warn};

#[cfg(all(feature = "tracing", not(feature = "nih_log")))]
use tracing::{debug, error, warn};

use crate::Error;
use crate::graphics::{Compositor, compositor};

mod proxy;

#[cfg(feature = "sysinfo")]
mod system;

pub mod clipboard;
pub mod conversion;
pub mod settings;
pub mod window;

use clipboard::Clipboard;
use settings::IcedBaseviewSettings;
use window::{
    IcedWindowHandler, InstanceWindow, WindowCommand, WindowHandle, WindowQueue,
    state::State as WindowState,
};

pub use proxy::Proxy;

/// An atomic flag used to notify the program when it should poll for new updates
/// and redraw (i.e. as a result of the host updating parameters or the audio thread
/// updating the state of meters). This flag is polled every frame right before
/// drawing. If the flag is set then the [`poll_events`] subscription will be called.
#[derive(Debug, Clone)]
pub struct PollSubNotifier {
    notify: Arc<AtomicBool>,
}

impl PollSubNotifier {
    pub fn new() -> Self {
        Self {
            notify: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn notify(&self) {
        self.notify.store(true, Ordering::Relaxed);
    }

    pub(crate) fn notify_flag_set(&self) -> bool {
        self.notify.swap(false, Ordering::Relaxed)
    }
}

impl Default for PollSubNotifier {
    fn default() -> Self {
        Self::new()
    }
}

// This is a bit hacky, but Iced doesn't let you define custom subscription
// events. So instead, we use an event that is never used in baseview.
//
// TODO: Ask Iced team to add custom subscription events.
const POLL_EVENT: iced_core::Event =
    iced_core::Event::Window(iced_core::window::Event::Moved(Point::ORIGIN));

/// A subscription which notifies the program when it should poll for new updates
/// and redraw (i.e. as a result of the host updating parameters or the audio thread
/// updating the state of meters).
pub fn poll_events() -> Subscription<()> {
    iced_futures::event::listen_raw(|event, _status, _window| match event {
        POLL_EVENT => Some(()),
        _ => None,
    })
}

/// Open a new window that blocks the current thread until the window is destroyed.
///
/// * `settings` - The settings of the window.
/// * `notifier` - An atomic flag used to notify the program when it should
/// poll for new updates and redraw (i.e. as a result of the host updating parameters
/// or the audio thread updating the state of meters). This flag is polled every frame
/// right before drawing. If the flag is set then the [`poll_events`] subscription
/// will be called.
/// * `build_program` - The function which builds the Iced program.
pub fn open_blocking<P, B>(
    settings: IcedBaseviewSettings,
    notifier: PollSubNotifier,
    build_program: B,
) where
    P: Program + 'static,
    B: Send + 'static + FnOnce() -> P,
{
    let (sender, receiver) = mpsc::unbounded();

    baseview::Window::open_blocking(
        clone_window_options(&settings.window),
        move |window: &mut baseview::Window<'_>| -> IcedWindowHandler<P> {
            let program = (build_program)();
            run_inner(window, settings, program, sender, receiver, notifier).expect("Launch window")
        },
    );
}

/// Open a new child window.
///
/// * `parent` - The parent window.
/// * `settings` - The settings of the window.
/// * `notifier` - An atomic flag used to notify the program when it should
/// poll for new updates and redraw (i.e. as a result of the host updating parameters
/// or the audio thread updating the state of meters). This flag is polled every frame
/// right before drawing. If the flag is set then the [`poll_events`] subscription
/// will be called.
/// * `build_program` - The functio which builds the Iced program.
pub fn open_parented<W, P, B>(
    parent: &W,
    settings: IcedBaseviewSettings,
    notifier: PollSubNotifier,
    build_program: B,
) -> WindowHandle<P::Message>
where
    W: HasRawWindowHandle,
    P: Program + 'static,
    B: Send + 'static + FnOnce() -> P,
{
    let (sender, receiver) = mpsc::unbounded();
    let sender_clone = sender.clone();

    let bv_handle = baseview::Window::open_parented(
        parent,
        clone_window_options(&settings.window),
        move |window: &mut baseview::Window<'_>| -> IcedWindowHandler<P> {
            let program = (build_program)();
            run_inner(window, settings, program, sender_clone, receiver, notifier)
                .expect("Launch window")
        },
    );

    WindowHandle::new(bv_handle, sender)
}

/// Runs a [`Program`] with the provided settings.
fn run_inner<P>(
    window: &mut baseview::Window<'_>,
    settings: IcedBaseviewSettings,
    program: P,
    event_sender: mpsc::UnboundedSender<RuntimeEvent<P::Message>>,
    event_receiver: mpsc::UnboundedReceiver<RuntimeEvent<P::Message>>,
    notifier: PollSubNotifier,
) -> Result<IcedWindowHandler<P>, Error>
where
    P: Program + 'static,
    P::Theme: theme::Base,
{
    let boot_span = iced_debug::boot();
    let program_settings = program.settings();
    let graphics_settings: iced_widget::graphics::Settings = program_settings.clone().into();

    let (runtime_tx, runtime_rx) = mpsc::unbounded::<Action<P::Message>>();

    // Assume scale for now until there is an event with a new one.
    let window_scale_factor = 1.0;

    let viewport = {
        let scale = match settings.window.scale {
            baseview::WindowScalePolicy::ScaleFactor(scale) => scale,
            baseview::WindowScalePolicy::SystemScaleFactor => window_scale_factor,
        };

        let physical_size = Size::new(
            (settings.window.size.width * scale) as u32,
            (settings.window.size.height * scale) as u32,
        );

        Viewport::with_physical_size(physical_size, scale as f32)
    };

    let proxy = Proxy::new(runtime_tx);

    #[cfg(feature = "debug")]
    {
        let proxy = proxy.clone();

        iced_debug::on_hotpatch(move || {
            let _ = proxy.send_action(Action::Reload);
        });
    }

    let mut runtime = {
        let executor = P::Executor::new().map_err(Error::ExecutorCreationFailed)?;

        Runtime::new(executor, proxy.clone())
    };

    let (program, init_task) = runtime.enter(|| iced_program::Instance::new(program));

    if let Some(stream) = iced_runtime::task::into_stream(init_task) {
        runtime.run(stream);
    }

    runtime.track(iced_futures::subscription::into_recipes(
        runtime.enter(|| program.subscription().map(Action::Output)),
    ));

    let window06 = crate::shell::conversion::convert_window(window);
    let mut compositor =
        crate::futures::executor::block_on(<P::Renderer as compositor::Default>::Compositor::new(
            graphics_settings,
            window06.clone(),
            window06.clone(),
            crate::graphics::Shell::new(proxy.clone()),
        ))?;
    let surface = compositor.create_surface(
        window06.clone(),
        viewport.physical_size().width,
        viewport.physical_size().height,
    );
    let renderer = compositor.create_renderer();

    for font in program_settings.fonts {
        compositor.load_font(font);
    }

    let (window_queue, window_queue_rx) = WindowQueue::new();
    let event_status = Rc::new(RefCell::new(baseview::EventStatus::Ignored));

    let window_id = iced_core::window::Id::unique();

    let clipboard = unsafe { Clipboard::connect(&window06) };

    let state = WindowState::new(
        &program,
        window_id,
        viewport.physical_size(),
        window_scale_factor as f32,
        settings.window.scale,
        iced_core::theme::Mode::None,
    );
    let surface_version = state.surface_version();

    let instance_window = InstanceWindow {
        state,
        mouse_interaction: mouse::Interaction::default(),
        surface,
        surface_version,
        compositor,
        renderer,
        //preedit: None,
        queue: window_queue,
        window06,
        id: window_id,
        always_redraw: settings.always_redraw,
        ignore_non_modifier_keys: settings.ignore_non_modifier_keys,
        redraw_requested: true,
        redraw_at: None,
    };

    let instance = Box::pin({
        let run_instance = run_instance(
            program,
            runtime,
            proxy,
            instance_window,
            event_receiver,
            clipboard,
            Rc::clone(&event_status),
            notifier,
        );
        run_instance
    });

    let runtime_context = task::Context::from_waker(task::noop_waker_ref());

    boot_span.finish();

    Ok(IcedWindowHandler {
        sender: event_sender,
        instance,
        runtime_context,
        runtime_rx,
        window_queue_rx,
        event_status,
        processed_close_signal: false,
    })
}

async fn run_instance<P>(
    mut program: iced_program::Instance<P>,
    mut runtime: Runtime<P::Executor, Proxy<P::Message>, Action<P::Message>>,
    proxy: Proxy<P::Message>,
    mut window: InstanceWindow<P, <P::Renderer as compositor::Default>::Compositor>,
    mut event_receiver: mpsc::UnboundedReceiver<RuntimeEvent<P::Message>>,
    mut clipboard: Clipboard,
    event_status: Rc<RefCell<baseview::EventStatus>>,
    notifier: PollSubNotifier,
) where
    P: Program + 'static,
    P::Theme: theme::Base,
{
    window.surface_version = window.state.surface_version();

    let cache = iced_runtime::user_interface::Cache::default();
    let mut events: Vec<(iced_core::window::Id, iced_core::Event)> = Vec::new();
    let mut messages = Vec::new();
    let mut system_theme = theme::Mode::None;

    let mut interface = ManuallyDrop::new(Some(build_user_interface(
        &program,
        cache,
        &mut window.renderer,
        window.state.logical_size(),
        window.id,
    )));

    window.mouse_interaction = mouse::Interaction::default();

    // Triggered whenever a baseview event gets sent
    window.redraw_requested = true;
    window.redraw_at = None;
    // May be triggered when processing baseview events, will cause the UI to be updated in the next
    // frame
    let mut did_process_event = false;

    'next_event: loop {
        // Empty the queue if possible
        let event = if let Ok(event) = event_receiver.try_recv() {
            Some(event)
        } else {
            event_receiver.next().await
        };

        let Some(event) = event else {
            break;
        };

        match event {
            RuntimeEvent::Poll => {
                if notifier.notify_flag_set() {
                    runtime.broadcast(iced_futures::subscription::Event::Interaction {
                        window: window.id,
                        event: POLL_EVENT,
                        status: iced_core::event::Status::Ignored,
                    });
                }
            }
            RuntimeEvent::OnFrame => {
                #[cfg(feature = "unconditional-rendering")]
                {
                    window.redraw_requested = true;
                }

                #[cfg(not(feature = "unconditional-rendering"))]
                if window.always_redraw {
                    window.redraw_requested = true;
                }

                if !window.redraw_requested
                    && !did_process_event
                    && events.is_empty()
                    && messages.is_empty()
                {
                    continue 'next_event;
                }
                did_process_event = false;

                let mut uis_stale = false;

                let interact_span = iced_debug::interact(window.id);
                let mut window_events = vec![];

                events.retain(|(event_window_id, event)| {
                    if *event_window_id == window.id {
                        window_events.push(event.clone());
                        false
                    } else {
                        true
                    }
                });

                if window_events.is_empty() && messages.is_empty() && !window.redraw_requested {
                    continue 'next_event;
                }

                let Some(ui) = interface.as_mut() else {
                    continue 'next_event;
                };

                let (ui_state, statuses) = ui.update(
                    &window_events,
                    window.state.cursor(),
                    &mut window.renderer,
                    &mut clipboard,
                    &mut messages,
                );

                match ui_state {
                    user_interface::State::Updated {
                        redraw_request: _redraw_request,
                        mouse_interaction,
                        ..
                    } => {
                        window.queue.send(WindowCommand::SetCursorIcon(
                            crate::shell::conversion::convert_mouse_interaction(mouse_interaction),
                        ));

                        #[cfg(not(feature = "unconditional-rendering"))]
                        if !window.always_redraw {
                            match _redraw_request {
                                RedrawRequest::NextFrame => {
                                    window.redraw_requested = true;
                                }
                                RedrawRequest::At(at) => {
                                    window.redraw_at = Some(at);
                                }
                                RedrawRequest::Wait => {}
                            }
                        }
                    }
                    user_interface::State::Outdated => {
                        uis_stale = true;
                    }
                }

                for (event, status) in window_events.into_iter().zip(statuses.into_iter()) {
                    runtime.broadcast(subscription::Event::Interaction {
                        window: window.id,
                        event,
                        status,
                    });
                }

                interact_span.finish();

                for (id, event) in events.drain(..) {
                    runtime.broadcast(subscription::Event::Interaction {
                        window: id,
                        event,
                        status: iced_core::event::Status::Ignored,
                    });
                }

                if !messages.is_empty() || uis_stale {
                    let cached_interface =
                        ManuallyDrop::into_inner(interface).unwrap().into_cache();

                    let actions = update(&mut program, &mut runtime, &mut messages);

                    interface = ManuallyDrop::new(Some(rebuild_user_interface(
                        &program,
                        &mut window,
                        cached_interface,
                    )));

                    for action in actions {
                        run_action(
                            action,
                            &program,
                            &mut runtime,
                            &mut window,
                            &mut messages,
                            &mut clipboard,
                            &mut interface,
                            &mut system_theme,
                        );
                    }

                    window.redraw_requested = true;
                }

                // -- Draw --------------------------------------------------------------------

                if let Some(redraw_at) = window.redraw_at {
                    if redraw_at <= Instant::now() {
                        window.redraw_requested = true;
                        window.redraw_at = None;
                    }
                }

                if window.surface_version != window.state.surface_version() {
                    window.redraw_requested = true;
                }

                if !window.redraw_requested {
                    continue 'next_event;
                }

                let physical_size = window.state.physical_size();
                let mut logical_size = window.state.logical_size();

                if physical_size.width == 0 || physical_size.height == 0 {
                    continue 'next_event;
                }

                // Window was resized between redraws
                if window.surface_version != window.state.surface_version() {
                    let ui = interface.take().expect("Remove user interface");

                    let layout_span = iced_debug::layout(window.id);
                    *interface = Some(ui.relayout(logical_size, &mut window.renderer));
                    layout_span.finish();

                    window.compositor.configure_surface(
                        &mut window.surface,
                        physical_size.width,
                        physical_size.height,
                    );

                    window.surface_version = window.state.surface_version();
                }

                let redraw_event = iced_core::Event::Window(
                    iced_core::window::Event::RedrawRequested(Instant::now()),
                );

                let cursor = window.state.cursor();

                assert!(interface.is_some(), "Get user interface");

                let interact_span = iced_debug::interact(window.id);
                let mut redraw_count = 0;

                let state = loop {
                    let message_count = messages.len();
                    let (state, _) = interface.as_mut().unwrap().update(
                        core::slice::from_ref(&redraw_event),
                        cursor,
                        &mut window.renderer,
                        &mut clipboard,
                        &mut messages,
                    );

                    if message_count == messages.len() && !state.has_layout_changed() {
                        break state;
                    }

                    if redraw_count >= 2 {
                        warn!(
                            "More than 3 consecutive RedrawRequested events produced layout \
                             invalidation"
                        );

                        break state;
                    }

                    redraw_count += 1;

                    if !messages.is_empty() {
                        let cache = ManuallyDrop::into_inner(interface).unwrap().into_cache();

                        let actions = update(&mut program, &mut runtime, &mut messages);

                        interface = ManuallyDrop::new(Some(rebuild_user_interface(
                            &program,
                            &mut window,
                            cache,
                        )));

                        for action in actions {
                            // Defer all window actions to avoid compositor
                            // race conditions while redrawing
                            if let Action::Window(_) = action {
                                proxy.send_action(action);
                                continue;
                            }

                            run_action(
                                action,
                                &program,
                                &mut runtime,
                                &mut window,
                                &mut messages,
                                &mut clipboard,
                                &mut interface,
                                &mut system_theme,
                            );
                        }

                        // Window scale factor changed during a redraw request
                        if logical_size != window.state.logical_size() {
                            logical_size = window.state.logical_size();

                            debug!("Window scale factor changed during a redraw request");

                            let ui = interface.take().expect("Remove user interface");

                            let layout_span = iced_debug::layout(window.id);
                            *interface = Some(ui.relayout(logical_size, &mut window.renderer));
                            layout_span.finish();
                        }
                    }
                };
                interact_span.finish();

                let draw_span = iced_debug::draw(window.id);
                interface.as_mut().unwrap().draw(
                    &mut window.renderer,
                    window.state.theme(),
                    &renderer::Style {
                        text_color: window.state.text_color(),
                    },
                    cursor,
                );
                window.redraw_requested = false;
                draw_span.finish();

                if let user_interface::State::Updated {
                    redraw_request,
                    mouse_interaction: new_mouse_interaction,
                    ..
                } = state
                {
                    match redraw_request {
                        RedrawRequest::NextFrame => window.redraw_requested = true,
                        RedrawRequest::At(instant) => window.redraw_at = Some(instant),
                        _ => {}
                    }

                    if window.mouse_interaction != new_mouse_interaction {
                        window.mouse_interaction = new_mouse_interaction;
                        window.queue.send(WindowCommand::SetCursorIcon(
                            crate::shell::conversion::convert_mouse_interaction(
                                window.mouse_interaction,
                            ),
                        ));
                    }
                }

                runtime.broadcast(subscription::Event::Interaction {
                    window: window.id,
                    event: redraw_event,
                    status: iced_core::event::Status::Ignored,
                });

                /*
                if let Some(preedit) = &window.preedit {
                    preedit.draw(
                        &mut window.renderer,
                        window.state.text_color(),
                        window.state.background_color(),
                        &Rectangle::new(Point::ORIGIN, window.state.viewport().logical_size()),
                    );
                }
                */

                let present_span = iced_debug::present(window.id);
                match window.compositor.present(
                    &mut window.renderer,
                    &mut window.surface,
                    window.state.viewport(),
                    window.state.background_color(),
                    || {},
                ) {
                    Ok(()) => {
                        present_span.finish();
                    }
                    Err(error) => match error {
                        compositor::SurfaceError::OutOfMemory => {
                            // This is an unrecoverable error.
                            panic!("{error:?}");
                        }
                        compositor::SurfaceError::Outdated | compositor::SurfaceError::Lost => {
                            present_span.finish();

                            // Reconfigure surface and try redrawing
                            let physical_size = window.state.physical_size();

                            if error == compositor::SurfaceError::Lost {
                                window.surface = window.compositor.create_surface(
                                    window.window06.clone(),
                                    physical_size.width,
                                    physical_size.height,
                                );
                            } else {
                                window.compositor.configure_surface(
                                    &mut window.surface,
                                    physical_size.width,
                                    physical_size.height,
                                );
                            }

                            window.redraw_requested = true;
                        }
                        _ => {
                            present_span.finish();

                            error!("Error {error:?} when presenting surface.");

                            // Try rendering all windows again next frame.
                            window.redraw_requested = true;
                        }
                    },
                }
            }
            RuntimeEvent::UserEvent(message) => {
                run_action(
                    message,
                    &program,
                    &mut runtime,
                    &mut window,
                    &mut messages,
                    &mut clipboard,
                    &mut interface,
                    &mut system_theme,
                );
            }
            RuntimeEvent::Baseview((event, do_send_status)) => {
                window.state.update(&event);

                match &event {
                    baseview::Event::Window(baseview::WindowEvent::Focused)
                    | baseview::Event::Window(baseview::WindowEvent::Unfocused) => {
                        window.redraw_requested = true;
                    }
                    _ => {}
                }

                crate::shell::conversion::baseview_to_iced_events(
                    event,
                    &mut events,
                    &mut window.state.modifiers,
                    window.ignore_non_modifier_keys,
                    window.id,
                );

                if events.is_empty() {
                    if do_send_status {
                        *event_status.borrow_mut() = EventStatus::Ignored;
                    }
                    continue;
                }

                did_process_event = true;
            }
            RuntimeEvent::WillClose => {
                run_action(
                    Action::Window(iced_runtime::window::Action::Close(window.id)),
                    &program,
                    &mut runtime,
                    &mut window,
                    &mut messages,
                    &mut clipboard,
                    &mut interface,
                    &mut system_theme,
                );

                break 'next_event;
            }
        }
    }

    // Manually drop the user interface
    let _ = ManuallyDrop::into_inner(interface);
}

fn rebuild_user_interface<'a, P: Program>(
    program: &'a iced_program::Instance<P>,
    window: &mut InstanceWindow<P, <P::Renderer as compositor::Default>::Compositor>,
    cache: user_interface::Cache,
) -> UserInterface<'a, P::Message, P::Theme, P::Renderer>
where
    P::Theme: theme::Base,
{
    window.state.synchronize(program, window.id);

    iced_debug::theme_changed(|| theme::Base::palette(window.state.theme()));

    build_user_interface(
        program,
        cache,
        &mut window.renderer,
        window.state.logical_size(),
        window.id,
    )
}

/// Builds a window's [`UserInterface`] for the [`Program`].
fn build_user_interface<'a, P: Program>(
    program: &'a iced_program::Instance<P>,
    cache: user_interface::Cache,
    renderer: &mut P::Renderer,
    size: Size,
    id: iced_core::window::Id,
) -> UserInterface<'a, P::Message, P::Theme, P::Renderer>
where
    P::Theme: theme::Base,
{
    let view_span = iced_debug::view(id);
    let view = program.view(id);
    view_span.finish();

    let layout_span = iced_debug::layout(id);
    let user_interface = UserInterface::build(view, size, cache, renderer);
    layout_span.finish();

    user_interface
}

fn update<P: Program, E: Executor>(
    program: &mut iced_program::Instance<P>,
    runtime: &mut Runtime<E, Proxy<P::Message>, Action<P::Message>>,
    messages: &mut Vec<P::Message>,
) -> Vec<Action<P::Message>>
where
    P::Theme: theme::Base,
{
    use iced_futures::futures::{self, StreamExt};

    let mut actions = Vec::new();

    for message in messages.drain(..) {
        let task = runtime.enter(|| program.update(message));

        if let Some(mut stream) = iced_runtime::task::into_stream(task) {
            let waker = futures::task::noop_waker_ref();
            let mut context = futures::task::Context::from_waker(waker);

            // Run immediately available actions synchronously (e.g. widget operations)
            loop {
                match runtime.enter(|| stream.poll_next_unpin(&mut context)) {
                    futures::task::Poll::Ready(Some(action)) => {
                        actions.push(action);
                    }
                    futures::task::Poll::Ready(None) => {
                        break;
                    }
                    futures::task::Poll::Pending => {
                        runtime.run(stream);
                        break;
                    }
                }
            }
        }
    }

    let subscription = runtime.enter(|| program.subscription());
    let recipes = subscription::into_recipes(subscription.map(Action::Output));

    runtime.track(recipes);

    actions
}

fn log_unsupported_window_action(command: &str) {
    warn!(
        "window::Action::{} is not supported in the baseview backend",
        command
    );
}

fn run_action<'a, P>(
    action: Action<P::Message>,
    program: &'a iced_program::Instance<P>,
    runtime: &mut Runtime<P::Executor, Proxy<P::Message>, Action<P::Message>>,
    window: &mut InstanceWindow<P, <P::Renderer as compositor::Default>::Compositor>,
    messages: &mut Vec<P::Message>,
    clipboard: &mut Clipboard,
    user_interface: &mut Option<UserInterface<'a, P::Message, P::Theme, P::Renderer>>,
    system_theme: &mut theme::Mode,
) where
    P: Program,
    P::Theme: theme::Base,
{
    use crate::runtime::clipboard;
    use crate::runtime::window;

    match action {
        Action::Output(message) => {
            messages.push(message);
        }
        Action::Clipboard(action) => match action {
            clipboard::Action::Read { target, channel } => {
                let _ = channel.send(clipboard.read(target));
            }
            clipboard::Action::Write { target, contents } => {
                clipboard.write(target, contents);
            }
        },
        Action::Window(action) => match action {
            window::Action::Open(..) => {
                log_unsupported_window_action("Open");
            }
            window::Action::Close(..) => {
                window.queue.send(WindowCommand::CloseWindow);
            }
            window::Action::GetOldest(channel) => {
                let _ = channel.send(Some(window.id));
            }
            window::Action::GetLatest(channel) => {
                let _ = channel.send(Some(window.id));
            }
            window::Action::Drag(..) => {
                log_unsupported_window_action("Drag");
            }
            window::Action::DragResize(..) => {
                log_unsupported_window_action("DragResize");
            }
            window::Action::Resize(_, size) => {
                window.queue.send(WindowCommand::ResizeWindow(size));
            }
            window::Action::SetMinSize(..) => {
                log_unsupported_window_action("SetMinSize");
            }
            window::Action::SetMaxSize(..) => {
                log_unsupported_window_action("SetMaxSize");
            }
            window::Action::SetResizeIncrements(..) => {
                log_unsupported_window_action("SetResizeIncrements");
            }
            window::Action::SetResizable(..) => {
                log_unsupported_window_action("SetResizable");
            }
            window::Action::GetSize(_, channel) => {
                let _ = channel.send(window.state.logical_size());
            }
            window::Action::GetMaximized(..) => {
                log_unsupported_window_action("GetMaximized");
            }
            window::Action::Maximize(..) => {
                log_unsupported_window_action("Maximize");
            }
            window::Action::GetMinimized(..) => {
                log_unsupported_window_action("GetMinimized");
            }
            window::Action::Minimize(..) => {
                log_unsupported_window_action("Minimize");
            }
            window::Action::GetPosition(..) => {
                log_unsupported_window_action("GetPosition");
            }
            window::Action::GetScaleFactor(_, channel) => {
                let _ = channel.send(window.state.window_scale_factor());
            }
            window::Action::Move(..) => {
                log_unsupported_window_action("Move");
            }
            window::Action::SetMode(..) => {
                log_unsupported_window_action("SetMode");
            }
            window::Action::SetIcon(..) => {
                log_unsupported_window_action("SetIcon");
            }
            window::Action::GetMode(..) => {
                log_unsupported_window_action("GetMode");
            }
            window::Action::ToggleMaximize(..) => {
                log_unsupported_window_action("ToggleMaximize");
            }
            window::Action::ToggleDecorations(..) => {
                log_unsupported_window_action("ToggleDecorations");
            }
            window::Action::RequestUserAttention(..) => {
                log_unsupported_window_action("RequestUserAttention");
            }
            window::Action::GainFocus(..) => {
                window.queue.send(WindowCommand::Focus);
            }
            window::Action::SetLevel(..) => {
                log_unsupported_window_action("SetLevel");
            }
            window::Action::ShowSystemMenu(..) => {
                log_unsupported_window_action("ShowSystemMenu");
            }
            window::Action::GetRawId(..) => {
                log_unsupported_window_action("GetRawId");
            }
            window::Action::Run(_, f) => {
                (f)(&window.window06);
            }
            window::Action::Screenshot(..) => {
                log_unsupported_window_action("Screenshot");
            }
            window::Action::EnableMousePassthrough(..) => {
                log_unsupported_window_action("EnableMousePassthrough");
            }
            window::Action::DisableMousePassthrough(..) => {
                log_unsupported_window_action("DisableMousePassthrough");
            }
            window::Action::GetMonitorSize(..) => {
                log_unsupported_window_action("GetMonitorSize");
            }
            window::Action::SetAllowAutomaticTabbing(..) => {
                log_unsupported_window_action("SetAllowAutomaticTabbing");
            }
            window::Action::RedrawAll => {
                window.redraw_requested = true;
            }
            window::Action::RelayoutAll => {
                if let Some(ui) = user_interface.take() {
                    *user_interface =
                        Some(ui.relayout(window.state.logical_size(), &mut window.renderer));
                }

                window.redraw_requested = true;
            }
        },
        Action::System(action) => match action {
            iced_runtime::system::Action::GetInformation(_channel) => {
                #[cfg(feature = "sysinfo")]
                {
                    let graphics_info = window.compositor.information();

                    let _ = std::thread::spawn(move || {
                        let information = crate::shell::system::system_information(graphics_info);

                        let _ = _channel.send(information);
                    });
                }
            }
            iced_runtime::system::Action::GetTheme(channel) => {
                let _ = channel.send(*system_theme);
            }
            iced_runtime::system::Action::NotifyTheme(mode) => {
                if mode != *system_theme {
                    *system_theme = mode;

                    runtime.broadcast(subscription::Event::SystemThemeChanged(mode));
                }

                window.state.set_system_theme(window.id, mode, program);
            }
        },
        Action::Widget(operation) => {
            let mut current_operation = Some(operation);

            while let Some(mut operation) = current_operation.take() {
                if let Some(ui) = user_interface.as_mut() {
                    ui.operate(&window.renderer, operation.as_mut());
                }

                match operation.finish() {
                    operation::Outcome::None => {}
                    operation::Outcome::Some(()) => {}
                    operation::Outcome::Chain(next) => {
                        current_operation = Some(next);
                    }
                }
            }
        }
        Action::Image(action) => match action {
            iced_runtime::image::Action::Allocate(handle, sender) => {
                use iced_core::Renderer as _;

                // TODO: Shared image cache in compositor
                window.renderer.allocate_image(&handle, move |allocation| {
                    let _ = sender.send(allocation);
                });
            }
        },
        Action::LoadFont { bytes, channel } => {
            // TODO: Error handling (?)
            window.compositor.load_font(bytes.clone());

            let _ = channel.send(Ok(()));
        }
        Action::Reload => {
            let Some(cached_interface) = user_interface.take().map(|ui| ui.into_cache()) else {
                return;
            };

            *user_interface = Some(build_user_interface(
                &program,
                cached_interface,
                &mut window.renderer,
                window.state.logical_size(),
                window.id,
            ));

            window.redraw_requested = true;
        }
        Action::Exit => {
            window.queue.send(WindowCommand::CloseWindow);
        }
    }
}

pub(crate) enum RuntimeEvent<Message: 'static + Send> {
    Baseview((baseview::Event, bool)),
    UserEvent(iced_runtime::Action<Message>),
    Poll,
    OnFrame,
    WillClose,
}

fn clone_window_options(window: &WindowOpenOptions) -> WindowOpenOptions {
    WindowOpenOptions {
        title: window.title.clone(),
        size: window.size,
        scale: window.scale,
        #[cfg(feature = "rustanalyzer_opengl_workaround")]
        gl_config: None,
    }
}
