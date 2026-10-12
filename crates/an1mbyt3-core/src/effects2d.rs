//! **2D draw effects** — components that change how a node (and
//! everything under it) is drawn, without touching its geometry or
//! transform in the document. Like Photoshop / Figma layer effects and
//! After Effects / Rive modifiers, they're live: edit the node and the
//! effect follows.
//!
//! - [`DropShadow2D`] / [`Outline2D`] — copies drawn behind, offset or
//!   ringed, in one colour (pixel-art outlines at 1 px, soft vector shadows
//!   with softness).
//! - [`Repeater2D`] — N copies stepping by an offset / turn / scale, fading.
//! - [`Mirror2D`] — symmetry: mirrored copies across the node's own axes.
//! - [`Wiggle2D`] — procedural shake / float (After Effects' `wiggle()`).
//! - [`Parallax2D`] — moves against the camera for depth.
//! - [`PixelSnap2D`] — snaps drawn positions to a pixel grid (no shimmer
//!   on pixel art moving at fractional speeds).
//! - [`StrokeStyle2D`] — dashed strokes and Trim Path (draw-on lines).
//! - [`CameraShake2D`] — screen shake on a camera, kicked from script.

use serde::{Deserialize, Serialize};

/// **Drop Shadow**: a solid-colour copy behind the node, offset by
/// `offset` pixels; `softness` (px) feathers it with ring samples.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DropShadow2D {
    pub color: [f32; 4],
    pub offset: [f32; 2],
    pub softness: f32,
}

impl Default for DropShadow2D {
    fn default() -> Self {
        Self { color: [0.0, 0.0, 0.0, 0.45], offset: [6.0, -6.0], softness: 0.0 }
    }
}

/// **Outline**: the node's silhouette, grown by `width` pixels, in
/// `color`, behind it. `smooth` uses 16 directions instead of 8 (rounder
/// on thick vector outlines; 8 keeps pixel art blocky).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Outline2D {
    pub color: [f32; 4],
    pub width: f32,
    pub smooth: bool,
}

impl Default for Outline2D {
    fn default() -> Self {
        Self { color: [0.05, 0.04, 0.08, 1.0], width: 2.0, smooth: false }
    }
}

/// **Repeater**: `count` copies (the original included), each stepping
/// from the previous by `offset` (px, in the node's space), `rotation`
/// (degrees) and `scale` (× per step), fading to `end_opacity` at the
/// last copy. Copies draw behind the original.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Repeater2D {
    pub count: u32,
    pub offset: [f32; 2],
    pub rotation: f32,
    pub scale: f32,
    pub end_opacity: f32,
}

impl Default for Repeater2D {
    fn default() -> Self {
        Self { count: 5, offset: [40.0, 0.0], rotation: 0.0, scale: 1.0, end_opacity: 0.3 }
    }
}

/// **Mirror**: mirrored copies of the node across its own vertical (`x`)
/// and/or horizontal (`y`) axis through its origin — symmetrical
/// characters, kaleidoscopes, tiles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Mirror2D {
    pub x: bool,
    pub y: bool,
}

impl Default for Mirror2D {
    fn default() -> Self {
        Self { x: true, y: false }
    }
}

/// **Wiggle**: procedural, smooth random motion — up to `position` px,
/// `rotation` degrees and `scale` (fraction) away from where the node is,
/// changing `frequency` times a second. `seed` keeps copies out of sync.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Wiggle2D {
    pub position: f32,
    pub rotation: f32,
    pub scale: f32,
    pub frequency: f32,
    pub seed: u32,
}

impl Default for Wiggle2D {
    fn default() -> Self {
        Self { position: 6.0, rotation: 4.0, scale: 0.0, frequency: 2.0, seed: 0 }
    }
}

/// **Parallax**: the node shifts by `factor` × the camera's position
/// (x, y). 0 = part of the world (no parallax); 0.5 = a far layer that
/// moves at half speed; 1 = stuck to the camera (sky); negative = a near
/// foreground layer that moves faster.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Parallax2D {
    pub factor: [f32; 2],
}

impl Default for Parallax2D {
    fn default() -> Self {
        Self { factor: [0.5, 0.5] }
    }
}

/// **Pixel Snap**: drawn positions snap to a `grid`-pixel grid (and
/// rotation to `angle_step` degrees when > 0) — crisp pixel art at any
/// movement speed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PixelSnap2D {
    pub grid: f32,
    pub angle_step: f32,
}

impl Default for PixelSnap2D {
    fn default() -> Self {
        Self { grid: 1.0, angle_step: 0.0 }
    }
}

/// **Stroke Style** on a shape with a stroke: dashes and Trim Path.
/// - `dash` / `gap` (px): dash pattern (0 dash = solid), shifted along the
///   path by `dash_offset` (animate it for marching ants).
/// - `trim_start` / `trim_end` (0..1): the visible part of the path, by
///   length (animate `trim_end` 0 → 1 to draw a line on); `trim_offset`
///   (0..1) slides the window round closed paths.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StrokeStyle2D {
    pub dash: f32,
    pub gap: f32,
    pub dash_offset: f32,
    pub trim_start: f32,
    pub trim_end: f32,
    pub trim_offset: f32,
}

impl Default for StrokeStyle2D {
    fn default() -> Self {
        Self { dash: 12.0, gap: 8.0, dash_offset: 0.0, trim_start: 0.0, trim_end: 1.0, trim_offset: 0.0 }
    }
}

/// **Camera Shake** on a Camera: screen shake that kicks in when a script
/// calls `ref.shake2(amount)` (or always, at `constant`) and settles over
/// `recovery` seconds. `amplitude` px at full strength, `frequency` shakes
/// per second. Strength is squared (the
/// "trauma" model), so small hits stay small and big ones feel big.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraShake2D {
    pub amplitude: f32,
    pub frequency: f32,
    pub recovery: f32,
    pub constant: f32,
}

impl Default for CameraShake2D {
    fn default() -> Self {
        Self { amplitude: 14.0, frequency: 18.0, recovery: 0.6, constant: 0.0 }
    }
}

/// Smooth value noise in -1..1 (shared by Wiggle and Camera Shake): `t`
/// in cycles, `channel` / `seed` pick independent streams.
pub fn noise1(t: f32, channel: u32, seed: u32) -> f32 {
    let hash = |i: i64| {
        let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((channel as u64) << 32 | seed as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 31;
        x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 29;
        ((x >> 40) as f32) / ((1u64 << 23) as f32) - 1.0
    };
    let i = t.floor();
    let f = t - i;
    let k = f * f * (3.0 - 2.0 * f);
    hash(i as i64) * (1.0 - k) + hash(i as i64 + 1) * k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_continuous() {
        let mut prev = noise1(0.0, 1, 2);
        for i in 1..2000 {
            let v = noise1(i as f32 * 0.01, 1, 2);
            assert!((-1.0..=1.0).contains(&v));
            assert!((v - prev).abs() < 0.2, "jump at {i}");
            prev = v;
        }
    }
}
