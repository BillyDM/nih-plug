//! Resizable window wrapper for Egui editor.

use egui::emath::GuiRounding;
use egui::{CentralPanel, Id, Pos2, Rect, Response, Sense, Ui, Vec2, pos2};
use egui::{InnerResponse, UiBuilder};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResizeWindowMode {
    ExpandViewport {
        min_size: Vec2,
        max_size: Option<Vec2>,
    },
    ZoomViewport {
        min_zoom_factor: f32,
        max_zoom_factor: f32,
    },
}

/// Adds a corner to the plugin window that can be dragged in order to resize it.
/// Resizing happens through plugin API, hence a custom implementation is needed.
pub struct ResizableWindow {
    id: Id,
    resize_mode: ResizeWindowMode,
}

impl ResizableWindow {
    pub fn new(id_source: impl egui::AsId, resize_mode: ResizeWindowMode) -> Self {
        Self {
            id: Id::new(id_source),
            resize_mode,
        }
    }

    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        CentralPanel::default().show(ui, move |ui| {
            let ui_rect = ui.clip_rect();
            let mut content_ui =
                ui.new_child(UiBuilder::new().max_rect(ui_rect).layout(*ui.layout()));

            let ret = add_contents(&mut content_ui);

            let corner_size = Vec2::splat(ui.visuals().resize_corner_size);
            let corner_rect = Rect::from_min_size(ui_rect.max - corner_size, corner_size);

            let id = self.id.with("cr");

            let corner_response = ui.interact(corner_rect, id, Sense::drag());

            match self.resize_mode {
                ResizeWindowMode::ExpandViewport { min_size, max_size } => {
                    #[derive(Clone, Copy)]
                    struct State {
                        start_window_size: Vec2,
                    }

                    if corner_response.dragged()
                        && let Some(total_drag_delta) = corner_response.total_drag_delta()
                    {
                        let state = ui.data_mut(|d| {
                            *d.get_persisted_mut_or_default::<Option<State>>(id)
                                .get_or_insert_with(|| State {
                                    start_window_size: ui_rect.max.to_vec2(),
                                })
                        });

                        let mut desired_size = state.start_window_size + total_drag_delta;

                        desired_size = desired_size.max(min_size);
                        if let Some(max_size) = max_size {
                            desired_size = desired_size.min(max_size);
                        }

                        ui.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
                    } else if corner_response.drag_stopped() {
                        ui.data_mut(|d| {
                            *d.get_persisted_mut_or_default::<Option<State>>(id) = None
                        });
                    }
                }
                ResizeWindowMode::ZoomViewport {
                    min_zoom_factor,
                    max_zoom_factor,
                } => {
                    // Egui's built in drag interaction is too glitchy when zooming the viewport while
                    // dragging, so manually detect pointer events instead.
                    struct InputState {
                        primary_pressed: bool,
                        primary_down: bool,
                    }
                    let input = ui.input(|input| InputState {
                        primary_pressed: input.pointer.primary_pressed(),
                        primary_down: input.pointer.primary_down()
                            || input.pointer.primary_released(),
                    });

                    #[derive(Clone, Copy)]
                    struct DragState {
                        start_window_size: Vec2,
                        start_zoom_factor: f32,
                        start_pointer_pos: Pos2,
                    }

                    if let Some(pointer_pos) = ui.pointer_latest_pos() {
                        let pointer_pos = pointer_pos * ui.pixels_per_point();

                        if input.primary_pressed && corner_response.hovered() {
                            let start_window_size =
                                ui.viewport_rect().size() * ui.pixels_per_point();
                            let start_zoom_factor = ui.zoom_factor();

                            ui.data_mut(|d| {
                                *d.get_persisted_mut_or_default::<Option<DragState>>(id) =
                                    Some(DragState {
                                        start_window_size,
                                        start_zoom_factor,
                                        start_pointer_pos: pointer_pos,
                                    });
                            });
                        }

                        if !input.primary_down {
                            ui.data_mut(|d| {
                                *d.get_persisted_mut_or_default::<Option<DragState>>(id) = None
                            });
                        }

                        let dragging_state = ui.data_mut(|d| {
                            d.get_persisted::<Option<DragState>>(id).unwrap_or_default()
                        });

                        if let Some(state) = dragging_state {
                            let total_drag_delta = pointer_pos - state.start_pointer_pos;

                            let mut drag_distance = total_drag_delta.length();
                            let drag_angle = total_drag_delta.angle();
                            if drag_angle < -0.25 * std::f32::consts::PI
                                || drag_angle > 0.75 * std::f32::consts::PI
                            {
                                drag_distance = -drag_distance;
                            }

                            let window_distance = state.start_window_size.length();

                            let new_zoom = state.start_zoom_factor
                                * ((window_distance + drag_distance) / window_distance);

                            let new_zoom = new_zoom.clamp(min_zoom_factor, max_zoom_factor);

                            if (new_zoom - ui.zoom_factor()).abs() > 0.001 {
                                ui.set_zoom_factor(new_zoom);
                            }
                        }
                    } else {
                        ui.data_mut(|d| {
                            *d.get_persisted_mut_or_default::<Option<DragState>>(id) = None
                        });
                    }
                }
            }

            paint_resize_corner(&content_ui, &corner_response);

            ret
        })
    }
}

pub fn paint_resize_corner(ui: &Ui, response: &Response) {
    let stroke = ui.style().interact(response).fg_stroke;

    let painter = ui.painter();
    let rect = response.rect.translate(-Vec2::splat(2.0)); // move away from the corner
    let cp = rect.max.round_to_pixels(painter.pixels_per_point());

    let mut w = 2.0;

    while w <= rect.width() && w <= rect.height() {
        painter.line_segment([pos2(cp.x - w, cp.y), pos2(cp.x, cp.y - w)], stroke);
        w += 4.0;
    }
}
