use iced_baseview::{
    Center, Fill, Font, IcedBaseviewSettings, Pixels, PollSubNotifier, Task, Theme, application,
    baseview::{Size, WindowOpenOptions, WindowScalePolicy},
    widget::{Column, checkbox, column, text, text_input},
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
                title: String::from("iced_baseview text input"),
                size: Size::new(500.0, 500.0),
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

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    ToggleSecureInput(bool),
    ToggleTextInputIcon(bool),
}

#[derive(Default)]
struct MyProgram {
    input_value: String,
    input_is_secure: bool,
    input_is_showing_icon: bool,
}

impl MyProgram {
    pub fn theme(&self) -> Option<Theme> {
        Some(iced_baseview::Theme::Dark)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(input_value) => {
                self.input_value = input_value;
            }
            Message::ToggleSecureInput(is_secure) => {
                self.input_is_secure = is_secure;
            }
            Message::ToggleTextInputIcon(show_icon) => {
                self.input_is_showing_icon = show_icon;
            }
        }

        Task::none()
    }

    pub fn view(&self) -> Column<'_, Message> {
        let value = &self.input_value;
        let is_secure = self.input_is_secure;
        let is_showing_icon = self.input_is_showing_icon;

        let mut text_input = text_input("Type something to continue...", value)
            .on_input(Message::InputChanged)
            .padding(10)
            .size(30);

        if is_showing_icon {
            text_input = text_input.icon(text_input::Icon {
                font: Font::default(),
                code_point: '🚀',
                size: Some(Pixels(28.0)),
                spacing: 10.0,
                side: text_input::Side::Right,
            });
        }

        let container = column![text("Text Input").size(50)].padding(20).spacing(20);

        container
            .push("Use a text input to ask for different kinds of information.")
            .push(text_input.secure(is_secure))
            .push(
                checkbox(is_secure)
                    .label("Enable password mode")
                    .on_toggle(Message::ToggleSecureInput),
            )
            .push(
                checkbox(is_showing_icon)
                    .label("Show icon")
                    .on_toggle(Message::ToggleTextInputIcon),
            )
            .push(
                "A text input produces a message every time it changes. It is very easy to keep \
                 track of its contents:",
            )
            .push(
                text(if value.is_empty() {
                    "You have not typed anything yet..."
                } else {
                    value
                })
                .width(Fill)
                .align_x(Center),
            )
            .into()
    }
}
