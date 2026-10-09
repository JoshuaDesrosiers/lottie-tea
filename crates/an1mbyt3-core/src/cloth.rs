//! [`Cloth2D`] — a soft fabric component: a grid of points held together by
//! springy links, pinned along an edge (or at its corners), pulled by
//! gravity and blown by gusty wind. Flags, banners, curtains, capes, nets.
//!
//! This is authored data only. The live simulation (where every point is
//! right now, which links have torn) is runtime state in
//! `an1mbyt3_runtime::cloth`, stepped in Playground and games; while
//! designing, the cloth is drawn hanging at rest.
//!
//! Scripts can grab and drag it (`ref.grabCloth(x, y)`, `ref.dragCloth`,
//! `ref.releaseCloth`), cut it (`ref.tearCloth(x, y, radius)`), gust it
//! (`ref.setClothWind(x, y)`) and mend it (`ref.resetCloth()`).

use serde::{Deserialize, Serialize};

/// Which points are nailed down (they follow the node as it moves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClothPin {
    /// The whole top edge — a curtain or banner.
    #[default]
    TopEdge,
    /// Just the two top corners — a hammock-like sag.
    TopCorners,
    /// The whole left edge — a flag on a pole.
    LeftEdge,
    /// Nothing — it falls (until a script grabs it).
    None,
}

/// What the fabric looks like when it has no image: a colour, or a simple
/// woven-in pattern with a second colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClothPatternKind {
    #[default]
    Solid,
    /// Stripes running across (horizontal bands).
    HorizontalStripes,
    /// Stripes running down (vertical bands).
    VerticalStripes,
    Checker,
}

/// A soft fabric — see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Cloth2D {
    /// Size at rest, world units. The node's origin is the cloth's centre
    /// (for `LeftEdge`, the pole is the left side).
    pub width: f32,
    pub height: f32,
    /// Points across and down (at least 2 each). More = smoother, costlier.
    pub columns: u32,
    pub rows: u32,
    pub pin: ClothPin,
    /// Main colour (straight linear RGBA), and the pattern's second colour.
    pub color: [f32; 4],
    pub pattern: ClothPatternKind,
    pub pattern_color: [f32; 4],
    /// Bands / squares across the cloth.
    pub pattern_count: u32,
    /// Shown instead of the colour / pattern, stretched over the cloth.
    pub image: Option<crate::ImageId>,
    /// Downward pull, world units / s².
    pub gravity: f32,
    /// Steady wind, world units / s² (blows the free points).
    pub wind: glam::Vec2,
    /// How gusty the wind is: 0 = steady, 1 = wild.
    pub turbulence: f32,
    /// Constraint passes per step: higher = stiffer, less stretchy fabric.
    pub stiffness: u32,
    /// Velocity lost per second (0 = floats forever, 1+ = heavy, damp).
    pub damping: f32,
    /// Links snap when stretched past this × their rest length (0 = never
    /// tears by stretching; scripts can still cut it).
    pub tear_stretch: f32,
    /// How strongly folds and creases are shaded (0 = flat).
    pub shading: f32,
}

impl Default for Cloth2D {
    fn default() -> Self {
        Self {
            width: 220.0,
            height: 150.0,
            columns: 22,
            rows: 15,
            pin: ClothPin::TopEdge,
            color: [0.85, 0.22, 0.25, 1.0],
            pattern: ClothPatternKind::Solid,
            pattern_color: [1.0, 1.0, 1.0, 1.0],
            pattern_count: 6,
            image: None,
            gravity: 600.0,
            wind: glam::Vec2::new(120.0, 0.0),
            turbulence: 0.5,
            stiffness: 6,
            damping: 0.6,
            tear_stretch: 0.0,
            shading: 0.6,
        }
    }
}

impl Cloth2D {
    /// Grid size clamped to something sane (2..=64 a side).
    pub fn grid(&self) -> (usize, usize) {
        (self.columns.clamp(2, 64) as usize, self.rows.clamp(2, 64) as usize)
    }

    /// Point `(column, row)` at rest, in the node's local space (row 0 is
    /// the top).
    pub fn rest_point(&self, column: usize, row: usize) -> glam::Vec2 {
        let (cols, rows) = self.grid();
        let u = column as f32 / (cols - 1) as f32;
        let v = row as f32 / (rows - 1) as f32;
        match self.pin {
            // A flag: hangs out from its pole (the left edge at x = 0 local)…
            // drawn flat at rest; wind lifts it in play.
            ClothPin::LeftEdge => glam::Vec2::new(u * self.width, self.height * 0.5 - v * self.height),
            _ => glam::Vec2::new((u - 0.5) * self.width, self.height * 0.5 - v * self.height),
        }
    }

    /// Whether point `(column, row)` is pinned.
    pub fn is_pinned(&self, column: usize, row: usize) -> bool {
        let (cols, _) = self.grid();
        match self.pin {
            ClothPin::TopEdge => row == 0,
            ClothPin::TopCorners => row == 0 && (column == 0 || column == cols - 1),
            ClothPin::LeftEdge => column == 0,
            ClothPin::None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_layout_and_pins() {
        let cloth = Cloth2D { width: 100.0, height: 50.0, columns: 5, rows: 3, ..Default::default() };
        assert_eq!(cloth.rest_point(0, 0), glam::Vec2::new(-50.0, 25.0));
        assert_eq!(cloth.rest_point(4, 2), glam::Vec2::new(50.0, -25.0));
        assert!(cloth.is_pinned(2, 0) && !cloth.is_pinned(2, 1));
        let flag = Cloth2D { pin: ClothPin::LeftEdge, ..cloth };
        assert_eq!(flag.rest_point(0, 0).x, 0.0);
        assert!(flag.is_pinned(0, 2) && !flag.is_pinned(1, 0));
        let corners = Cloth2D { pin: ClothPin::TopCorners, ..cloth };
        assert!(corners.is_pinned(0, 0) && corners.is_pinned(4, 0) && !corners.is_pinned(2, 0));
    }

    #[test]
    fn round_trips_through_json() {
        let cloth = Cloth2D { pattern: ClothPatternKind::Checker, tear_stretch: 2.5, ..Default::default() };
        let json = serde_json::to_string(&cloth).unwrap();
        assert!(json.contains("\"tearStretch\":2.5"));
        assert_eq!(serde_json::from_str::<Cloth2D>(&json).unwrap(), cloth);
    }
}
