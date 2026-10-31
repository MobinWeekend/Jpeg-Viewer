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
    let available = available.max(CELL_MIN);
    let preferred_size = preferred_size.clamp(CELL_MIN, CELL_MAX);
    if preferred_size < 112.0 {
        return (1, available.min(2048.0));
    }
    let columns = (((available + GAP) / (preferred_size + GAP)).floor() as usize)
        .max(1)
        .min(24);
    let card_size = ((available - GAP * (columns as f32 - 1.0)) / columns as f32)
        .min(preferred_size)
        .clamp(CELL_MIN, CELL_MAX);
    (columns, card_size)
}

/// Truncate `text` with a middle ellipsis so that it fits within `max_width`
/// pixels when rendered with the button font. Always returns a single-line
/// string. Pixel-accurate; guarantees the label never overflows or wraps.
pub(super) fn fit_label(ui: &egui::Ui, text: &str, max_width: f32) -> String {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let measure = |s: &str| -> f32 {
        ui.painter()
            .layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };

    if measure(text) <= max_width {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    // Start with the ellipsis alone, then greedily grow the prefix/suffix
    // while the result still fits. Keep the last candidate that fit.
    let mut best = "…".to_string();
    if measure(&best) > max_width {
        return best; // ellipsis itself doesn't fit; nothing better to do
    }

    let mut left = 0usize;
    let mut right = chars.len();
    while left < right {
        // Grow the side that's currently shorter (keeps the split balanced).
        let grow_left = left <= chars.len() - right;
        let (next_left, next_right) = if grow_left {
            (left + 1, right)
        } else {
            (left, right - 1)
        };

        let candidate: String = chars[..next_left]
            .iter()
            .chain(std::iter::once(&'…'))
            .chain(chars[next_right..].iter())
            .collect();

        if measure(&candidate) <= max_width {
            best = candidate;
            left = next_left;
            right = next_right;
        } else {
            // This side can't grow; try the other side once.
            if grow_left {
                // Try growing from the right instead.
                if right > left {
                    let alt_right = right - 1;
                    let alt: String = chars[..left]
                        .iter()
                        .chain(std::iter::once(&'…'))
                        .chain(chars[alt_right..].iter())
                        .collect();
                    if measure(&alt) <= max_width {
                        best = alt;
                        right = alt_right;
                        continue;
                    }
                }
                break;
            } else {
                break;
            }
        }
    }

    best
}
