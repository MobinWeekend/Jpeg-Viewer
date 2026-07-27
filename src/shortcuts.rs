use eframe::egui;
use std::collections::HashMap;


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerCommand {
    NextImage,
    PreviousImage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ToggleFit,
    OpenFile,
}


#[derive(Debug, Clone)]
pub struct KeyBindings {
    pub keys: HashMap<ViewerCommand, egui::Key>,
}


impl Default for KeyBindings {
    fn default() -> Self {
        let mut keys = HashMap::new();

        keys.insert(
            ViewerCommand::NextImage,
            egui::Key::ArrowRight,
        );

        keys.insert(
            ViewerCommand::PreviousImage,
            egui::Key::ArrowLeft,
        );

        keys.insert(
            ViewerCommand::ZoomIn,
            egui::Key::Plus,
        );

        keys.insert(
            ViewerCommand::ZoomOut,
            egui::Key::Minus,
        );

        keys.insert(
            ViewerCommand::ResetZoom,
            egui::Key::R,
        );

        keys.insert(
            ViewerCommand::ToggleFit,
            egui::Key::F,
        );

        keys.insert(
            ViewerCommand::OpenFile,
            egui::Key::O,
        );

        Self {
            keys,
        }
    }
}


pub fn handle_keyboard(
    ctx: &egui::Context,
    bindings: &KeyBindings,
) -> Vec<ViewerCommand> {

    let mut commands = Vec::new();

    ctx.input(|input| {

        for (command, key) in &bindings.keys {

            if input.key_pressed(*key) {
                commands.push(*command);
            }

        }

    });

    commands
}

pub fn handle_mouse(
    ctx: &egui::Context,
    is_ctrl_invert: bool,
    mouse_over: bool,
) -> Vec<ViewerCommand> {

    let mut commands = Vec::new();

    if !mouse_over {
        return commands;
    }

    ctx.input(|input| {
        let scroll = input.raw_scroll_delta.y;

        if scroll == 0.0 {
            return;
        }

        let ctrl = input.modifiers.ctrl;

        let zooming =
            (!is_ctrl_invert && ctrl)
            ||
            (is_ctrl_invert && !ctrl);


        if zooming {
            if scroll > 0.0 {
                commands.push(ViewerCommand::ZoomIn);
            } else {
                commands.push(ViewerCommand::ZoomOut);
            }
        }
        else {
            if scroll > 0.0 {
                commands.push(ViewerCommand::PreviousImage);
            } else {
                commands.push(ViewerCommand::NextImage);
            }
        }
    });

    commands
}