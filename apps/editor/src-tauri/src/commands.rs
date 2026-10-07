//! The native shell's only real job (see `lib.rs`'s module doc): file-open/
//! save dialogs plus the disk I/O a webview can't do itself. Deliberately
//! two steps, not one "save the document" command — the dialog only needs a
//! suggested name and returns a path; the caller (`App.tsx`) then asks
//! `WasmEditor` to serialize into whichever format matches the chosen path's
//! extension (`.tea` vs `.teab`) before calling the plain write command. That
//! split keeps this crate ignorant of the document format entirely, matching
//! `lib.rs`'s "no document logic here" boundary.

use tauri_plugin_dialog::DialogExt;

/// Opens a native "Save As" dialog pre-filled with `suggested_name`, offering
/// both an1mbyt3 extensions as filters. Returns `None` if the user cancels
/// — never an error, cancelling a save isn't a failure.
#[tauri::command]
pub async fn pick_save_path(app: tauri::AppHandle, suggested_name: String) -> Option<String> {
    app.dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter("an1mbyt3 source (.tea)", &["tea"])
        .add_filter("an1mbyt3 binary (.teab)", &["teab"])
        .blocking_save_file()
        .map(|path| path.to_string())
}

/// Opens a native "Open" dialog filtered to an1mbyt3 files. Returns `None`
/// if the user cancels.
#[tauri::command]
pub async fn pick_open_path(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .add_filter("an1mbyt3 files", &["tea", "teab"])
        .blocking_pick_file()
        .map(|path| path.to_string())
}

/// Opens a native "Open" dialog filtered to `.svg` — the Shapes panel's
/// "Import SVG" action. Separate from `pick_open_path` (not a generalized
/// "pick with these extensions" command) since that one's callers rely on
/// its filter always meaning "a an1mbyt3 document"; keeping this one
/// dedicated avoids either command's behavior drifting for the other's sake.
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_open_svg_path(app: tauri::AppHandle) -> Option<String> {
    app.dialog().file().add_filter("SVG files", &["svg"]).blocking_pick_file().map(|path| path.to_string())
}

/// Same "dedicated filter, not a generalized picker" reasoning as
/// `pick_open_svg_path` — the Shapes panel's "Import Image/GIF" action.
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_open_image_path(app: tauri::AppHandle) -> Option<String> {
    app.dialog().file().add_filter("Image files", &["png", "jpg", "jpeg", "gif"]).blocking_pick_file().map(|path| path.to_string())
}

/// + menu → Import → Audio: a sound file.
#[tauri::command]
pub async fn pick_open_audio_path(app: tauri::AppHandle) -> Option<String> {
    app.dialog().file().add_filter("Sound files", &["mp3", "wav", "ogg", "m4a", "aac", "flac", "webm"]).blocking_pick_file().map(|path| path.to_string())
}

/// + menu → Import → 3D Model: a glTF / GLB file. (The frontend then reads
/// the textures / buffers it references from beside it.)
#[tauri::command]
pub async fn pick_open_model_path(app: tauri::AppHandle) -> Option<String> {
    app.dialog().file().add_filter("3D models (glTF)", &["glb", "gltf"]).blocking_pick_file().map(|path| path.to_string())
}

/// Same "dedicated filter, not a generalized picker" reasoning as
/// `pick_open_svg_path` — the toolbar's "Export GIF" action, a save (not
/// open) dialog since this writes a new file rather than reading an
/// existing document. Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_save_gif_path(app: tauri::AppHandle, suggested_name: String) -> Option<String> {
    app.dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter("GIF image (.gif)", &["gif"])
        .blocking_save_file()
        .map(|path| path.to_string())
}

/// Opens a native "Save As" dialog filtered to `.html` — the toolbar's
/// "Export Game" action (one self-contained, double-clickable web page).
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_save_html_path(app: tauri::AppHandle, suggested_name: String) -> Option<String> {
    app.dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter("Web game (.html)", &["html"])
        .blocking_save_file()
        .map(|path| path.to_string())
}

#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn write_binary_file(path: String, contents: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn read_binary_file(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|err| err.to_string())
}

// ---- Project folders (Unity-style projects: a folder of files) ----
//
// The editor keeps a project as an ordinary folder (manifest + Assets/…, see
// `an1mbyt3_format::project`). These are the plain file-system primitives it
// needs for that — still no document logic here (`lib.rs`'s boundary): the
// frontend decides *what* to write, this only reads/writes/lists.

/// Native folder picker — "New Project" / "Open Project". `None` = cancelled.
#[tauri::command]
pub async fn pick_project_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog().file().blocking_pick_folder().map(|path| path.to_string())
}

/// One entry of `list_project_files`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    /// Relative to the listed root, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Last-modified time, ms since the Unix epoch (0 if unavailable).
    pub modified: f64,
}

/// Every file under `root`, recursively (hidden `.`-prefixed entries
/// skipped) — the Resources panel's disk view and the "new file dropped into
/// Assets" scan. Capped at 20k entries so pointing it at a huge folder by
/// mistake can't hang the UI.
#[tauri::command]
pub fn list_project_files(root: String) -> Result<Vec<ProjectEntry>, String> {
    let root = std::path::PathBuf::from(root);
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if dir == root => return Err(err.to_string()),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(&root) {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0.0, |d| d.as_millis() as f64);
                out.push(ProjectEntry { path: relative.to_string_lossy().replace('\\', "/"), size: meta.len(), modified });
                if out.len() >= 20_000 {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

/// `mkdir -p`.
#[tauri::command]
pub fn ensure_dir(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|err| err.to_string())
}

/// Deletes one file (a stale asset file after a scene rename). A missing
/// file is not an error.
#[tauri::command]
pub fn remove_file(path: String) -> Result<(), String> {
    match std::fs::remove_file(&path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.to_string()),
        _ => Ok(()),
    }
}

#[tauri::command]
pub fn path_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

/// Minimal `%XX` decoder for the `x-path` header (`encodeURIComponent` on
/// the JS side — header values must be ASCII, paths may not be).
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Writes raw bytes (the IPC body itself, no JSON number array) to the path
/// in the `x-path` header, creating parent folders. Used for images, where
/// `write_binary_file`'s `Vec<u8>` JSON encoding is several times the size.
#[tauri::command]
pub fn write_file_raw(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let path = request.headers().get("x-path").and_then(|v| v.to_str().ok()).map(percent_decode).ok_or("missing x-path header")?;
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else { return Err("expected a raw byte body".into()) };
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(&path, bytes).map_err(|err| err.to_string())
}

/// Reads a file as a raw `ArrayBuffer` response (no JSON number array).
#[tauri::command]
pub fn read_file_raw(path: String) -> Result<tauri::ipc::Response, String> {
    std::fs::read(&path).map(tauri::ipc::Response::new).map_err(|err| err.to_string())
}

/// Opens `path` (a folder) in the OS file manager.
#[tauri::command]
pub fn reveal_in_explorer(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program).arg(path.replace('/', std::path::MAIN_SEPARATOR_STR)).spawn().map(|_| ()).map_err(|err| err.to_string())
}

/// Where crash-recovery autosaves of unsaved, folder-less documents go
/// (`<app data>/recovery`, created on demand).
#[tauri::command]
pub fn recovery_dir(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?.join("recovery");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir.to_string_lossy().replace('\\', "/"))
}

/// Opens a native "Save As" dialog for **Export Game → Desktop app** (an
/// `.exe` on Windows; a plain executable elsewhere).
#[tauri::command]
pub async fn pick_save_exe_path(app: tauri::AppHandle, suggested_name: String) -> Option<String> {
    let dialog = app.dialog().file().set_file_name(&suggested_name);
    #[cfg(target_os = "windows")]
    let dialog = dialog.add_filter("Desktop game (.exe)", &["exe"]);
    dialog.blocking_save_file().map(|path| path.to_string())
}

/// The prebuilt desktop game runner (`apps/runner`): bundled next to the
/// editor as a resource in releases; in development, the workspace's own
/// `cargo build -p an1mbyt3-runner --release` output.
fn runner_executable(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    use tauri::Manager;
    let name = if cfg!(target_os = "windows") { "an1mbyt3-runner.exe" } else { "an1mbyt3-runner" };
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        candidates.push(dir.join("runner").join(name));
        candidates.push(dir.join(name));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        candidates.push(dir.join(name));
    }
    // apps/editor/src-tauri -> workspace target/release.
    candidates.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/release").join(name));
    candidates.into_iter().find(|p| p.is_file())
}

/// **Export Game → Desktop app**: writes one executable = the runner +
/// the exported game page + a trailer (page length as `u64` LE, then
/// `AN1GAME\x01`) — see `apps/runner/src/main.rs`.
#[tauri::command]
pub fn export_desktop_game(app: tauri::AppHandle, path: String, html: String) -> Result<(), String> {
    let runner = runner_executable(&app).ok_or("the desktop runner isn't built — run `cargo build -p an1mbyt3-runner --release`")?;
    let mut bytes = std::fs::read(&runner).map_err(|err| format!("couldn't read the runner: {err}"))?;
    let page = html.into_bytes();
    let len = page.len() as u64;
    bytes.extend_from_slice(&page);
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(b"AN1GAME\x01");
    std::fs::write(&path, bytes).map_err(|err| err.to_string())?;
    // Unix: make it runnable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// A "Save As" dialog for any one file type (`filter_name`, `extensions`
/// without dots) — the Export menu's GIF / WebM / MP4 targets.
#[tauri::command]
pub async fn pick_save_file_path(app: tauri::AppHandle, suggested_name: String, filter_name: String, extensions: Vec<String>) -> Option<String> {
    let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
    app.dialog().file().set_file_name(&suggested_name).add_filter(&filter_name, &extensions).blocking_save_file().map(|path| path.to_string())
}
