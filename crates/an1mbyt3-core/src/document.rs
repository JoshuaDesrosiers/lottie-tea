//! Document: the top-level container, and the single source of truth described
//! in AN1MBYT3_BUILD_PLAN.md §4 — editor, runtime, renderer, importer, and
//! exporter all read/write this same type, never a private copy of their own.
//!
//! Also owns the undo/redo stack (§4, "Undo/Redo Architecture") — living
//! here rather than on the editor's UI state is what makes undo/redo work
//! for any embedder, not just the React editor, and is why every mutation
//! goes through [`Document::apply`]/[`Command`] instead of a direct field
//! write via `artboard_mut`/`node_mut`.

use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

use crate::{
    artboard::Artboard, command::Command, component::ComponentDefinition, font_asset::FontAsset, ids::ArtboardId,
    ids::ComponentId, ids::FontId, ids::ImageId, ids::MaterialId, ids::NodeId, ids::ScriptId, ids::SpriteGroupId,
    image_asset::ImageAsset, material::Material, node::Node, script::ScriptDefinition, sprite_group::SpriteGroup,
    ids::PrefabId, prefab::PrefabDefinition,
};

/// A whole `.tea`/`.teab` document: a set of artboards, the reusable
/// [`ComponentDefinition`]s and [`FontAsset`]s they can reference, plus the
/// undo/redo history of edits made to it.
///
/// `undo_stack`/`redo_stack` are `#[serde(skip)]` — undo history is runtime
/// editing state, not part of the document's saved content (AN1MBYT3_BUILD_PLAN.md
/// §7's `.tea`/`.teab` formats have no business persisting "what you could
/// undo," only the document's current state). `components`/`fonts` are *not*
/// skipped — a component definition or font asset is real document content,
/// the same way an artboard is (AN1MBYT3_BUILD_PLAN.md §10 Phases 3, 7).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Document {
    artboards: SlotMap<ArtboardId, Artboard>,
    components: SlotMap<ComponentId, ComponentDefinition>,
    #[serde(default)]
    fonts: SlotMap<FontId, FontAsset>,
    #[serde(default)]
    images: SlotMap<ImageId, ImageAsset>,
    #[serde(default)]
    sprite_groups: SlotMap<SpriteGroupId, SpriteGroup>,
    #[serde(default)]
    materials: SlotMap<MaterialId, Material>,
    #[serde(default)]
    scripts: SlotMap<ScriptId, ScriptDefinition>,
    /// Reusable node-subtree templates — see `crate::prefab`.
    #[serde(default)]
    prefabs: SlotMap<PrefabId, PrefabDefinition>,
    /// Imported 3D models (glTF / GLB) — see `crate::model_asset`.
    #[serde(default)]
    models: SlotMap<crate::ModelId, crate::ModelAsset>,
    /// Imported sounds — see `crate::audio_asset`.
    #[serde(default)]
    audio: SlotMap<crate::AudioId, crate::AudioAsset>,
    /// Saved Paint-mode drawings — see `crate::drawing`.
    #[serde(default)]
    drawings: SlotMap<crate::DrawingId, crate::DrawingAsset>,
    /// Code / markup files (HTML, CSS, JS…) — see `crate::text_file`. Keep
    /// this the *last* serialized field: `.teab` files written before it
    /// existed end right before it, and `an1mbyt3_format`'s loader pads such
    /// a file with empty-map bytes (see `teab::from_teab_bytes`).
    #[serde(default)]
    text_files: SlotMap<crate::TextFileId, crate::TextFile>,
    /// Scenes' typed properties (Rive view models) — see `crate::events`.
    /// After `text_files`, the newest trailing field (same padding note).
    #[serde(default)]
    properties: SlotMap<crate::PropertyId, crate::PropertyDef>,
    /// Playback performance settings (adaptive resolution). After
    /// `properties`, the newest trailing field: its all-zero encoding is
    /// the default, so older `.teab` files padded with zero bytes load
    /// with it on (see `AdaptiveResolution`).
    #[serde(default)]
    adaptive_resolution: AdaptiveResolution,
    #[serde(skip)]
    undo_stack: Vec<Command>,
    #[serde(skip)]
    redo_stack: Vec<Command>,
    /// Bumped by every edit, undo and redo — never saved. The editor
    /// compares it against the value at its last save to know whether there
    /// are unsaved changes (title dot, close prompt, autosave).
    #[serde(skip)]
    revision: u64,
    /// Unique per in-memory document (fresh on every `new`/load) — lets the
    /// editor notice the whole document was replaced (a demo loaded, a
    /// `.tea` imported) and stop autosaving it into a project folder it
    /// didn't come from.
    #[serde(skip)]
    instance: DocumentInstance,
}

/// **Adaptive resolution** for playing the game (Playground and exported
/// players): when frames run long — a big window, a 4K screen, a heavy
/// post-effect stack — the scene renders at a lower internal resolution
/// (smoothly upscaled) to hold the frame rate, and climbs back to native
/// when there's headroom. Encoded so all-zero bytes mean "on, default
/// floor" (older files are padded with zeros — see `Document`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AdaptiveResolution {
    /// Off: always native resolution (pixel-perfect / capture use).
    pub disabled: bool,
    /// The lowest it may go, in percent of native (0 = the default, 50 %).
    pub min_percent: u8,
}

impl AdaptiveResolution {
    pub fn enabled(&self) -> bool {
        !self.disabled
    }
    /// The floor as a fraction (0.4 … 1).
    pub fn min_scale(&self) -> f32 {
        if self.min_percent == 0 { 0.5 } else { (self.min_percent as f32 / 100.0).clamp(0.4, 1.0) }
    }
}

/// See `Document::instance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentInstance(u64);

impl Default for DocumentInstance {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_artboard(&mut self, artboard: Artboard) -> ArtboardId {
        self.artboards.insert(artboard)
    }

    pub fn artboard(&self, id: ArtboardId) -> Option<&Artboard> {
        self.artboards.get(id)
    }

    pub fn artboard_mut(&mut self, id: ArtboardId) -> Option<&mut Artboard> {
        self.artboards.get_mut(id)
    }

    /// Every non-deleted artboard — the Scenes panel's listing, and
    /// everything `dto::summarize_document` builds from it. Mirrors
    /// `Document::components`/`Document::scripts`'s own filter-out-`deleted`
    /// convention; `Document::artboard`/`artboard_mut` still resolve a
    /// deleted one (undo/redo needs to reach it to flip the flag back).
    pub fn artboards(&self) -> impl Iterator<Item = (ArtboardId, &Artboard)> {
        self.artboards.iter().filter(|(_, a)| !a.deleted)
    }

    pub fn add_component(&mut self, definition: ComponentDefinition) -> ComponentId {
        self.components.insert(definition)
    }

    pub fn component(&self, id: ComponentId) -> Option<&ComponentDefinition> {
        self.components.get(id)
    }

    pub(crate) fn component_mut(&mut self, id: ComponentId) -> Option<&mut ComponentDefinition> {
        self.components.get_mut(id)
    }

    /// Every non-deleted component definition, for the Components panel's
    /// library listing — mirrors `Artboard::state_machines()`'s
    /// filter-out-`deleted` convention.
    pub fn components(&self) -> impl Iterator<Item = (ComponentId, &ComponentDefinition)> {
        self.components.iter().filter(|(_, def)| !def.deleted)
    }

    /// See the `instance` field.
    pub fn instance(&self) -> u64 {
        self.instance.0
    }

    /// See the `revision` field.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Marks a change made outside the command system (e.g. an import that
    /// writes assets directly) as unsaved.
    pub fn touch(&mut self) {
        self.revision += 1;
    }

    pub fn add_prefab(&mut self, definition: PrefabDefinition) -> PrefabId {
        self.prefabs.insert(definition)
    }

    /// Resolves deleted prefabs too (undo needs to reach them).
    pub fn prefab(&self, id: PrefabId) -> Option<&PrefabDefinition> {
        self.prefabs.get(id)
    }

    pub(crate) fn prefab_mut(&mut self, id: PrefabId) -> Option<&mut PrefabDefinition> {
        self.prefabs.get_mut(id)
    }

    pub fn add_drawing(&mut self, drawing: crate::DrawingAsset) -> crate::DrawingId {
        self.drawings.insert(drawing)
    }

    /// Resolves deleted drawings too (undo needs to reach them).
    pub fn drawing(&self, id: crate::DrawingId) -> Option<&crate::DrawingAsset> {
        self.drawings.get(id)
    }

    pub(crate) fn drawing_mut(&mut self, id: crate::DrawingId) -> Option<&mut crate::DrawingAsset> {
        self.drawings.get_mut(id)
    }

    /// Every non-deleted drawing, for the Resources panel.
    pub fn drawings(&self) -> impl Iterator<Item = (crate::DrawingId, &crate::DrawingAsset)> {
        self.drawings.iter().filter(|(_, d)| !d.deleted)
    }

    pub fn add_text_file(&mut self, file: crate::TextFile) -> crate::TextFileId {
        self.text_files.insert(file)
    }

    /// Resolves deleted files too (undo needs to reach them).
    pub fn text_file(&self, id: crate::TextFileId) -> Option<&crate::TextFile> {
        self.text_files.get(id)
    }

    pub(crate) fn text_file_mut(&mut self, id: crate::TextFileId) -> Option<&mut crate::TextFile> {
        self.text_files.get_mut(id)
    }

    /// Every non-deleted text file, in path order.
    pub fn text_files(&self) -> impl Iterator<Item = (crate::TextFileId, &crate::TextFile)> {
        let mut files: Vec<_> = self.text_files.iter().filter(|(_, f)| !f.deleted).collect();
        files.sort_by(|a, b| a.1.path.cmp(&b.1.path));
        files.into_iter()
    }

    /// The non-deleted file at `path` (normalized), if any.
    pub fn text_file_by_path(&self, path: &str) -> Option<(crate::TextFileId, &crate::TextFile)> {
        let path = crate::text_file::normalize_path(path);
        self.text_files.iter().find(|(_, f)| !f.deleted && f.path == path)
    }

    /// Editing a file's text as you type: no undo entry per keystroke (the
    /// code editor keeps its own undo), but it's a real change — it bumps
    /// the revision so the document counts as unsaved.
    pub fn set_text_file_text(&mut self, id: crate::TextFileId, text: String) -> bool {
        let Some(file) = self.text_files.get_mut(id) else { return false };
        if file.text == text {
            return true;
        }
        file.text = text;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// The text `value` stands for — an `@file:` reference's file text, or
    /// `value` itself (see [`crate::text_file::resolve_file_ref`]).
    pub fn resolve_text<'a>(&'a self, value: &'a str) -> Option<&'a str> {
        crate::text_file::resolve_file_ref(value, |path| self.text_files.values().find(|f| !f.deleted && f.path == path).map(|f| f.text.as_str()))
    }

    pub fn add_property(&mut self, property: crate::PropertyDef) -> crate::PropertyId {
        self.properties.insert(property)
    }

    /// Resolves deleted properties too (undo needs to reach them).
    pub fn property(&self, id: crate::PropertyId) -> Option<&crate::PropertyDef> {
        self.properties.get(id)
    }

    pub(crate) fn property_mut(&mut self, id: crate::PropertyId) -> Option<&mut crate::PropertyDef> {
        self.properties.get_mut(id)
    }

    /// Every non-deleted property of every scene, in creation order.
    /// The adaptive-resolution setting (see [`AdaptiveResolution`]).
    pub fn adaptive_resolution(&self) -> AdaptiveResolution {
        self.adaptive_resolution
    }

    /// Changes the adaptive-resolution setting. A project setting, not an
    /// edit to the artwork: no undo entry, but it does count as a change
    /// to save.
    pub fn set_adaptive_resolution(&mut self, value: AdaptiveResolution) {
        if self.adaptive_resolution != value {
            self.adaptive_resolution = value;
            self.revision += 1;
        }
    }

    pub fn properties(&self) -> impl Iterator<Item = (crate::PropertyId, &crate::PropertyDef)> {
        self.properties.iter().filter(|(_, p)| !p.deleted)
    }

    /// Every non-deleted prefab, for the Resources panel.
    pub fn prefabs(&self) -> impl Iterator<Item = (PrefabId, &PrefabDefinition)> {
        self.prefabs.iter().filter(|(_, def)| !def.deleted)
    }

    pub fn add_font(&mut self, font: FontAsset) -> FontId {
        self.fonts.insert(font)
    }

    pub fn font(&self, id: FontId) -> Option<&FontAsset> {
        self.fonts.get(id)
    }

    /// Every font in the document (fonts are never deleted).
    pub fn fonts(&self) -> impl Iterator<Item = (FontId, &FontAsset)> {
        self.fonts.iter()
    }

    pub fn add_image(&mut self, image: ImageAsset) -> ImageId {
        self.images.insert(image)
    }

    pub fn image(&self, id: ImageId) -> Option<&ImageAsset> {
        self.images.get(id)
    }

    pub fn image_mut(&mut self, id: ImageId) -> Option<&mut ImageAsset> {
        self.images.get_mut(id)
    }

    /// Every image asset, for the Resources panel's listing.
    pub fn images(&self) -> impl Iterator<Item = (ImageId, &ImageAsset)> {
        self.images.iter()
    }

    pub fn add_sprite_group(&mut self, group: SpriteGroup) -> SpriteGroupId {
        self.sprite_groups.insert(group)
    }

    pub fn sprite_group(&self, id: SpriteGroupId) -> Option<&SpriteGroup> {
        self.sprite_groups.get(id)
    }

    /// Every sprite group, for the Resources panel's listing.
    pub fn sprite_groups(&self) -> impl Iterator<Item = (SpriteGroupId, &SpriteGroup)> {
        self.sprite_groups.iter()
    }

    pub fn add_audio(&mut self, audio: crate::AudioAsset) -> crate::AudioId {
        self.audio.insert(audio)
    }

    pub fn audio(&self, id: crate::AudioId) -> Option<&crate::AudioAsset> {
        self.audio.get(id).filter(|a| !a.deleted)
    }

    pub fn audio_mut(&mut self, id: crate::AudioId) -> Option<&mut crate::AudioAsset> {
        self.audio.get_mut(id)
    }

    /// Every (non-deleted) imported sound.
    pub fn audio_assets(&self) -> impl Iterator<Item = (crate::AudioId, &crate::AudioAsset)> {
        self.audio.iter().filter(|(_, a)| !a.deleted)
    }

    /// Every sound slot, deleted or not, in slot order (project saving
    /// walks the same order the serialized slot map has).
    pub fn audio_slots(&self) -> impl Iterator<Item = (crate::AudioId, &crate::AudioAsset)> {
        self.audio.iter()
    }

    /// Every model slot, deleted ones included (project saving keeps slot
    /// order).
    pub fn model_slots(&self) -> impl Iterator<Item = (crate::ModelId, &crate::ModelAsset)> {
        self.models.iter()
    }

    pub fn add_model(&mut self, model: crate::ModelAsset) -> crate::ModelId {
        self.models.insert(model)
    }

    pub fn model(&self, id: crate::ModelId) -> Option<&crate::ModelAsset> {
        self.models.get(id).filter(|m| !m.deleted)
    }

    pub fn model_mut(&mut self, id: crate::ModelId) -> Option<&mut crate::ModelAsset> {
        self.models.get_mut(id)
    }

    /// Every (non-deleted) imported model.
    pub fn models(&self) -> impl Iterator<Item = (crate::ModelId, &crate::ModelAsset)> {
        self.models.iter().filter(|(_, m)| !m.deleted)
    }

    pub fn add_material(&mut self, material: Material) -> MaterialId {
        self.materials.insert(material)
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id)
    }

    pub fn material_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id)
    }

    pub fn add_script(&mut self, definition: ScriptDefinition) -> ScriptId {
        self.scripts.insert(definition)
    }

    pub fn script(&self, id: ScriptId) -> Option<&ScriptDefinition> {
        self.scripts.get(id)
    }

    pub fn script_mut(&mut self, id: ScriptId) -> Option<&mut ScriptDefinition> {
        self.scripts.get_mut(id)
    }

    /// Every non-deleted script definition, for the Inspector's "Add
    /// Behavior" picker — mirrors `Document::components`'s
    /// filter-out-`deleted` convention.
    pub fn scripts(&self) -> impl Iterator<Item = (ScriptId, &ScriptDefinition)> {
        self.scripts.iter().filter(|(_, def)| !def.deleted)
    }

    /// Every (non-deleted) material, for the Resources panel's listing.
    pub fn materials(&self) -> impl Iterator<Item = (MaterialId, &Material)> {
        self.materials.iter().filter(|(_, m)| !m.deleted)
    }

    /// Looks up a node within a specific artboard. Panics if either doesn't
    /// exist — see [`Command::apply`]'s doc comment for why that's the
    /// right behavior for this kind of lookup (a caller bug, not a
    /// recoverable runtime condition).
    pub(crate) fn node_mut(&mut self, artboard: ArtboardId, node: NodeId) -> &mut Node {
        self.artboards
            .get_mut(artboard)
            .expect("Document::node_mut: artboard not found")
            .node_mut(node)
            .expect("Document::node_mut: node not found in artboard")
    }

    /// Applies `command`, pushing its inverse onto the undo stack and
    /// clearing the redo stack (the standard "new edit invalidates redo
    /// history" rule — there's no way to redo-forward into a future that a
    /// fresh edit just replaced). Returns the inverse too — most callers
    /// ignore it, but `Command::AddNode`'s caller needs it to learn the
    /// freshly assigned `NodeId` (see `Command::AddNode`'s doc comment: its
    /// inverse, a `SetDeleted`, is the only place that id is ever produced).
    pub fn apply(&mut self, command: Command) -> Command {
        self.revision += 1;
        let inverse = command.apply(self);
        self.undo_stack.push(inverse.clone());
        self.redo_stack.clear();
        inverse
    }

    /// Undoes the most recent applied command, if any. Returns `false` with
    /// no effect if the undo stack is empty.
    pub fn undo(&mut self) -> bool {
        let Some(command) = self.undo_stack.pop() else { return false };
        self.revision += 1;
        let inverse = command.apply(self);
        self.redo_stack.push(inverse);
        true
    }

    /// Re-applies the most recently undone command, if any. Returns `false`
    /// with no effect if the redo stack is empty.
    pub fn redo(&mut self) -> bool {
        let Some(command) = self.redo_stack.pop() else { return false };
        self.revision += 1;
        let inverse = command.apply(self);
        self.undo_stack.push(inverse);
        true
    }

    /// Folds a sequence of already-applied commands' inverses into one
    /// combined undo-stack entry, as `Command::Batch`'s own `apply` would —
    /// for callers that had to apply each command one at a time via the raw
    /// `Command::apply(self, &mut document)` (bypassing this `apply`, which
    /// pushes a stack entry *per call*) because a later command's
    /// construction depended on an *earlier* one's freshly assigned id (e.g.
    /// a child node's `AddNode.node.parent` needing the parent's own
    /// just-inserted `NodeId`, which a plain `Command::Batch` can't express
    /// since its whole `Vec<Command>` must be built — and every id in it
    /// known — before any of it applies). `inverses` must already be in undo
    /// order (reverse of application order), matching `Command::Batch`'s own
    /// convention. `editor.rs`'s SVG-import group-hierarchy builder is the
    /// only caller so far.
    pub fn commit_batch(&mut self, inverses: Vec<Command>) {
        self.revision += 1;
        self.undo_stack.push(Command::Batch(inverses));
        self.redo_stack.clear();
    }

    /// Folds the last `count` undo entries into one (they undo together) —
    /// e.g. an Inspector edit plus the keyframes record mode made from it.
    pub fn merge_last(&mut self, count: usize) {
        let count = count.min(self.undo_stack.len());
        if count < 2 {
            return;
        }
        // Popped newest-first = undo order, `Command::Batch`'s convention.
        let merged: Vec<Command> = (0..count).filter_map(|_| self.undo_stack.pop()).collect();
        self.undo_stack.push(Command::Batch(merged));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{Node, NodeKind, ShapeNode};
    use crate::path::Path;
    use glam::Vec2;

    #[test]
    fn construct_document_with_one_shape() {
        let mut doc = Document::new();
        let artboard_id = doc.add_artboard(Artboard::new("Main", 512.0, 512.0));

        let artboard = doc.artboard_mut(artboard_id).unwrap();
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode {
                path: Path::rect(100.0, 100.0),
                fill_color: [1.0, 0.0, 0.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        assert_eq!(artboard.root_nodes(), &[node_id]);
        assert!(artboard.node(node_id).is_some());
    }

    fn document_with_one_node() -> (Document, ArtboardId, NodeId) {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        (document, artboard_id, node_id)
    }

    #[test]
    fn undo_reverses_the_last_apply() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        document.apply(Command::SetTranslation {
            artboard: artboard_id,
            node: node_id,
            value: Vec2::new(5.0, 5.0),
        });
        assert!(document.undo());

        assert_eq!(
            document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform.translation,
            Vec2::ZERO
        );
    }

    #[test]
    fn redo_reapplies_an_undone_command() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        document.apply(Command::SetTranslation {
            artboard: artboard_id,
            node: node_id,
            value: Vec2::new(5.0, 5.0),
        });
        document.undo();
        assert!(document.redo());

        assert_eq!(
            document.artboard(artboard_id).unwrap().node(node_id).unwrap().transform.translation,
            Vec2::new(5.0, 5.0)
        );
    }

    #[test]
    fn a_new_apply_clears_redo_history() {
        let (mut document, artboard_id, node_id) = document_with_one_node();

        document.apply(Command::SetTranslation {
            artboard: artboard_id,
            node: node_id,
            value: Vec2::new(5.0, 5.0),
        });
        document.undo();
        document.apply(Command::SetTranslation {
            artboard: artboard_id,
            node: node_id,
            value: Vec2::new(9.0, 9.0),
        });

        assert!(!document.redo());
    }

    #[test]
    fn undo_and_redo_on_empty_stacks_return_false() {
        let mut document = Document::new();
        assert!(!document.undo());
        assert!(!document.redo());
    }

    #[test]
    fn add_font_then_font_returns_it() {
        use crate::font_asset::FontAsset;

        let mut document = Document::new();
        let font_id = document.add_font(FontAsset { name: "Demo".into(), data: vec![1, 2, 3] });

        assert_eq!(document.font(font_id).unwrap().name, "Demo");
        assert_eq!(document.font(font_id).unwrap().data, vec![1, 2, 3]);
    }

    #[test]
    fn add_image_then_image_returns_it() {
        use crate::image_asset::ImageAsset;

        let mut document = Document::new();
        let image_id = document.add_image(ImageAsset { name: "Sprite".into(), source: vec![9, 9, 9], processing: Default::default(), file: None });

        assert_eq!(document.image(image_id).unwrap().name, "Sprite");
        assert_eq!(document.image(image_id).unwrap().source, vec![9, 9, 9]);
        assert_eq!(document.images().count(), 1);
    }

    #[test]
    fn add_sprite_group_then_sprite_group_returns_it() {
        use crate::image_asset::ImageAsset;
        use crate::sprite_group::SpriteGroup;

        let mut document = Document::new();
        let frame_a = document.add_image(ImageAsset { name: "Frame 0".into(), source: vec![], processing: Default::default(), file: None });
        let frame_b = document.add_image(ImageAsset { name: "Frame 1".into(), source: vec![], processing: Default::default(), file: None });
        let group_id = document.add_sprite_group(SpriteGroup { name: "Wave.gif".into(), frames: vec![(frame_a, 100), (frame_b, 150)] });

        let group = document.sprite_group(group_id).unwrap();
        assert_eq!(group.name, "Wave.gif");
        assert_eq!(group.frames, vec![(frame_a, 100), (frame_b, 150)]);
        assert_eq!(document.sprite_groups().count(), 1);
    }
}
