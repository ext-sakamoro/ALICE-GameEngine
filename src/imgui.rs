//! Immediate-mode GUI builder (egui / Bevy `bevy_egui` style).
//!
//! Game logic constructs a fresh UI every frame by chaining
//! [`UiContext`] methods. The context records each widget into a
//! list of [`UiCommand`]s that the renderer can later draw or that
//! tests can inspect. Widget interactions (clicks, slider drags)
//! flow back through [`UiInteraction`] for the next frame.

use crate::math::Vec2;

#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    Label {
        position: Vec2,
        text: String,
    },
    Button {
        position: Vec2,
        size: Vec2,
        label: String,
        hovered: bool,
        pressed: bool,
    },
    Slider {
        position: Vec2,
        size: Vec2,
        label: String,
        value: f32,
        min: f32,
        max: f32,
    },
    Checkbox {
        position: Vec2,
        label: String,
        checked: bool,
    },
}

/// What the user is doing this frame. Provided by the host
/// application from its actual input layer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UiInput {
    pub cursor: Vec2,
    pub primary_pressed: bool,
    pub primary_just_released: bool,
}

/// One-frame builder. Create with [`UiContext::new`], build the UI
/// by calling widgets, then read [`UiContext::commands`] or
/// [`UiContext::interactions`].
#[derive(Debug, Clone)]
pub struct UiContext {
    pub input: UiInput,
    commands: Vec<UiCommand>,
    interactions: Vec<UiInteraction>,
    cursor_y: f32,
    indent: f32,
    spacing: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiInteraction {
    ButtonClicked { label: String },
    SliderChanged { label: String, new_value: f32 },
    CheckboxToggled { label: String, new_value: bool },
}

impl UiContext {
    #[must_use]
    pub fn new(input: UiInput) -> Self {
        Self {
            input,
            commands: Vec::new(),
            interactions: Vec::new(),
            cursor_y: 8.0,
            indent: 8.0,
            spacing: 6.0,
        }
    }

    pub fn label(&mut self, text: &str) {
        self.commands.push(UiCommand::Label {
            position: Vec2::new(self.indent, self.cursor_y),
            text: text.to_string(),
        });
        self.cursor_y += 18.0 + self.spacing;
    }

    pub fn button(&mut self, label: &str) -> bool {
        let position = Vec2::new(self.indent, self.cursor_y);
        let size = Vec2::new(120.0, 24.0);
        let hovered = point_in_rect(self.input.cursor, position, size);
        let pressed = hovered && self.input.primary_pressed;
        let clicked = hovered && self.input.primary_just_released;
        self.commands.push(UiCommand::Button {
            position,
            size,
            label: label.to_string(),
            hovered,
            pressed,
        });
        if clicked {
            self.interactions.push(UiInteraction::ButtonClicked {
                label: label.to_string(),
            });
        }
        self.cursor_y += size.y() + self.spacing;
        clicked
    }

    pub fn slider(&mut self, label: &str, value: &mut f32, min: f32, max: f32) -> bool {
        let position = Vec2::new(self.indent, self.cursor_y);
        let size = Vec2::new(160.0, 20.0);
        let mut changed = false;
        if point_in_rect(self.input.cursor, position, size) && self.input.primary_pressed {
            let local_x = (self.input.cursor.x() - position.x()) / size.x();
            let new = min + local_x.clamp(0.0, 1.0) * (max - min);
            if (new - *value).abs() > 1e-6 {
                *value = new;
                self.interactions.push(UiInteraction::SliderChanged {
                    label: label.to_string(),
                    new_value: new,
                });
                changed = true;
            }
        }
        self.commands.push(UiCommand::Slider {
            position,
            size,
            label: label.to_string(),
            value: *value,
            min,
            max,
        });
        self.cursor_y += size.y() + self.spacing;
        changed
    }

    pub fn checkbox(&mut self, label: &str, checked: &mut bool) -> bool {
        let position = Vec2::new(self.indent, self.cursor_y);
        let size = Vec2::new(18.0, 18.0);
        let mut toggled = false;
        if point_in_rect(self.input.cursor, position, size) && self.input.primary_just_released {
            *checked = !*checked;
            self.interactions.push(UiInteraction::CheckboxToggled {
                label: label.to_string(),
                new_value: *checked,
            });
            toggled = true;
        }
        self.commands.push(UiCommand::Checkbox {
            position,
            label: label.to_string(),
            checked: *checked,
        });
        self.cursor_y += size.y() + self.spacing;
        toggled
    }

    #[must_use]
    pub fn commands(&self) -> &[UiCommand] {
        &self.commands
    }

    #[must_use]
    pub fn interactions(&self) -> &[UiInteraction] {
        &self.interactions
    }
}

fn point_in_rect(p: Vec2, origin: Vec2, size: Vec2) -> bool {
    p.x() >= origin.x()
        && p.x() <= origin.x() + size.x()
        && p.y() >= origin.y()
        && p.y() <= origin.y() + size.y()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_appends_command() {
        let mut ui = UiContext::new(UiInput::default());
        ui.label("hello");
        assert_eq!(ui.commands().len(), 1);
        match &ui.commands()[0] {
            UiCommand::Label { text, .. } => assert_eq!(text, "hello"),
            _ => panic!("expected label"),
        }
    }

    #[test]
    fn button_records_click_on_release_inside() {
        let mut ui = UiContext::new(UiInput {
            cursor: Vec2::new(20.0, 12.0),
            primary_pressed: false,
            primary_just_released: true,
        });
        let clicked = ui.button("ok");
        assert!(clicked);
        assert_eq!(ui.interactions().len(), 1);
    }

    #[test]
    fn button_no_click_when_cursor_outside() {
        let mut ui = UiContext::new(UiInput {
            cursor: Vec2::new(500.0, 500.0),
            primary_just_released: true,
            ..UiInput::default()
        });
        assert!(!ui.button("nope"));
        assert!(ui.interactions().is_empty());
    }

    #[test]
    fn slider_updates_value_when_dragged() {
        let mut ui = UiContext::new(UiInput {
            cursor: Vec2::new(88.0, 18.0),
            primary_pressed: true,
            primary_just_released: false,
        });
        let mut v = 0.0_f32;
        let changed = ui.slider("vol", &mut v, 0.0, 1.0);
        assert!(changed);
        assert!(v > 0.4 && v < 0.6);
    }

    #[test]
    fn checkbox_toggles_state_on_release() {
        let mut ui = UiContext::new(UiInput {
            cursor: Vec2::new(12.0, 12.0),
            primary_just_released: true,
            ..UiInput::default()
        });
        let mut on = false;
        let toggled = ui.checkbox("enable", &mut on);
        assert!(toggled);
        assert!(on);
    }

    #[test]
    fn multiple_widgets_stack_vertically() {
        let mut ui = UiContext::new(UiInput::default());
        ui.label("a");
        ui.label("b");
        ui.label("c");
        let ys: Vec<f32> = ui
            .commands()
            .iter()
            .map(|c| match c {
                UiCommand::Label { position, .. } => position.y(),
                _ => 0.0,
            })
            .collect();
        assert!(ys[1] > ys[0]);
        assert!(ys[2] > ys[1]);
    }
}
