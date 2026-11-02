use crate::app::image_metadata::FileMetadata;
use crate::app::types::ViewerApp;
use eframe::egui;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const METADATA_BUTTON_HEIGHT: f32 = 28.0;
const METADATA_BUTTON_MIN_WIDTH: f32 = 28.0;
const METADATA_LABEL_FONT_SIZE: f32 = 14.0;
const METADATA_ICON_FONT_SIZE: f32 = 16.0;
const METADATA_GROW_HYSTERESIS: f32 = 4.0;
const METADATA_POPUP_MIN_WIDTH: f32 = 280.0;
const METADATA_POPUP_MAX_WIDTH: f32 = 420.0;
const METADATA_POPUP_MAX_HEIGHT: f32 = 620.0;

impl ViewerApp {
    /// Render the richest metadata label that fits the current remaining width.
    ///
    /// `file_metadata` is populated when the selected entry changes, not during
    /// every UI frame. Keep this method focused on formatting, measuring and
    /// rendering that already-cached snapshot.
    pub fn file_metadata_button_with_width(&mut self, ui: &mut egui::Ui, width_budget: f32) {
        // NaN / negative / non-finite budget == "no width constraint" from the
        // caller (nothing else is competing for the row). In that case the button
        // should render its richest label rather than fitting itself to a guessed
        // width, so treat the budget as effectively unlimited.
        let unconstrained =
            width_budget.is_nan() || width_budget.is_sign_negative() || !width_budget.is_finite();

        let available = if unconstrained {
            f32::MAX
        } else {
            let available = width_budget.max(ui.available_width().max(0.0));
            if available.is_finite() {
                available
            } else {
                f32::MAX
            }
        };

        // Do not create an invisible, zero-width button. There is no useful
        // clickable target at this size, and the toolbar can reclaim the space.
        if available < METADATA_BUTTON_MIN_WIDTH {
            self.file_metadata_open = false;
            return;
        }

        let candidates = self
            .file_metadata
            .as_ref()
            .map(metadata_label_candidates)
            .unwrap_or_default();
        let label_font_size = crate::ui_theme::scaled_text_size(ui.ctx(), METADATA_LABEL_FONT_SIZE);
        let icon_font_size = crate::ui_theme::scaled_text_size(ui.ctx(), METADATA_ICON_FONT_SIZE);
        let candidate_widths: Vec<f32> = candidates
            .iter()
            .map(|candidate| metadata_button_width(ui, candidate, label_font_size))
            .collect();

        // Include the candidate text in the key. When the selected image changes,
        // its previous tier cannot leak into the new image's layout decision.
        let mut hasher = DefaultHasher::new();
        candidates.hash(&mut hasher);
        let tier_id = ui.id().with(("file_metadata_tier", hasher.finish()));
        let previous_tier = ui.ctx().data(|data| data.get_temp::<usize>(tier_id));
        let chosen_tier = choose_metadata_tier(&candidate_widths, available, previous_tier);

        let (label, font_size, desired_width) = if let Some(index) = chosen_tier {
            ui.ctx().data_mut(|data| data.insert_temp(tier_id, index));
            (
                candidates[index].as_str(),
                label_font_size,
                candidate_widths[index],
            )
        } else {
            ui.ctx().data_mut(|data| data.remove::<usize>(tier_id));
            (" i ", icon_font_size, METADATA_BUTTON_MIN_WIDTH)
        };

        let button_width = desired_width.max(METADATA_BUTTON_MIN_WIDTH).min(available);
        let button_height =
            (font_size + ui.spacing().button_padding.y * 2.0).max(METADATA_BUTTON_HEIGHT);
        let response = ui
            .add_sized(
                egui::vec2(button_width, button_height),
                egui::Button::new(egui::RichText::new(label).size(font_size))
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE),
            )
            .on_hover_text("File Info");

        if response.clicked() {
            let opening = !self.file_metadata_open;
            self.file_metadata_open = opening;
            if opening {
                ui.ctx().data_mut(|data| {
                    data.remove::<String>(egui::Id::new("file_metadata_clipboard_error"));
                });
            }
        }
        if !self.file_metadata_open {
            return;
        }

        let ctx = ui.ctx();
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.file_metadata_open = false;
            return;
        }

        let popup_pos = response.rect.left_bottom() + egui::vec2(0.0, 8.0);
        let mut close_popup = false;
        let clipboard_error_id = egui::Id::new("file_metadata_clipboard_error");
        let mut clipboard_error = ctx.data(|data| data.get_temp::<String>(clipboard_error_id));
        let metadata = self.file_metadata.as_ref();
        let popup_response = egui::Area::new(egui::Id::new("file_metadata_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .constrain(true)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_min_width(METADATA_POPUP_MIN_WIDTH);
                        ui.set_max_width(METADATA_POPUP_MAX_WIDTH);

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("File information").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("×").on_hover_text("Close").clicked() {
                                        close_popup = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        if let Some(metadata) = metadata {
                            ui.horizontal_wrapped(|ui| {
                                if let Some(source) = metadata.source.as_deref() {
                                    if ui.small_button("Copy source").clicked() {
                                        match copy_metadata_text(source) {
                                            Ok(()) => {
                                                ctx.data_mut(|data| {
                                                    data.remove::<String>(clipboard_error_id);
                                                });
                                                clipboard_error = None;
                                            }
                                            Err(error) => {
                                                ctx.data_mut(|data| {
                                                    data.insert_temp(
                                                        clipboard_error_id,
                                                        error.clone(),
                                                    );
                                                });
                                                clipboard_error = Some(error);
                                            }
                                        }
                                    }
                                }
                                if let Some((width, height)) = metadata.dimensions {
                                    if ui.small_button("Copy dimensions").clicked() {
                                        match copy_metadata_text(&format!("{width}×{height}")) {
                                            Ok(()) => {
                                                ctx.data_mut(|data| {
                                                    data.remove::<String>(clipboard_error_id);
                                                });
                                                clipboard_error = None;
                                            }
                                            Err(error) => {
                                                ctx.data_mut(|data| {
                                                    data.insert_temp(
                                                        clipboard_error_id,
                                                        error.clone(),
                                                    );
                                                });
                                                clipboard_error = Some(error);
                                            }
                                        }
                                    }
                                }
                            });
                            if let Some(error) = &clipboard_error {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    format!("Could not copy to clipboard: {error}"),
                                );
                            }

                            ui.add_space(4.0);
                            egui::ScrollArea::vertical()
                                .max_height(METADATA_POPUP_MAX_HEIGHT)
                                .auto_shrink([false, true])
                                .show(ui, |ui| self.render_file_metadata(ui, metadata));
                        } else {
                            ui.label("Metadata is being prepared…");
                        }
                    });
            });

        if close_popup {
            self.file_metadata_open = false;
            return;
        }

        // Close only for a click outside both the anchor and popup. Escape and
        // the explicit close control are handled above.
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
/// metadata, omit date, omit aspect ratio, dimensions only. If dimensions are
/// unavailable, retain the file size as the final useful fallback instead.
fn metadata_label_candidates(metadata: &FileMetadata) -> Vec<String> {
    let size = metadata
        .file_size
        .map(crate::app::ui::helpers::format_file_size);
    let dimensions = metadata
        .dimensions
        .filter(|(width, height)| *width > 0 && *height > 0)
        .map(|(width, height)| format!("{width}×{height}"));
    let aspect_ratio = metadata.aspect_ratio.as_deref();
    let date = compact_metadata_date(metadata);

    let mut candidates = vec![
        join_metadata_fields(&[
            size.as_deref(),
            dimensions.as_deref(),
            aspect_ratio,
            date.as_deref(),
        ]),
        join_metadata_fields(&[size.as_deref(), dimensions.as_deref(), aspect_ratio]),
        join_metadata_fields(&[size.as_deref(), dimensions.as_deref()]),
    ];

    if let Some(dimensions) = dimensions.as_deref() {
        candidates.push(dimensions.to_owned());
    } else if let Some(size) = size.as_deref() {
        candidates.push(size.to_owned());
    }

    // Missing metadata can make adjacent tiers identical. Remove duplicates while
    // preserving order so the fit check measures each distinct label only once.
    let mut unique = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !candidate.is_empty() && unique.last() != Some(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

/// Prefer the EXIF capture date; otherwise explicitly identify the filesystem
/// timestamp so it is never mistaken for the moment the photo was taken.
fn compact_metadata_date(metadata: &FileMetadata) -> Option<String> {
    if let Some((_, date)) = metadata
        .exif
        .iter()
        .find(|(name, value)| name == "Date Taken" && !value.trim().is_empty())
    {
        return Some(format!("Taken: {date}"));
    }

    metadata
        .modified
        .as_deref()
        .filter(|date| !date.trim().is_empty())
        .map(|date| format!("Modified: {date}"))
        .or_else(|| {
            metadata
                .created
                .as_deref()
                .filter(|date| !date.trim().is_empty())
                .map(|date| format!("Created: {date}"))
        })
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

/// Measure the full button width, including the active style's horizontal padding.
fn metadata_button_width(ui: &egui::Ui, text: &str, font_size: f32) -> f32 {
    let text_width = ui
        .painter()
        .layout_no_wrap(
            text.to_owned(),
            egui::FontId::proportional(font_size),
            ui.visuals().text_color(),
        )
        .size()
        .x;
    (text_width + ui.spacing().button_padding.x * 2.0).max(METADATA_BUTTON_MIN_WIDTH)
}

/// Select the richest label that fits. A previously displayed tier is retained
/// until a richer tier has an extra few points of clearance; downgrades happen
/// immediately so the button never gets clipped during a resize.
fn choose_metadata_tier(
    candidate_widths: &[f32],
    available_width: f32,
    previous_tier: Option<usize>,
) -> Option<usize> {
    if !available_width.is_finite() || available_width < METADATA_BUTTON_MIN_WIDTH {
        return None;
    }

    let fits = |width: f32| width.is_finite() && width <= available_width;
    let richest_fitting = candidate_widths.iter().position(|width| fits(*width));

    match previous_tier.filter(|index| {
        candidate_widths
            .get(*index)
            .is_some_and(|width| fits(*width))
    }) {
        Some(previous) => {
            let richer_tier = (0..previous).find(|index| {
                candidate_widths.get(*index).is_some_and(|width| {
                    width.is_finite() && *width + METADATA_GROW_HYSTERESIS <= available_width
                })
            });
            richer_tier.or(Some(previous))
        }
        None => richest_fitting,
    }
}

fn copy_metadata_text(value: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(value.to_owned()))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metadata() -> FileMetadata {
        FileMetadata {
            file_size: Some(4_500_000),
            dimensions: Some((1920, 1080)),
            aspect_ratio: Some("16:9".to_owned()),
            exif: vec![("Date Taken".to_owned(), "2026-10-31".to_owned())],
            ..Default::default()
        }
    }

    #[test]
    fn metadata_label_candidates_follow_the_requested_fallback_order() {
        assert_eq!(
            metadata_label_candidates(&sample_metadata()),
            vec![
                "4.29 MB - 1920×1080 - 16:9 - Taken: 2026-10-31",
                "4.29 MB - 1920×1080 - 16:9",
                "4.29 MB - 1920×1080",
                "1920×1080",
            ]
        );
    }

    #[test]
    fn missing_optional_fields_do_not_leave_empty_or_duplicate_tiers() {
        let metadata = FileMetadata {
            file_size: Some(1_024),
            dimensions: Some((1920, 1080)),
            ..Default::default()
        };
        assert_eq!(
            metadata_label_candidates(&metadata),
            vec!["1.0 KB - 1920×1080", "1920×1080"]
        );
    }

    #[test]
    fn file_size_is_retained_when_dimensions_are_unavailable() {
        let metadata = FileMetadata {
            file_size: Some(1_024),
            ..Default::default()
        };
        assert_eq!(metadata_label_candidates(&metadata), vec!["1.0 KB"]);
    }

    #[test]
    fn compact_date_identifies_capture_and_filesystem_dates_separately() {
        let taken = FileMetadata {
            modified: Some("Modified date".to_owned()),
            exif: vec![("Date Taken".to_owned(), "Capture date".to_owned())],
            ..Default::default()
        };
        assert_eq!(
            compact_metadata_date(&taken).as_deref(),
            Some("Taken: Capture date")
        );

        let modified = FileMetadata {
            modified: Some("Modified date".to_owned()),
            created: Some("Created date".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            compact_metadata_date(&modified).as_deref(),
            Some("Modified: Modified date")
        );
    }

    #[test]
    fn tier_selection_uses_hysteresis_for_upgrades_but_not_downgrades() {
        let widths = [100.0, 75.0, 50.0];

        // The previous compact tier remains selected until the richer tier has
        // enough spare room to avoid flicker around the exact fit boundary.
        assert_eq!(choose_metadata_tier(&widths, 103.0, Some(1)), Some(1));
        assert_eq!(choose_metadata_tier(&widths, 104.0, Some(1)), Some(0));

        // A compact tier that no longer fits is immediately replaced.
        assert_eq!(choose_metadata_tier(&widths, 49.0, Some(2)), None);
        assert_eq!(choose_metadata_tier(&widths, 51.0, Some(2)), Some(2));
    }

    #[test]
    fn invalid_width_budgets_never_select_a_tier() {
        let widths = [100.0, 75.0, 50.0];
        assert_eq!(choose_metadata_tier(&widths, 0.0, None), None);
        assert_eq!(choose_metadata_tier(&widths, f32::NAN, None), None);
        assert_eq!(choose_metadata_tier(&widths, f32::INFINITY, None), None);
    }
}
