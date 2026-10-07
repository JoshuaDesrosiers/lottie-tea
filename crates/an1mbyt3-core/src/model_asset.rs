//! Imported 3D models (glTF / GLB) — a document-level asset like
//! [`crate::ImageAsset`]: the model file's bytes are saved once, with any
//! external files it references (`Textures/texture-a.png`, a `.bin`
//! buffer), and any number of Node3D meshes reference it
//! ([`crate::Primitive3D::Model`]). Parsing happens in `an1mbyt3-geometry`
//! (`parse_gltf`), so the document stays a lossless copy of what was
//! imported.

use serde::{Deserialize, Serialize};

/// One file a model needs besides itself, by the path the model uses for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelFile {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelAsset {
    pub name: String,
    /// The `.glb` (or `.gltf` JSON) bytes.
    pub source: Vec<u8>,
    /// Referenced external files (textures, buffers).
    #[serde(default)]
    pub files: Vec<ModelFile>,
    /// Soft-deleted (undo of an import).
    #[serde(default)]
    pub deleted: bool,
}

impl ModelAsset {
    /// A referenced file by the model's URI — matched exactly, else by file
    /// name (`Textures/texture-a.png` finds an imported `texture-a.png`).
    pub fn file(&self, uri: &str) -> Option<&[u8]> {
        let uri = uri.replace('\u{5c}', "/");
        let name = uri.rsplit('/').next().unwrap_or(&uri).to_ascii_lowercase();
        self.files
            .iter()
            .find(|f| f.path.replace('\u{5c}', "/") == uri)
            .or_else(|| self.files.iter().find(|f| f.path.replace('\u{5c}', "/").rsplit('/').next().unwrap_or("").to_ascii_lowercase() == name))
            .map(|f| f.data.as_slice())
    }
}
