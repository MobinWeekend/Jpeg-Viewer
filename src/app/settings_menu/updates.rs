use crate::app::types::ViewerApp;
use crate::app::updater::{UpdateInstallStatus, UpdateStatus};
use eframe::egui;

pub(super) fn render(app: &mut ViewerApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.collapsing(egui::RichText::new("🔄 Software updates").size(15.0), |ui| {
        ui.add_space(4.0);

        let status = app.updater.status().clone();
        let checking = matches!(&status, UpdateStatus::Checking);
        let install_status = app.updater.install_status().clone();
        let installing = install_status.is_busy();
        let button_label = match &status {
            UpdateStatus::Idle => "Check for updates",
            UpdateStatus::Checking => "Checking…",
            UpdateStatus::UpToDate { .. } => "Up to date · Check again",
            UpdateStatus::UpdateAvailable { .. } => "Update available · Check again",
            UpdateStatus::Failed { .. } => "Check failed · Retry",
        };

        ui.group(|ui| {
            ui.label(
                egui::RichText::new(format!("Current version: v{}", env!("CARGO_PKG_VERSION")))
                    .small()
                    .weak(),
            );

            ui.horizontal(|ui| {
                let response = ui.add_enabled(
                    !checking,
                    egui::Button::new(button_label).min_size(egui::vec2(0.0, 28.0)),
                );
                if checking {
                    ui.spinner();
                }
                if response.clicked()
                    && let Some(attempted_at) = app.updater.start_manual_check(ctx)
                    && let Err(error) = app.settings_manager.update(|settings| {
                        settings.update_check_last_attempt_unix = attempted_at;
                    })
                {
                    eprintln!("Could not save update-check timestamp: {error}");
                }
            });

            match status {
                UpdateStatus::Idle => {
                    ui.label(
                        egui::RichText::new(
                            "Automatically checks for stable releases once every 7 days.",
                        )
                        .small()
                        .weak(),
                    );
                }
                UpdateStatus::Checking => {
                    ui.label("Checking GitHub for the latest stable release…");
                }
                UpdateStatus::UpToDate { latest_version } => {
                    ui.label(format!(
                        "You're up to date. Latest stable version: v{latest_version}."
                    ));
                }
                UpdateStatus::UpdateAvailable { release } => {
                    ui.label(
                        egui::RichText::new(format!(
                            "Update available: {} (v{}).",
                            release.title, release.version
                        ))
                        .strong(),
                    );
                    let install_response = ui.add_enabled(
                        !installing,
                        egui::Button::new(format!("Download and install v{}", release.version))
                            .min_size(egui::vec2(0.0, 30.0)),
                    );
                    if installing {
                        ui.spinner();
                    }
                    if install_response.clicked()
                        && let Err(error) = app.updater.start_install(ctx)
                    {
                        app.updater.set_install_failed(error);
                    }
                    if ui.button("View release and download options").clicked()
                        && let Err(error) = open::that(&release.url)
                    {
                        eprintln!("Could not open GitHub release page: {error}");
                    }
                    if let Some(published_at) = release.published_at.as_deref() {
                        ui.label(
                            egui::RichText::new(format!("Published: {published_at}"))
                                .small()
                                .weak(),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!("Release tag: {}", release.tag_name))
                            .small()
                            .weak(),
                    );
                }
                UpdateStatus::Failed { message } => {
                    ui.label(
                        egui::RichText::new(format!("Couldn't check for updates: {message}"))
                            .color(ui.visuals().error_fg_color),
                    );
                    if ui.link("Open GitHub Releases manually").clicked()
                        && let Err(error) =
                            open::that("https://github.com/MobinWeekend/Jpeg-Viewer/releases")
                    {
                        eprintln!("Could not open GitHub Releases: {error}");
                    }
                }
            }

            let last_attempt = app.settings_manager.get().update_check_last_attempt_unix;
            if last_attempt > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "Last check attempt: {}",
                        format_local_time(last_attempt)
                    ))
                    .small()
                    .weak(),
                );
            }
            match install_status {
                UpdateInstallStatus::Idle => {
                    ui.label(
                        egui::RichText::new(
                            "The installer automatically selects the matching operating system, architecture, and installation type.",
                        )
                        .small()
                        .weak(),
                    );
                }
                UpdateInstallStatus::Downloading { asset_name } => {
                    ui.label(format!("Downloading update: {asset_name}"));
                }
                UpdateInstallStatus::Installing { asset_name } => {
                    ui.label(format!("Installing update: {asset_name}"));
                }
                UpdateInstallStatus::Restarting => {
                    ui.label("Restarting with the updated version…");
                }
                UpdateInstallStatus::Launched => {
                    ui.label(
                        egui::RichText::new(
                            "The platform installer has been launched. Complete any authorization prompts, then restart JPEG Viewer after it finishes.",
                        )
                        .small(),
                    );
                }
                UpdateInstallStatus::Failed { message } => {
                    ui.label(
                        egui::RichText::new(format!("Update installation failed: {message}"))
                            .color(ui.visuals().error_fg_color),
                    );
                }
            }
        });
        ui.add_space(4.0);
    });
}

fn format_local_time(timestamp: i64) -> String {
    use chrono::TimeZone;

    chrono::Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|date_time| date_time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}
