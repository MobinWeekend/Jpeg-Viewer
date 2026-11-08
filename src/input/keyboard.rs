//! Keyboard shortcut defaults and command dispatch.

use eframe::egui;
use std::collections::HashMap;

use super::shortcuts::{InputBindings, KeyBinding, ViewerCommand};

pub(super) fn default_bindings() -> HashMap<ViewerCommand, Vec<KeyBinding>> {
    let mut keyboard = HashMap::new();

    keyboard.insert(
        ViewerCommand::Settings,
        vec![KeyBinding::plain(egui::Key::F5)],
    );

    keyboard.insert(ViewerCommand::Help, vec![KeyBinding::plain(egui::Key::F1)]);

    keyboard.insert(
        ViewerCommand::NextImage,
        vec![
            KeyBinding::plain(egui::Key::ArrowRight),
            KeyBinding::plain(egui::Key::D),
        ],
    );

    keyboard.insert(
        ViewerCommand::PreviousImage,
        vec![
            KeyBinding::plain(egui::Key::ArrowLeft),
            KeyBinding::plain(egui::Key::A),
        ],
    );

    keyboard.insert(
        ViewerCommand::ZoomIn,
        vec![KeyBinding::plain(egui::Key::Plus)],
    );

    keyboard.insert(
        ViewerCommand::ZoomOut,
        vec![KeyBinding::plain(egui::Key::Minus)],
    );

    keyboard.insert(
        ViewerCommand::ResetZoom,
        vec![
            KeyBinding::plain(egui::Key::W),
            KeyBinding::plain(egui::Key::ArrowUp),
        ],
    );

    keyboard.insert(
        ViewerCommand::MakeFit,
        vec![
            KeyBinding::plain(egui::Key::S),
            KeyBinding::plain(egui::Key::ArrowDown),
            KeyBinding::plain(egui::Key::Num0),
        ],
    );
    keyboard.insert(
        ViewerCommand::WrapWindowToImage,
        vec![KeyBinding {
            key: egui::Key::W,
            ctrl: Some(true),
            shift: Some(false),
            alt: Some(true),
        }],
    );

    keyboard.insert(
        ViewerCommand::OpenFile,
        vec![KeyBinding::ctrl(egui::Key::O)],
    );
    keyboard.insert(
        ViewerCommand::OpenFolder,
        vec![KeyBinding::ctrl_shift(egui::Key::O)],
    );

    keyboard.insert(
        ViewerCommand::ToggleFullscreen,
        vec![
            KeyBinding::plain(egui::Key::F11),
            KeyBinding::plain(egui::Key::Enter),
            KeyBinding::plain(egui::Key::F),
        ],
    );
    keyboard.insert(
        ViewerCommand::ToggleGifPlay,
        vec![KeyBinding::plain(egui::Key::Space)],
    );

    // GIF speed controls
    keyboard.insert(
        ViewerCommand::GifSpeedHalf,
        vec![KeyBinding::plain(egui::Key::OpenBracket)],
    );
    keyboard.insert(
        ViewerCommand::GifSpeedUp,
        vec![KeyBinding::plain(egui::Key::CloseBracket)],
    );
    keyboard.insert(
        ViewerCommand::GifSpeedReset,
        vec![KeyBinding::plain(egui::Key::P)],
    );

    // Slideshow controls
    keyboard.insert(
        ViewerCommand::ToggleSlideshow,
        vec![KeyBinding::plain(egui::Key::L)], // L for Loop/Slideshow
    );
    keyboard.insert(
        ViewerCommand::SlideshowSpeedUp,
        vec![KeyBinding::plain(egui::Key::Period)], // > for faster
    );
    keyboard.insert(
        ViewerCommand::SlideshowSpeedDown,
        vec![KeyBinding::plain(egui::Key::Comma)], // < for slower
    );
    keyboard.insert(
        ViewerCommand::CopyPath,
        vec![KeyBinding::ctrl_shift(egui::Key::C)],
    );
    keyboard.insert(
        ViewerCommand::CopyImage,
        vec![KeyBinding::ctrl(egui::Key::C)],
    );

    keyboard.insert(
        ViewerCommand::JumpToFirst,
        vec![
            KeyBinding::plain(egui::Key::Home),
            KeyBinding::ctrl(egui::Key::ArrowLeft),
            KeyBinding::ctrl(egui::Key::A),
        ],
    );

    keyboard.insert(
        ViewerCommand::JumpToLast,
        vec![
            KeyBinding::plain(egui::Key::End),
            KeyBinding::ctrl(egui::Key::ArrowRight),
            KeyBinding::ctrl(egui::Key::D),
        ],
    );

    keyboard.insert(
        ViewerCommand::ShowFileSidebar,
        vec![
            KeyBinding::plain(egui::Key::T),
            KeyBinding::plain(egui::Key::F3),
        ],
    );

    keyboard.insert(
        ViewerCommand::ToggleCompact,
        vec![KeyBinding::plain(egui::Key::F4)],
    );

    keyboard
}

pub fn handle_keyboard(ctx: &egui::Context, bindings: &InputBindings) -> Vec<ViewerCommand> {
    let mut commands = Vec::new();
    if ctx.wants_keyboard_input() {
        return commands;
    }

    ctx.input(|input| {
        for (command, bindings) in &bindings.keyboard {
            if bindings.iter().any(|binding| binding.matches(input)) {
                commands.push(*command);
            }
        }
    });

    commands
}
