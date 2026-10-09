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
    Audio,
    Model,
    /// A code / markup file (`an1mbyt3_core::TextFile`), stored at
    /// `Assets/Code/<its path>`.
    TextFile,
}

impl ProjectAssetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Prefab => "prefab",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Model => "model",
            Self::TextFile => "text",
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
/// A model's referenced file (`Textures/wood.png`, `../shared/a.bin`) as a
/// safe path inside the model's folder: each component cleaned; `..`, `.`
/// and drive letters dropped.
fn model_file_path(path: &str) -> String {
    let parts: Vec<String> = path.split(['/', '\u{5c}']).filter(|p| !p.is_empty() && *p != "." && *p != ".." && !p.ends_with(':')).map(safe_name).collect();
    if parts.is_empty() {
        "file".into()
    } else {
        parts.join("/")
    }
}

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
    // Sounds the same way (deleted ones stay inline — their bytes are
    // already cleared).
    let audio_ids: Vec<_> = stripped.audio_slots().map(|(id, a)| (id, a.deleted)).collect();
    let mut audio_bytes = Vec::new();
    for (id, deleted) in audio_ids {
        let asset = stripped.audio_mut(id).expect("listed above");
        audio_bytes.push((deleted, asset.name.clone(), asset.file.clone(), if deleted { Vec::new() } else { std::mem::take(&mut asset.source) }));
    }
    // Models too: the model file plus its textures / buffers, each a real
    // file in the model's own folder (deleted ones stay inline).
    let model_ids: Vec<_> = stripped.model_slots().map(|(id, m)| (id, m.deleted)).collect();
    let mut model_bytes = Vec::new();
    for (id, deleted) in model_ids {
        let asset = stripped.model_mut(id).expect("listed above");
        if deleted {
            model_bytes.push(None);
        } else {
            model_bytes.push(Some((asset.name.clone(), std::mem::take(&mut asset.source), std::mem::take(&mut asset.files))));
        }
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
    for (_, _, file, _) in &audio_bytes {
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

    if let Some(slots) = doc.get_mut("audio") {
        for ((idx, slot), (deleted, name, file, bytes)) in indexed_slots(slots).zip(audio_bytes) {
            if deleted {
                continue;
            }
            let asset = Some(ProjectAsset { kind: ProjectAssetKind::Audio, idx, version: slot_version(slot) });
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied audio slot");
            let linked = file.is_some();
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem);
            let path = file.unwrap_or_else(|| unique_path("Assets/Sounds", &safe_name(stem), an1mbyt3_core::audio_extension(&bytes), &mut taken));
            value.insert("$source".into(), Value::String(path.clone()));
            asset_paths.push(path.clone());
            files.push(ProjectFile { path, contents: ProjectContents::Bytes(bytes), asset, linked });
        }
    }

    if let Some(slots) = doc.get_mut("models") {
        for ((idx, slot), entry) in indexed_slots(slots).zip(model_bytes) {
            let Some((name, source, extra)) = entry else { continue };
            let asset = Some(ProjectAsset { kind: ProjectAssetKind::Model, idx, version: slot_version(slot) });
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied model slot");
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem);
            // `Assets/Models/<name>/<name>.glb`, with its textures beside it
            // at the relative paths the model refers to them by.
            let folder = unique_path("Assets/Models", &safe_name(stem), "", &mut taken);
            let ext = if source.starts_with(b"glTF") { ".glb" } else { ".gltf" };
            let path = format!("{folder}/{}{ext}", safe_name(stem));
            value.insert("$source".into(), Value::String(path.clone()));
            asset_paths.push(path.clone());
            files.push(ProjectFile { path, contents: ProjectContents::Bytes(source), asset, linked: false });
            let mut refs = Vec::new();
            for file in extra {
                let on_disk = format!("{folder}/{}", model_file_path(&file.path));
                taken.insert(on_disk.to_lowercase());
                refs.push(json!({ "path": file.path, "file": on_disk }));
                asset_paths.push(on_disk.clone());
                files.push(ProjectFile { path: on_disk, contents: ProjectContents::Bytes(file.data), asset: None, linked: false });
            }
            value.insert("$files".into(), Value::Array(refs));
        }
    }

    // Code files: each a real text file at `Assets/Code/<path>` (so outside
    // editors can work on them); the manifest keeps only the path.
    if let Some(slots) = doc.get_mut("text_files") {
        for (idx, slot) in indexed_slots(slots) {
            let asset = Some(ProjectAsset { kind: ProjectAssetKind::TextFile, idx, version: slot_version(slot) });
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied text file slot");
            if value.get("deleted").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let rel = an1mbyt3_core::text_file::normalize_path(value.get("path").and_then(Value::as_str).unwrap_or("untitled.txt"));
            let path = format!("Assets/Code/{rel}");
            taken.insert(path.to_lowercase());
            let text = value.remove("text").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            value.insert("$file".into(), Value::String(path.clone()));
            asset_paths.push(path.clone());
            files.push(ProjectFile { path, contents: ProjectContents::Text(text), asset, linked: false });
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

    let mut audio_files = Vec::new();
    if let Some(slots) = doc.get_mut("audio") {
        for slot in occupied_slots(slots) {
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied audio slot");
            let path = value.remove("$source").and_then(|v| v.as_str().map(String::from));
            if path.is_some() {
                value.insert("source".into(), Value::Array(Vec::new()));
            }
            audio_files.push(path);
        }
    }

    // Models: `$source` + `$files` back into bytes (in slot order).
    let mut model_files = Vec::new();
    if let Some(slots) = doc.get_mut("models") {
        for slot in occupied_slots(slots) {
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied model slot");
            let source = value.remove("$source").and_then(|v| v.as_str().map(String::from));
            let refs: Vec<(String, String)> = value
                .remove("$files")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|r| Some((r.get("path")?.as_str()?.to_string(), r.get("file")?.as_str()?.to_string())))
                .collect();
            if source.is_some() {
                value.insert("source".into(), Value::Array(Vec::new()));
                value.insert("files".into(), Value::Array(Vec::new()));
            }
            model_files.push(source.map(|s| (s, refs)));
        }
    }

    // Code files: read each back from disk. One that went missing (deleted
    // outside the editor) loads empty rather than refusing the project.
    if let Some(slots) = doc.get_mut("text_files") {
        for slot in occupied_slots(slots) {
            let value = slot.get_mut("value").and_then(Value::as_object_mut).expect("occupied text file slot");
            if let Some(path) = value.remove("$file").and_then(|v| v.as_str().map(String::from)) {
                let text = read(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                value.insert("text".into(), Value::String(text));
            }
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
    let ids: Vec<_> = document.audio_slots().map(|(id, _)| id).collect();
    for (id, path) in ids.into_iter().zip(audio_files) {
        let Some(path) = path else { continue };
        let bytes = read(&path).ok_or_else(|| ProjectLoadError::MissingFile(path.clone()))?;
        document.audio_mut(id).expect("listed above").source = bytes;
    }
    let ids: Vec<_> = document.model_slots().map(|(id, _)| id).collect();
    for (id, entry) in ids.into_iter().zip(model_files) {
        let Some((source, refs)) = entry else { continue };
        let bytes = read(&source).ok_or_else(|| ProjectLoadError::MissingFile(source.clone()))?;
        let mut files = Vec::with_capacity(refs.len());
        for (path, file) in refs {
            let data = read(&file).ok_or_else(|| ProjectLoadError::MissingFile(file.clone()))?;
            files.push(an1mbyt3_core::ModelFile { path, data });
        }
        let model = document.model_mut(id).expect("listed above");
        model.source = bytes;
        model.files = files;
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
    fn sounds_are_saved_as_files_and_restored() {
        let mut document = Document::new();
        document.add_artboard(Artboard::new("Level 1", 100.0, 100.0));
        let wav = b"RIFF\0\0\0\0WAVEfmt data".to_vec();
        document.add_audio(an1mbyt3_core::AudioAsset { name: "jump.wav".into(), source: wav.clone(), file: None, deleted: false });
        let gone = document.add_audio(an1mbyt3_core::AudioAsset { name: "old.wav".into(), source: Vec::new(), file: None, deleted: true });
        let files = save_project(&document, "Game").unwrap();
        assert!(files.iter().any(|f| f.path == "Assets/Sounds/jump.wav" && f.contents == ProjectContents::Bytes(wav.clone())));
        let store: HashMap<String, Vec<u8>> = files
            .iter()
            .map(|f| (f.path.clone(), match &f.contents { ProjectContents::Text(t) => t.clone().into_bytes(), ProjectContents::Bytes(b) => b.clone() }))
            .collect();
        let manifest = String::from_utf8(store["Game.an1proj"].clone()).unwrap();
        assert!(!manifest.contains("82,73,70,70"), "sound bytes aren't inlined in the manifest");
        let loaded = load_project(&manifest, |p| store.get(p).cloned()).unwrap();
        let (_, jump) = loaded.audio_assets().next().unwrap();
        assert_eq!(jump.source, wav);
        assert_eq!(loaded.audio_assets().count(), 1);
        assert!(loaded.audio(gone).is_none(), "the deleted sound stays deleted");
    }

    #[test]
    fn code_files_are_real_files_under_assets_code() {
        let mut document = Document::new();
        document.add_text_file(an1mbyt3_core::TextFile::new("UI/menu.html", "<h1>Hi</h1>"));
        let files = save_project(&document, "Game").unwrap();
        let file = files.iter().find(|f| f.path == "Assets/Code/UI/menu.html").expect("written as a file");
        assert_eq!(file.contents, ProjectContents::Text("<h1>Hi</h1>".into()));
        let manifest = files.iter().find(|f| f.path.ends_with(".an1proj")).unwrap();
        let ProjectContents::Text(manifest) = &manifest.contents else { panic!() };
        assert!(!manifest.contains("<h1>Hi</h1>"), "the text lives in the file, not the manifest");
        // Edited outside the editor, then reopened: the disk copy wins.
        let loaded = load_project(manifest, |p| if p == "Assets/Code/UI/menu.html" { Some(b"<h1>Edited</h1>".to_vec()) } else { files.iter().find(|f| f.path == p).map(|f| match &f.contents { ProjectContents::Text(t) => t.clone().into_bytes(), ProjectContents::Bytes(b) => b.clone() }) }).unwrap();
        assert_eq!(loaded.text_file_by_path("UI/menu.html").unwrap().1.text, "<h1>Edited</h1>");
    }

    #[test]
    fn models_are_saved_as_a_folder_with_their_textures() {
        use an1mbyt3_core::{ModelAsset, ModelFile};
        let mut document = Document::new();
        document.add_artboard(Artboard::new("Level 1", 100.0, 100.0));
        let glb = b"glTF   model".to_vec();
        let png = vec![0x89, b'P', b'N', b'G', 9, 9];
        document.add_model(ModelAsset {
            name: "character-a.glb".into(),
            source: glb.clone(),
            // A sneaky `..` must not escape the model's folder.
            files: vec![ModelFile { path: "Textures/../texture-a.png".into(), data: png.clone() }],
            deleted: false,
        });
        let files = save_project(&document, "Game").unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"Assets/Models/character-a/character-a.glb"), "{paths:?}");
        assert!(paths.contains(&"Assets/Models/character-a/Textures/texture-a.png"), "{paths:?}");
        let store: HashMap<String, Vec<u8>> = files
            .iter()
            .map(|f| (f.path.clone(), match &f.contents { ProjectContents::Text(t) => t.clone().into_bytes(), ProjectContents::Bytes(b) => b.clone() }))
            .collect();
        let manifest = String::from_utf8(store["Game.an1proj"].clone()).unwrap();
        assert!(!manifest.contains("103,108,84,70"), "model bytes aren't inlined in the manifest");
        let loaded = load_project(&manifest, |p| store.get(p).cloned()).unwrap();
        let (_, model) = loaded.models().next().unwrap();
        assert_eq!(model.source, glb);
        assert_eq!(model.files, vec![ModelFile { path: "Textures/../texture-a.png".into(), data: png }], "original reference path kept");
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
