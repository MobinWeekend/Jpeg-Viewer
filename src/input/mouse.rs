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

    // Double-left-click fits the current image. Opening a file is an explicit
    // File-menu / keyboard action rather than a gesture that conflicts with
    // image viewing and context-menu interactions.
    mouse.insert(
        ViewerCommand::MakeFit,
        vec![MouseBinding::plain(
            egui::PointerButton::Primary,
            MouseAction::DoubleClick,
        )],
    );

    mouse
}

const MOUSE_COMMANDS_ALLOWED_OVER_UI: &[ViewerCommand] = &[ViewerCommand::ToggleFullscreen];

/// Convert wheel movement events from this frame into viewer commands.
///
/// Use the modifiers recorded on each event rather than `InputState::modifiers`:
/// on some platforms the aggregate modifier state can lag the native wheel
/// event. This is especially important for Invert scroll zoom, where Ctrl+wheel
/// must navigate while an unmodified wheel gesture zooms.
fn scroll_commands_from_events(
    events: &[egui::Event],
    b_ctrl_invert: bool,
) -> Vec<ViewerCommand> {
    events
        .iter()
        .filter_map(|event| match event {
            egui::Event::MouseWheel {
                delta,
                phase: egui::TouchPhase::Move,
                modifiers,
                ..
            } if delta.y.abs() > f32::EPSILON => {
                let ctrl = modifiers.ctrl;
                let zooming = if b_ctrl_invert { !ctrl } else { ctrl };
                Some(if zooming {
                    if delta.y > 0.0 {
                        ViewerCommand::ZoomIn
                    } else {
                        ViewerCommand::ZoomOut
                    }
                } else if delta.y > 0.0 {
                    ViewerCommand::PreviousImage
                } else {
                    ViewerCommand::NextImage
                })
            }
            _ => None,
        })
        .collect()
}

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

        // Navigation is discrete: use only wheel movement events received this
        // frame, never egui's smoothed delta from previous frames. Each event
        // carries its own modifier snapshot so Ctrl+wheel remains navigation
        // when Invert scroll zoom is enabled.
        if !over_ui {
            commands.extend(scroll_commands_from_events(&input.events, b_ctrl_invert));
        }
    });

    commands
}


#[cfg(test)]
mod tests {
    use super::scroll_commands_from_events;
    use eframe::egui;
    use crate::input::shortcuts::ViewerCommand;

    fn wheel_event(phase: egui::TouchPhase, y: f32, ctrl: bool) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, y),
            phase,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers {
                ctrl,
                ..Default::default()
            },
        }
    }

    #[test]
    fn invert_scroll_zoom_keeps_ctrl_wheel_for_navigation() {
        let events = [
            wheel_event(egui::TouchPhase::Move, 120.0, true),
            wheel_event(egui::TouchPhase::Move, -120.0, true),
        ];
        assert_eq!(
            scroll_commands_from_events(&events, true),
            vec![ViewerCommand::PreviousImage, ViewerCommand::NextImage]
        );
    }

    #[test]
    fn invert_scroll_zoom_uses_plain_wheel_for_zoom() {
        let events = [
            wheel_event(egui::TouchPhase::Move, 120.0, false),
            wheel_event(egui::TouchPhase::Move, -120.0, false),
        ];
        assert_eq!(
            scroll_commands_from_events(&events, true),
            vec![ViewerCommand::ZoomIn, ViewerCommand::ZoomOut]
        );
    }

    #[test]
    fn normal_scroll_zoom_uses_ctrl_wheel_for_zoom() {
        let events = [
            wheel_event(egui::TouchPhase::Move, 120.0, true),
            wheel_event(egui::TouchPhase::Move, -120.0, true),
        ];
        assert_eq!(
            scroll_commands_from_events(&events, false),
            vec![ViewerCommand::ZoomIn, ViewerCommand::ZoomOut]
        );
    }

    #[test]
    fn non_movement_wheel_phases_do_not_trigger_commands() {
        let events = [
            wheel_event(egui::TouchPhase::Start, 120.0, false),
            wheel_event(egui::TouchPhase::End, -120.0, true),
        ];
        assert!(scroll_commands_from_events(&events, true).is_empty());
    }
}
