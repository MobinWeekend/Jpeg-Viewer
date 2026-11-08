//! Shared shortcut definitions and modifier-matching helpers.

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
    WrapWindowToImage,
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
    Help,
    CopyPath,
    CopyImage,
    RebuildIndex,
    // Slideshow commands
    ToggleSlideshow,
    SlideshowSpeedUp,
    SlideshowSpeedDown,
    ShowFileSidebar,
    ToggleCompact,
}

#[derive(Debug, Clone)]
pub enum MouseAction {
    Click,
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
            MouseAction::Click => input.pointer.button_clicked(self.button),
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
        Self {
            keyboard: super::keyboard::default_bindings(),
            mouse: super::mouse::default_bindings(),
        }
    }
}
