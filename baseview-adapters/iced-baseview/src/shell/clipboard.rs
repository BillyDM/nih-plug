//! Access the clipboard.

use crate::core::clipboard::Kind;
use raw_window_handle_06::HasDisplayHandle;

#[cfg(feature = "nih_log")]
use nih_plug::log::warn;
#[cfg(all(feature = "tracing", not(feature = "nih_log")))]
use tracing::warn;

/// A buffer for short-term storage and transfer within and between
/// applications.
pub struct Clipboard {
    state: State,
}

enum State {
    Connected {
        clipboard: window_clipboard::Clipboard,
    },
    Unavailable,
}

impl Clipboard {
    /// Creates a new [`Clipboard`] for the given window.
    ///
    /// # SAFETY: The window handle must outlive this struct.
    pub unsafe fn connect<W: HasDisplayHandle>(window: &W) -> Clipboard {
        let clipboard = unsafe { window_clipboard::Clipboard::connect(window) };

        let state = match clipboard {
            Ok(clipboard) => State::Connected { clipboard },
            Err(_) => State::Unavailable,
        };

        Clipboard { state }
    }

    /// Creates a new [`Clipboard`] that isn't associated with a window.
    /// This clipboard will never contain a copied value.
    pub fn unconnected() -> Clipboard {
        Clipboard {
            state: State::Unavailable,
        }
    }

    /// Reads the current content of the [`Clipboard`] as text.
    pub fn read(&self, kind: Kind) -> Option<String> {
        match &self.state {
            State::Connected { clipboard, .. } => match kind {
                Kind::Standard => clipboard.read().ok(),
                Kind::Primary => clipboard.read_primary().and_then(Result::ok),
            },
            State::Unavailable => None,
        }
    }

    /// Writes the given text contents to the [`Clipboard`].
    pub fn write(&mut self, kind: Kind, contents: String) {
        match &mut self.state {
            State::Connected { clipboard, .. } => {
                let result = match kind {
                    Kind::Standard => clipboard.write(contents),
                    Kind::Primary => clipboard.write_primary(contents).unwrap_or(Ok(())),
                };

                match result {
                    Ok(()) => {}
                    Err(error) => {
                        warn!("error writing to clipboard: {error}");
                    }
                }
            }
            State::Unavailable => {}
        }
    }
}

impl crate::core::Clipboard for Clipboard {
    fn read(&self, kind: Kind) -> Option<String> {
        self.read(kind)
    }

    fn write(&mut self, kind: Kind, contents: String) {
        self.write(kind, contents);
    }
}
