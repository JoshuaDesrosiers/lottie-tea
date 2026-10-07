//! UI components — what turns shapes and text into an interface without
//! hand-placing every element in script:
//!
//! - [`UiAnchor`]: pins a node to a point of the screen (a corner, an edge,
//!   the centre) with an offset, so a HUD stays put however the window or
//!   camera zoom changes. Meant for children of the active Camera (the HUD
//!   layer): its position is set in the camera's view rectangle, where
//!   `(0, 0)` is the bottom-left corner and `(1, 1)` the top-right.
//! - [`UiStack`]: a layout container — lays its visible children out in a
//!   row or column with spacing, padding and alignment (Unity's
//!   Horizontal / Vertical Layout Group).
//! - [`UiButton`]: hover / press feedback (tints, a press "pop") and click
//!   events scripts read as `ref.clicked` / `ref.hovered` / `ref.pressed`.
//!
//! Anchors and stacks are applied by the editor every frame (Design mode
//! too, so what you author is what you get); buttons are handled by the
//! script host, which knows the pointer.

use glam::Vec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiAnchor {
    /// Where on the screen, `0..1` each axis (`(0, 1)` = top-left).
    pub anchor: Vec2,
    /// Offset from that point, world units (+y up).
    pub offset: Vec2,
}

impl Default for UiAnchor {
    fn default() -> Self {
        Self { anchor: Vec2::new(0.5, 0.5), offset: Vec2::ZERO }
    }
}

/// Which way a [`UiStack`] runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StackDirection {
    #[default]
    Vertical,
    Horizontal,
}

/// Cross-axis alignment of a [`UiStack`]'s children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StackAlign {
    /// Left (vertical stack) / top (horizontal stack).
    Start,
    #[default]
    Center,
    /// Right / bottom.
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStack {
    pub direction: StackDirection,
    /// Gap between children (world units).
    pub spacing: f32,
    /// Space before the first child.
    pub padding: f32,
    pub align: StackAlign,
}

impl Default for UiStack {
    fn default() -> Self {
        Self { direction: StackDirection::Vertical, spacing: 8.0, padding: 0.0, align: StackAlign::Center }
    }
}

/// How a [`UiButton`] (or any interactive UI control) shows its state —
/// Unity's `Selectable.transition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiTransition {
    /// No visual change (scripts still get clicks).
    None,
    /// Multiply the node's own colour by the state's colour.
    #[default]
    ColorTint,
    /// Show a different image per state (an Image node, or a Shape's image
    /// fill); states without one keep the normal image.
    SpriteSwap,
}

/// One colour / image / scale per interaction state — Unity's `ColorBlock`
/// (+ `SpriteState`), shared by buttons, slider handles and scrollbar
/// handles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStateStyle {
    pub transition: UiTransition,
    pub normal_color: [f32; 4],
    pub highlighted_color: [f32; 4],
    pub pressed_color: [f32; 4],
    pub disabled_color: [f32; 4],
    /// Brightens / darkens every state colour (Unity's multiplier).
    pub color_multiplier: f32,
    /// Seconds to blend between states' colours (0 = instant).
    pub fade_duration: f32,
    pub highlighted_image: Option<crate::ImageId>,
    pub pressed_image: Option<crate::ImageId>,
    pub disabled_image: Option<crate::ImageId>,
    /// Scale while hovered / held (1 = unchanged) — a hover "lift" and a
    /// press "push in".
    pub highlighted_scale: f32,
    pub pressed_scale: f32,
    /// Also tint the node's children (a button's label) with the state colour.
    pub tint_children: bool,
}

impl Default for UiStateStyle {
    fn default() -> Self {
        // Unity's defaults (normal white, highlighted / pressed slightly
        // darker, disabled faded) plus a small press-in.
        Self {
            transition: UiTransition::ColorTint,
            normal_color: [1.0, 1.0, 1.0, 1.0],
            highlighted_color: [0.96, 0.96, 0.96, 1.0],
            pressed_color: [0.78, 0.78, 0.78, 1.0],
            disabled_color: [0.78, 0.78, 0.78, 0.5],
            color_multiplier: 1.0,
            fade_duration: 0.1,
            highlighted_image: None,
            pressed_image: None,
            disabled_image: None,
            highlighted_scale: 1.0,
            pressed_scale: 0.95,
            tint_children: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiButton {
    /// Off = the disabled look, and no clicks.
    pub interactable: bool,
    #[serde(default)]
    pub style: UiStateStyle,
}

impl Default for UiButton {
    fn default() -> Self {
        Self { interactable: true, style: UiStateStyle::default() }
    }
}

/// Which way a [`UiSlider`] / [`UiScrollbar`] fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiFillDirection {
    #[default]
    LeftToRight,
    RightToLeft,
    BottomToTop,
    TopToBottom,
}

/// Unity's `Slider`: on a track shape whose children named **"Fill"** (a
/// shape stretched from the start up to the value) and **"Handle"** (moved
/// to the value) are positioned for it. Drag the track or handle to change
/// the value; scripts read / write `ref.value` and see `ref.changed`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSlider {
    pub min: f32,
    pub max: f32,
    pub value: f32,
    /// Snap to integers (a 1–10 volume step, a level picker).
    pub whole_numbers: bool,
    pub direction: UiFillDirection,
    pub interactable: bool,
    /// The handle's state look.
    #[serde(default)]
    pub style: UiStateStyle,
}

impl Default for UiSlider {
    fn default() -> Self {
        Self { min: 0.0, max: 1.0, value: 0.5, whole_numbers: false, direction: UiFillDirection::LeftToRight, interactable: true, style: UiStateStyle::default() }
    }
}

/// Unity's `Scrollbar`: on a track shape whose child named **"Handle"** is
/// sized to `size` of the track and placed by `value` (0..1). Drag the
/// handle (or click the track to jump).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiScrollbar {
    pub value: f32,
    /// The handle's length as a fraction of the track (0.1..1).
    pub size: f32,
    pub direction: UiFillDirection,
    /// 0 = continuous; else the value snaps to that many positions.
    pub steps: u32,
    pub interactable: bool,
    #[serde(default)]
    pub style: UiStateStyle,
}

impl Default for UiScrollbar {
    fn default() -> Self {
        Self { value: 0.0, size: 0.25, direction: UiFillDirection::TopToBottom, steps: 0, interactable: true, style: UiStateStyle::default() }
    }
}

/// Unity's `ScrollRect`: a viewport (turn on *Clip children to this shape*)
/// whose child named **"Content"** scrolls by dragging and the mouse wheel,
/// within its bounds. A child named **"Scrollbar"** (with a UI Scrollbar)
/// is kept in sync both ways.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiScrollView {
    pub horizontal: bool,
    pub vertical: bool,
    /// World units per wheel notch.
    pub wheel_step: f32,
    /// Keep sliding after a drag is released (with this much friction per
    /// second; 0 = stop at once).
    pub inertia: bool,
}

impl Default for UiScrollView {
    fn default() -> Self {
        Self { horizontal: false, vertical: true, wheel_step: 40.0, inertia: true }
    }
}

/// Unity's `Toggle` (+ `ToggleGroup`): a box whose child named
/// **"Checkmark"** is shown while it's on. Clicking flips it; toggles that
/// share a non-zero `group` (siblings or not) act as radio buttons — turning
/// one on turns the others off. Scripts read / write `ref.isOn` and see
/// `ref.changed`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiToggle {
    pub is_on: bool,
    pub interactable: bool,
    /// 0 = independent; else a radio group number.
    pub group: u32,
    /// In a group: whether clicking the on toggle turns it off (leaving
    /// none on). Unity's `ToggleGroup.allowSwitchOff`.
    pub allow_switch_off: bool,
    /// The box's state look.
    #[serde(default)]
    pub style: UiStateStyle,
}

impl Default for UiToggle {
    fn default() -> Self {
        Self { is_on: false, interactable: true, group: 0, allow_switch_off: false, style: UiStateStyle::default() }
    }
}

/// **HTML UI**: a box of real HTML + CSS shown over the scene at this
/// node — its position, rotation and scale (and the camera) move it like
/// any other node. Rendered by the host as a DOM element laid over the
/// canvas (so it always draws on top of the canvas), in a shadow root so
/// its CSS can't leak out.
///
/// Wiring without code: clicking an element with `data-trigger="name"`
/// fires that state-machine trigger; an `<input data-input="name">` sets
/// that input (checkbox → bool, range / number → number, else enum
/// value). Scripts get the live DOM as `ref.dom` (a `ShadowRoot`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiHtml {
    pub html: String,
    pub css: String,
    /// The element's box, world units (CSS px at zoom 1).
    pub width: f32,
    pub height: f32,
    /// Receives clicks / typing in Playground and games (off = the pointer
    /// goes through it to the game).
    pub interactive: bool,
}

impl Default for UiHtml {
    fn default() -> Self {
        Self {
            html: "<div class=\"card\">\n  <h1>Hello!</h1>\n  <p>Real HTML + CSS, in your scene.</p>\n  <button data-trigger=\"start\">Start</button>\n</div>".into(),
            css: ".card {\n  box-sizing: border-box; width: 100%; height: 100%;\n  padding: 18px; border-radius: 16px;\n  font-family: system-ui, sans-serif; color: #f4f1ff;\n  background: linear-gradient(135deg, #5b4bd6, #c2418f);\n  box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);\n}\nh1 { margin: 0 0 6px; font-size: 26px; }\np { margin: 0 0 14px; opacity: 0.85; }\nbutton {\n  font: inherit; font-weight: 600; padding: 8px 18px;\n  border: 0; border-radius: 999px; background: #fff; color: #5b4bd6; cursor: pointer;\n}\nbutton:hover { transform: scale(1.05); }".into(),
            width: 320.0,
            height: 180.0,
            interactive: true,
        }
    }
}

/// The 0..1 position of a value in `[min, max]`.
pub fn slider_fraction(slider: &UiSlider) -> f32 {
    let span = slider.max - slider.min;
    if span.abs() < f32::EPSILON {
        return 0.0;
    }
    ((slider.value - slider.min) / span).clamp(0.0, 1.0)
}

/// Lays out `sizes` (each child's `(width, height)`, centred on its own
/// origin) for `stack`: each child's centre, relative to the stack node.
/// The run starts at the stack's origin and grows right (horizontal) or
/// down (vertical).
pub fn stack_positions(stack: &UiStack, sizes: &[Vec2]) -> Vec<Vec2> {
    let cross_max = sizes.iter().map(|s| if stack.direction == StackDirection::Vertical { s.x } else { s.y }).fold(0.0f32, f32::max);
    let mut cursor = stack.padding;
    sizes
        .iter()
        .map(|size| {
            let (main, cross) = match stack.direction {
                StackDirection::Vertical => (size.y, size.x),
                StackDirection::Horizontal => (size.x, size.y),
            };
            let along = cursor + main / 2.0;
            cursor += main + stack.spacing;
            let offset = match stack.align {
                StackAlign::Start => (cross - cross_max) / 2.0,
                StackAlign::Center => 0.0,
                StackAlign::End => (cross_max - cross) / 2.0,
            };
            match stack.direction {
                // Down the column; Start = left-aligned.
                StackDirection::Vertical => Vec2::new(offset, -along),
                // Along the row; Start = top-aligned.
                StackDirection::Horizontal => Vec2::new(along, -offset),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_fraction_clamps_and_handles_empty_ranges() {
        let s = UiSlider { min: 10.0, max: 20.0, value: 15.0, ..Default::default() };
        assert_eq!(slider_fraction(&s), 0.5);
        assert_eq!(slider_fraction(&UiSlider { value: 99.0, ..s }), 1.0);
        assert_eq!(slider_fraction(&UiSlider { min: 3.0, max: 3.0, ..s }), 0.0);
    }

    #[test]
    fn old_buttons_load_with_the_default_style() {
        // A button saved before state styling existed (it had tints).
        let old: UiButton = serde_json::from_str(r#"{"hoverTint":[1,1,1,1],"pressedTint":[1,1,1,1],"pressedScale":0.9,"interactable":true}"#).unwrap();
        assert_eq!(old.style, UiStateStyle::default());
    }

    #[test]
    fn a_vertical_stack_runs_down_with_spacing() {
        let stack = UiStack { spacing: 10.0, ..Default::default() };
        let p = stack_positions(&stack, &[Vec2::new(100.0, 20.0), Vec2::new(60.0, 30.0)]);
        assert_eq!(p[0], Vec2::new(0.0, -10.0));
        assert_eq!(p[1], Vec2::new(0.0, -(20.0 + 10.0 + 15.0)));
    }

    #[test]
    fn start_alignment_lines_up_left_edges() {
        let stack = UiStack { align: StackAlign::Start, ..Default::default() };
        let p = stack_positions(&stack, &[Vec2::new(100.0, 20.0), Vec2::new(60.0, 20.0)]);
        // Left edges: 0 - 50 = -50 and p1.x - 30 must match.
        assert_eq!(p[0].x - 50.0, p[1].x - 30.0);
    }
}
