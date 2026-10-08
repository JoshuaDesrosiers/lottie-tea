//! `.tea`: the human-readable source format — pretty-printed JSON, diff-friendly
//! for version control (AN1MBYT3_BUILD_PLAN.md §2/§7). This is what an editor
//! saves while a designer is actively authoring; `.teab` (`teab.rs`) is the
//! compact production format a compiled asset actually ships as.
//!
//! Uses `Document`'s own `serde` impl directly (unlike the Tauri IPC
//! boundary's hand-built DTOs, `apps/editor/src-tauri/src/dto.rs`) — a save
//! file's whole point is round-tripping the real internal layout, which is
//! exactly the case `dto.rs`'s doc comment calls out as the right place for
//! a library's own `Serialize`/`Deserialize` impl, not a custom DTO.

use an1mbyt3_core::Document;

use crate::versioned::{check_schema_version, UnsupportedSchemaVersion, VersionedDocument};

/// Serializes `document` to pretty-printed `.tea` JSON, wrapped with the
/// current schema version (`versioned.rs`).
pub fn save_tea(document: &Document) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&VersionedDocument::wrap(document))
}

/// Why a `.tea` file failed to load: either it isn't valid JSON matching the
/// expected shape, or its `schema_version` isn't one this build understands.
#[derive(Debug)]
pub enum TeaLoadError {
    Json(serde_json::Error),
    UnsupportedSchemaVersion(UnsupportedSchemaVersion),
}

impl std::fmt::Display for TeaLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeaLoadError::Json(err) => write!(f, ".tea file is not valid: {err}"),
            TeaLoadError::UnsupportedSchemaVersion(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TeaLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TeaLoadError::Json(err) => Some(err),
            TeaLoadError::UnsupportedSchemaVersion(err) => Some(err),
        }
    }
}

/// Parses `json` (the contents of a `.tea` file) back into a [`Document`].
pub fn load_tea(json: &str) -> Result<Document, TeaLoadError> {
    let versioned: VersionedDocument = serde_json::from_str(json).map_err(TeaLoadError::Json)?;
    check_schema_version(versioned.schema_version).map_err(TeaLoadError::UnsupportedSchemaVersion)?;
    Ok(versioned.document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{Artboard, Node, NodeKind, Path, ShapeNode};

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

    #[test]
    fn round_trips_a_document_through_json() {
        let original = sample_document();
        let json = save_tea(&original).unwrap();
        let loaded = load_tea(&json).unwrap();

        assert_eq!(loaded.artboards().count(), original.artboards().count());
        let (_, artboard) = loaded.artboards().next().unwrap();
        assert_eq!(artboard.name, "Main");
        let (_, node) = artboard.shapes().next().unwrap();
        let NodeKind::Shape(shape) = &node.kind else { unreachable!() };
        assert_eq!(shape.fill_color, [0.85, 0.2, 0.35, 1.0]);
    }

    #[test]
    fn rejects_an_unrecognized_schema_version() {
        let malformed = crate::versioned::VersionedDocument { schema_version: 999, document: Document::new() };
        let json = serde_json::to_string(&malformed).unwrap();
        let err = load_tea(&json).unwrap_err();
        assert!(matches!(err, TeaLoadError::UnsupportedSchemaVersion(_)));
    }

    /// Reordered layers (`Artboard::child_order`, keyed by `NodeId`) must
    /// save as JSON and as `.teab`, and load back in the same order.
    #[test]
    fn reordered_children_round_trip_through_tea_and_teab() {
        use an1mbyt3_core::{Artboard, GroupNode, Node, NodeKind};
        let mut document = Document::new();
        let artboard_id = document.add_artboard(Artboard::new("Main", 100.0, 100.0));
        let artboard = document.artboard_mut(artboard_id).unwrap();
        let group = artboard.add_node(Node::new("Group", None, NodeKind::Group(GroupNode::default())));
        let a = artboard.add_node(Node::new("A", Some(group), NodeKind::Group(GroupNode::default())));
        let b = artboard.add_node(Node::new("B", Some(group), NodeKind::Group(GroupNode::default())));
        assert!(artboard.move_node_forward(a), "A moves above B");
        let order = |d: &Document| d.artboards().next().unwrap().1.children_of(group).map(|(id, _)| id).collect::<Vec<_>>();
        assert_eq!(order(&document), vec![b, a]);

        let json = save_tea(&document).expect("a document with reordered layers saves as .tea");
        assert_eq!(order(&load_tea(&json).unwrap()), vec![b, a]);
        let bytes = crate::save_teab(&document).unwrap();
        assert_eq!(order(&crate::load_teab(&bytes).unwrap()), vec![b, a]);
    }
}
