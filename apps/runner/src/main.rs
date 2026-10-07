//! The an1mbyt3 desktop game runner — what the editor's **Export Game →
//! Desktop app** ships.
//!
//! An exported desktop game is *one* executable: this runner's own bytes,
//! followed by the exported game's self-contained `.html` (the same page
//! "Export Game → Web page" writes), followed by a 16-byte trailer:
//! the page's length (`u64`, little-endian) and the magic
//! [`TRAILER_MAGIC`]. The OS loader ignores bytes past the end of the
//! executable image, so the file still runs; on start the runner reads its
//! own file, finds the trailer and serves the page to a system webview
//! (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux).
//!
//! The page is served from a custom `game://` protocol rather than a
//! `file://` URL: it shows up as `http://game.localhost` on Windows, which
//! browsers treat as a secure context — required for WebGPU — and it keeps
//! the game away from the user's file system.
//!
//! Extras over a browser tab: a real window titled after the game, **F11**
//! toggles fullscreen, and the webview's data (local storage / saves) lives
//! in the user's app-data folder under the game's name, so saves persist
//! between runs.
//!
//! Without an appended game (running the bare runner during development)
//! it falls back to a `game.html` next to the executable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Fullscreen, WindowBuilder};
use wry::http::{header::CONTENT_TYPE, Response};
use wry::{WebContext, WebViewBuilder};

/// Ends every exported desktop game (after the page length).
pub const TRAILER_MAGIC: &[u8; 8] = b"AN1GAME\x01";

/// Messages from the page (via `window.ipc.postMessage`) to the window.
enum UserEvent {
    ToggleFullscreen,
}

/// The game page appended to this executable, if any.
fn embedded_game() -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    let bytes = std::fs::read(exe).ok()?;
    let n = bytes.len();
    if n < 16 || &bytes[n - 8..] != TRAILER_MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(bytes[n - 16..n - 8].try_into().ok()?) as usize;
    let start = (n - 16).checked_sub(len)?;
    Some(bytes[start..n - 16].to_vec())
}

/// Development fallback: `game.html` beside the executable.
fn sibling_game() -> Option<Vec<u8>> {
    let exe = std::env::current_exe().ok()?;
    std::fs::read(exe.parent()?.join("game.html")).ok()
}

/// The page's `<title>` (the window's title and app-data folder name).
fn page_title(html: &[u8]) -> String {
    let text = String::from_utf8_lossy(&html[..html.len().min(8192)]);
    text.split_once("<title>")
        .and_then(|(_, rest)| rest.split_once("</title>"))
        .map(|(title, _)| title.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "an1mbyt3 game".into())
}

/// Where this game's webview keeps local storage / caches.
fn data_dir(title: &str) -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    let safe: String = title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
    Some(base.join("an1mbyt3 Games").join(safe.trim()))
}

/// Shown when there's no game to play (the bare runner).
const NO_GAME: &str = "<!doctype html><meta charset=utf-8><title>an1mbyt3 runner</title><body style=\"margin:0;height:100vh;display:flex;align-items:center;justify-content:center;background:#000;color:#cfc8e0;font:14px system-ui\">No game is attached to this runner. Use Export Game → Desktop app in the an1mbyt3 editor.</body>";

/// F11 → fullscreen (the page has the keyboard focus, so it forwards it).
const INIT_SCRIPT: &str = r#"
window.addEventListener('keydown', (e) => {
  if (e.key === 'F11') { e.preventDefault(); window.ipc.postMessage('fullscreen') }
}, true)
"#;

fn main() -> wry::Result<()> {
    let html: Arc<Vec<u8>> = Arc::new(embedded_game().or_else(sibling_game).unwrap_or_else(|| NO_GAME.as_bytes().to_vec()));
    let title = page_title(&html);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(LogicalSize::new(1280.0, 720.0))
        .with_min_inner_size(LogicalSize::new(320.0, 240.0))
        .build(&event_loop)
        .expect("failed to open the game window");

    let mut context = WebContext::new(data_dir(&title));
    let page = html.clone();
    let builder = WebViewBuilder::new_with_web_context(&mut context)
        .with_custom_protocol("game".into(), move |_id, request| {
            // One page holds everything (engine, game, player code).
            if request.uri().path() == "/" || request.uri().path() == "/index.html" {
                Response::builder().header(CONTENT_TYPE, "text/html; charset=utf-8").body(Cow::Owned(page.to_vec())).expect("valid response")
            } else {
                Response::builder().status(404).body(Cow::Borrowed(&[][..])).expect("valid response")
            }
        })
        .with_initialization_script(INIT_SCRIPT)
        .with_ipc_handler(move |request| {
            if request.body() == "fullscreen" {
                let _ = proxy.send_event(UserEvent::ToggleFullscreen);
            }
        })
        .with_devtools(cfg!(debug_assertions))
        .with_url("game://localhost/");

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let _webview = builder.build(&window)?;
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        builder.build_gtk(window.default_vbox().expect("gtk vbox"))?
    };

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEvent::ToggleFullscreen) => {
                let next = if window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) };
                window.set_fullscreen(next);
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_title() {
        assert_eq!(page_title(b"<html><head><title>Sunset &amp; Tactics</title>"), "Sunset & Tactics");
        assert_eq!(page_title(b"<html>"), "an1mbyt3 game");
    }
}
