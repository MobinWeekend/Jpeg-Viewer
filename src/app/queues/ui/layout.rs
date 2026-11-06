//! Shared geometry and label layout helpers for queue-related thumbnail grids.
use eframe::egui;

/// Maximum side length of each thumbnail cell.
pub(super) const CELL_MAX: f32 = 240.0;
/// Minimum side length of each thumbnail cell.
pub(in crate::app::queues) const CELL_MIN: f32 = 64.0;
/// Gap between grid cells (both axes).
pub(in crate::app::queues) const GAP: f32 = 10.0;

/// Choose the number of columns and the card size.
///
/// Cards stay between CELL_MIN and CELL_MAX whenever the available width
/// permits it. The layout uses the fewest columns that can fill the width
/// without exceeding CELL_MAX.
pub(in crate::app::queues) fn fit_cards(available: f32, preferred_size: f32) -> (usize, f32) {
    let available = available.max(1.0);
    let preferred_size = preferred_size.clamp(CELL_MIN, CELL_MAX);
    if preferred_size < 112.0 {
        return (1, available.min(2048.0));
    }
    let columns = (((available + GAP) / (preferred_size + GAP)).floor() as usize)
        .max(1)
        .min(24);
    let cell_min = CELL_MIN.min(available);
    let card_size = ((available - GAP * (columns as f32 - 1.0)) / columns as f32)
        .min(preferred_size)
        .clamp(cell_min, CELL_MAX);
    (columns, card_size)
}

/// Wrap a card label within its measured width. Long unbroken filenames are split
/// at character boundaries rather than overflowing the sidebar; at most `max_lines`
/// are drawn so card hitboxes remain predictable.
pub(super) fn wrap_label(ui: &egui::Ui, text: &str, max_width: f32, max_lines: usize) -> String {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let measure = |value: &str| -> f32 {
        ui.painter()
            .layout_no_wrap(value.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    let width = max_width.max(4.0);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();

    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
        if measure(&candidate) <= width {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // A single long token can be wider than the card. Split it by Unicode
        // scalar value so the displayed label never establishes a larger width.
        for character in word.chars() {
            let mut candidate = line.clone();
            candidate.push(character);
            if !line.is_empty() && measure(&candidate) > width {
                lines.push(std::mem::take(&mut line));
            }
            line.push(character);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.is_empty() {
        return String::new();
    }
    if lines.len() > max_lines.max(1) {
        lines.truncate(max_lines.max(1));
        let last = lines.last_mut().expect("at least one line");
        while !last.is_empty() && measure(&format!("{last}…")) > width {
            last.pop();
        }
        last.push('…');
    }
    lines.join("\n")
}
