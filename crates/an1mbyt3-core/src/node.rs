//! Scene graph node.
//!
//! A `Node` is one entry in an [`crate::Artboard`]'s node arena. Every node
//! carries a `Transform2D` regardless of its `NodeKind` — this is what lets the
//! renderer treat "move this node" identically whether it's a shape, a bone, or
//! a group (AN1MBYT3_BUILD_PLAN.md §6: "Transform-only animation should update
//! matrices rather than rebuild geometry" applies to every node kind uniformly).
//!
//! `NodeKind` has `Shape`/`Group`/`Bone`/`Instance`/`Text`/`Image` variants —
//! each was added in the phase that actually implements it, not stubbed in
//! ahead of time, so this enum only ever has variants something in the
//! codebase actually handles. `Bone` was
//! added for real in Phase 5 rather than demo-hacked via `Group` because §4
//! explicitly describes bones/constraints as first-class document objects.
//! `Instance` (Phase 7) is the same story for components. `Group` gained an
//! optional mask reference (Phase 10, [`GroupNode`]) — `NodeKind::Group` went
//! from a bare unit variant to `Group(GroupNode)`. `Text` (Phase 3) replaces
//! the `text_shape` example's earlier `NodeKind::Group`-plus-manually-tracked-
//! glyph-meshes hack (see `an1mbyt3-text`'s `text_shape` example) now that
//! the document model actually needs to *save* text nodes, not just render
//! one for a demo — appended last (not inserted between existing variants)
//! since `.teab` (`an1mbyt3-format`) encodes enum variants by index.

use serde::{Deserialize, Serialize};

use crate::{
    behavior::NodeBehaviorEntry, blend_mode::BlendMode, component::ComponentInstance, ids::FontId, ids::ImageId, ids::NodeId, path::Path,
    transform::Transform2D,
};

/// One node in the scene graph: a transform plus a `kind`-specific payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    /// `None` for a root node directly under the artboard; `Some(parent)` for
    /// anything nested. Child ordering (for draw order/hit-testing) lives on
    /// the parent's `Artboard::root_nodes` / a future `children` list — not
    /// tracked here to avoid two sources of truth for the same relationship.
    pub parent: Option<NodeId>,
    pub transform: Transform2D,
    pub kind: NodeKind,
    /// Soft-delete flag — see `Command::SetDeleted`'s doc comment
    /// (`command.rs`) for why deletion flips this rather than actually
    /// removing the node from the `Artboard`'s `slotmap`: undo/redo needs a
    /// deleted node's `NodeId` to stay valid (any earlier command referencing
    /// it, e.g. a transform edit made before the delete, must still resolve),
    /// and `slotmap` has no API to reinsert a value at a specific
    /// previously-freed key. Every reader that lists nodes for rendering/UI
    /// (`Artboard::shapes`/`texts`, the wasm `dto::summarize_document`) skips
    /// nodes with `deleted: true`; `Artboard::node`/`node_mut` still resolve
    /// them (an undo/redo command needs to reach them to flip the flag back).
    #[serde(default)]
    pub deleted: bool,
    /// Unity-style attached components — see `crate::behavior`'s module doc
    /// for why this is a separate, additive list rather than folded into
    /// `kind`. `#[serde(default)]` since this field postdates every
    /// existing `.tea`/`.teab` file, same convention as `deleted`.
    #[serde(default)]
    pub behaviors: Vec<NodeBehaviorEntry>,
    /// A stable, author-facing identifier — the "tag" a `getCompByID` in a
    /// Code-mode Programmable State's script looks a node up by (see
    /// `apps/editor/src/scriptRuntime.ts`), same role an HTML element's
    /// `id` attribute plays for `document.getElementById`. Must be unique
    /// within an artboard by convention (not enforced here — same "caller's
    /// own mistake, not a validity concern this data model enforces"
    /// stance `Command`'s own doc comment already takes elsewhere; a
    /// duplicate id just means `getCompByID` resolves to whichever match
    /// comes first). Left empty (`""`) by `Node::new` — `Artboard::add_node`
    /// is what actually fills in the default `shape(<serial>)` form, since
    /// generating a genuinely unique serial needs the artboard's own
    /// counter, which a bare `Node::new` call has no access to.
    /// `#[serde(default)]` since this field postdates every existing
    /// `.tea`/`.teab` file — an old file loads with every node's `id`
    /// empty, same as if `Node::new` had never given it one; nothing
    /// currently *requires* a non-empty id, so this is harmless, not a
    /// silent corruption the way an empty `name` might be.
    #[serde(default)]
    pub id: String,
    /// Author-facing tags — same "like HTML class attributes" role
    /// `getCompByClass` selects by by (`scriptRuntime.ts`). Purely a
    /// selector mechanism for scripts; nothing in the renderer/runtime
    /// reads this. `#[serde(default)]` for the same back-compat reason
    /// `id` has.
    #[serde(default)]
    pub classes: Vec<String>,
    /// Clips this node's own children to this node's own shape — a second,
    /// simpler masking mechanism alongside [`GroupNode::mask`], generic to
    /// every node kind (not just `Group`) since anything with a shape
    /// (a `Shape`, most commonly) can want its descendants clipped to it.
    /// Where `GroupNode::mask` points at a *separate*, otherwise-invisible
    /// sibling child used purely as a clip shape (needed for an arbitrary
    /// clip path, e.g. imported SVG `<clipPath>` content, whose shape has no
    /// reason to match anything else in the scene), `clip_children` needs no
    /// extra node at all: the node still draws its own fill normally *and*
    /// that same geometry also becomes the stencil mask for whatever's
    /// nested inside it — the "just parent one shape under another, no
    /// wrapper `Group` or hidden mask twin" shape a simple nested "room"/
    /// tunnel rig wants (see `build_demo_parallax_state_machine`,
    /// `an1mbyt3-wasm`). Composes with `GroupNode::mask` if a node somehow
    /// uses both — each contributes its own stencil depth level, clipping to
    /// their intersection. `#[serde(default)]` (`false`) for the same
    /// back-compat reason `id`/`classes` have.
    #[serde(default)]
    pub clip_children: bool,
    /// Whether this node draws at all — the "show"/"hide" Looks concept, and
    /// a Blocks program's `Visible` `GraphProperty` axis (`SetProperty`/
    /// `GetProperty`/`ChangeProperty` all reach it for free through that
    /// generic mechanism, no dedicated command needed). `true` by default
    /// (`#[serde(default = "default_true")]`, since a bare `#[serde(default)]`
    /// on a `bool` would silently make every node loaded from a `.tea`/
    /// `.teab` saved before this field existed start *invisible*, exactly
    /// backwards from "an old file renders exactly as it always did").
    /// Distinct from `deleted`: a deleted node is gone from every reader
    /// (soft-removed, undo/redo can still resurrect it); an invisible node
    /// is still fully present (still selectable/editable in the editor,
    /// still hit-testable, still has live children) — it just doesn't draw.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// A manual draw-order override within this node's own parent (or
    /// `root_nodes`, for a root node) — Unity's "Order in Layer." Children of
    /// the *same* parent sort ascending by this (smaller draws first/behind,
    /// larger draws last/in front), ties breaking to whatever the existing
    /// authored order already was (`Artboard::children_of`/`root_nodes`'s own
    /// "Bring Forward"/"Send Backward" order) — same stable-sort convention
    /// as everywhere else in this codebase that layers one ordering on top of
    /// another. Ignored for children of a `GroupNode::y_sort_enabled` parent
    /// — see that field's own doc comment for why a *dynamic*, Y-position-
    /// driven order is a different mechanism from this *static* one, not a
    /// combination of both. `#[serde(default)]` (`0.0`, meaning "unordered",
    /// i.e. keep today's existing authored order) for the usual back-compat
    /// reason every additive field here gets one.
    #[serde(default)]
    pub z_index: f32,
    /// Skips 2D lighting for this node *and everything under it* — Unity's
    /// `Sprite-Unlit-Default` material. The usual choice for HUD/UI parented
    /// under a camera, which shouldn't go dark when the scene does. Has no
    /// effect in a scene without any `Light2D`. `#[serde(default)]` (lit).
    #[serde(default)]
    pub unlit: bool,
    /// Unity's `Shadow Caster 2D` component — `Some` makes this node block
    /// light from shadow-casting `Light2D`s. See [`crate::ShadowCaster2D`].
    #[serde(default)]
    pub shadow_caster: Option<crate::ShadowCaster2D>,
    /// Set when this node was instantiated from a prefab — which prefab,
    /// and which of its nodes. See [`crate::PrefabLink`] and the
    /// `crate::prefab` module doc. `None` for ordinary nodes.
    #[serde(default)]
    pub prefab: Option<crate::PrefabLink>,
}

fn default_true() -> bool {
    true
}

/// Generates a component accessor pair: the enabled component of one kind,
/// whether it lives in a behavior (current) or the node's legacy `kind`
/// (files saved before these became components — see
/// [`Node::migrate_legacy_kind_to_behaviors`]).
macro_rules! component_accessors {
    ($get:ident, $get_mut:ident, $ty:ty, $behavior:ident, $legacy:ident) => {
        /// The node's enabled component of this kind, if any.
        pub fn $get(&self) -> Option<&$ty> {
            if let NodeKind::$legacy(value) = &self.kind {
                return Some(value);
            }
            self.behaviors.iter().find_map(|entry| match &entry.behavior {
                crate::NodeBehavior::$behavior(value) if entry.enabled => Some(value),
                _ => None,
            })
        }

        /// Mutable access to the node's component of this kind (enabled or
        /// not — editing a disabled component is still editing it).
        pub fn $get_mut(&mut self) -> Option<&mut $ty> {
            if let NodeKind::$legacy(value) = &mut self.kind {
                return Some(value);
            }
            self.behaviors.iter_mut().find_map(|entry| match &mut entry.behavior {
                crate::NodeBehavior::$behavior(value) => Some(value),
                _ => None,
            })
        }
    };
}

impl Node {
    component_accessors!(camera, camera_mut, CameraNode, Camera, Camera);
    component_accessors!(light2d, light2d_mut, crate::Light2DNode, Light2D, Light2D);
    component_accessors!(trail, trail_mut, crate::TrailNode, TrailRenderer, Trail);
    component_accessors!(particle_system, particle_system_mut, crate::ParticleSystemNode, ParticleSystem, ParticleSystem);

    /// The node's "fill color": a Shape/Text/Image's own fill, or — for any
    /// other node — its Light 2D component's color. What `SetFillColor`,
    /// FillColor keyframes and scripts' `fillColor` all read/write, so a
    /// light's color animates like any fill.
    pub fn fill_color(&self) -> Option<[f32; 4]> {
        match &self.kind {
            NodeKind::Shape(shape) => Some(shape.fill_color),
            NodeKind::Text(text) => Some(text.fill_color),
            NodeKind::Image(image) => Some(image.fill_color),
            // A 3D mesh's colour (scripts tint 3D props with `ref.fillColor`).
            NodeKind::Node3D(crate::Node3D { content: crate::Node3DContent::Mesh(mesh), .. }) => Some(mesh.color),
            _ => self.light2d().map(|light| light.color),
        }
    }

    /// Mutable counterpart of [`Node::fill_color`].
    pub fn fill_color_mut(&mut self) -> Option<&mut [f32; 4]> {
        if matches!(self.kind, NodeKind::Shape(_) | NodeKind::Text(_) | NodeKind::Image(_) | NodeKind::Node3D(_)) {
            return match &mut self.kind {
                NodeKind::Shape(shape) => Some(&mut shape.fill_color),
                NodeKind::Text(text) => Some(&mut text.fill_color),
                NodeKind::Image(image) => Some(&mut image.fill_color),
                NodeKind::Node3D(crate::Node3D { content: crate::Node3DContent::Mesh(mesh), .. }) => Some(&mut mesh.color),
                _ => None,
            };
        }
        self.light2d_mut().map(|light| &mut light.color)
    }

    /// The node's enabled Shadow Caster 2D (component, or the legacy
    /// `shadow_caster` field).
    pub fn shadow_caster(&self) -> Option<&crate::ShadowCaster2D> {
        if let Some(caster) = &self.shadow_caster {
            return Some(caster);
        }
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::ShadowCaster2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Rigidbody 2D.
    pub fn rigidbody(&self) -> Option<&crate::Rigidbody2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Rigidbody2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Hinge Joint 2D, if any.
    pub fn hinge_joint(&self) -> Option<&crate::HingeJoint2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::HingeJoint2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled IK constraint, if any.
    pub fn ik_constraint(&self) -> Option<&crate::IkConstraint2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::IkConstraint2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Liquid 2D, if any.
    /// This node's enabled Particle Fluid 2D (liquid look for its particles).
    pub fn particle_fluid(&self) -> Option<&crate::ParticleFluid2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::ParticleFluid2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Goo 2D (member of a liquid group).
    pub fn goo(&self) -> Option<&crate::Goo2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Goo2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled VRM Poses (avatar states), if any.
    pub fn vrm_poses(&self) -> Option<&crate::VrmPoses> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::VrmPoses(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled DropShadow2D, if any.
    pub fn drop_shadow(&self) -> Option<&crate::DropShadow2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::DropShadow2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Outline2D, if any.
    pub fn outline2d(&self) -> Option<&crate::Outline2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Outline2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Repeater2D, if any.
    pub fn repeater(&self) -> Option<&crate::Repeater2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Repeater2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Mirror2D, if any.
    pub fn mirror2d(&self) -> Option<&crate::Mirror2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Mirror2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Wiggle2D, if any.
    pub fn wiggle(&self) -> Option<&crate::Wiggle2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Wiggle2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Parallax2D, if any.
    pub fn parallax(&self) -> Option<&crate::Parallax2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Parallax2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled PixelSnap2D, if any.
    pub fn pixel_snap(&self) -> Option<&crate::PixelSnap2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::PixelSnap2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled StrokeStyle2D, if any.
    pub fn stroke_style(&self) -> Option<&crate::StrokeStyle2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::StrokeStyle2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled CameraShake2D, if any.
    pub fn camera_shake(&self) -> Option<&crate::CameraShake2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::CameraShake2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's clip fade (mask / hidden-part opacity), if any.
    pub fn clip_fade(&self) -> crate::ClipFade2D {
        self.behaviors
            .iter()
            .find_map(|entry| match &entry.behavior {
                crate::NodeBehavior::ClipFade2D(value) if entry.enabled => Some(*value),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// This node's clipping options (invisible / inverted), if any.
    pub fn clip_options(&self) -> crate::ClipOptions2D {
        self.behaviors
            .iter()
            .find_map(|entry| match &entry.behavior {
                crate::NodeBehavior::ClipOptions2D(value) if entry.enabled => Some(*value),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// This node's enabled Mask 2D, if any.
    pub fn mask2d(&self) -> Option<&crate::Mask2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Mask2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Particle Path, if any.
    pub fn particle_path(&self) -> Option<&crate::ParticlePath2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::ParticlePath2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Flipbook, if any.
    pub fn flipbook(&self) -> Option<&crate::Flipbook> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Flipbook(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's own enabled Sorting Layer, if any.
    pub fn sorting_layer(&self) -> Option<u32> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::SortingLayer2D(value) if entry.enabled => Some(value.layer),
            _ => None,
        })
    }

    /// This node's enabled Light Layers, if any.
    pub fn light_layers(&self) -> Option<&crate::LightLayers2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::LightLayers2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Light Flicker, if any.
    pub fn light_flicker(&self) -> Option<&crate::LightFlicker2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::LightFlicker2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Particle Look, if any.
    pub fn particle_look(&self) -> Option<&crate::ParticleLook2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::ParticleLook2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Goo Material, if any.
    pub fn goo_material(&self) -> Option<&crate::GooMaterial2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::GooMaterial2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Group Fill, if any.
    pub fn group_fill(&self) -> Option<&crate::GroupFill> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::GroupFill(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Bone Attachment, if any.
    pub fn bone_attachment(&self) -> Option<&crate::BoneAttachment> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::BoneAttachment(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Foot IK, if any.
    pub fn foot_ik(&self) -> Option<&crate::FootIK> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::FootIK(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Humanoid Animator, if any.
    pub fn humanoid_animator(&self) -> Option<&crate::HumanoidAnimator> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::HumanoidAnimator(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Material Overrides, if any.
    pub fn material_slots(&self) -> Option<&crate::MaterialSlots> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::MaterialSlots(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Ragdoll 3D, if any.
    pub fn ragdoll3d(&self) -> Option<&crate::Ragdoll3D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Ragdoll3D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Antialias 2D (smooth liquid edges), if any.
    pub fn antialias(&self) -> Option<&crate::Antialias2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Antialias2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// This node's enabled Particle Behaviours 2D.
    pub fn particle_behaviors(&self) -> Option<&crate::ParticleBehaviors2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::ParticleBehaviors2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    pub fn liquid(&self) -> Option<&crate::Liquid2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Liquid2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Tree 2D, if any.
    pub fn tree(&self) -> Option<&crate::Tree2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Tree2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Cloth 2D, if any.
    pub fn cloth(&self) -> Option<&crate::Cloth2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::Cloth2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Audio Source, if any.
    pub fn audio_source(&self) -> Option<&crate::AudioSource> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::AudioSource(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// The node's enabled Sprite Skin, if any.
    pub fn sprite_skin(&self) -> Option<&crate::SpriteSkin2D> {
        self.behaviors.iter().find_map(|entry| match &entry.behavior {
            crate::NodeBehavior::SpriteSkin2D(value) if entry.enabled => Some(value),
            _ => None,
        })
    }

    /// Every enabled collider on the node, in component order.
    pub fn colliders(&self) -> impl Iterator<Item = crate::behavior::ColliderRef<'_>> {
        self.behaviors.iter().filter(|entry| entry.enabled).filter_map(|entry| match &entry.behavior {
            crate::NodeBehavior::BoxCollider2D(c) => Some(crate::behavior::ColliderRef::Box(c)),
            crate::NodeBehavior::CircleCollider2D(c) => Some(crate::behavior::ColliderRef::Circle(c)),
            crate::NodeBehavior::CapsuleCollider2D(c) => Some(crate::behavior::ColliderRef::Capsule(c)),
            crate::NodeBehavior::PolygonCollider2D(c) => Some(crate::behavior::ColliderRef::Polygon(c)),
            _ => None,
        })
    }

    /// Rewrites a node from before Camera/Light 2D/Trail/Particle System
    /// were components: such a node's `kind` becomes an empty `Group` (a
    /// plain GameObject) carrying that component instead, and a legacy
    /// `shadow_caster` field becomes a Shadow Caster 2D component. Returns
    /// whether anything changed. Idempotent.
    pub fn migrate_legacy_kind_to_behaviors(&mut self) -> bool {
        let behavior = match std::mem::replace(&mut self.kind, NodeKind::Group(GroupNode::default())) {
            NodeKind::Camera(c) => Some(crate::NodeBehavior::Camera(c)),
            NodeKind::Light2D(l) => Some(crate::NodeBehavior::Light2D(l)),
            NodeKind::Trail(t) => Some(crate::NodeBehavior::TrailRenderer(t)),
            NodeKind::ParticleSystem(p) => Some(crate::NodeBehavior::ParticleSystem(p)),
            other => {
                self.kind = other;
                None
            }
        };
        let mut changed = false;
        if let Some(behavior) = behavior {
            self.behaviors.insert(0, crate::NodeBehaviorEntry { behavior, enabled: true });
            changed = true;
        }
        if let Some(caster) = self.shadow_caster.take() {
            self.behaviors.push(crate::NodeBehaviorEntry { behavior: crate::NodeBehavior::ShadowCaster2D(caster), enabled: true });
            changed = true;
        }
        changed
    }

    /// A plain GameObject (an empty `Group`) carrying `components` — how the
    /// + menu's Camera / Light 2D / Trail / Particle System items create
    /// objects now, like Unity's GameObject menu.
    pub fn with_components(name: impl Into<String>, parent: Option<NodeId>, components: Vec<crate::NodeBehavior>) -> Self {
        let mut node = Self::new(name, parent, NodeKind::Group(GroupNode::default()));
        node.behaviors = components.into_iter().map(|behavior| crate::NodeBehaviorEntry { behavior, enabled: true }).collect();
        node
    }

    pub fn new(name: impl Into<String>, parent: Option<NodeId>, kind: NodeKind) -> Self {
        Self {
            name: name.into(),
            parent,
            transform: Transform2D::default(),
            kind,
            deleted: false,
            behaviors: Vec::new(),
            id: String::new(),
            classes: Vec::new(),
            clip_children: false,
            visible: true,
            z_index: 0.0,
            unlit: false,
            shadow_caster: None,
            prefab: None,
        }
    }
}

/// What a node actually is, beyond its transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    /// A filled vector shape — the only drawable kind implemented in Phase 1.
    Shape(ShapeNode),
    /// A grouping node with no visual payload of its own beyond an optional
    /// mask — see [`GroupNode`].
    Group(GroupNode),
    /// A rigid bone in a skeletal chain — see [`BoneNode`].
    Bone(BoneNode),
    /// One instance of a reusable [`crate::ComponentDefinition`] — see
    /// [`ComponentInstance`].
    Instance(ComponentInstance),
    /// A shaped, rasterized run of text — see [`TextNode`].
    Text(TextNode),
    /// A raster image drawn on a quad — see [`ImageNode`]. Appended last
    /// (not inserted between existing variants), same `.teab` index-encoding
    /// reason `Text` was appended last before it.
    Image(ImageNode),
    /// A Unity-`ParticleSystem`-style emitter — see
    /// [`crate::ParticleSystemNode`]. Appended last, same reason as `Image`.
    ParticleSystem(crate::ParticleSystemNode),
    /// A Unity-`Tilemap`-style tile grid — see [`crate::TilemapNode`].
    /// Appended last, same reason as `Image`/`ParticleSystem`.
    Tilemap(crate::TilemapNode),
    /// A Unity-`Camera`-style viewpoint — see [`CameraNode`]. Appended last,
    /// same reason as `Image`/`ParticleSystem`/`Tilemap`.
    Camera(CameraNode),
    /// Unity's `Light 2D` — see [`crate::Light2DNode`]. Appended last, same
    /// `.teab` variant-index reason as every kind before it.
    Light2D(crate::Light2DNode),
    /// Unity's `Trail Renderer` — see [`crate::TrailNode`]. Appended last.
    Trail(crate::TrailNode),
    /// A 3D node (mesh / camera / light / pivot) with its own 3D transform —
    /// see [`crate::Node3D`] and the `space3d` module. Appended last.
    Node3D(crate::Node3D),
}

/// Payload for [`NodeKind::Group`]: exists to give its children a shared
/// parent transform, and (Phase 10) optionally clip them to another node's
/// shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct GroupNode {
    /// A child of this same group (nothing enforces that relationship here —
    /// see `Artboard::children_of`) whose shape clips the rest of the
    /// group's children when rendered, via the stencil-buffer technique in
    /// `an1mbyt3_renderer::Renderer::render_with_mask` — AN1MBYT3_BUILD_PLAN.md
    /// §6's "masks... dedicated shader passes / stencil-buffer techniques"
    /// (Phase 10 exit criterion: "a mask layer clips a group's contents").
    /// `None` (`Default`) is an ordinary, unmasked group — the common case,
    /// and the only case Phases 1-9 ever construct.
    pub mask: Option<NodeId>,
    /// Unity/Godot "Y Sort" — when `true`, this group's direct children draw
    /// in descending order of their own current *world* Y translation (a
    /// larger Y, i.e. visually higher/further back in a +Y-up, top-down
    /// scene, draws first/behind; a smaller Y, lower/nearer the viewer, draws
    /// last/in front) instead of their authored `Node::z_index`/draw-order —
    /// recomputed fresh every frame (`Artboard::draw_order_node_ids`), so a
    /// character walking in front of/behind a tree as it crosses the tree's
    /// own Y position re-sorts automatically, the "dynamic... above/below"
    /// depth a static z-index can't express on its own. Doesn't recurse: a
    /// grandchild group's own children still sort by *its* own setting
    /// (`z_index` or its own `y_sort_enabled`), independent of this one.
    /// `#[serde(default)]` (`false`, today's existing "sort by z_index/
    /// authored order" behavior) for the usual back-compat reason.
    #[serde(default)]
    pub y_sort_enabled: bool,
}

/// Payload for [`NodeKind::Bone`]: a bone's reach.
///
/// A bone's own `Node::transform` is its joint pose (translation/rotation
/// relative to its parent, per AN1MBYT3_BUILD_PLAN.md §5); `length` is the
/// one thing a bone needs beyond that shared transform — how far along its
/// local +x axis its tip (where a child bone attaches, or where an IK chain
/// ends) sits. Skinning weights themselves (`an1mbyt3_geometry::skin::SkinBinding`)
/// aren't stored on the bone or the document at all yet — a demo builds its
/// own `SkinBinding` alongside its mesh (see `an1mbyt3-constraints`'s
/// `bone_skin` example); making that a saved, authored part of the document
/// model is later work, not part of Phase 5's bones-are-real-nodes step.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoneNode {
    pub length: f32,
}

/// Payload for [`NodeKind::Camera`]: a Unity-`Camera`-style scene viewpoint.
/// Its own `Node::transform` (translation/rotation, per the shared-transform
/// convention every `NodeKind` follows) is the camera's position/facing in
/// the artboard; `zoom` is the one thing a camera needs beyond that —
/// Unity's orthographic `size` equivalent (smaller = more zoomed in). An
/// artboard's `active_camera` (`Artboard::active_camera`) names which one
/// `WasmEditor::render` actually views the scene through while Playground is
/// running (see that field's own doc comment) — a `Camera` node with no
/// artboard pointing at it as its `active_camera` still exists, is still
/// selectable/editable, just isn't currently driving anything, the same
/// "authored but not wired up yet" relationship `StateMachineDef` has to
/// `Artboard::active_state_machine`.
///
/// The camera's *frame* — the world-space rectangle it actually sees — is
/// its artboard's own `width`x`height` divided by `zoom`, centered on its
/// world position: the artboard size acts as the game's reference
/// resolution (Unity's Game-view resolution), so a camera frame drawn in the
/// editor is exactly what Playground will show, fit to whatever window size
/// it's played in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraNode {
    pub zoom: f32,
    /// Unity's Camera "Background" (with Clear Flags = Solid Color) — what
    /// the whole canvas clears to while this camera is the one Playground is
    /// viewing through. Straight linear RGBA, same convention as
    /// `ShapeNode::fill_color`. `#[serde(default = ...)]` for the usual
    /// back-compat reason.
    #[serde(default = "default_camera_background")]
    pub background_color: [f32; 4],
    /// A node this camera tracks during Playground — e.g. the player.
    /// `None` = a fixed camera. A stale/deleted id degrades to "fixed,"
    /// same "stale reference degrades gracefully" convention
    /// `Artboard::active_camera` uses. Only ever *moves* the camera at
    /// runtime (Playground's own play session — the authored position is
    /// restored on leaving it, same as every other runtime-driven pose).
    #[serde(default)]
    pub follow_target: Option<NodeId>,
    /// How quickly the camera catches up to `follow_target`, as an
    /// exponential-smoothing rate per second (each frame closes
    /// `1 - e^(-rate * dt)` of the remaining gap) — `0.0` snaps instantly,
    /// `~5.0` is a gentle trailing follow, higher is tighter. Frame-rate
    /// independent by construction, unlike a fixed per-frame lerp fraction.
    #[serde(default)]
    pub follow_smoothing: f32,
    /// URP's Vignette override, carried by the camera — darkens/tints the
    /// frame's edges while this camera is the one Playground views through.
    #[serde(default)]
    pub vignette: CameraVignette,
    /// Unity's `Pixel Perfect Camera` component, carried by the camera.
    #[serde(default)]
    pub pixel_perfect: PixelPerfect,
}

/// Unity's Pixel Perfect Camera grid-snapping mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PixelGridSnapping {
    /// Only the integer scale is enforced.
    None,
    /// The camera moves in whole world pixels, so the scene never sits
    /// between pixels while scrolling.
    #[default]
    PixelSnapping,
    /// The whole frame renders at the reference resolution and is
    /// nearest-upscaled — every sprite (rotated/scaled too) stays on the
    /// pixel grid. The most "authentic" retro look.
    UpscaleRenderTexture,
}

/// Unity's Pixel Perfect Camera `Crop Frame` — what fills the window
/// outside the integer-scaled reference frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PixelCropFrame {
    /// Show whatever extra world fits around the frame.
    #[default]
    None,
    /// Black bars left/right only.
    Pillarbox,
    /// Black bars top/bottom only.
    Letterbox,
    /// Black bars on all sides — exactly the reference frame, scaled.
    Windowbox,
    /// Crop to the frame, but stretch it to fill the window (non-integer
    /// scale — gives up pixel perfection for no bars).
    StretchFill,
}

/// Unity's `Pixel Perfect Camera`. This engine's world unit is already one
/// art pixel (Unity's Assets Pixels Per Unit = 1) and the reference
/// resolution is the scene's own width x height, so what's left to choose is
/// how strictly the image is held to the pixel grid.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PixelPerfect {
    /// Off = the camera fits the frame to the window at any (fractional)
    /// scale, the default behavior.
    pub enabled: bool,
    pub grid_snapping: PixelGridSnapping,
    pub crop_frame: PixelCropFrame,
}

/// A camera's vignette (URP Volume "Vignette": color, intensity, radius,
/// smoothness). Rendered as [`crate::PostEffect::Vignette`] appended to the
/// artboard's own effect stack while the camera is active.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraVignette {
    pub enabled: bool,
    /// Edge tint (alpha ignored; use `intensity`).
    pub color: [f32; 4],
    /// 0..=1 — how strongly the edge color covers the image.
    pub intensity: f32,
    /// 0..=1.5 — how far from center the clear area reaches (1 ~ the
    /// frame's half-diagonal).
    pub radius: f32,
    /// 0..=1 — URP's `Smoothness`: how soft/blurred the vignette edge is.
    pub blur: f32,
}

impl Default for CameraVignette {
    fn default() -> Self {
        Self { enabled: false, color: [0.0, 0.0, 0.0, 1.0], intensity: 0.45, radius: 0.75, blur: 0.5 }
    }
}

fn default_camera_background() -> [f32; 4] {
    [0.11, 0.11, 0.13, 1.0]
}

impl Default for CameraNode {
    fn default() -> Self {
        Self { zoom: 1.0, background_color: default_camera_background(), follow_target: None, follow_smoothing: 5.0, vignette: CameraVignette::default(), pixel_perfect: PixelPerfect::default() }
    }
}

/// Payload for [`NodeKind::Shape`]: a path plus a flat fill color.
///
/// Stroke, gradients, and blend-mode/opacity properties are out of scope for
/// Phase 1 (AN1MBYT3_BUILD_PLAN.md §10) — flat fill is the minimum needed to
/// prove the draw → animate → save → render pipeline in §11.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeNode {
    pub path: Path,
    /// Straight (non-premultiplied) linear RGBA, each component in `0.0..=1.0`.
    pub fill_color: [f32; 4],
    /// How this shape's fill composites against whatever's drawn behind it
    /// — see `crate::BlendMode`'s own doc comment for why this moved into
    /// `an1mbyt3-core` (Phase 10 originally had it as a per-`DrawCall`
    /// rendering-only concept with no saved representation at all).
    /// `#[serde(default)]` for the same back-compat reason every additive
    /// field here gets one — a `.tea`/`.teab` saved before this field
    /// existed loads every shape as `BlendMode::Normal`, its prior implicit
    /// behavior.
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// `Some` overrides `fill_color` with a gradient — see
    /// `crate::gradient`'s module doc comment for the overall design.
    /// `#[serde(default)]` (`None`) for the usual back-compat reason: every
    /// `.tea`/`.teab` saved before gradients existed is exactly a flat
    /// `fill_color`, unchanged.
    #[serde(default)]
    pub gradient: Option<crate::gradient::GradientDef>,
    /// `Some` textures this shape with an already-imported raster asset
    /// instead of a flat/gradient fill — "everything can be anything," a
    /// plain vector shape gets the same Unity-`Sprite`-field-style asset
    /// reference `NodeKind::Image` itself uses, rather than needing to
    /// become a whole different node kind just to show a picture. Takes
    /// priority over `gradient`/`fill_color` at render time when set (see
    /// `an1mbyt3_renderer::DrawCall::texture`'s own doc comment) — this
    /// shape's own tessellated path still defines its silhouette either
    /// way, only the fill changes; `fill_color` still tints the image the
    /// same way it already tints a plain `NodeKind::Image`. `#[serde(default)]`
    /// (`None`) for the usual back-compat reason: every shape saved before
    /// this field existed is exactly its flat/gradient fill, unchanged.
    #[serde(default)]
    pub image: Option<ImageId>,
    /// `Some` outlines this shape's path with its own paint, independent of
    /// `fill_color`/`gradient`/`image` above — the same "everything can be
    /// anything" reasoning `image` already established, just for a stroke
    /// instead of a fill. `#[serde(default)]` (`None`) for the usual
    /// back-compat reason: every shape saved before stroke existed had none.
    #[serde(default)]
    pub stroke: Option<Stroke>,
}

/// A shape's outline paint + geometry — mirrors `fill_color`/`gradient`/
/// `image`'s three-way "flat, or overridden by a gradient, or overridden by
/// an image" shape for the stroke instead of the fill, plus the
/// width/cap/join a stroke needs that a fill doesn't. Deliberately not a
/// single unified `Paint` enum shared with the fill fields above — that
/// would rename/restructure every existing fill read site (`KeyframeValue`,
/// `GraphProperty`, the renderer's draw-building, every DTO) for no benefit
/// over just mirroring the same three-field shape here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f32,
    /// Straight (non-premultiplied) linear RGBA — same convention as
    /// `ShapeNode::fill_color`. Overridden by `gradient`, then by `image`,
    /// same priority order as the fill fields.
    pub color: [f32; 4],
    #[serde(default)]
    pub gradient: Option<crate::gradient::GradientDef>,
    #[serde(default)]
    pub image: Option<ImageId>,
    #[serde(default)]
    pub cap: StrokeCap,
    #[serde(default)]
    pub join: StrokeJoin,
}

/// How a stroke's ends are capped — SVG's `stroke-linecap`. A fresh copy of
/// the same three options `an1mbyt3_geometry::tessellate::StrokeCap`
/// already defines, not a re-export: `an1mbyt3-geometry` depends on
/// `an1mbyt3-core` (see `path.rs`'s own module doc), not the other way
/// around, so core can't reference geometry's enum directly — whichever
/// call site tessellates a stroke (`an1mbyt3-wasm`, which already depends
/// on both) maps between the two with a trivial `match`, the same
/// "each layer keeps its own copy across a dependency boundary" convention
/// every wasm-bindgen DTO in this codebase already follows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeCap {
    #[default]
    Butt,
    Square,
    Round,
}

/// How two stroke segments meet at a corner — SVG's `stroke-linejoin`. See
/// [`StrokeCap`]'s doc comment for why this mirrors, rather than reuses,
/// `an1mbyt3_geometry::tessellate::StrokeJoin`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Payload for [`NodeKind::Text`]: a string, the font it's set in, and its
/// size — everything `an1mbyt3_text::shape_and_tessellate` needs to shape
/// and tessellate the run into per-glyph meshes.
///
/// Deliberately holds *inputs* to shaping (string/font/size), not the shaped
/// output itself — the shaped glyph meshes are a derived, cacheable artifact
/// (AN1MBYT3_BUILD_PLAN.md §6: "re-shaping only happens when the string,
/// font, or size actually changes"), not part of the document's saved
/// content, the same way a `ShapeNode`'s tessellated `Mesh` never lives here
/// either — only its `Path`. One style per node (font/size/color uniform
/// across the whole `content` string) — multiple styles within one run
/// ("rich text") is out of scope (AN1MBYT3_BUILD_PLAN.md §16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextNode {
    pub content: String,
    pub font: FontId,
    pub size_px: f32,
    /// Straight (non-premultiplied) linear RGBA, each component in `0.0..=1.0`
    /// — same convention as `ShapeNode::fill_color`, applied uniformly to
    /// every glyph in the run.
    pub fill_color: [f32; 4],
    /// `Some` overrides `fill_color` with a gradient, same as
    /// `ShapeNode::gradient` — see `crate::gradient`'s module doc comment.
    #[serde(default)]
    pub gradient: Option<crate::gradient::GradientDef>,
}

/// Payload for [`NodeKind::Image`]: a reference to a [`crate::ImageAsset`]
/// plus an authored display size and tint — everything needed to draw a
/// textured quad. Deliberately holds a *reference* (`image`), never inline
/// decoded pixels or a GPU texture — same "input, not derived artifact"
/// discipline as [`TextNode`]; the baked/uploaded texture is a cache
/// concern for whatever's rendering, not part of the document.
///
/// `width`/`height` are the node's authored display size, independent of
/// the source image's own pixel dimensions (the same way a `ShapeNode`'s
/// `Path` and a node's `Transform2D` scale are independent) — resizing an
/// `Image` node on canvas is a `Transform2D::scale` edit, not a
/// `ImageAsset::processing.resize` edit (that field is for baking a
/// *cheaper/re-sampled* source texture, a separate concern from this node's
/// on-canvas size).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageNode {
    pub image: ImageId,
    pub width: f32,
    pub height: f32,
    /// Straight (non-premultiplied) linear RGBA multiplier — `[1.0; 4]` is
    /// no tint, same convention as `ShapeNode::fill_color`.
    pub fill_color: [f32; 4],
    /// Not yet honored by the renderer — Image draws are Normal-blend only
    /// for this pass (matches masks' own "Normal-only for now" scope-narrowing).
    #[serde(default)]
    pub blend_mode: BlendMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stroke_round_trips_through_json_including_its_defaulted_fields() {
        let shape = ShapeNode {
            path: crate::path::Path::rect(10.0, 10.0),
            fill_color: [1.0, 1.0, 1.0, 1.0],
            blend_mode: BlendMode::default(),
            gradient: None,
            image: None,
            stroke: Some(Stroke { width: 2.0, color: [0.0, 0.0, 0.0, 1.0], gradient: None, image: None, cap: StrokeCap::Round, join: StrokeJoin::Bevel }),
        };
        let json = serde_json::to_string(&shape).expect("serialize");
        let round_tripped: ShapeNode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round_tripped, shape);
    }

    #[test]
    fn a_shape_saved_before_stroke_existed_deserializes_with_no_stroke() {
        // No `stroke` key at all — the shape of every `.tea` saved before
        // this field existed.
        let json = r#"{"path":{"commands":[]},"fill_color":[1.0,1.0,1.0,1.0]}"#;
        let shape: ShapeNode = serde_json::from_str(json).expect("deserialize");
        assert_eq!(shape.stroke, None);
    }
}
