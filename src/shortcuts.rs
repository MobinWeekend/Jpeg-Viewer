//needs to be splited down the road
use eframe::egui;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerCommand {
    NextImage,
    PreviousImage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    MakeFit,
    OpenFile,
    OpenFolder,
    ToggleFullscreen,
    JumpToFirst,
    JumpToLast,
    ToggleGifPlay,
    GifSpeedHalf,
    GifSpeedUp,
    GifSpeedReset,
    Settings,
    CopyPath,
    CopyImage,
    // Slideshow commands
    ToggleSlideshow,
    SlideshowSpeedUp,
    SlideshowSpeedDown,
}

#[derive(Debug, Clone)]
pub enum MouseAction {
    DoubleClick,
}

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub key: egui::Key,
    pub ctrl: Option<bool>,
    pub shift: Option<bool>,
    pub alt: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct MouseBinding {
    pub button: egui::PointerButton,
    pub action: MouseAction,
    pub ctrl: Option<bool>,
    pub shift: Option<bool>,
    pub alt: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct InputBindings {
    pub keyboard: HashMap<ViewerCommand, Vec<KeyBinding>>,
    pub mouse: HashMap<ViewerCommand, Vec<MouseBinding>>,
}

impl KeyBinding {
    pub fn plain(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: Some(false),
            shift: Some(false),
            alt: Some(false),
        }
    }
    pub fn ctrl(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: Some(true),
            shift: Some(false),
            alt: Some(false),
        }
    }
    pub fn ctrl_shift(key: egui::Key) -> Self {
        Self {
            key,
            ctrl: Some(true),
            shift: Some(true),
            alt: Some(false),
        }
    }

    pub fn matches(&self, input: &egui::InputState) -> bool {
        if !input.key_pressed(self.key) {
            return false;
        }

        if let Some(ctrl) = self.ctrl {
            if input.modifiers.ctrl != ctrl {
                return false;
            }
        }

        if let Some(shift) = self.shift {
            if input.modifiers.shift != shift {
                return false;
            }
        }

        if let Some(alt) = self.alt {
            if input.modifiers.alt != alt {
                return false;
            }
        }

        true
    }
}

impl MouseBinding {
    pub fn plain(button: egui::PointerButton, action: MouseAction) -> Self {
        Self {
            button,
            action,
            ctrl: Some(false),
            shift: Some(false),
            alt: Some(false),
        }
    }
    pub fn matches(&self, input: &egui::InputState) -> bool {
        let clicked = match self.action {
            MouseAction::DoubleClick => input.pointer.button_double_clicked(self.button),
        };

        if !clicked {
            return false;
        }

        if let Some(ctrl) = self.ctrl {
            if input.modifiers.ctrl != ctrl {
                return false;
            }
        }

        if let Some(shift) = self.shift {
            if input.modifiers.shift != shift {
                return false;
            }
        }

        if let Some(alt) = self.alt {
            if input.modifiers.alt != alt {
                return false;
            }
        }

        true
    }
}

impl Default for InputBindings {
    fn default() -> Self {
        let mut keyboard = HashMap::new();

        keyboard.insert(
            ViewerCommand::Settings,
            vec![KeyBinding::plain(egui::Key::Tab)],
        );

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

        let mut mouse = HashMap::new();

        mouse.insert(
            ViewerCommand::ToggleFullscreen,
            vec![MouseBinding::plain(
                egui::PointerButton::Middle,
                MouseAction::DoubleClick,
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

        Self { keyboard, mouse }
    }
}

pub fn handle_keyboard(ctx: &egui::Context, bindings: &InputBindings) -> Vec<ViewerCommand> {
    let mut commands = Vec::new();

    ctx.input(|input| {
        for (command, bindings) in &bindings.keyboard {
            if bindings.iter().any(|binding| binding.matches(input)) {
                commands.push(*command);
            }
        }
    });

    commands
}

pub fn handle_mouse(
    ctx: &egui::Context,
    bindings: &InputBindings,
    mouse_over: bool,
    b_ctrl_invert: bool,
) -> Vec<ViewerCommand> {
    let mut commands = Vec::new();

    if !mouse_over {
        return commands;
    }

    ctx.input(|input| {
        for (command, bindings) in &bindings.mouse {
            if bindings.iter().any(|binding| binding.matches(input)) {
                commands.push(*command);
            }
        }
        let scroll = input.raw_scroll_delta.y;

        if scroll != 0.0 {
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
