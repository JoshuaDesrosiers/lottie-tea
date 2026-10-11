//! `an1mbyt3-core`: the document model and scene graph.
//!
//! This is the single source of truth shared by the editor, runtime, renderer,
//! importer, and exporter — see AN1MBYT3_BUILD_PLAN.md §4 ("Document Model").
//!
//! Owns:
//! - The `Document → Artboards → Nodes` hierarchy ([`Document`], [`Artboard`],
//!   [`Node`]/[`NodeKind`]).
//! - Node identity: a generational-arena (`slotmap`) `NodeId`/`ArtboardId`/
//!   `ComponentId` scheme (`ids.rs`), so deleted/reused slots can't alias a
//!   stale reference.
//! - 2D transforms ([`Transform2D`]) and the plain vector path representation
//!   ([`Path`]/[`PathCommand`]) that `an1mbyt3-geometry` tessellates.
//! - Reusable [`ComponentDefinition`]s and their [`ComponentInstance`]s
//!   (`component.rs`) — AN1MBYT3_BUILD_PLAN.md §10 Phase 7.
//! - Authored keyframe data: [`Easing`]/[`Keyframe`]/[`Track`]/[`AnimationClip`]
//!   (`easing.rs`/`keyframe.rs`/`animation.rs`) — moved here from
//!   `an1mbyt3-runtime` in Phase 4, once keyframes needed to actually persist
//!   as part of a saved `.tea`/`.teab` document rather than only existing as
//!   runtime-constructed demo content; see `animation.rs`'s module doc for
//!   why this doesn't cross the "no animation evaluation" line below (it's
//!   pure data plus pure interpolation math, not time-integration or
//!   rendering). `an1mbyt3-runtime` still re-exports all four under the same
//!   names for source compatibility.
//! - Named, multi-node, multi-property authored animations: [`Animation`]/
//!   [`PropertyTrack`]/[`PropertyKind`]/[`KeyframeValue`] (`timeline.rs`) —
//!   what the editor's Timeline panel actually creates/edits, distinct from
//!   the single-node `AnimationClip` above; see `timeline.rs`'s module doc
//!   for the relationship between the two.
//! - Saved, authored state machine *definitions*: [`StateMachineDef`]/
//!   [`StateDef`]/[`TransitionDef`]/[`ConditionDef`]/[`InputDef`]/[`InputKind`]/
//!   [`LoopMode`]/[`PlaybackDirection`] (`state_machine.rs`) — moved here
//!   (Phase 8) for the same reason `AnimationClip` moved earlier: these need
//!   to persist as part of a saved `.tea`/`.teab` document, not only exist
//!   as hand-built runtime demo content. The *live* running instance
//!   (`an1mbyt3_runtime::StateMachine`) is built from one of these.
//!
//! Does NOT own (yet, or ever):
//! - Path tessellation or mesh generation — see `an1mbyt3-geometry`.
//! - Time-integrated playback (current play position, loop mode, play/pause)
//!   — see `an1mbyt3-runtime`'s `Playback`.
//! - Bone/IK/constraint solving — see `an1mbyt3-constraints`.
//! - Rendering — see `an1mbyt3-renderer`.
//!
//! Status: Phase 1 document model/transforms/paths, Phase 2's keyframes/
//! `AnimationClip`, Phase 3's `NodeKind::Text`/`FontAsset`, Phase 4's
//! [`Command`]/undo-redo/node-create-delete/multi-property multi-animation
//! timeline, Phase 5's `NodeKind::Bone`, and Phase 7's `NodeKind::Instance`/
//! `ComponentDefinition` are all implemented (AN1MBYT3_BUILD_PLAN.md §10).
//! `NodeKind` only has variants something in the codebase actually handles —
//! an `Image` variant arrives in the phase that implements it, not ahead of
//! time.

mod animation;
mod artboard;
mod behavior;
mod blend_mode;
mod block_program;
mod command;
mod component;
mod document;
mod easing;
mod font_asset;
mod gradient;
mod ids;
mod image_asset;
mod keyframe;
mod light2d;
mod material;
mod model_asset;
mod audio_asset;
pub mod ui;
pub mod shader_graph;
pub mod space3d;
mod node;
mod particle;
mod path;
mod physics2d;
mod rig2d;
mod post_effect;
mod prefab;
mod cloth;
mod tree;
mod drawing;
pub mod events;
pub mod particle3d;
pub mod particle_fx2d;
mod vrm_pose;
pub mod text_file;
mod program;
mod script;
mod sprite_group;
mod state_machine;
mod tilemap;
mod timeline;
mod trail;
mod transform;

pub use animation::AnimationClip;
pub use artboard::{Artboard, SceneIndex};
pub use behavior::{ColliderRef, NodeBehavior, NodeBehaviorEntry, SpriteRendererBehavior};
pub use script::{ScriptDefinition, ScriptFieldDef, ScriptFieldKind, ScriptFieldValue, ScriptInstance};
pub use blend_mode::BlendMode;
pub use block_program::{BlockProgramDef, BlockScript, EventKind, Expr, ListExpr, MathFn, NodeRef, Statement};
pub use command::Command;
pub use component::{ComponentDefinition, ComponentInstance, ComponentVariant};
pub use document::{AdaptiveResolution, Document};
pub use easing::Easing;
pub use font_asset::FontAsset;
pub use gradient::{GradientDef, GradientKind, GradientStop};
pub use ids::{AnimationId, ArtboardId, ComponentId, DrawingId, FontId, ImageId, AudioId, MaterialId, ModelId, NodeId, PrefabId, ScriptId, SpriteGroupId, StateMachineId, TextFileId, PropertyId};
pub use events::{ActionKind, BindTarget, Binding, EventAction, PropertyBindings, PropertyDef, PropKind, UiEvents};
pub use text_file::{TextFile, FILE_REF_PREFIX};
pub use cloth::{Cloth2D, ClothPatternKind, ClothPin};
pub use tree::{Tree2D, TreeSpecies};
pub use particle_fx2d::{Antialias2D, Goo2D, ParticleBehaviors2D, ParticleFluid2D};
pub use vrm_pose::{BonePose, ExpressionWeight, VrmPose, VrmPoses};
pub use particle3d::{Emitter3DShape, LifeCurve, ParticleBehavior3D, ParticleBehaviorKind3D, ParticleBlend3D, ParticleSystem3D, PointSpace};
pub use drawing::{DrawingAnimation, DrawingAsset, DrawingCel, DrawingCelContent, DrawingContent, DrawingLayer, Flipbook};
pub use prefab::{has_node_refs, ref_uid, remap_node_refs, uid_ref, PrefabDefinition, PrefabLink, PrefabNode};
pub use image_asset::{ImageAsset, ImageFilterMode, ImageProcessing};
pub use keyframe::{Keyframe, Lerp, Track};
pub use material::{Material, MaterialKind};
pub use model_asset::{ModelAsset, ModelFile};
pub use audio_asset::{audio_extension, AudioAsset, AudioSource};
pub mod layout;
pub mod physics3d;
pub use physics3d::{default_gravity_3d, Collider3D, Collider3DShape, Rigidbody3D, RigidbodyType3D};
pub use layout::{responsive_view, solve_layouts, LayoutWrite, ScreenInfo, ScreenMode, ScreenSettings, UiAlign, UiEdges, UiFlexDirection, UiFlexStyle, UiJustify, UiLayout, UiLayoutItem, UiScreenCondition, UiShowWhen, UiSize};
pub use ui::{slider_fraction, stack_positions, StackAlign, StackDirection, UiAnchor, UiButton, UiFillDirection, UiScrollView, UiScrollbar, UiSlider, UiHtml, UiStack, UiStateStyle, UiToggle, UiTransition};
pub use space3d::{AnimationName, Camera3D, Light3D, Light3DKind, Mesh3D, Node3D, Node3DContent, Primitive3D, Transform3D};
pub use shader_graph::{compile_wgsl, ShaderDomain, ShaderEdgeDef, ShaderGraphDef, ShaderGraphError, ShaderNodeDef, ShaderNodeKind, ShaderType};
pub use node::{BoneNode, CameraNode, CameraVignette, GroupNode, PixelCropFrame, PixelGridSnapping, PixelPerfect, ImageNode, Node, NodeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin, TextNode};
pub use particle::{EmitterShape, ParticleBurst, ParticleEmission, ParticleMain, ParticleRange, ParticleSystemNode, SimulationSpace, SizeKeyframe};
pub use path::{Path, PathCommand};
pub use rig2d::{affine_to_array, array_to_affine, HingeJoint2D, IkConstraint2D, SpriteSkin2D};
pub use physics2d::{
    BoxCollider2D, CapsuleCollider2D, CapsuleDirection2D, CircleCollider2D, CollisionDetection2D, Liquid2D, PhysicsMaterial2D, PolygonCollider2D,
    Rigidbody2D, RigidbodyType2D,
};
pub use post_effect::{PostEffect, PostEffectLayer};
pub use program::{GraphEdgeDef, GraphNodeDef, GraphNodeKind, GraphProperty, NodeGraphDef, ProgramDef, SocketOverride};
pub use sprite_group::SpriteGroup;
pub use state_machine::{Blend1DDef, ConditionDef, InputDef, InputKind, LoopMode, PlaybackDirection, StateDef, StateMachineDef, StateMachineLayerDef, TransitionDef};
pub use tilemap::TilemapNode;
pub use light2d::{Light2DNode, Light2DType, LightBlendStyle, LightOverlapOperation, ShadowCaster2D, ShadowCastingOption};
pub use trail::TrailNode;
pub use timeline::{Animation, KeyframeValue, PropertyKind, PropertyTrack};
pub use transform::Transform2D;
