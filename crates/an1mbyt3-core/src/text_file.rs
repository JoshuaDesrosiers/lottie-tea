//! [`TextFile`] — a code / markup file that lives in the project: HTML and
//! CSS for HTML UI, JavaScript for scripts, anything else text. The Code
//! tab edits them like an IDE; in a project folder each one is a real file
//! under `Assets/Code/<path>` (so outside editors work too), and exported
//! games carry them inside the document.
//!
//! Other data points at a file by **path** with an `@file:` reference
//! instead of holding the text inline — an HTML UI component's `html` /
//! `css` fields, or a state's Code program, can be `"@file:UI/menu.html"`.
//! A plain string field keeps old documents loading unchanged; resolving
//! the reference is [`resolve_file_ref`].

use serde::{Deserialize, Serialize};

/// The prefix that turns a text field into a reference to a [`TextFile`].
pub const FILE_REF_PREFIX: &str = "@file:";

/// A project text file — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextFile {
    /// Folder-style path inside the project's code area, `/`-separated,
    /// no leading slash: `"UI/menu.html"`, `"Scripts/player.js"`.
    pub path: String,
    pub text: String,
    #[serde(default)]
    pub deleted: bool,
}

impl TextFile {
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self { path: normalize_path(&path.into()), text: text.into(), deleted: false }
    }

    /// The extension, lowercased (`"html"`, `"css"`, `"js"`…), or `""`.
    pub fn extension(&self) -> String {
        self.path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).filter(|e| !e.contains('/')).unwrap_or_default()
    }
}

/// Cleans a file path: forward slashes, no leading / trailing slash, no
/// empty, `.` or `..` segments (a path can't escape the code folder).
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").split('/').map(str::trim).filter(|s| !s.is_empty() && *s != "." && *s != "..").collect::<Vec<_>>().join("/")
}

/// If `value` is an `@file:` reference, the path it names.
pub fn file_ref_path(value: &str) -> Option<&str> {
    value.strip_prefix(FILE_REF_PREFIX).map(str::trim)
}

/// The text `value` stands for: the named file's text if it's an `@file:`
/// reference (`lookup` finds a file by path), else `value` itself. A
/// reference to a missing file resolves to `None`.
pub fn resolve_file_ref<'a>(value: &'a str, lookup: impl Fn(&str) -> Option<&'a str>) -> Option<&'a str> {
    match file_ref_path(value) {
        Some(path) => lookup(&normalize_path(path)),
        None => Some(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized_and_cannot_escape() {
        assert_eq!(normalize_path("\\UI\\./menu.html/"), "UI/menu.html");
        assert_eq!(normalize_path("../../secret/../x.js"), "secret/x.js");
        assert_eq!(TextFile::new("Scripts/Player.JS", "").extension(), "js");
        assert_eq!(TextFile::new("Makefile", "").extension(), "");
    }

    #[test]
    fn references_resolve_through_the_lookup() {
        let files = [TextFile::new("UI/menu.html", "<b>hi</b>")];
        let lookup = |p: &str| files.iter().find(|f| f.path == p).map(|f| f.text.as_str());
        assert_eq!(resolve_file_ref("@file: UI/menu.html", lookup), Some("<b>hi</b>"));
        assert_eq!(resolve_file_ref("<i>inline</i>", lookup), Some("<i>inline</i>"));
        assert_eq!(resolve_file_ref("@file:UI/missing.html", lookup), None);
    }
}
