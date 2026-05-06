//! Listen and react to time.
pub use crate::core::time::*;

use crate::task::Task;

/// Returns a [`Task`] that produces the current [`Instant`]
/// by calling [`Instant::now`].
///
/// While you can call [`Instant::now`] directly in your application;
/// that renders your application "impure" (i.e. no referential transparency).
///
/// You may care about purity if you want to leverage the `time-travel`
/// feature properly.
pub fn now() -> Task<Instant> {
    Task::future(async { Instant::now() })
}
