//! The **project folder** format — Unity/Rive-style: a project is a folder
//! of ordinary files on disk rather than one monolithic document blob.
//!
//! ```text
//! MyGame/
//!   MyGame.an1proj                 manifest: settings + everything small
//!   Assets/
//!     Scenes/<scene>.scene         one JSON file per scene (artboard)
//!     Prefabs/<prefab>.prefab      one JSON file per prefab
//!     Images/<image>.png|jpg|gif   the original image files, byte for byte
//! ```
//!
//! Why split it up:
//!
//! - **Images are real files.** In a `.tea` every image's bytes are a JSON
//!   number array (several times the file's size, unreadable); here they're
//!   the actual PNGs, viewable and editable in any tool, and a re-save only
//!   rewrites files that changed (the editor skips unchanged images).
//! - **Diff/merge-friendly.** Editing one scene touches one scene file.
//! - **Crash-safe saves.** Each file is written independently; the
//!   manifest is written last, so an interrupted save leaves the previous
//!   manifest pointing at a complete, consistent set.
//!
//! ## How it maps onto `Document`
//!
//! The document is serialized exactly as a `.tea` would be
//! (`VersionedDocument`, same schema version and migrations), then the big
//! pieces are lifted out of the JSON tree:
//!
//! - each occupied artboard / prefab slot's `value` is replaced by
//!   `{"$file": "Assets/Scenes/…"}` and written to that file,
//! - each image's `source` byte array is emptied and a sibling
//!   `"$source": "Assets/Images/…"` path is recorded.
//!
//! Loading reverses it: `$file`s are read and spliced back, the document is
//! deserialized, and image bytes are filled in from their files. Working at
//! the JSON level keeps slotmap ids (and therefore every cross reference)
//! exactly as they were — nothing is re-keyed.

use std::collections::HashSet;

use an1mbyt3_core::Document;
use serde_json::{json, Map, Value};

use crate::versioned::{check_schema_version, UnsupportedSchemaVersion, VersionedDocument};

/// Identifies the manifest format (`"format"` key) — so the editor can tell
/// a project manifest from some unrelated JSON file.
pub const PROJECT_FORMAT: &str = "an1mbyt3-project";
/// Manifest layout version (independent of the document schema version).
pub const PROJECT_FORMAT_VERSION: u32 = 1;
/// The manifest's file extension.
pub const PROJECT_EXTENSION: &str = "an1proj";

/// One file of a saved project, path relative to the project folder
/// (always `/`-separated).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectFile {
    pub path: String,
    pub contents: ProjectContents,
    /// Which document item this file holds (`None` for the manifest) — the
    /// Resources panel maps files on disk back to scenes / prefabs / images
    /// with it.
    pub asset: Option<ProjectAsset>,
    /// The file is an image's own source file (`ImageAsset::file`): the copy
    /// on disk is authoritative, so a save only writes it if it's missing.
    pub linked: bool,
}

/// The document item a project file holds: kind plus its slotmap key
/// (`idx`/`version`, the same shape ids have on the JS side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectAsset {
    pub kind: ProjectAssetKind,
    pub idx: u32,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectAssetKind {
    Scene,
    Prefab,
    Image,
}

impl ProjectAssetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Prefab => "prefab",
            Self::Image => "image",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectContents {
    /// UTF-8 text (manifest, scenes, prefabs).
    Text(String),
    /// Raw bytes (images).
    Bytes(Vec<u8>),
}

/// Why a project failed to load.
#[derive(Debug)]
pub enum ProjectLoadError {
    Json(serde_json::Error),
    NotAProject,
    MissingFile(String),
    UnsupportedSchemaVersion(UnsupportedSchemaVersion),
}

impl std::fmt::Display for ProjectLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(err) => write!(f, "invalid project JSON: {err}"),
            Self::NotAProject => write!(f, "not an an1mbyt3 project manifest"),
            Self::MissingFile(path) => write!(f, "project file missing: {path}"),
            Self::UnsupportedSchemaVersion(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ProjectLoadError {}

/// A file-name-safe version of `name`: keeps letters, digits, space, `-`,
/// `_`, `.`; everything else becomes `_`. Never empty.
fn safe_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') { c } else { '_' }).collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed
    }
}

/// `dir/stem.ext`, made unique against `taken` (case-insensitive — Windows
/// and macOS file systems are) by appending ` 2`, ` 3`, …
fn unique_path(dir: &str, stem: &str, ext: &str, taken: &mut HashSet<String>) -> String {
    let mut n = 1;
    loop {
        let candidate = if n == 1 { format!("{dir}/{stem}{ext}") } else { format!("{dir}/{stem} {n}{ext}") };
        if taken.insert(candidate.to_lowercase()) {
            return candidate;
        }
        n += 1;
    }
}

/// The file extension matching an image's actual bytes.
fn image_extension(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => ".png",
        [0xFF, 0xD8, ..] => ".jpg",
        [b'G', b'I', b'F', ..] => ".gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => ".webp",
        _ => ".bin",
    }
}

/// Strips a known image extension off an asset name (`knight.png` ->
/// `knight`) so the saved file doesn't become `knight.png.png`.
fn image_stem(name: &str) -> &str {
    let lower = name.to_lowercase();
    for ext in [".png", ".jpg", ".jpeg", ".gif", ".webp"] {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// The occupied slots of a serialized slotmap (`[{"value": …, "version": n}, …]`).
fn occupied_slots(slotmap: &mut Value) -> impl Iterator<Item = &mut Map<String, Value>> {
    indexed_slots(slotmap).map(|(_, slot)| slot)
}

/// `occupied_slots` with each slot's array index — which *is* the key's
/// `idx` (slotmap serializes its whole slot vector, sentinel included).
fn indexed_slots(slotmap: &mut Value) -> impl Iterator<Item = (u32, &mut Map<String, Value>)> {
    slotmap
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, slot)| slot.as_object_mut().map(|s| (i as u32, s)))
        .filter(|(_, slot)| slot.get("value").is_some_and(|v| !v.is_null()))
}

fn slot_version(slot: &Map<String, Value>) -> u32 {
    slot.get("version").and_then(Value::as_u64).unwrap_or(0) as u32
}

/// Splits `document` into project files. `project_name` names the manifest
/// (`<project_name>.an1proj`). Paths are deterministic for a given document
/// (same names -> same files), so re-saving overwrites in place.
pub fn save_project(document: &Document, project_name: &str) -> serde_json::Result<Vec<ProjectFile>> {
    // Serialize without the image bytes (they're written as files below).
    let mut stripped = document.clone();
    let image_ids: Vec<_> = stripped.images().map(|(id, _)| id).collect();
    let mut image_bytes = Vec::new();
    for id in image_ids {
        let asset = stripped.image_mut(id).expect("listed above");
        image_bytes.push((asset.name.clone(), asset.file.clone(), std::mem::take(&mut asset.source)));
    }
    let mut root = serde_json::to_value(VersionedDocument::wrap(&stripped))?;
    let doc = root.get_mut("document").expect("VersionedDocument has a document");

    let mut files = Vec::new();
    let mut taken = HashSet::new();
    let mut asset_paths = Vec::new();
    // Linked image files keep their exact paths — reserve them before any
    // generated name could collide with one.
    for (_, file, _) in &image_bytes {
        if let Some(file) = file {
            taken.insert(file.to_lowercase());
        }
    }

    for (key, dir, ext, kind) in [("artboards", "Assets/Scenes", ".scene", ProjectAssetKind::Scene), ("prefabs", "Assets/Prefabs", ".prefab", ProjectAssetKind::Prefab)] {
        let Some(slots) = doc.get_mut(key) else { continue };
        for (idx, slot) in indexed_slots(slots) {
            let asset = Some(ProjectAsset { kind, idx, version: slot_version(slot) });
            let value = slot.get("value").expect("occupied").clone();
            let deleted = value.get("deleted").and_then(Value::as_bool).unwrap_or(false);
            let name = value.get("name").and_then(Value::as_str).unwrap_or("Untitled");
            // Deleted scenes/prefabs stay inline (they're undo history the
            // user can't see — not worth a visible file).
            if deleted {
                continue;
            }
            let path = unique_path(dir, &safe_name(name), ext, &mut taken);
            files.push(ProjectFile { path: path.clone(), contents: ProjectContents::Text(serde_json::to_string_pretty(&value)?), asset, linked: false });
            asset_paths.push(path.clone());
            slot.insert("value".into(), json!({ "$file": path }));
        }
    }

    if let Some(slots) = doc.get_mut("images") {
        for ((idx, slot), (name, file, bytes)) in indexed_slots(slots).zip(image_bytes) {
            let asset = Some(ProjectAsset { kind: ProjectAssetKind::Image, idx, version: slot_version(slot) });
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied image slot");
            let linked = file.is_some();
            let path = file.unwrap_or_else(|| unique_path("Assets/Images", &safe_name(image_stem(&name)), image_extension(&bytes), &mut taken));
            value.insert("$source".into(), Value::String(path.clone()));
            asset_paths.push(path.clone());
            files.push(ProjectFile { path, contents: ProjectContents::Bytes(bytes), asset, linked });
        }
    }

    let schema_version = root.get("schema_version").cloned().unwrap_or(Value::Null);
    let manifest = json!({
        "format": PROJECT_FORMAT,
        "formatVersion": PROJECT_FORMAT_VERSION,
        "name": project_name,
        "schema_version": schema_version,
        // Every asset file this manifest references — lets the editor tell
        // which files on disk a previous save wrote (stale ones from a
        // renamed scene can be cleaned up) and which are new drop-ins.
        "files": asset_paths,
        "document": root.get("document").cloned().unwrap_or(Value::Null),
    });
    // Manifest last: see the module doc ("crash-safe saves").
    files.push(ProjectFile {
        path: format!("{}.{PROJECT_EXTENSION}", safe_name(project_name)),
        contents: ProjectContents::Text(serde_json::to_string_pretty(&manifest)?),
        asset: None,
        linked: false,
    });
    Ok(files)
}

/// The asset paths a manifest references (its `"files"` list), without
/// loading anything — what the editor needs to read before `load_project`.
pub fn project_asset_paths(manifest: &str) -> Result<Vec<String>, ProjectLoadError> {
    let value: Value = serde_json::from_str(manifest).map_err(ProjectLoadError::Json)?;
    if value.get("format").and_then(Value::as_str) != Some(PROJECT_FORMAT) {
        return Err(ProjectLoadError::NotAProject);
    }
    Ok(value.get("files").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default())
}

/// Rebuilds a `Document` from a manifest plus its files. `read(path)`
/// returns a referenced file's bytes (`None` = missing, which fails the
/// load — a project with a missing scene must not silently open without it
/// and then overwrite it on the next save).
pub fn load_project(manifest: &str, mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Result<Document, ProjectLoadError> {
    let mut value: Value = serde_json::from_str(manifest).map_err(ProjectLoadError::Json)?;
    if value.get("format").and_then(Value::as_str) != Some(PROJECT_FORMAT) {
        return Err(ProjectLoadError::NotAProject);
    }
    let schema_version = value.get("schema_version").cloned().unwrap_or(Value::Null);
    let mut doc = value.get_mut("document").map(Value::take).ok_or(ProjectLoadError::NotAProject)?;

    for key in ["artboards", "prefabs"] {
        let Some(slots) = doc.get_mut(key) else { continue };
        for slot in occupied_slots(slots) {
            let Some(path) = slot.get("value").and_then(|v| v.get("$file")).and_then(Value::as_str).map(String::from) else { continue };
            let bytes = read(&path).ok_or_else(|| ProjectLoadError::MissingFile(path.clone()))?;
            let inner: Value = serde_json::from_slice(&bytes).map_err(ProjectLoadError::Json)?;
            slot.insert("value".into(), inner);
        }
    }

    let mut image_files = Vec::new();
    if let Some(slots) = doc.get_mut("images") {
        for slot in occupied_slots(slots) {
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied image slot");
            image_files.push(value.remove("$source").and_then(|v| v.as_str().map(String::from)));
            value.insert("source".into(), Value::Array(Vec::new()));
        }
    }

    let versioned: VersionedDocument = serde_json::from_value(json!({ "schema_version": schema_version, "document": doc })).map_err(ProjectLoadError::Json)?;
    check_schema_version(versioned.schema_version).map_err(ProjectLoadError::UnsupportedSchemaVersion)?;
    let mut document = versioned.document;

    // Slot order is the same on both sides, so the n-th occupied image slot
    // in the JSON is the n-th image `images()` yields.
    let ids: Vec<_> = document.images().map(|(id, _)| id).collect();
    for (id, path) in ids.into_iter().zip(image_files) {
        let Some(path) = path else { continue };
        let bytes = read(&path).ok_or_else(|| ProjectLoadError::MissingFile(path.clone()))?;
        document.image_mut(id).expect("listed above").source = bytes;
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{Artboard, GroupNode, ImageAsset, Node, NodeKind};
    use std::collections::HashMap;

    #[test]
    fn project_round_trips_scenes_and_images() {
        let mut document = Document::new();
        let a = document.add_artboard(Artboard::new("Level 1", 100.0, 100.0));
        document.add_artboard(Artboard::new("Level/2", 100.0, 100.0));
        document.artboard_mut(a).unwrap().add_node(Node::new("Box", None, NodeKind::Group(GroupNode::default())));
        let png = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        document.add_image(ImageAsset { name: "knight.png".into(), source: png.clone(), processing: Default::default(), file: None });

        let files = save_project(&document, "My Game").unwrap();
        // File -> item mapping uses the real slotmap keys.
        let scene_file = files.iter().find(|f| f.path == "Assets/Scenes/Level 1.scene").unwrap();
        let key = slotmap::Key::data(&a).as_ffi();
        assert_eq!(scene_file.asset, Some(ProjectAsset { kind: ProjectAssetKind::Scene, idx: key as u32, version: (key >> 32) as u32 }));
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Assets/Scenes/Level 1.scene"));
        assert!(paths.contains(&"Assets/Scenes/Level_2.scene"));
        assert!(paths.contains(&"Assets/Images/knight.png"));
        assert_eq!(paths.last(), Some(&"My Game.an1proj"));

        let store: HashMap<String, Vec<u8>> = files
            .iter()
            .map(|f| {
                let bytes = match &f.contents {
                    ProjectContents::Text(t) => t.clone().into_bytes(),
                    ProjectContents::Bytes(b) => b.clone(),
                };
                (f.path.clone(), bytes)
            })
            .collect();
        let manifest = String::from_utf8(store["My Game.an1proj"].clone()).unwrap();
        assert_eq!(project_asset_paths(&manifest).unwrap().len(), 3);
        let loaded = load_project(&manifest, |p| store.get(p).cloned()).unwrap();
        assert_eq!(loaded.artboards().count(), 2);
        assert_eq!(loaded.artboard(a).unwrap().node_ids().count(), 1);
        assert_eq!(loaded.images().next().unwrap().1.source, png);
    }

    #[test]
    fn linked_image_keeps_its_file() {
        let mut document = Document::new();
        document.add_image(ImageAsset { name: "hero".into(), source: vec![0x89, b'P', b'N', b'G'], processing: Default::default(), file: Some("Assets/Art/hero.png".into()) });
        document.add_image(ImageAsset { name: "hero".into(), source: vec![0x89, b'P', b'N', b'G'], processing: Default::default(), file: None });
        let files = save_project(&document, "P").unwrap();
        let images: Vec<(&str, bool)> = files.iter().filter(|f| matches!(f.asset, Some(ProjectAsset { kind: ProjectAssetKind::Image, .. }))).map(|f| (f.path.as_str(), f.linked)).collect();
        assert_eq!(images, [("Assets/Art/hero.png", true), ("Assets/Images/hero.png", false)]);
    }

    #[test]
    fn missing_scene_file_fails_loudly() {
        let mut document = Document::new();
        document.add_artboard(Artboard::new("Main", 100.0, 100.0));
        let files = save_project(&document, "P").unwrap();
        let ProjectContents::Text(manifest) = &files.last().unwrap().contents else { panic!() };
        assert!(matches!(load_project(manifest, |_| None), Err(ProjectLoadError::MissingFile(_))));
    }
}
