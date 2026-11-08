//! Mouse shortcut defaults, pointer handling, and scroll gestures.

use eframe::egui;
use std::collections::HashMap;

use super::shortcuts::{InputBindings, MouseAction, MouseBinding, ViewerCommand};

pub(super) fn default_bindings() -> HashMap<ViewerCommand, Vec<MouseBinding>> {
    let mut mouse = HashMap::new();

    mouse.insert(
        ViewerCommand::ToggleFullscreen,
        vec![MouseBinding::plain(
            egui::PointerButton::Middle,
            MouseAction::Click,
        )],
    );

    mouse.insert(
        ViewerCommand::OpenFile,
        vec![MouseBinding::plain(
            egui::PointerButton::Primary,
            MouseAction::DoubleClick,
        )],
    );

    mouse.insert(
        ViewerCommand::MakeFit,
        vec![MouseBinding::plain(
            egui::PointerButton::Secondary,
            MouseAction::DoubleClick,
        )],
    );

    mouse
}

const MOUSE_COMMANDS_ALLOWED_OVER_UI: &[ViewerCommand] = &[ViewerCommand::ToggleFullscreen];

pub fn handle_mouse(
    ctx: &egui::Context,
    bindings: &InputBindings,
    mouse_over: bool,
    over_ui: bool,
    b_ctrl_invert: bool,
) -> Vec<ViewerCommand> {
    let mut commands = Vec::new();

    if !mouse_over {
        return commands;
    }

    ctx.input(|input| {
        for (command, bindings) in &bindings.mouse {
            // When over UI, only allow whitelisted commands.
            if over_ui && !MOUSE_COMMANDS_ALLOWED_OVER_UI.contains(command) {
                continue;
            }

            if bindings.iter().any(|binding| binding.matches(input)) {
                commands.push(*command);
            }
        }

        let scroll = input.raw_scroll_delta.y;

        // Don't allow image scrolling/zooming while over UI.
        if scroll != 0.0 && !over_ui {
            let ctrl = input.modifiers.ctrl;
            let zooming = (!b_ctrl_invert && ctrl) || (b_ctrl_invert && !ctrl);

            if zooming {
                commands.push(if scroll > 0.0 {
                    ViewerCommand::ZoomIn
                } else {
                    ViewerCommand::ZoomOut
                });
            } else {
                commands.push(if scroll > 0.0 {
                    ViewerCommand::PreviousImage
                } else {
                    ViewerCommand::NextImage
                });
            }
        }
    });

    commands
}
