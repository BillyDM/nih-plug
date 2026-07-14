use once_cell::sync::OnceCell;

mod builder;
mod logger;
mod platform;

pub use builder::{LoggerBuilder, OutputTarget, SetLoggerError, SetTargetError};
pub use log::LevelFilter;

/// The current logger instance. Initialized in [`LoggerBuilder::build_global()`] and then set as
/// the global logger using [`log::set_logger()`].
static LOGGER_INSTANCE: OnceCell<logger::Logger> = OnceCell::new();
