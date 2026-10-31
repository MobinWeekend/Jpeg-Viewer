use super::helpers::{format_file_size, icon_button};
use crate::app::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    /// Render metadata using an external width budget rather than the auto-sized
    /// egui Area's previous-frame width. That allows the label to expand again as
    /// soon as compact mode is disabled or the central viewport grows.
    pub fn file_metadata_button_with_width(&mut self, ui: &mut egui::Ui, width_budget: f32) {
        // Metadata is deliberately omitted in compact mode rather than competing
        // with navigation and slideshow controls for the remaining toolbar width.
        if self.compactui {
            self.file_metadata_open = false;
            return;
        }

        // Keep the metadata button useful across narrow toolbar widths. Drop fields in
        // a predictable order, from least essential to most essential, and hide the
        // button if even the dimensions cannot fit.
        let candidates = if let Some(metadata) = &self.file_metadata {
            metadata_label_candidates(
                metadata.file_size.map(format_file_size),
                metadata
                    .dimensions
                    .map(|(w, h)| format!("{}×{}", w, h)),
                metadata.aspect_ratio.as_deref(),
                metadata.created.as_deref(),
            )
        } else {
            vec!["Info".to_owned()]
        };
        let Some(button_text) = candidates
            .into_iter()
            .find(|candidate| !candidate.is_empty() && metadata_label_fits(ui, candidate, width_budget))
        else {
            self.file_metadata_open = false;
            return;
        };
        let response = icon_button(ui, &button_text, "File Info");

        if response.clicked() {
            self.file_metadata_open = !self.file_metadata_open;
        }

        if !self.file_metadata_open {
            return;
        }

        let ctx = ui.ctx();
        let popup_pos = response.rect.left_bottom() + egui::vec2(0.0, 8.0);

        let popup_response = egui::Area::new(egui::Id::new("file_metadata_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::both()
                            .max_height(768.0)
                            .max_width(420.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if let Some(metadata) = &self.file_metadata {
                                    self.render_file_metadata(ui, metadata);
                                }
                            });
                    });
            });

        // Close when clicking anywhere outside the button or popup.
        if ctx.input(|input| input.pointer.any_click()) {
            if let Some(pointer_pos) = ctx.input(|input| input.pointer.interact_pos()) {
                let clicked_button = response.rect.contains(pointer_pos);
                let clicked_popup = popup_response.response.rect.contains(pointer_pos);

                if !clicked_button && !clicked_popup {
                    self.file_metadata_open = false;
                }
            }
        }
    }
}



/// Build progressively shorter labels in the required fallback order: full
/// metadata, omit date, omit aspect ratio, dimensions only.
fn metadata_label_candidates(
    size: Option<String>,
    dimensions: Option<String>,
    aspect_ratio: Option<&str>,
    created: Option<&str>,
) -> Vec<String> {
    let candidates = [
        join_metadata_fields(&[
            size.as_deref(),
            dimensions.as_deref(),
            aspect_ratio,
            created,
        ]),
        join_metadata_fields(&[size.as_deref(), dimensions.as_deref(), aspect_ratio]),
        join_metadata_fields(&[size.as_deref(), dimensions.as_deref()]),
        join_metadata_fields(&[dimensions.as_deref()]),
    ];

    // Missing metadata can make adjacent tiers identical. Remove duplicates while
    // preserving order so the fit check only measures each distinct label once.
    let mut unique = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !candidate.is_empty() && unique.last() != Some(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

/// Join optional metadata values without leaving dangling separators when a file
/// does not provide one of the fields.
fn join_metadata_fields(fields: &[Option<&str>]) -> String {
    fields
        .iter()
        .filter_map(|field| *field)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" - ")
}

/// Measure the label using the same font size as `icon_button`, while reserving
/// the standard egui button padding. This avoids depending on the toolbar's theme
/// or placement when deciding which metadata tier fits.
fn metadata_label_fits(ui: &egui::Ui, text: &str, available_width: f32) -> bool {
    if !available_width.is_finite() {
        return true;
    }

    // Ui::painter exposes the read-only layout path for the currently active
    // fonts. `Fonts::layout_no_wrap` requires mutable access in current egui.
    let text_width = ui
        .painter()
        .layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(14.0),
            ui.visuals().text_color(),
        )
        .size()
        .x;
    let button_padding = ui.spacing().button_padding.x * 2.0;
    let desired_width = (text_width + button_padding).max(24.0);
    desired_width <= available_width.max(0.0)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_label_candidates_follow_the_requested_fallback_order() {
        assert_eq!(
            metadata_label_candidates(
                Some("4.5 MB".to_owned()),
                Some("1920×1080".to_owned()),
                Some("16:9"),
                Some("2026-10-31"),
            ),
            vec![
                "4.5 MB - 1920×1080 - 16:9 - 2026-10-31",
                "4.5 MB - 1920×1080 - 16:9",
                "4.5 MB - 1920×1080",
                "1920×1080",
            ]
        );
    }

    #[test]
    fn missing_optional_fields_do_not_leave_empty_or_duplicate_tiers() {
        assert_eq!(
            metadata_label_candidates(
                Some("4.5 MB".to_owned()),
                Some("1920×1080".to_owned()),
                None,
                None,
            ),
            vec!["4.5 MB - 1920×1080", "1920×1080"]
        );
    }
}
