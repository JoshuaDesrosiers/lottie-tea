//! Attachable per-node behaviors — the Unity-style "component" concept
//! (a GameObject's Transform plus a stack of attached components). Named
//! `NodeBehavior`, not `NodeComponent`, on purpose: this codebase already
//! has an established, unrelated meaning for "Component"
//! ([`crate::ComponentDefinition`]/[`crate::ComponentInstance`] — a Rive/
//! Figma-style reusable *prefab*, nothing to do with an attachable
//! behavior). UI copy can still say "Component"/"Add Component" (Unity's own
//! words); only the code-level name needs to dodge the collision.
//!
//! Deliberately additive, not a replacement for [`crate::NodeKind`]: every
//! `Node` keeps its existing `kind` exactly as before, and separately gains
//! an ordered `Vec<NodeBehaviorEntry>` (see `crate::Node::behaviors`). This
//! means, for instance, `NodeKind::Image` and a node wearing a
//! `SpriteRenderer` behavior are two independent ways to display an image —
//! a known, deliberate overlap for now; unifying them is a later decision
//! once the component model is proven, not assumed here.

use serde::{Deserialize, Serialize};

use crate::ids::MaterialId;
use crate::script::ScriptInstance;

/// One behavior a node can carry, plus whether it's currently active — same
/// `{payload, enabled}` shape as [`crate::PostEffectLayer`], for the same
/// reason: a behavior can be temporarily switched off without losing its
/// authored parameters/position in the list, the same "disable without
/// deleting" convenience every stacked-entry list in this codebase already
/// gives (post-effects, state machine layers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeBehaviorEntry {
    pub behavior: NodeBehavior,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeBehavior {
    /// Draws this node as a textured (or flat-tinted) quad — see
    /// [`SpriteRendererBehavior`]. Appended-last-only applies to future
    /// variants, same `.teab` index-encoding reason `NodeKind` documents.
    SpriteRenderer(SpriteRendererBehavior),
    /// One instance of a user-authored [`crate::ScriptDefinition`] — the
    /// Unity "custom script component" analog, see `crate::script`'s own
    /// module doc comment for this pass's scope. Appended last, same reason
    /// as `SpriteRenderer`.
    Script(ScriptInstance),
    // ---- Unity components that used to be node *kinds* ----
    // (every variant below: appended last, same `.teab` reason.) Attach any
    // of these to any node — a light on the player, particles on an enemy.
    // A node holds at most one of each of these first five.
    /// Unity's `Camera`.
    Camera(crate::CameraNode),
    /// Unity's `Light 2D`.
    Light2D(crate::Light2DNode),
    /// Unity's `Trail Renderer`.
    TrailRenderer(crate::TrailNode),
    /// Unity's `Particle System`.
    ParticleSystem(crate::ParticleSystemNode),
    /// Unity's `Shadow Caster 2D`.
    ShadowCaster2D(crate::ShadowCaster2D),
    /// Unity's `Rigidbody 2D` (one per node).
    Rigidbody2D(crate::Rigidbody2D),
    /// Unity's 2D colliders (any number per node).
    BoxCollider2D(crate::BoxCollider2D),
    CircleCollider2D(crate::CircleCollider2D),
    CapsuleCollider2D(crate::CapsuleCollider2D),
    PolygonCollider2D(crate::PolygonCollider2D),
    /// Unity's `Hinge Joint 2D` (ragdoll limbs, pendulums).
    HingeJoint2D(crate::HingeJoint2D),
    /// Live IK on the chain ending at this bone.
    IkConstraint2D(crate::IkConstraint2D),
    /// Unity's `Sprite Skin`: bone-deformed image.
    SpriteSkin2D(crate::SpriteSkin2D),
    /// A particle fluid — see [`crate::Liquid2D`].
    Liquid2D(crate::Liquid2D),
    /// Unity's `AudioSource` — see [`crate::AudioSource`].
    AudioSource(crate::AudioSource),
    /// Pins the node to a point of the screen — see [`crate::UiAnchor`].
    UiAnchor(crate::UiAnchor),
    /// Row / column layout of the children — see [`crate::UiStack`].
    UiStack(crate::UiStack),
    /// Hover / press feedback and clicks — see [`crate::UiButton`].
    UiButton(crate::UiButton),
    /// A value slider — see [`crate::UiSlider`].
    UiSlider(crate::UiSlider),
    /// A scrollbar — see [`crate::UiScrollbar`].
    UiScrollbar(crate::UiScrollbar),
    /// A scrolling viewport — see [`crate::UiScrollView`].
    UiScrollView(crate::UiScrollView),
    /// An on / off checkbox or radio button — see [`crate::UiToggle`].
    UiToggle(crate::UiToggle),
    /// A flexbox container — see [`crate::UiLayout`].
    UiLayout(crate::UiLayout),
    /// Per-child flexbox settings — see [`crate::UiLayoutItem`].
    UiLayoutItem(crate::UiLayoutItem),
    /// Show only on some screens — see [`crate::UiShowWhen`].
    UiShowWhen(crate::UiShowWhen),
    /// HTML + CSS shown at this node — see [`crate::UiHtml`].
    UiHtml(crate::UiHtml),
    /// A 3D physics body — see [`crate::Rigidbody3D`].
    Rigidbody3D(crate::Rigidbody3D),
    /// A 3D collision shape — see [`crate::Collider3D`].
    Collider3D(crate::Collider3D),
    /// Shows one child at a time on a clock — see [`crate::Flipbook`].
    /// (Appended last.)
    Flipbook(crate::Flipbook),
    /// Soft fabric: flags, curtains, capes — see [`crate::Cloth2D`].
    /// (Appended last.)
    Cloth2D(crate::Cloth2D),
    /// A procedural tree — see [`crate::Tree2D`]. (Appended last.)
    Tree2D(crate::Tree2D),
    /// On Click actions — see [`crate::UiEvents`]. (Appended last.)
    UiEvents(crate::UiEvents),
    /// Drives the node from scene properties — see
    /// [`crate::PropertyBindings`]. (Appended last.)
    PropertyBindings(crate::PropertyBindings),
}

/// One collider attached to a node, any shape — what physics iterates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColliderRef<'a> {
    Box(&'a crate::BoxCollider2D),
    Circle(&'a crate::CircleCollider2D),
    Capsule(&'a crate::CapsuleCollider2D),
    Polygon(&'a crate::PolygonCollider2D),
}

/// Payload for [`NodeBehavior::SpriteRenderer`]: draws the node it's
/// attached to as a textured-or-flat quad, the Unity `SpriteRenderer`
/// analog. Independent of `NodeKind` — attach this to a plain `Group` node
/// (which has no visual payload of its own) to give it a sprite, or to any
/// other node kind to layer a sprite on top of whatever it already draws.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpriteRendererBehavior {
    /// `None` falls back to `color` alone — a flat-color sprite with no
    /// material asset, the same "authored default, no asset needed yet"
    /// shape `ComponentInstance::variant`'s `None` case already has.
    pub material: Option<MaterialId>,
    /// Tint multiplier when `material` resolves to a `Textured` material,
    /// or the sprite's own flat color when `material` is `None`/unresolved
    /// — straight (non-premultiplied) linear RGBA, same convention as
    /// `ShapeNode::fill_color`.
    pub color: [f32; 4],
}
