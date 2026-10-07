//! Unity's `Trail Renderer`, as a node kind ([`TrailNode`]).
//!
//! A trail records where its own node has been (world space) and draws a
//! ribbon through those points, fading/narrowing toward the oldest end.
//! Parent it under anything that moves — a sword tip, a projectile, the
//! player — and it streaks behind it. The recorded points are runtime
//! state (owned by whoever renders, never saved), so a trail always starts
//! empty when a scene loads, the same as Unity.

use serde::{Deserialize, Serialize};

use crate::gradient::GradientStop;
use crate::ids::ImageId;
use crate::particle::SizeKeyframe;

/// Payload for `NodeKind::Trail` — Unity's `Trail Renderer` component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrailNode {
    /// Unity's `Time`: seconds a point lives before it drops off the tail.
    pub time: f32,
    /// Unity's `Min Vertex Distance`: how far (world units) the node must
    /// move before a new point is recorded. Smaller = smoother, costlier.
    pub min_vertex_distance: f32,
    /// Unity's `Width` multiplier (world units).
    pub width: f32,
    /// Unity's width curve: multiplies `width` along the trail, `t = 0` at
    /// the head (newest) to `t = 1` at the tail (oldest). Empty = constant.
    pub width_curve: Vec<SizeKeyframe>,
    /// Unity's `Color` gradient along the trail, head (`offset 0`) to tail
    /// (`offset 1`). Empty = solid white.
    pub color_gradient: Vec<GradientStop>,
    /// Unity's `Emitting`: when off, no new points are added (the existing
    /// trail still fades out) — toggle it to "lift the pen".
    pub emitting: bool,
    /// Unity's `End Cap Vertices`: rounds both ends with this many extra
    /// vertices (0 = square ends).
    pub end_cap_vertices: u32,
    /// Unity's `Corner Vertices`: rounds sharp turns with this many extra
    /// vertices per bend (0 = plain mitered joins).
    pub corner_vertices: u32,
    /// Optional ribbon texture (Unity's material texture), stretched once
    /// head to tail (Unity's default `Stretch` texture mode) and multiplied
    /// by `color_gradient`. Unity's Tile/per-segment modes and non-Normal
    /// blending aren't offered: the ribbon draws as one textured mesh, and
    /// this renderer's textured draws clamp (no repeat) and blend Normal.
    pub texture: Option<ImageId>,
}

impl Default for TrailNode {
    /// Unity's defaults, scaled to this engine's pixel-sized world units
    /// (Unity's 1 unit ~ 16-32 px here).
    fn default() -> Self {
        Self {
            time: 0.6,
            min_vertex_distance: 2.0,
            width: 10.0,
            width_curve: vec![SizeKeyframe { t: 0.0, size_multiplier: 1.0 }, SizeKeyframe { t: 1.0, size_multiplier: 0.0 }],
            color_gradient: vec![
                GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
                GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
            ],
            emitting: true,
            end_cap_vertices: 0,
            corner_vertices: 0,
            texture: None,
        }
    }
}
