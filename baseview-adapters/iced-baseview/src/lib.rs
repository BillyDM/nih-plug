#[cfg(not(any(
    target_arch = "wasm32",
    feature = "thread-pool",
    feature = "tokio",
    feature = "smol"
)))]
compile_error!(
    "No futures executor has been enabled! You must enable an executor feature.\nAvailable \
     options: thread-pool, tokio, or smol."
);

#[cfg(all(
    target_family = "unix",
    not(target_os = "macos"),
    //not(feature = "wayland"),
    not(feature = "x11"),
))]
compile_error!(
    "No Unix display server backend has been enabled. You must enable a display server \
     feature.\nAvailable options: x11, wayland."
);

pub use baseview;
pub use iced_program as program;
use iced_widget::graphics;
use iced_widget::renderer;
pub use program::core;
pub use program::runtime;

pub use iced_futures::futures;
pub use iced_futures::stream;

#[cfg(feature = "app-builder")]
pub mod application;

pub mod shell;
pub mod time;

mod error;

#[cfg(feature = "highlighter")]
pub use iced_highlighter as highlighter;

#[cfg(feature = "wgpu")]
pub use iced_renderer::wgpu::wgpu;

#[cfg(feature = "advanced")]
pub mod advanced;

pub use crate::core::alignment;
pub use crate::core::animation;
pub use crate::core::border;
pub use crate::core::color;
pub use crate::core::gradient;
pub use crate::core::padding;
pub use crate::core::theme;
pub use crate::core::{
    Alignment, Animation, Background, Border, Color, ContentFit, Degrees, Function, Gradient,
    Length, Never, Padding, Pixels, Point, Radians, Rectangle, Rotation, Settings, Shadow, Size,
    Theme, Transformation, Vector, never,
};
pub use crate::program::Preset;
pub use crate::program::message;
pub use crate::runtime::exit;
pub use iced_futures::Subscription;

pub use Alignment::Center;
pub use Length::{Fill, FillPortion, Shrink};
pub use alignment::Horizontal::{Left, Right};
pub use alignment::Vertical::{Bottom, Top};

pub mod debug {
    //! Debug your applications.
    pub use iced_debug::{Span, time, time_with};
}

pub mod task {
    //! Create runtime tasks.
    pub use crate::runtime::task::{Handle, Task};

    #[cfg(feature = "sipper")]
    pub use crate::runtime::task::{Never, Sipper, Straw, sipper, stream};
}

pub mod clipboard {
    //! Access the clipboard.
    pub use crate::runtime::clipboard::{read, read_primary, write, write_primary};
}

pub mod executor {
    //! Choose your preferred executor to power your application.
    pub use iced_futures::Executor;
    pub use iced_futures::backend::default::Executor as Default;
}

pub mod font {
    //! Load and use fonts.
    pub use crate::core::font::*;
    pub use crate::runtime::font::*;
}

pub mod event {
    //! Handle events of a user interface.
    pub use crate::core::event::{Event, Status};
    pub use iced_futures::event::{listen, listen_raw, listen_with};
}

pub mod keyboard {
    //! Listen and react to keyboard events.
    pub use crate::core::keyboard::key;
    pub use crate::core::keyboard::{Event, Key, Location, Modifiers};
    pub use iced_futures::keyboard::listen;
}

pub mod mouse {
    //! Listen and react to mouse events.
    pub use crate::core::mouse::{Button, Cursor, Event, Interaction, ScrollDelta};
}

#[cfg(feature = "sysinfo")]
pub mod system {
    //! Retrieve system information.
    pub use crate::runtime::system::{Information, information};
}

pub mod overlay {
    //! Display interactive elements on top of other widgets.

    /// A generic overlay.
    ///
    /// This is an alias of an [`overlay::Element`] with a default `Renderer`.
    ///
    /// [`overlay::Element`]: crate::core::overlay::Element
    pub type Element<'a, Message, Theme = crate::Renderer, Renderer = crate::Renderer> =
        crate::core::overlay::Element<'a, Message, Theme, Renderer>;

    pub use iced_widget::overlay::*;
}

pub mod touch {
    //! Listen and react to touch events.
    pub use crate::core::touch::{Event, Finger};
}

#[allow(hidden_glob_reexports)]
pub mod widget {
    //! Use the built-in widgets or create your own.
    pub use iced_runtime::widget::*;
    pub use iced_widget::*;

    #[cfg(feature = "image")]
    pub mod image {
        //! Images display raster graphics in different formats (PNG, JPG, etc.).
        pub use iced_runtime::image::{Allocation, Error, allocate};
        pub use iced_widget::image::*;
    }

    // We hide the re-exported modules by `iced_widget`
    mod core {}
    mod graphics {}
    mod renderer {}
}

pub mod window {
    pub use crate::core::window::*;
    pub use crate::runtime::window::*;
}

#[cfg(feature = "app-builder")]
pub use application::Application;

pub use error::Error;
pub use event::Event;
pub use executor::Executor;
pub use font::Font;
pub use program::Program;
pub use renderer::Renderer;
pub use shell::{
    PollSubNotifier, open_blocking, open_parented, poll_events, settings::IcedBaseviewSettings,
};
pub use task::Task;
pub use window::Window;

#[cfg(feature = "app-builder")]
#[doc(inline)]
pub use application::application;

/// A generic widget.
///
/// This is an alias of an `iced_native` element with a default `Renderer`.
pub type Element<'a, Message, Theme = crate::Theme, Renderer = crate::Renderer> =
    crate::core::Element<'a, Message, Theme, Renderer>;

/// The result of running an iced program.
pub type Result = std::result::Result<(), Error>;
