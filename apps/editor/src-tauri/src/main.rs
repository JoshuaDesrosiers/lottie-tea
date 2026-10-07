//! Entry point for the native Tauri process. All real logic lives in `lib.rs`
//! (`an1mbyt3_editor_lib`) — this file only exists because Tauri's Windows
//! release-build console-suppression attribute has to sit on the actual
//! `fn main`, not a library crate's `run()` function.

// Prevents an additional console window on Windows in release builds — DO NOT REMOVE.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    an1mbyt3_editor_lib::run();
}
