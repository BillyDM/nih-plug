use iced_runtime::Action;
use iced_runtime::{
    futures::futures::{
        Sink,
        channel::mpsc,
        task::{Context, Poll},
    },
    window,
};
use iced_widget::graphics::shell::Notifier;
use std::pin::Pin;

/// An event loop proxy that implements `Sink`.
#[derive(Debug)]
pub struct Proxy<T: 'static> {
    sender: mpsc::UnboundedSender<Action<T>>,
}

impl<T: 'static> Clone for Proxy<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<T: 'static> Proxy<T> {
    /// Creates a new [`Proxy`] from an `mpsc::Sender`.
    pub fn new(sender: mpsc::UnboundedSender<Action<T>>) -> Self {
        Self { sender }
    }

    /// Sends a value to the event loop.
    ///
    /// Note: This skips the backpressure mechanism with an unbounded
    /// channel. Use sparingly!
    pub fn send(&self, value: T) {
        self.send_action(Action::Output(value));
    }

    /// Sends an action to the event loop.
    ///
    /// Note: This skips the backpressure mechanism with an unbounded
    /// channel. Use sparingly!
    pub fn send_action(&self, action: Action<T>) {
        let _ = self.sender.unbounded_send(action);
    }
}

impl<T: 'static> Sink<Action<T>> for Proxy<T> {
    type Error = mpsc::SendError;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.sender.poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, message: Action<T>) -> Result<(), Self::Error> {
        self.sender.start_send(message)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.sender.poll_ready(cx) {
            Poll::Ready(Err(ref e)) if e.is_disconnected() => {
                // If the receiver disconnected, we consider the sink to be flushed.
                Poll::Ready(Ok(()))
            }
            x => x,
        }
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.sender.disconnect();
        Poll::Ready(Ok(()))
    }
}

impl<T> Notifier for Proxy<T>
where
    T: Send,
{
    fn request_redraw(&self) {
        self.send_action(Action::Window(window::Action::RedrawAll));
    }

    fn invalidate_layout(&self) {
        self.send_action(Action::Window(window::Action::RelayoutAll));
    }
}
