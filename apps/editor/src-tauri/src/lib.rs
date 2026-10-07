//! `an1mbyt3-editor`'s Tauri backend: a thin native shell — no document
//! model, no rendering. See AN1MBYT3_BUILD_PLAN.md §1's original
//! desktop-editor principle ("Tauri backend runs the real engine, not WASM,
//! for the desktop case") and why it was deliberately reversed: the
//! document + rendering now live in `crates/an1mbyt3-wasm`'s `WasmEditor`,
//! running directly in the frontend's webview — the same wasm module the
//! browser runtime uses, with a real (non-headless) `wgpu::Surface` built
//! from the canvas, zero IPC bridge for documents or frames. This mirrors
//! how Rive's own web-based editor works (no native desktop shell at all
//! for *their* editor); see `WasmEditor`'s module doc for the full
//! reasoning, including what this reversal cost/saved relative to the
//! native-Rust-plus-readback-bridge approach it replaced.
//!
//! What this crate actually owns now: OS-level integration a webview can't
//! do itself — file-open/save dialogs and disk I/O (`commands.rs`, via
//! `tauri-plugin-dialog` + plain `std::fs`), window chrome (handled via
//! Tauri's own window plugin, see `TitleBar.tsx`). It knows nothing about
//! `.tea`/`.teab`'s actual byte layout — `WasmEditor::save_tea`/`save_teab`/
//! `load_tea`/`load_teab` (in the webview) own that; this crate only ever
//! sees opaque paths and byte/text blobs.

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::pick_save_path,
            commands::pick_open_path,
            commands::pick_open_svg_path,
            commands::pick_open_image_path,
            commands::pick_open_model_path,
            commands::pick_open_audio_path,
            commands::pick_save_gif_path,
            commands::pick_save_html_path,
            commands::write_text_file,
            commands::write_binary_file,
            commands::read_text_file,
            commands::read_binary_file,
            commands::pick_project_folder,
            commands::list_project_files,
            commands::ensure_dir,
            commands::remove_file,
            commands::path_exists,
            commands::write_file_raw,
            commands::read_file_raw,
            commands::reveal_in_explorer,
            commands::recovery_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
