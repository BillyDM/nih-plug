use iced_baseview::{
    Center, Fill, IcedBaseviewSettings, PollSubNotifier, Task, Theme, application,
    baseview::{Size, WindowOpenOptions, WindowScalePolicy},
    widget::{Column, column, container, slider, text, vertical_slider},
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
                title: String::from("iced_baseview slider"),
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
    SliderChanged(f32),
}

#[derive(Default)]
struct MyProgram {
    value: f32,
}

impl MyProgram {
    pub fn theme(&self) -> Option<Theme> {
        Some(iced_baseview::Theme::Dark)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SliderChanged(value) => {
                self.value = value;
            }
        }

        Task::none()
    }

    pub fn view(&self) -> Column<'_, Message> {
        let h_slider = container(
            slider(0.0..=1.0, self.value, Message::SliderChanged)
                .default(0.5)
                .step(0.01)
                .shift_step(0.1),
        )
        .width(250);

        let v_slider = container(
            vertical_slider(0.0..=1.0, self.value, Message::SliderChanged)
                .default(0.5)
                .step(0.01)
                .shift_step(0.1),
        )
        .height(200);

        let text = text(format!("{:.2}", self.value));

        column![v_slider, h_slider, text,]
            .width(Fill)
            .align_x(Center)
            .spacing(20)
            .padding(20)
            .into()
    }
}
