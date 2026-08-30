use atomic_refcell::AtomicRefMut;
use nice_plug_core::{
    context::{
        PluginApi,
        activate::ActivateContext,
        process::{ProcessContext, Transport},
    },
    midi::PluginNoteEvent,
};
use std::cell::Cell;
use std::collections::VecDeque;

#[cfg(feature = "editor")]
use vst3::Steinberg::Vst::IComponentHandlerTrait;

#[cfg(feature = "editor")]
use nice_plug_core::{
    context::gui::GuiContextInner, params::internals::ParamPtr, plugin::PluginState,
};

use crate::wrapper::vst3::Vst3Plugin;

use super::inner::{Task, WrapperInner};

/// An [`ActivateContext`] implementation for the wrapper.
///
/// # Note
///
/// Requests to change the latency are only sent when this object is dropped. Otherwise there's the
/// risk that the host will immediately deactivate/reactivate the plugin while still in the init
/// call. Reentrannt function calls are difficult to handle in Rust without forcing everything to
/// use interior mutability, so this will have to do for now. This does mean that `Plugin` mutex
/// lock has to be dropped before this object.
pub(crate) struct WrapperActivateContext<'a, P: Vst3Plugin> {
    pub(super) inner: &'a WrapperInner<P>,
    pub(super) pending_requests: PendingActivateContextRequests,
}

/// Any requests that should be sent out when the [`WrapperActivateContext`] is dropped. See that
/// struct's docstring for mroe information.
#[derive(Debug, Default)]
pub(crate) struct PendingActivateContextRequests {
    /// The value of the last `.set_latency_samples()` call.
    latency_changed: Cell<Option<u32>>,
}

/// A [`ProcessContext`] implementation for the wrapper. This is a separate object so it can hold on
/// to lock guards for event queues. Otherwise reading these events would require constant
/// unnecessary atomic operations to lock the uncontested locks.
pub(crate) struct WrapperProcessContext<'a, P: Vst3Plugin> {
    pub(super) inner: &'a WrapperInner<P>,
    pub(super) input_events_guard: AtomicRefMut<'a, VecDeque<PluginNoteEvent<P>>>,
    pub(super) output_events_guard: AtomicRefMut<'a, VecDeque<PluginNoteEvent<P>>>,
    pub(super) transport: Transport,
}

/// A [`GuiContext`] implementation for the wrapper. This is passed to the plugin in
/// [`Editor::spawn()`][crate::prelude::Editor::spawn()] so it can interact with the rest of the plugin and
/// with the host for things like setting parameters.
#[cfg(feature = "editor")]
pub(crate) struct WrapperGuiContext<P: Vst3Plugin> {
    pub(super) inner: std::sync::Weak<WrapperInner<P>>,
    #[cfg(debug_assertions)]
    pub(super) param_gesture_checker:
        atomic_refcell::AtomicRefCell<crate::wrapper::util::context_checks::ParamGestureChecker>,
}

impl<P: Vst3Plugin> Drop for WrapperActivateContext<'_, P> {
    fn drop(&mut self) {
        if let Some(samples) = self.pending_requests.latency_changed.take() {
            self.inner.set_latency_samples(samples)
        }
    }
}

impl<P: Vst3Plugin> ActivateContext<P> for WrapperActivateContext<'_, P> {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }

    fn execute(&self, task: P::BackgroundTask) {
        (self.inner.task_executor.lock())(task);
    }

    fn set_latency_samples(&self, samples: u32) {
        // See this struct's docstring
        self.pending_requests.latency_changed.set(Some(samples));
    }

    fn set_current_voice_capacity(&self, _capacity: u32) {
        // This is only supported by CLAP
    }
}

impl<P: Vst3Plugin> ProcessContext<P> for WrapperProcessContext<'_, P> {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }

    fn execute_background(&self, task: P::BackgroundTask) {
        let task_posted = self.inner.schedule_background(Task::PluginTask(task));
        crate::nice_debug_assert!(task_posted, "The task queue is full, dropping task...");
    }

    fn execute_gui(&self, task: P::BackgroundTask) {
        let task_posted = self.inner.schedule_gui(Task::PluginTask(task));
        crate::nice_debug_assert!(task_posted, "The task queue is full, dropping task...");
    }

    #[inline]
    fn transport(&self) -> &Transport {
        &self.transport
    }

    fn next_event(&mut self) -> Option<PluginNoteEvent<P>> {
        self.input_events_guard.pop_front()
    }

    fn send_event(&mut self, event: PluginNoteEvent<P>) {
        self.output_events_guard.push_back(event);
    }

    fn set_latency_samples(&self, samples: u32) {
        self.inner.set_latency_samples(samples)
    }

    fn set_current_voice_capacity(&self, _capacity: u32) {
        // This is only supported by CLAP
    }

    fn request_restart(&self) {
        self.inner.request_restart();
    }
}

#[cfg(feature = "editor")]
impl<P: Vst3Plugin + Send> GuiContextInner for WrapperGuiContext<P> {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Vst3
    }

    // All of these functions are supposed to be called from the main thread, so we'll put some
    // trust in the caller and assume that this is indeed the case
    unsafe fn raw_begin_set_parameter(&self, param: ParamPtr) {
        let inner = self.inner.upgrade().unwrap();

        match &*inner.component_handler.borrow() {
            Some(handler) => match inner.param_ptr_to_hash.get(&param) {
                Some(hash) => unsafe {
                    handler.beginEdit(*hash);
                },
                None => crate::nice_debug_assert_failure!("Unknown parameter: {:?}", param),
            },
            None => crate::nice_debug_assert_failure!("Component handler not yet set"),
        }

        #[cfg(debug_assertions)]
        match inner.param_id_from_ptr(param) {
            Some(param_id) => self
                .param_gesture_checker
                .borrow_mut()
                .begin_set_parameter(param_id),
            None => crate::nice_debug_assert_failure!(
                "raw_begin_set_parameter() called with an unknown ParamPtr"
            ),
        }
    }

    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        let inner = self.inner.upgrade().unwrap();

        match &*inner.component_handler.borrow() {
            Some(handler) => match inner.param_ptr_to_hash.get(&param) {
                Some(hash) => {
                    // Only update the parameters manually if the host is not processing audio. If
                    // the plugin is currently processing audio, the host will pass this change back
                    // to the plugin in the audio callback. This also prevents the values from
                    // changing in the middle of the process callback, which would be unsound.
                    // FIXME: So this doesn't work for REAPER, because they just silently stop
                    //        processing audio when you bypass the plugin. Great. We can add a time
                    //        based heuristic to work around this in the meantime.
                    if !inner
                        .is_processing
                        .load(std::sync::atomic::Ordering::SeqCst)
                    {
                        inner.set_normalized_value_by_hash(
                            *hash,
                            normalized,
                            inner.current_buffer_config.load().map(|c| c.sample_rate),
                        );
                    }

                    unsafe { handler.performEdit(*hash, normalized as f64) };
                }
                None => crate::nice_debug_assert_failure!("Unknown parameter: {:?}", param),
            },
            None => crate::nice_debug_assert_failure!("Component handler not yet set"),
        }

        #[cfg(debug_assertions)]
        match inner.param_id_from_ptr(param) {
            Some(param_id) => self
                .param_gesture_checker
                .borrow_mut()
                .set_parameter(param_id),
            None => {
                crate::nice_debug_assert_failure!(
                    "raw_set_parameter() called with an unknown ParamPtr"
                )
            }
        }
    }

    unsafe fn raw_end_set_parameter(&self, param: ParamPtr) {
        let inner = self.inner.upgrade().unwrap();

        match &*inner.component_handler.borrow() {
            Some(handler) => match inner.param_ptr_to_hash.get(&param) {
                Some(hash) => unsafe {
                    handler.endEdit(*hash);
                },
                None => crate::nice_debug_assert_failure!("Unknown parameter: {:?}", param),
            },
            None => crate::nice_debug_assert_failure!("Component handler not yet set"),
        }

        #[cfg(debug_assertions)]
        match inner.param_id_from_ptr(param) {
            Some(param_id) => self
                .param_gesture_checker
                .borrow_mut()
                .end_set_parameter(param_id),
            None => {
                crate::nice_debug_assert_failure!(
                    "raw_end_set_parameter() called with an unknown ParamPtr"
                )
            }
        }
    }

    fn get_state(&self) -> PluginState {
        self.inner.upgrade().unwrap().get_state_object()
    }

    fn set_state(&self, state: PluginState) {
        self.inner
            .upgrade()
            .unwrap()
            .set_state_object_from_gui(state)
    }

    fn request_restart(&self) {
        self.inner.upgrade().unwrap().request_restart();
    }
}
