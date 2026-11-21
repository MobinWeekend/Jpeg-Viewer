//! Window sizing around the active image.
use super::types::ViewerApp;
use eframe::egui;

impl ViewerApp {
    /// Resize the window once the active image dimensions are available.
    /// The passed size is the decoded image size in pixels (treated as logical
    /// points for a 1:1 fit, then constrained by the selected limits).
    pub fn wrap_window_around_image(
        &mut self,
        ctx: &egui::Context,
        image_size: egui::Vec2,
    ) -> bool {
        if !image_size.x.is_finite()
            || !image_size.y.is_finite()
            || image_size.x <= 0.0
            || image_size.y <= 0.0
        {
            self.window_fit_pending = false;
            return false;
        }
        let (fullscreen, maximized, reported_monitor_size, outer_rect, inner_size) =
            ctx.input(|input| {
                let viewport = input.viewport();
                (
                    viewport.fullscreen.unwrap_or(false),
                    viewport.maximized.unwrap_or(false),
                    viewport.monitor_size,
                    viewport.outer_rect,
                    viewport.inner_rect.map(|rect| rect.size()),
                )
            });
        // Fitting is a windowed operation. Do not queue a surprise resize for later
        // when the user leaves fullscreen or restores a maximized window.
        if fullscreen || maximized {
            self.window_fit_pending = false;
            return false;
        }

        let settings = self.settings_manager.get().clone();
        let monitors = crate::platform::monitors::available_monitors();
        let selected_monitor = crate::platform::monitors::resolve_default_monitor_at(
            &monitors,
            &settings.window_default_monitor,
            outer_rect,
        );
        let monitor_padding = if settings.fit_window_limit == "monitor" {
            settings.fit_window_monitor_offset.clamp(0.0, 4096.0)
        } else {
            0.0
        };
        let selected_bounds =
            selected_monitor.map(|monitor| monitor.inset_bounds(monitor_padding));
        let reported_usable_size = reported_monitor_size.map(|size| {
            egui::vec2(
                (size.x - monitor_padding * 2.0).max(1.0),
                (size.y - monitor_padding * 2.0).max(1.0),
            )
        });
        let monitor = selected_bounds
            .map(|bounds| bounds.size())
            .or(reported_usable_size)
            .unwrap_or_else(|| {
                ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.size()))
                    .unwrap_or(egui::vec2(1920.0, 1080.0))
            });
        let zoom = ctx.zoom_factor().max(0.01);
        // The monitor offset is an inset on every edge, already reflected in
        // `monitor`; do not subtract it again from the width and height.
        let constrained_monitor = egui::vec2(monitor.x.max(512.0), monitor.y.max(512.0));
        let (min_width, max_width, min_height, max_height) =
            if settings.fit_window_limit == "custom" {
                (
                    settings.fit_window_min_width,
                    settings.fit_window_max_width,
                    settings.fit_window_min_height,
                    settings.fit_window_max_height,
                )
            } else {
                (
                    512.0,
                    constrained_monitor.x,
                    512.0,
                    constrained_monitor.y,
                )
            };

        let left_panel = if settings.show_file_sidebar {
            self.sidebar_actual_width.max(0.0)
        } else {
            0.0
        };
        let right_panel = self.right_sidebar_actual_width.max(0.0);
        let padding = if settings.fit_window_to_image {
            let profile = crate::ui_theme::current_look_profile(ctx);
            if matches!(profile, "win98" | "cosmic" | "gnome") {
                let safe_top = self.effective_view_padding(ctx)[0];
                [safe_top, 0.0, 0.0, 0.0]
            } else {
                [0.0; 4]
            }
        } else {
            self.effective_view_padding(ctx)
        };
        let horizontal_padding = padding[1] + padding[3];
        let vertical_padding = padding[0] + padding[2];
        let top_chrome = (if !settings.show_titlebar {
            crate::app::ui::custom_titlebar_height(ctx)
        } else {
            0.0
        }) + (if crate::app::ui::uses_desktop_menu_bar(ctx) {
            crate::app::ui::desktop_menu_bar_height(ctx)
        } else {
            0.0
        });

        let fixed_width = left_panel + right_panel + horizontal_padding;
        let fixed_height = top_chrome + vertical_padding;
        let min_image_width = (min_width - fixed_width).max(1.0);
        let max_image_width = (max_width - fixed_width).max(1.0);
        let min_image_height = (min_height - fixed_height).max(1.0);
        let max_image_height = (max_height - fixed_height).max(1.0);

        let mut fitted_image = if settings.fit_window_preserve_aspect {
            let min_scale = (min_image_width / image_size.x).max(min_image_height / image_size.y);
            let max_scale = (max_image_width / image_size.x).min(max_image_height / image_size.y);
            // If the custom constraints are contradictory, prioritize the maximum
            // dimensions so the window never exceeds the selected upper bounds.
            let scale = if max_scale >= min_scale {
                1.0_f32.clamp(min_scale, max_scale)
            } else {
                max_scale
            };
            image_size * scale
        } else {
            egui::vec2(
                image_size.x.clamp(min_image_width, max_image_width),
                image_size.y.clamp(min_image_height, max_image_height),
            )
        };
        if !fitted_image.x.is_finite() || !fitted_image.y.is_finite() {
            self.window_fit_pending = false;
            return false;
        }
        fitted_image.x = fitted_image.x.max(1.0);
        fitted_image.y = fitted_image.y.max(1.0);

        let target_inner = egui::vec2(
            (fitted_image.x + fixed_width).clamp(min_width, max_width),
            (fitted_image.y + fixed_height).clamp(min_height, max_height),
        );
        let native_inner = target_inner / zoom;
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(native_inner));

        let frame_delta = match (outer_rect, inner_size) {
            (Some(outer), Some(inner)) => (outer.size() - inner).max(egui::Vec2::ZERO),
            _ => egui::Vec2::ZERO,
        };
        // Viewport geometry and enumerated monitor bounds use egui logical points;
        // only InnerSize is converted to native logical points above.
        let target_outer = target_inner + frame_delta;
        if settings.fit_window_center {
            if let Some(bounds) = selected_bounds {
                let centered = bounds.min
                    + ((constrained_monitor - target_outer) * 0.5).max(egui::Vec2::ZERO);
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(centered));
            }
        } else if let (Some(outer), Some(bounds)) = (outer_rect, selected_bounds) {
            // Choosing a default monitor also affects image-fit commands: move the
            // window there when it currently lives on a different display, without
            // changing the user's preference for centered vs. top-left placement.
            let max_pos = (bounds.max - target_outer).max(bounds.min);
            let outside_usable_area = outer.min.x < bounds.min.x - 1.0
                || outer.min.y < bounds.min.y - 1.0
                || outer.max.x > bounds.max.x + 1.0
                || outer.max.y > bounds.max.y + 1.0;
            if !bounds.contains(outer.center()) || outside_usable_area {
                let target_pos = egui::pos2(
                    outer.min.x.clamp(bounds.min.x, max_pos.x),
                    outer.min.y.clamp(bounds.min.y, max_pos.y),
                );
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target_pos));
            }
        }
        self.window_fit_pending = false;
        true
    }

    /// Request the quick command or the automatic-on-navigation behavior.
    pub fn request_wrap_window_around_image(&mut self, ctx: &egui::Context) {
        if self.image_entries.is_empty()
            || ctx.input(|input| {
                let viewport = input.viewport();
                viewport.fullscreen.unwrap_or(false) || viewport.maximized.unwrap_or(false)
            })
        {
            return;
        }
        self.window_fit_pending = true;
        self.b_fit_to_window = true;
        self.b_zoom_used = false;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
        ctx.request_repaint();
    }

    /// Resolve the padding used for the active Look profile, including its per-look fine tune.
    pub fn effective_view_padding(&self, ctx: &egui::Context) -> [f32; 4] {
        let settings = self.settings_manager.get();
        let profile = crate::ui_theme::current_look_profile(ctx);
        let placement =
            crate::ui_theme::resolve_main_ui_placement(&settings.main_ui_placement_preference, ctx);
        let base = crate::app::settings_menu::viewing::default_look_padding(profile, placement);
        match settings.view_padding_mode.as_str() {
            "custom" => settings
                .view_padding_custom
                .map(|value| value.clamp(0.0, 400.0)),
            "fine_tune" => {
                let delta = settings
                    .view_padding_fine_tune
                    .get(profile)
                    .copied()
                    .unwrap_or([0.0; 4]);
                std::array::from_fn(|index| (base[index] + delta[index]).clamp(0.0, 400.0))
            }
            _ => base,
        }
    }
}
