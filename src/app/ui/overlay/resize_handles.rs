// Window resize handles only used with Custom title bar
use eframe::egui;

const RESIZE_HANDLE_SIZE: f32 = 6.0;

pub fn render_resize_handles(ctx: &egui::Context) {
    let is_fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
    if is_fullscreen {
        return;
    }

    let rect = ctx.available_rect();
    let s = RESIZE_HANDLE_SIZE;

    // Edges
    render_handle(ctx, "resize_top", top_edge_rect(&rect, s), egui::ResizeDirection::North, egui::CursorIcon::ResizeVertical);
    render_handle(ctx, "resize_bottom", bottom_edge_rect(&rect, s), egui::ResizeDirection::South, egui::CursorIcon::ResizeVertical);
    render_handle(ctx, "resize_left", left_edge_rect(&rect, s), egui::ResizeDirection::West, egui::CursorIcon::ResizeHorizontal);
    render_handle(ctx, "resize_right", right_edge_rect(&rect, s), egui::ResizeDirection::East, egui::CursorIcon::ResizeHorizontal);

    // Corners
    render_handle(ctx, "resize_top_left", top_left_rect(&rect, s), egui::ResizeDirection::NorthWest, egui::CursorIcon::ResizeNorthWest);
    render_handle(ctx, "resize_top_right", top_right_rect(&rect, s), egui::ResizeDirection::NorthEast, egui::CursorIcon::ResizeNorthEast);
    render_handle(ctx, "resize_bottom_left", bottom_left_rect(&rect, s), egui::ResizeDirection::SouthWest, egui::CursorIcon::ResizeSouthWest);
    render_handle(ctx, "resize_bottom_right", bottom_right_rect(&rect, s), egui::ResizeDirection::SouthEast, egui::CursorIcon::ResizeSouthEast);
}

fn render_handle(
    ctx: &egui::Context,
    id: &'static str,
    rect: egui::Rect,
    direction: egui::ResizeDirection,
    cursor: egui::CursorIcon,
) {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            let size = rect.size();
            ui.set_min_size(size);
            ui.set_max_size(size);

            let (_id, response) = ui.allocate_space(size);
            let response = ui.interact(
                response,
                ui.id().with("resize_handle"),
                egui::Sense::click_and_drag(),
            );

            if response.hovered() {
                ctx.set_cursor_icon(cursor);
            }

            if response.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        });
}

// Rect helper functions
fn top_edge_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + s, rect.top()),
        egui::pos2(rect.right() - s, rect.top() + s),
    )
}

fn bottom_edge_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + s, rect.bottom() - s),
        egui::pos2(rect.right() - s, rect.bottom()),
    )
}

fn left_edge_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.top() + s),
        egui::pos2(rect.left() + s, rect.bottom() - s),
    )
}

fn right_edge_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.right() - s, rect.top() + s),
        egui::pos2(rect.right(), rect.bottom() - s),
    )
}

fn top_left_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_size(rect.min, egui::vec2(s, s))
}

fn top_right_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.right() - s, rect.top()),
        egui::pos2(rect.right(), rect.top() + s),
    )
}

fn bottom_left_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left(), rect.bottom() - s),
        egui::pos2(rect.left() + s, rect.bottom()),
    )
}

fn bottom_right_rect(rect: &egui::Rect, s: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.right() - s, rect.bottom() - s),
        rect.max,
    )
}