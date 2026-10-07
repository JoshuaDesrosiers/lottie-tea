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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiButton {
    /// Multiplies the node's fill while the pointer is over it.
    pub hover_tint: [f32; 4],
    /// … and while it's held down.
    pub pressed_tint: [f32; 4],
    /// Scale while held (a "press in" — 1 = none).
    pub pressed_scale: f32,
    /// Off = no feedback and no clicks (a greyed-out button).
    pub interactable: bool,
}

impl Default for UiButton {
    fn default() -> Self {
        Self { hover_tint: [1.15, 1.15, 1.15, 1.0], pressed_tint: [0.8, 0.8, 0.8, 1.0], pressed_scale: 0.94, interactable: true }
    }
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
