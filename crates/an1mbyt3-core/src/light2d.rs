//! Unity URP-style 2D lighting: the `Light 2D` component (as a node kind,
//! [`Light2DNode`]) and the `Shadow Caster 2D` component (as an optional
//! per-node property, [`ShadowCaster2D`]).
//!
//! The model mirrors Unity's: once a scene contains *any* `Light2D`, every
//! lit node's color is multiplied by the accumulated light at its pixel
//! (plus whatever Additive-blend-style lights add on top), so a scene with
//! lights but no Global light is dark everywhere no light reaches — exactly
//! the behavior a Unity author expects. A scene with no `Light2D` at all
//! renders unlit, as it always has (nothing to opt out of). Nodes marked
//! `Node::unlit` (and everything under them) skip lighting entirely —
//! Unity's `Sprite-Unlit-Default` material, the usual choice for HUD/UI.
//!
//! What's deliberately *not* modeled from Unity, and why:
//! - **Target Sorting Layers** — this engine has no sorting-layer concept;
//!   lighting applies to every lit node, `Node::unlit` is the opt-out.
//! - **Normal maps** — no sprite here carries a normal map, so Unity's
//!   Normal Map Quality/Distance would have nothing to act on.
//! - **Parametric lights** — deprecated in Unity itself (replaced by
//!   Freeform), so not reproduced.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::ids::ImageId;

/// Unity's `Light Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Light2DType {
    /// Lights everything uniformly — Unity's ambient/"global" light. Has no
    /// shape, position or shadows; only color/intensity/blend style matter.
    Global,
    /// A radial light with an optional cone (Unity merged its old "Point"
    /// light into this): `inner_radius`..`outer_radius` falloff, and
    /// `inner_spot_angle`..`outer_spot_angle` (full cone, degrees; 360 = a
    /// plain point light) aimed along the node's local +Y.
    #[default]
    Spot,
    /// An arbitrary polygon (`shape_points`, node-local) lit at full
    /// intensity inside, fading out over `falloff_size` past its edge.
    Freeform,
    /// A sprite's own pixels used as the light's shape/color (cookie).
    Sprite,
}

/// Unity's 2D Renderer `Blend Style` — how this light's contribution
/// combines with the lit node's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LightBlendStyle {
    /// `color * light` — the default; light is what makes things visible.
    #[default]
    Multiply,
    /// `color + light` — adds light on top, brightening/tinting without
    /// needing a base level (rim lights, glowing magic).
    Additive,
}

/// Unity's `Overlap Operation` — how this light combines with lights
/// already accumulated before it (in `light_order`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LightOverlapOperation {
    /// Light adds up where lights overlap — the default.
    #[default]
    Additive,
    /// This light covers earlier ones by its own falloff alpha instead of
    /// adding to them (e.g. a dark-tinted light carving out a shadowy area).
    AlphaBlend,
    /// Not in Unity: where lights overlap, the brightest one wins — lights
    /// merge into one pool instead of stacking up into a blown-out hotspot.
    /// The pixel-art-friendly choice when many lights crowd together.
    Max,
}

/// Payload for `NodeKind::Light2D` — Unity's `Light 2D` component. The
/// node's own transform positions/rotates/scales the light (a Spot points
/// along local +Y, so rotating the node aims the cone).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Light2DNode {
    pub light_type: Light2DType,
    /// Straight linear RGB(A); alpha is ignored (intensity controls strength).
    pub color: [f32; 4],
    /// Multiplies `color`. Values above 1 over-brighten (HDR), as in Unity.
    pub intensity: f32,
    pub blend_style: LightBlendStyle,
    pub overlap_operation: LightOverlapOperation,
    /// Unity's `Light Order` — lower draws first. Only matters for
    /// `AlphaBlend` overlap, where order decides who covers whom.
    pub light_order: i32,

    // ---- Spot ----
    /// Full intensity inside this radius (world units, before node scale).
    pub inner_radius: f32,
    /// Light reaches zero at this radius.
    pub outer_radius: f32,
    /// Full-intensity cone width, degrees (0..=360).
    pub inner_spot_angle: f32,
    /// Light reaches zero at this cone width, degrees (0..=360).
    pub outer_spot_angle: f32,
    /// Unity's `Falloff Strength` (0..=1): 0 = gentle linear-ish fade,
    /// 1 = light holds then drops sharply. Shared by Spot and Freeform.
    pub falloff_strength: f32,

    // ---- Freeform ----
    /// Polygon vertices in node-local space (closed implicitly).
    pub shape_points: Vec<Vec2>,
    /// How far past the polygon edge the light fades out (world units).
    pub falloff_size: f32,

    // ---- Sprite ----
    pub sprite: Option<ImageId>,
    /// Drawn size of the sprite light (world units, before node scale).
    pub sprite_width: f32,
    pub sprite_height: f32,

    // ---- Shadows ----
    pub shadows_enabled: bool,
    /// 0..=1 — how much a caster blocks this light (1 = fully black shadow).
    pub shadow_strength: f32,
    /// Unity 6's `Shadow Softness` (0..=1) — blurs shadow edges.
    pub shadow_softness: f32,

    // ---- Volumetric ----
    /// Unity's `Volumetric` — the light also glows "in the air", visible
    /// even where there's nothing to light (fog/beam look).
    pub volumetric_enabled: bool,
    pub volumetric_intensity: f32,
    pub volumetric_shadows_enabled: bool,
    pub volumetric_shadow_strength: f32,

    // ---- Pixel art (not in Unity — retro-friendly lighting) ----
    /// How many crisp brightness steps the falloff is drawn in — concentric
    /// rings, brightest at the center, faintest at the edge (the pixel-art
    /// look). `0`/`1` = one solid, hard-edged pool.
    #[serde(default = "default_falloff_steps")]
    pub falloff_steps: u32,
    /// Blur between those steps: `0` = crisp rings (default), `1` = Unity's
    /// smooth gradient falloff, in between = a mix. Shadows have the
    /// matching `shadow_softness`.
    #[serde(default)]
    pub edge_blur: f32,
    /// `0` = per screen pixel. `n` = the light is computed once per `n`x`n`
    /// block of world units, so its shape is made of chunky pixels too.
    #[serde(default)]
    pub pixel_size: f32,
}

impl Default for Light2DNode {
    /// Unity's defaults for a freshly added Spot Light 2D.
    fn default() -> Self {
        Self {
            light_type: Light2DType::Spot,
            color: [1.0, 1.0, 1.0, 1.0],
            intensity: 1.0,
            blend_style: LightBlendStyle::Multiply,
            overlap_operation: LightOverlapOperation::Additive,
            light_order: 0,
            inner_radius: 0.0,
            outer_radius: 120.0,
            inner_spot_angle: 360.0,
            outer_spot_angle: 360.0,
            falloff_strength: 0.5,
            shape_points: vec![Vec2::new(-40.0, -40.0), Vec2::new(40.0, -40.0), Vec2::new(40.0, 40.0), Vec2::new(-40.0, 40.0)],
            falloff_size: 20.0,
            sprite: None,
            sprite_width: 64.0,
            sprite_height: 64.0,
            shadows_enabled: false,
            shadow_strength: 1.0,
            shadow_softness: 0.3,
            volumetric_enabled: false,
            volumetric_intensity: 0.25,
            volumetric_shadows_enabled: false,
            volumetric_shadow_strength: 1.0,
            falloff_steps: default_falloff_steps(),
            edge_blur: 0.0,
            pixel_size: 0.0,
        }
    }
}

fn default_falloff_steps() -> u32 {
    3
}

impl Light2DNode {
    /// A Global light at Unity's default ambient (white, intensity 1).
    pub fn global() -> Self {
        Self { light_type: Light2DType::Global, ..Self::default() }
    }

    /// Node-local axis-aligned bounds of what this light can touch — the
    /// quad the renderer draws it through, and the editor's gizmo/pick box.
    /// `None` for a Global light (it has no extent).
    pub fn local_extent(&self) -> Option<(Vec2, Vec2)> {
        match self.light_type {
            Light2DType::Global => None,
            Light2DType::Spot => {
                let r = self.outer_radius.max(1.0);
                Some((Vec2::splat(-r), Vec2::splat(r)))
            }
            Light2DType::Freeform => {
                let mut min = Vec2::splat(f32::INFINITY);
                let mut max = Vec2::splat(f32::NEG_INFINITY);
                for p in &self.shape_points {
                    min = min.min(*p);
                    max = max.max(*p);
                }
                if !min.is_finite() {
                    return Some((Vec2::splat(-1.0), Vec2::splat(1.0)));
                }
                let pad = Vec2::splat(self.falloff_size.max(0.0));
                Some((min - pad, max + pad))
            }
            Light2DType::Sprite => {
                let half = Vec2::new(self.sprite_width, self.sprite_height) * 0.5;
                Some((-half, half))
            }
        }
    }
}

/// Unity's Shadow Caster 2D `Casting Option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ShadowCastingOption {
    /// Casts a shadow behind itself; its own surface stays lit.
    #[default]
    CastShadow,
    /// Doesn't cast onto others, but is darkened by its own shape.
    SelfShadow,
    /// Both.
    CastAndSelfShadow,
}

/// Unity's `Shadow Caster 2D` component, as an optional per-node property
/// (`Node::shadow_caster`). The caster shape comes from the node itself — a
/// `Shape`'s own path outline, or every other kind's bounds rectangle —
/// Unity's "Casting Source: Sprite Renderer/Shape Editor" collapsed into
/// "use what the node already is".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShadowCaster2D {
    pub enabled: bool,
    pub casting_option: ShadowCastingOption,
    /// Unity's `Trim Edge`: shrinks the caster outline inward (world units)
    /// so shadows start slightly inside a sprite's silhouette.
    pub trim_edge: f32,
}

impl Default for ShadowCaster2D {
    fn default() -> Self {
        Self { enabled: true, casting_option: ShadowCastingOption::CastShadow, trim_edge: 0.0 }
    }
}

/// **Sorting Layer** of a 2D node (and, unless they set their own,
/// everything under it): which of the project's sorting layers it draws
/// on — see `crate::SortingLayerDef`. Without one a node is on "Normal"
/// (or its parent's layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SortingLayer2D {
    pub layer: u32,
}

/// **Light Layers** on a Light 2D: the sorting layers it lights (Unity's
/// Target Sorting Layers). Empty (or no component) = every layer.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LightLayers2D {
    pub layers: Vec<u32>,
}

impl LightLayers2D {
    /// Does this light reach `layer`?
    pub fn lights(&self, layer: u32) -> bool {
        self.layers.is_empty() || self.layers.contains(&layer)
    }
}

/// **Flicker** on a Light 2D: its brightness (and a little of its size)
/// wavers like a candle, torch or failing neon.
/// - `amount` (0..1): how deep the dips go.
/// - `speed`: flickers per second.
/// - `smoothness` (0..1): 0 = jittery / electric, 1 = soft candle sway.
/// - `radius_amount` (0..1): how much the radius breathes with it.
/// - `seed`: so neighbouring torches don't flicker in sync.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LightFlicker2D {
    pub amount: f32,
    pub speed: f32,
    pub smoothness: f32,
    pub radius_amount: f32,
    pub seed: u32,
}

impl Default for LightFlicker2D {
    fn default() -> Self {
        Self { amount: 0.35, speed: 8.0, smoothness: 0.6, radius_amount: 0.1, seed: 0 }
    }
}

impl LightFlicker2D {
    /// The flicker's brightness factor at time `t` (seconds): 1 = full,
    /// down to `1 - amount`.
    pub fn factor(&self, t: f32) -> f32 {
        // Value noise at two octaves; smoothness eases between samples and
        // mutes the fast octave.
        let hash = |i: i64| {
            let mut x = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (self.seed as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x ^= x >> 31;
            x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
            ((x >> 40) as f32) / ((1u64 << 24) as f32)
        };
        let noise = |x: f32| {
            let i = x.floor();
            let f = x - i;
            let s = self.smoothness.clamp(0.0, 1.0);
            // Smoothstep blend; at 0 smoothness it holds then snaps.
            let k = if s <= 0.0 { if f > 0.5 { 1.0 } else { 0.0 } } else { let e = f * f * (3.0 - 2.0 * f); f + (e - f) * s };
            let k = k.clamp(0.0, 1.0);
            hash(i as i64) * (1.0 - k) + hash(i as i64 + 1) * k
        };
        let x = t * self.speed.max(0.0);
        let s = self.smoothness.clamp(0.0, 1.0);
        let n = noise(x) * 0.7 + noise(x * 2.7 + 13.0) * 0.3 * (1.0 - s * 0.7);
        1.0 - self.amount.clamp(0.0, 1.0) * n
    }
}
