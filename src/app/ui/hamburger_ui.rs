use crate::app::types::ViewerApp;
use crate::app::ui::toolbar_frame;
use crate::shortcuts::ViewerCommand;
use eframe::egui;

const MENU_OFFSET: f32 = 8.0;
const HAMBURGER_SIZE: f32 = 28.0;

impl ViewerApp {
    /// Render the hamburger button.
    pub fn render_hamburger_ui(&mut self, ui: &mut egui::Ui) {
        let icon = "☰";

        let button = ui
            .add(
                egui::Button::new(egui::RichText::new(icon).size(24.0))
                    .min_size(egui::vec2(HAMBURGER_SIZE, HAMBURGER_SIZE)),
            )
            .on_hover_text("Menu");

        if button.clicked() {
            self.hamburger_menu_open = !self.hamburger_menu_open;
        }
    }

    pub fn render_hamburger_menu_ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
    ) {
        if !self.hamburger_menu_open {
            return;
        }

        toolbar_frame(ctx).show(ui, |ui| {
            ui.vertical(|ui| {
                // Open File
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("📂  Open File").size(14.0),
                        )
                        .min_size(egui::vec2(140.0, 30.0)),
                    )
                    .on_hover_text("Load an Image or Archive")
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFile);
                    self.hamburger_menu_open = false;
                }

                // Open Folder
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("📁  Open Folder").size(14.0),
                        )
                        .min_size(egui::vec2(140.0, 30.0)),
                    )
                    .on_hover_text("Open a Folder of Images")
                    .clicked()
                {
                    self.handle_command(ctx, ViewerCommand::OpenFolder);
                    self.hamburger_menu_open = false;
                }

                ui.separator();

                // Settings
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("⚙  Settings").size(14.0),
                        )
                        .min_size(egui::vec2(140.0, 30.0)),
                    )
                    .on_hover_text("Settings")
                    .clicked()
                {
                    crate::app::ui::toggle_settings_menu(self);
                    self.hamburger_menu_open = false;
                }

                // Help
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("❓  Help").size(14.0),
                        )
                        .min_size(egui::vec2(140.0, 30.0)),
                    )
                    .on_hover_text("Keyboard shortcuts and features")
                    .clicked()
                {
                    self.toggle_help_menu();
                    self.hamburger_menu_open = false;
                }

                // Loading indicator
                if self.is_loading() {
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(
                            egui::RichText::new("Loading...").size(12.0),
                        );
                    });
                }

                // Exit
                ui.separator();

                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("X  Exit").size(14.0),
                        )
                        .min_size(egui::vec2(140.0, 30.0)),
                    )
                    .on_hover_text("Exit")
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // Close the menu when clicking outside of it.
        let menu_rect = ui.min_rect();

        ctx.input(|input| {
            if let Some(pointer_pos) = input.pointer.interact_pos() {
                if input.pointer.any_pressed() {
                    let hamburger_rect = egui::Rect::from_min_size(
                        egui::pos2(MENU_OFFSET, MENU_OFFSET),
                        egui::vec2(HAMBURGER_SIZE, HAMBURGER_SIZE),
                    );

                    if !menu_rect.contains(pointer_pos)
                        && !hamburger_rect.contains(pointer_pos)
                    {
                        self.hamburger_menu_open = false;
                    }
                }
            }
        });
    }
}