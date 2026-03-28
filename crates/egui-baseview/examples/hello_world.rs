use baseview::{Size, WindowOpenOptions, WindowScalePolicy};
use egui::{CentralPanel, Context, Ui};
use egui_baseview::{EguiWindow, GraphicsConfig, Queue};

fn main() {
    let settings = WindowOpenOptions {
        title: String::from("egui-baseview hello world"),
        size: Size::new(300.0, 110.0),
        scale: WindowScalePolicy::SystemScaleFactor,
        gl_config: None,
    };

    let state = ();

    EguiWindow::open_blocking(
        settings,
        GraphicsConfig::default(),
        state,
        |_egui_ctx: &Context, _queue: &mut Queue, _state: &mut ()| {},
        |ui: &mut Ui, _queue: &mut Queue, _state: &mut ()| {
            CentralPanel::default().show_inside(ui, |ui| {
                ui.label("Hello World!");
            });
        },
    );
}
