//! Node/artboard identity.
//!
//! Uses `slotmap`'s generational-key scheme instead of raw `usize` indices or an
//! `Rc<RefCell<_>>` graph (AN1MBYT3_BUILD_PLAN.md §4, "Identity & storage"): each
//! key packs an index plus a generation counter, so deleting a node and later
//! reusing its slot can never make a stale `NodeId` alias the wrong node — a
//! stale key simply fails the lookup instead of silently pointing at whatever
//! now occupies that slot. This also keeps IDs serializable as plain integers
//! (via slotmap's `serde` feature), which matters for the `.tea`/`.teab` file
//! formats (§4, "Serialization").

use slotmap::new_key_type;

new_key_type! {
    /// Identifies one node within a single [`crate::Artboard`]'s node arena.
    ///
    /// A `NodeId` is only meaningful relative to the `Artboard` that produced it —
    /// there is no cross-artboard node lookup, matching the `Document → Artboards
    /// → Nodes` hierarchy in AN1MBYT3_BUILD_PLAN.md §4.
    pub struct NodeId;

    /// Identifies one artboard within a [`crate::Document`].
    pub struct ArtboardId;

    /// Identifies one reusable [`crate::ComponentDefinition`] within a
    /// [`crate::Document`] — components are a document-level asset (usable
    /// from any artboard), unlike `NodeId`, which is only meaningful within
    /// one artboard.
    pub struct ComponentId;

    /// Identifies one reusable [`crate::FontAsset`] within a [`crate::Document`]
    /// — same document-level-asset shape as `ComponentId`: a font's bytes are
    /// saved once and referenced by any number of `NodeKind::Text` nodes,
    /// across any artboard.
    pub struct FontId;

    /// Identifies one [`crate::Animation`] within an [`crate::Artboard`] —
    /// artboard-scoped like `NodeId` (an animation's tracks reference
    /// `NodeId`s, which only make sense within one artboard), not
    /// document-level like `ComponentId`/`FontId`.
    pub struct AnimationId;

    /// Identifies one [`crate::StateMachineDef`] within an [`crate::Artboard`]
    /// — artboard-scoped like `AnimationId` (a state references an
    /// `AnimationId`, which only makes sense within one artboard).
    pub struct StateMachineId;

    /// Identifies one reusable [`crate::ImageAsset`] within a [`crate::Document`]
    /// — same document-level-asset shape as `FontId`: an image's bytes are
    /// saved once and referenced by any number of `NodeKind::Image` nodes,
    /// across any artboard.
    pub struct ImageId;

    /// Identifies one [`crate::SpriteGroup`] within a [`crate::Document`] —
    /// a purely organizational grouping of `ImageId`s (e.g. a GIF's decoded
    /// frames), document-level like `ImageId`/`FontId`.
    pub struct SpriteGroupId;

    /// Identifies one reusable [`crate::PrefabDefinition`] within a
    /// [`crate::Document`] — document-level like `ImageId`: any scene can
    /// instantiate any prefab.
    pub struct PrefabId;

    /// Identifies one reusable [`crate::Material`] within a [`crate::Document`]
    /// — same document-level-asset shape as `ImageId`/`FontId`: a material is
    /// saved once and referenced by any number of `SpriteRendererBehavior`s,
    /// across any artboard.
    pub struct MaterialId;

    /// Identifies one imported 3D model ([`crate::ModelAsset`]) within a
    /// [`crate::Document`] — document-level like `ImageId`.
    pub struct ModelId;

    /// Identifies one reusable [`crate::ScriptDefinition`] within a
    /// [`crate::Document`] — same document-level-asset shape as
    /// `ComponentId`: a script type is authored once and attached (as a
    /// [`crate::ScriptInstance`]) to any number of nodes, across any artboard.
    pub struct ScriptId;
}
