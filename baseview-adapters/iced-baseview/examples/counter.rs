use iced_baseview::{
    Center, IcedBaseviewSettings, PollSubNotifier, Theme, application,
    baseview::{Size, WindowOpenOptions, WindowScalePolicy},
    widget::{Column, button, column, text},
};

fn main() {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::DEBUG)
            .finish(),
    )
    .unwrap();

    iced_baseview::open_blocking(
        IcedBaseviewSettings {
            window: WindowOpenOptions {
                title: String::from("iced_baseview counter"),
                size: Size::new(500.0, 300.0),
                scale: WindowScalePolicy::SystemScaleFactor,
                // NOTE: OpenGL support is not needed here, but rust-analyzer gets confused when
                // some crates do use it and others don't. You should disable the opengl feature
                // in your crate.
                gl_config: None,
            },
            ..Default::default()
        },
        PollSubNotifier::new(),
        || {
            application(MyProgram::default, MyProgram::update, MyProgram::view)
                .theme(MyProgram::theme)
                .run()
        },
    );
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Increment,
    Decrement,
}

#[derive(Default)]
struct MyProgram {
    value: i64,
}

impl MyProgram {
    pub fn theme(&self) -> Option<Theme> {
        Some(iced_baseview::Theme::Dark)
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Increment => {
                self.value += 1;
            }
            Message::Decrement => {
                self.value -= 1;
            }
        }
    }

    pub fn view(&self) -> Column<'_, Message> {
        column![
            button("Increment").on_press(Message::Increment),
            text(self.value).size(50),
            button("Decrement").on_press(Message::Decrement)
        ]
        .padding(20)
        .align_x(Center)
    }
}
