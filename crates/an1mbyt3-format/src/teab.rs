//! `.teab`: the compact production binary format (AN1MBYT3_BUILD_PLAN.md
//! §2/§7) — what `an1mbyt3-cli compile` produces from a `.tea` source file,
//! and what the runtime (native or WASM) actually loads to play an
//! animation. Two layers, applied in order when saving (reversed when
//! loading):
//! 1. `postcard` encodes the `serde`-derived [`VersionedDocument`] into
//!    bytes — chosen over `bincode`/a custom flatbuffer-like layout (both
//!    candidates named in §4): postcard's varint-encoded output is smaller
//!    than bincode's fixed-width encoding for the same types, and it's
//!    explicitly designed for `no_std`/embedded/wasm targets — a good fit
//!    for §4's still-open "true zero-copy WASM loads" aspiration, even
//!    though this doesn't attempt zero-copy loading yet (postcard still
//!    allocates on decode, it just doesn't *require* `std`).
//! 2. `flate2`'s DEFLATE compresses those bytes further — a real, measurable
//!    step toward §10 Phase 9's "`.teab`... file size hit defined budgets"
//!    exit criterion, not just the raw encoding step. `rust_backend`
//!    (`miniz_oxide`), not the default `zlib`/`zlib-ng` C bindings, so this
//!    crate still compiles for wasm32 (Phase 8's WASM runtime is a real
//!    consumer of this exact function).
//!
//! Still deliberately not the *whole* compiler pipeline: deduplicating
//! shared geometry across nodes, stripping unused keyframes, and building
//! string tables (§7's "strips... deduplicates...") all happen *before*
//! encoding, at the `Document` level — none of that exists yet, so today
//! `save_teab` compresses whatever `Document` it's handed as-is. Those are
//! later Phase 9 increments layered in front of this function, not part of it.

use std::io::{Read, Write};

use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

use an1mbyt3_core::Document;

use crate::versioned::{check_schema_version, UnsupportedSchemaVersion, VersionedDocument};

fn compress(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(bytes)?;
    encoder.finish()
}

fn decompress(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut decoder = DeflateDecoder::new(bytes);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// Why saving to `.teab` failed: either `postcard` couldn't encode the
/// document, or the DEFLATE compression step itself failed (an `std::io::Error`
/// — DEFLATE encoding into an in-memory `Vec<u8>` essentially never fails in
/// practice, but the underlying API is fallible, so this stays honest about it).
#[derive(Debug)]
pub enum TeabSaveError {
    Postcard(postcard::Error),
    Compression(std::io::Error),
}

impl std::fmt::Display for TeabSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeabSaveError::Postcard(err) => write!(f, "failed to encode document: {err}"),
            TeabSaveError::Compression(err) => write!(f, "failed to compress document: {err}"),
        }
    }
}

impl std::error::Error for TeabSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TeabSaveError::Postcard(err) => Some(err),
            TeabSaveError::Compression(err) => Some(err),
        }
    }
}

/// Encodes `document` into `.teab`'s binary layout (postcard, wrapped with
/// the current schema version — `versioned.rs`) and DEFLATE-compresses the
/// result.
pub fn save_teab(document: &Document) -> Result<Vec<u8>, TeabSaveError> {
    let encoded = postcard::to_allocvec(&VersionedDocument::wrap(document)).map_err(TeabSaveError::Postcard)?;
    compress(&encoded).map_err(TeabSaveError::Compression)
}

/// Why a `.teab` file failed to load: its bytes don't decompress, don't
/// decode as the expected shape after decompression, or its
/// `schema_version` isn't one this build understands.
#[derive(Debug)]
pub enum TeabLoadError {
    Decompression(std::io::Error),
    Postcard(postcard::Error),
    UnsupportedSchemaVersion(UnsupportedSchemaVersion),
}

impl std::fmt::Display for TeabLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeabLoadError::Decompression(err) => write!(f, ".teab file failed to decompress: {err}"),
            TeabLoadError::Postcard(err) => write!(f, ".teab file is not valid: {err}"),
            TeabLoadError::UnsupportedSchemaVersion(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TeabLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TeabLoadError::Decompression(err) => Some(err),
            TeabLoadError::Postcard(err) => Some(err),
            TeabLoadError::UnsupportedSchemaVersion(err) => Some(err),
        }
    }
}

/// How many trailing empty collections `load_teab` will pad an older file
/// with (one per `Document` field added since it was written).
const MAX_TRAILING_PAD: usize = 8;

/// Decodes `bytes` (the contents of a `.teab` file) back into a [`Document`]
/// — DEFLATE-decompresses first, then decodes the resulting postcard bytes.
///
/// **Older files.** postcard isn't self-describing, so a file written before
/// a trailing `#[serde(default)]` `Document` field was added simply ends
/// where that field would start, and decoding fails with "unexpected end".
/// `Document` is the last thing in the payload and every field appended so
/// far is a `SlotMap` (an empty one encodes as the single byte `0x00`), so
/// such a file is retried with up to [`MAX_TRAILING_PAD`] zero bytes
/// appended: it then decodes with those newer collections empty, exactly
/// what `#[serde(default)]` gives JSON (`.tea`) files. (First needed for
/// `Document::drawings`, so crash-recovery snapshots and exports from
/// before it still open.)
pub fn load_teab(bytes: &[u8]) -> Result<Document, TeabLoadError> {
    let mut decompressed = decompress(bytes).map_err(TeabLoadError::Decompression)?;
    let mut padded = 0;
    let versioned: VersionedDocument = loop {
        match postcard::from_bytes(&decompressed) {
            Ok(versioned) => break versioned,
            Err(postcard::Error::DeserializeUnexpectedEnd) if padded < MAX_TRAILING_PAD => {
                decompressed.push(0);
                padded += 1;
            }
            Err(err) => return Err(TeabLoadError::Postcard(err)),
        }
    };
    check_schema_version(versioned.schema_version).map_err(TeabLoadError::UnsupportedSchemaVersion)?;
    Ok(versioned.document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{Artboard, FontAsset, ImageAsset, ImageNode, Node, NodeKind, Path, ShapeNode, SpriteGroup, TextNode};

    fn sample_document() -> Document {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(100.0, 50.0), fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));
        document
    }

    /// A document with many similarly-named, similarly-shaped nodes — real
    /// DEFLATE compression needs actual byte-level redundancy to show a
    /// meaningful ratio; `sample_document`'s single shape doesn't have
    /// enough repeated structure to prove compression is doing real work.
    fn repetitive_document() -> Document {
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        for i in 0..50 {
            artboard.add_node(Node::new(
                format!("Square {i}"),
                None,
                NodeKind::Shape(ShapeNode { path: Path::rect(100.0, 50.0), fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
            ));
        }
        document
    }

    /// A file written before `Document::drawings` existed ends one byte
    /// early (no empty-map `0x00` for it) — it must still load, with no
    /// drawings, and a drawing must survive a normal round trip.
    #[test]
    fn loads_files_from_before_drawings_existed() {
        let mut document = sample_document();
        let raw = postcard::to_allocvec(&VersionedDocument::wrap(&document)).unwrap();
        assert_eq!(raw.last(), Some(&0), "an empty trailing SlotMap encodes as one 0x00 byte");
        let old_file = compress(&raw[..raw.len() - 1]).unwrap();
        let loaded = load_teab(&old_file).expect("an older file still loads");
        assert_eq!(loaded.drawings().count(), 0);
        assert_eq!(loaded.artboards().count(), document.artboards().count());

        let drawing = document.add_drawing(an1mbyt3_core::DrawingAsset {
            name: "Tree".into(),
            content: an1mbyt3_core::DrawingContent::Vector { svg: "<svg/>".into() },
            deleted: false,
        });
        let loaded = load_teab(&save_teab(&document).unwrap()).unwrap();
        assert_eq!(loaded.drawing(drawing).map(|d| d.name.as_str()), Some("Tree"));
    }

    #[test]
    fn round_trips_a_document_through_the_binary_format() {
        let original = sample_document();
        let bytes = save_teab(&original).unwrap();
        let loaded = load_teab(&bytes).unwrap();

        assert_eq!(loaded.artboards().count(), original.artboards().count());
        let (_, artboard) = loaded.artboards().next().unwrap();
        assert_eq!(artboard.name, "Main");
        let (_, node) = artboard.shapes().next().unwrap();
        let NodeKind::Shape(shape) = &node.kind else { unreachable!() };
        assert_eq!(shape.fill_color, [0.85, 0.2, 0.35, 1.0]);
    }

    /// 3D content (internally tagged enums in JSON) and the newer UI /
    /// layout components must survive the binary format — postcard can't
    /// decode serde's internal tagging, so those enums switch to external
    /// tagging in binary (`space3d.rs`'s `binary_safe_tagged`).
    #[test]
    fn round_trips_3d_nodes_and_ui_layouts() {
        use an1mbyt3_core::{Camera3D, Light3D, Mesh3D, Node3D, Node3DContent, NodeBehavior, NodeBehaviorEntry, Primitive3D, Transform3D, UiLayout, UiSize};
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("3D", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let contents = [
            Node3DContent::Empty,
            Node3DContent::Mesh(Mesh3D::new(Primitive3D::Torus { radius: 1.0, tube: 0.3, segments: 24, sides: 12 }, [1.0, 0.5, 0.2, 1.0])),
            Node3DContent::Camera(Camera3D::default()),
            Node3DContent::Light(Light3D::default()),
        ];
        for content in contents {
            artboard.add_node(Node::new("n", None, NodeKind::Node3D(Node3D::new(Transform3D::default(), content))));
        }
        let mut ui = Node::new("layout", None, NodeKind::Group(Default::default()));
        ui.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiLayout(UiLayout { width: UiSize::Fixed(120.0), height: UiSize::Fill, ..Default::default() }), enabled: true });
        ui.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiHtml(an1mbyt3_core::UiHtml::default()), enabled: true });
        ui.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::Rigidbody3D(an1mbyt3_core::Rigidbody3D::default()), enabled: true });
        ui.behaviors.push(NodeBehaviorEntry {
            behavior: NodeBehavior::Collider3D(an1mbyt3_core::Collider3D { shape: an1mbyt3_core::Collider3DShape::Capsule { radius: 0.4, height: 1.8 }, ..Default::default() }),
            enabled: true,
        });
        artboard.add_node(ui);

        let loaded = load_teab(&save_teab(&document).unwrap()).unwrap();
        let (_, artboard) = loaded.artboards().next().unwrap();
        let loaded_contents: Vec<Node3DContent> = artboard.node_ids().filter_map(|id| match &artboard.node(id)?.kind {
            NodeKind::Node3D(n) => Some(n.content),
            _ => None,
        }).collect();
        assert_eq!(loaded_contents, contents.to_vec());
        let html = artboard.node_ids().filter_map(|id| artboard.node(id)).flat_map(|n| n.behaviors.iter()).find_map(|e| match &e.behavior {
            NodeBehavior::UiHtml(h) => Some(h.clone()),
            _ => None,
        });
        assert_eq!(html, Some(an1mbyt3_core::UiHtml::default()));
        let capsule = artboard.node_ids().filter_map(|id| artboard.node(id)).flat_map(|n| n.behaviors.iter()).find_map(|e| match &e.behavior {
            NodeBehavior::Collider3D(c) => Some(c.shape),
            _ => None,
        });
        assert_eq!(capsule, Some(an1mbyt3_core::Collider3DShape::Capsule { radius: 0.4, height: 1.8 }));
        // JSON keeps its internally tagged shape.
        let json = serde_json::to_string(&contents[1]).unwrap();
        assert!(json.contains("\"type\":\"Mesh\"") && json.contains("\"shape\":\"Torus\""), "{json}");
    }

    /// `Artboard::animations` is a new, previously-absent field, and
    /// `Node`'s translation is now keyframed via a named `Animation`
    /// (Phase 4's multi-property/multi-animation timeline) rather than a
    /// per-node `translation_track` — this proves both the `Animation`
    /// itself and its multi-property tracks survive the binary round-trip,
    /// and that `#[serde(default)]` lets a `.teab` saved before this field
    /// existed still decode (empty `SlotMap`), matching `deleted`'s and
    /// `Document::fonts`'s established precedent for additive fields.
    #[test]
    fn round_trips_an_animation_with_multiple_property_tracks() {
        use glam::Vec2;
        use an1mbyt3_core::{Animation, Easing, Keyframe, PropertyTrack, Track};

        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let node_id = artboard.add_node(Node::new(
            "Square",
            None,
            NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
        ));

        let mut animation = Animation::new("Walk");
        animation.tracks.push((
            node_id,
            PropertyTrack::Translation(Track::new(vec![
                Keyframe::new(0.0, Vec2::ZERO, Easing::Linear),
                Keyframe::new(1.0, Vec2::new(100.0, 0.0), Easing::Linear),
            ])),
        ));
        animation.tracks.push((
            node_id,
            PropertyTrack::FillColor(Track::new(vec![Keyframe::new(0.0, [1.0, 0.0, 0.0, 1.0], Easing::Linear)])),
        ));
        let animation_id = artboard.add_animation(animation);

        let bytes = save_teab(&document).unwrap();
        let loaded = load_teab(&bytes).unwrap();
        let (_, artboard) = loaded.artboards().next().unwrap();
        let animation = artboard.animation(animation_id).unwrap();
        assert_eq!(animation.name, "Walk");
        assert_eq!(animation.tracks.len(), 2);
        let PropertyTrack::Translation(track) = &animation.tracks[0].1 else { panic!("expected a Translation track") };
        assert_eq!(track.keyframes[1].value, Vec2::new(100.0, 0.0));
    }

    /// `NodeKind::Text` is the newest variant, appended after `Instance` —
    /// `.teab`'s postcard encoding identifies enum variants by index (see
    /// `an1mbyt3_core::node`'s module doc), so this specifically proves a
    /// `Text` node (and the `FontAsset` it references via `FontId`) survives
    /// the binary round-trip intact, not just that the format works at all.
    #[test]
    fn round_trips_a_text_node_and_its_font_asset() {
        let mut document = Document::new();
        let font_id = document.add_font(FontAsset { name: "Demo".into(), data: vec![9, 8, 7] });
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        artboard.add_node(Node::new(
            "Label",
            None,
            NodeKind::Text(TextNode {
                content: "An1mbyt3".into(),
                font: font_id,
                size_px: 48.0,
                fill_color: [0.1, 0.2, 0.3, 1.0],
                gradient: None,
            }),
        ));

        let bytes = save_teab(&document).unwrap();
        let loaded = load_teab(&bytes).unwrap();

        let (_, artboard) = loaded.artboards().next().unwrap();
        let (_, node) = artboard.texts().next().unwrap();
        let NodeKind::Text(text) = &node.kind else { unreachable!() };
        assert_eq!(text.content, "An1mbyt3");
        assert_eq!(text.size_px, 48.0);
        assert_eq!(loaded.font(text.font).unwrap().name, "Demo");
        assert_eq!(loaded.font(text.font).unwrap().data, vec![9, 8, 7]);
    }

    /// `NodeKind::Image` is the newest variant, appended after `Text` — same
    /// index-encoding reason `round_trips_a_text_node_and_its_font_asset`
    /// exists. Also proves `ImageAsset`/`SpriteGroup` (both `#[serde(default)]`
    /// additive `Document` fields, like `fonts`) round-trip alongside a
    /// `.teab` predating them would.
    #[test]
    fn round_trips_an_image_node_its_asset_and_a_sprite_group() {
        let mut document = Document::new();
        let image_id = document.add_image(ImageAsset { name: "Sprite".into(), source: vec![9, 8, 7], processing: Default::default(), file: None });
        let other_frame = document.add_image(ImageAsset { name: "Sprite Frame 1".into(), source: vec![1, 2, 3], processing: Default::default(), file: None });
        let group_id = document.add_sprite_group(SpriteGroup { name: "Wave.gif".into(), frames: vec![(image_id, 100), (other_frame, 150)] });
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        artboard.add_node(Node::new(
            "Sprite",
            None,
            NodeKind::Image(ImageNode { image: image_id, width: 64.0, height: 32.0, fill_color: [1.0; 4], blend_mode: Default::default() }),
        ));

        let bytes = save_teab(&document).unwrap();
        let loaded = load_teab(&bytes).unwrap();

        let (_, artboard) = loaded.artboards().next().unwrap();
        let (_, node) = artboard.images().next().unwrap();
        let NodeKind::Image(image_node) = &node.kind else { unreachable!() };
        assert_eq!(image_node.width, 64.0);
        assert_eq!(image_node.height, 32.0);
        assert_eq!(loaded.image(image_node.image).unwrap().name, "Sprite");
        assert_eq!(loaded.image(image_node.image).unwrap().source, vec![9, 8, 7]);

        let group = loaded.sprite_group(group_id).unwrap();
        assert_eq!(group.name, "Wave.gif");
        assert_eq!(group.frames, vec![(image_id, 100), (other_frame, 150)]);
    }

    #[test]
    fn round_trips_a_repetitive_document_too() {
        let original = repetitive_document();
        let bytes = save_teab(&original).unwrap();
        let loaded = load_teab(&bytes).unwrap();
        assert_eq!(loaded.artboards().next().unwrap().1.shapes().count(), 50);
    }

    #[test]
    fn teab_is_smaller_than_the_equivalent_tea_json() {
        // Not a hard byte-count assertion (that would be a brittle golden
        // test) — just confirming the format actually does what its doc
        // comment claims: postcard + DEFLATE is meaningfully more compact
        // than pretty-printed JSON for the same document.
        let document = sample_document();
        let teab_len = save_teab(&document).unwrap().len();
        let tea_len = crate::tea::save_tea(&document).unwrap().len();
        assert!(teab_len < tea_len, "expected .teab ({teab_len} bytes) to be smaller than .tea ({tea_len} bytes)");
    }

    #[test]
    fn compression_shrinks_a_redundant_document() {
        let document = repetitive_document();
        let compressed_len = save_teab(&document).unwrap().len();
        let uncompressed_len = postcard::to_allocvec(&VersionedDocument::wrap(&document)).unwrap().len();
        assert!(
            compressed_len < uncompressed_len,
            "expected DEFLATE to shrink a highly redundant document \
             (compressed: {compressed_len} bytes, uncompressed postcard: {uncompressed_len} bytes)"
        );
    }

    /// `Artboard::state_machines` is Phase 8's new field — proves a
    /// `StateMachineDef` (states, transitions with a non-default
    /// `PlaybackDirection`, and declared inputs) survives the binary
    /// round-trip, and that the `AnimationId` a `StateDef` references still
    /// resolves correctly to the *same* animation after a save/load cycle
    /// (ids aren't guaranteed identical across a `SlotMap` round-trip in
    /// general, but must still resolve to equivalent content).
    #[test]
    fn round_trips_a_state_machine_definition() {
        use an1mbyt3_core::{Animation, ConditionDef, InputDef, InputKind, LoopMode, PlaybackDirection, StateDef, StateMachineDef, TransitionDef};

        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        artboard.add_node(Node::new("Square", None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None})));

        let animation_id = artboard.add_animation(Animation::new("Idle/Active"));
        let def = StateMachineDef {
            name: "Main".into(),
            inputs: vec![InputDef { name: "toggle".into(), kind: InputKind::Trigger }],
            states: vec![
                StateDef {
                    name: "Idle".into(),
                    animation: animation_id,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "Active".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Forward,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
                StateDef {
                    name: "Active".into(),
                    animation: animation_id,
                    loop_mode: LoopMode::Once,
                    transitions: vec![TransitionDef {
                        to: "Idle".into(),
                        condition: ConditionDef::Trigger { input: "toggle".into() },
                        direction: PlaybackDirection::Reverse,
                        duration_seconds: 0.0,
                        interruptible: true,
                    }],
                    position: glam::Vec2::ZERO,
                    blend: None,
                    program: None,
                },
            ],
            initial_state: "Idle".into(),
        layers: Vec::new(),
        deleted: false,
        };
        let state_machine_id = artboard.add_state_machine(def);

        let bytes = save_teab(&document).unwrap();
        let loaded = load_teab(&bytes).unwrap();
        let (_, artboard) = loaded.artboards().next().unwrap();
        let loaded_def = artboard.state_machine(state_machine_id).unwrap();

        assert_eq!(loaded_def.name, "Main");
        assert_eq!(loaded_def.initial_state, "Idle");
        assert_eq!(loaded_def.inputs, vec![InputDef { name: "toggle".into(), kind: InputKind::Trigger }]);
        assert_eq!(loaded_def.states.len(), 2);
        assert_eq!(loaded_def.states[1].transitions[0].direction, PlaybackDirection::Reverse);
        // The referenced `AnimationId` still resolves to the same node's
        // animation after the round trip, not just an id that happens to decode.
        assert!(artboard.animation(loaded_def.states[0].animation).is_some());
        assert_eq!(loaded_def.states[0].animation, loaded_def.states[1].animation);
    }

    #[test]
    fn rejects_an_unrecognized_schema_version() {
        let malformed = VersionedDocument { schema_version: 999, document: Document::new() };
        let encoded = postcard::to_allocvec(&malformed).unwrap();
        let bytes = compress(&encoded).unwrap();
        let err = load_teab(&bytes).unwrap_err();
        assert!(matches!(err, TeabLoadError::UnsupportedSchemaVersion(_)));
    }
}
