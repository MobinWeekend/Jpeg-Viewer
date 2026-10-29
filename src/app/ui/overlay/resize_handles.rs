// Custom window border and resize hit areas used when native decorations are disabled.
use eframe::egui;

const MIN_RESIZE_HANDLE_SIZE: f32 = 2.0;
const MAX_RESIZE_HANDLE_SIZE: f32 = 12.0;
pub(super) const CORNER_RESIZE_HIT_SIZE: f32 = 26.0;
const FRAME_STROKE_ALPHA: u8 = 72; // visually subtle; hit areas stay fully active

pub fn render_resize_handles(ctx: &egui::Context, thickness: f32, border_color: egui::Color32) {
    let viewport = ctx.input(|input| input.viewport().clone());
    if viewport.fullscreen.unwrap_or(false) || viewport.minimized.unwrap_or(false) {
        return;
    }

    // The full root available rectangle is used instead of the remaining central
    // panel rectangle, so handles stay on the outer native-window edges.
    let rect = ctx.input(|input| input.viewport_rect());
    let s = if thickness.is_finite() {
        thickness.clamp(MIN_RESIZE_HANDLE_SIZE, MAX_RESIZE_HANDLE_SIZE)
    } else {
        6.0
    };
    if rect.width() < s * 2.0 || rect.height() < s * 2.0 {
        return;
    }

    // Use a much larger corner target than the visible frame thickness. Corners
    // overlap the title-bar's outermost strip intentionally, just like native resize
    // borders. Caption buttons are inset by the same distance to keep their hitboxes clear.
    let corner = CORNER_RESIZE_HIT_SIZE.min(rect.width() * 0.5).min(rect.height() * 0.5);
    // The handle areas initiate native resize gestures; render them after other overlay
    // controls so their edge/corner input can win over the custom title-bar drag region.
    render_handle(ctx, "resize_top", top_edge_rect(&rect, corner, s), egui::ResizeDirection::North, egui::CursorIcon::ResizeVertical);
    render_handle(ctx, "resize_bottom", bottom_edge_rect(&rect, corner, s), egui::ResizeDirection::South, egui::CursorIcon::ResizeVertical);
    render_handle(ctx, "resize_left", left_edge_rect(&rect, corner, s), egui::ResizeDirection::West, egui::CursorIcon::ResizeHorizontal);
    render_handle(ctx, "resize_right", right_edge_rect(&rect, corner, s), egui::ResizeDirection::East, egui::CursorIcon::ResizeHorizontal);
    render_handle(ctx, "resize_top_left", top_left_rect(&rect, corner), egui::ResizeDirection::NorthWest, egui::CursorIcon::ResizeNorthWest);
    render_handle(ctx, "resize_top_right", top_right_rect(&rect, corner), egui::ResizeDirection::NorthEast, egui::CursorIcon::ResizeNorthEast);
    render_handle(ctx, "resize_bottom_left", bottom_left_rect(&rect, corner), egui::ResizeDirection::SouthWest, egui::CursorIcon::ResizeSouthWest);
    render_handle(ctx, "resize_bottom_right", bottom_right_rect(&rect, corner), egui::ResizeDirection::SouthEast, egui::CursorIcon::ResizeSouthEast);

    let profile = crate::ui_theme::current_look_profile(ctx);
    let radius = if profile == "win98" { egui::CornerRadius::ZERO } else { egui::CornerRadius::same(3) };
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("jpeg_viewer_custom_window_frame"),
    )).rect_stroke(
        rect.shrink(s * 0.5),
        radius,
        egui::Stroke::new(s, crate::ui_theme::with_alpha(border_color, FRAME_STROKE_ALPHA)),
        egui::StrokeKind::Inside,
    );
}

fn render_handle(
    ctx: &egui::Context,
    id: &'static str,
    rect: egui::Rect,
    direction: egui::ResizeDirection,
    cursor: egui::CursorIcon,
) {
    let layer_id = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(id));
    // Areas can retain a prior z-order from interaction history; explicitly raise
    // the resize hitbox so the title-bar Area cannot swallow corner/edge drags.
    ctx.move_to_top(layer_id);
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .interactable(true)
        .movable(false)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            let size = rect.size();
            ui.set_min_size(size);
            ui.set_max_size(size);
            let (_id, rect) = ui.allocate_space(size);
            let response = ui.interact(rect, ui.id().with("resize_handle"), egui::Sense::click_and_drag());
            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }
            if response.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        });
}

fn top_edge_rect(rect: &egui::Rect, corner: f32, edge: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.left() + corner, rect.top()), egui::pos2(rect.right() - corner, rect.top() + edge))
}
fn bottom_edge_rect(rect: &egui::Rect, corner: f32, edge: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.left() + corner, rect.bottom() - edge), egui::pos2(rect.right() - corner, rect.bottom()))
}
fn left_edge_rect(rect: &egui::Rect, corner: f32, edge: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.left(), rect.top() + corner), egui::pos2(rect.left() + edge, rect.bottom() - corner))
}
fn right_edge_rect(rect: &egui::Rect, corner: f32, edge: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.right() - edge, rect.top() + corner), egui::pos2(rect.right(), rect.bottom() - corner))
}
fn top_left_rect(rect: &egui::Rect, size: f32) -> egui::Rect {
    egui::Rect::from_min_size(rect.min, egui::vec2(size, size))
}
fn top_right_rect(rect: &egui::Rect, size: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.right() - size, rect.top()), egui::pos2(rect.right(), rect.top() + size))
}
fn bottom_left_rect(rect: &egui::Rect, size: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - size), egui::pos2(rect.left() + size, rect.bottom()))
}
fn bottom_right_rect(rect: &egui::Rect, size: f32) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(rect.right() - size, rect.bottom() - size), rect.max)
}
