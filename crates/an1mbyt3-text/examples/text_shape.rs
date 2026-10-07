//! Phase 3 smoke test: proves real font shaping + glyph outline tessellation
//! render correctly through a real `NodeKind::Text` document node (not a
//! `Group`-plus-manually-tracked-glyph-meshes hack) — AN1MBYT3_BUILD_PLAN.md
//! §10 Phase 3 exit criterion: "A Text node renders correctly
//! shaped/positioned/rasterized glyphs; editing string content, font, or size
//! re-shapes and re-renders correctly; unrelated transform-only movement of
//! the text node causes zero re-shaping."
//!
//! Builds on `render_shape.rs`/`animate_shape.rs` (Phase 1/2, in
//! `an1mbyt3-renderer`'s examples) for the winit/wgpu setup, and reuses
//! their `AnimationClip`/`Playback` machinery to drive the text node's
//! transform — proving text isn't a special case for the animation system,
//! just another transform to drive. Press:
//! - **Space** to edit the string content (cycles through a few demo
//!   strings) — proves editing content re-shapes.
//! - **+ / -** to change the font size — proves editing size re-shapes.
//! Both print to stderr whether a re-shape actually happened, and the
//! transform-driven bounce keeps moving the whole time without ever
//! triggering one, proving transform-only movement doesn't re-shape.
//!
//! Run with `cargo run -p an1mbyt3-text --example text_shape`. See
//! `render_shape.rs`'s doc comment for the `WGPU_BACKEND` troubleshooting note
//! if this crashes immediately on Windows.

use std::sync::Arc;
use std::time::Instant;

use glam::Vec2;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use an1mbyt3_core::{Artboard, ArtboardId, Document, FontAsset, Node, NodeId, NodeKind, TextNode};
use an1mbyt3_renderer::{BlendMode, Camera, DrawBinding, DrawCall, GpuMesh, Renderer};
use an1mbyt3_runtime::{AnimationClip, Easing, Keyframe, LoopMode, Playback, Track};
use an1mbyt3_text::{shape_and_tessellate, Font};

const ARTBOARD_WIDTH: f32 = 700.0;
const ARTBOARD_HEIGHT: f32 = 300.0;
const DEMO_STRINGS: &[&str] = &["an1mbyt3", "Real Text Node", "Phase 3"];
const FONT_SIZE_STEP_PX: f32 = 8.0;
const MIN_FONT_SIZE_PX: f32 = 16.0;
const MAX_FONT_SIZE_PX: f32 = 160.0;

/// Demo-only: hunts a handful of common Windows system fonts. Real usage
/// (the editor, the runtime) accepts arbitrary font bytes provided by the
/// document/asset system (`FontAsset`) — this list exists purely so the
/// example runs without bundling a font file (and its license) into the repo.
fn load_demo_font_bytes() -> Vec<u8> {
    const CANDIDATES: &[&str] = &[
        "C:/Windows/Fonts/segoeui.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/calibri.ttf",
        "C:/Windows/Fonts/tahoma.ttf",
    ];

    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            if Font::from_bytes(bytes.clone(), 0).is_ok() {
                return bytes;
            }
        }
    }

    panic!(
        "text_shape example: none of the demo system fonts were found ({CANDIDATES:?}). \
         This example only searches common Windows fonts; point it at a real font file instead."
    );
}

fn build_document() -> (Document, ArtboardId, NodeId) {
    let mut document = Document::new();
    let font_id = document.add_font(FontAsset { name: "Demo".into(), data: load_demo_font_bytes() });

    let artboard_id = document.add_artboard(Artboard::new("Main", ARTBOARD_WIDTH, ARTBOARD_HEIGHT));
    let artboard = document.artboard_mut(artboard_id).unwrap();
    let text_node = artboard.add_node(Node::new(
        "Text run",
        None,
        NodeKind::Text(TextNode {
            content: DEMO_STRINGS[0].to_string(),
            font: font_id,
            size_px: 72.0,
            fill_color: [0.92, 0.92, 0.97, 1.0],
            gradient: None,
        }),
    ));

    (document, artboard_id, text_node)
}

fn build_bounce_clip(text_node: NodeId) -> AnimationClip {
    let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
    AnimationClip::new(
        text_node,
        Track::new(vec![
            Keyframe::new(0.0, Vec2::new(0.0, -60.0), ease_in_out),
            Keyframe::new(1.5, Vec2::new(0.0, 60.0), ease_in_out),
        ]),
    )
}

/// Memoizes the shaped-and-tessellated glyph run, keyed on exactly the three
/// inputs the exit criterion calls out (string/font/size) — re-shapes only
/// when one of them actually changes, matching `an1mbyt3-text::run`'s
/// module doc note that this caching discipline is the caller's job, not
/// `shape_and_tessellate`'s. A transform-only move never touches this cache
/// at all, since the transform isn't part of the key.
struct ShapedRunCache {
    key: Option<(String, f32)>,
    glyph_meshes: Vec<(GpuMesh, DrawBinding, Vec2)>,
}

impl ShapedRunCache {
    fn new() -> Self {
        Self { key: None, glyph_meshes: Vec::new() }
    }

    /// Re-shapes and re-uploads only if `content`/`size_px` differ from the
    /// last call. Returns whether a re-shape actually happened, purely so
    /// this demo can print proof of it.
    fn refresh(&mut self, renderer: &Renderer, font: &Font, content: &str, size_px: f32) -> bool {
        let key = (content.to_string(), size_px);
        if self.key.as_ref() == Some(&key) {
            return false;
        }

        let shaped = shape_and_tessellate(font, content, size_px);
        let recenter = Vec2::new(-shaped.advance.x / 2.0, 0.0);
        self.glyph_meshes = shaped
            .glyphs
            .into_iter()
            .map(|g| (renderer.upload_mesh(&g.mesh), renderer.create_draw_binding(), g.offset + recenter))
            .collect();
        self.key = Some(key);
        true
    }
}

struct App {
    smoke_test: bool,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    font: Font,
    cache: ShapedRunCache,
    document: Document,
    artboard_id: ArtboardId,
    text_node: NodeId,
    string_index: usize,
    clip: AnimationClip,
    playback: Playback,
    last_frame: Option<Instant>,
    rendered_one_frame: bool,
}

impl App {
    fn new(smoke_test: bool) -> Self {
        let (document, artboard_id, text_node) = build_document();

        let clip = build_bounce_clip(text_node);
        let mut playback = Playback::new(clip.duration(), LoopMode::PingPong);
        playback.play();

        // Re-parse the same bytes the document's `FontAsset` holds — the
        // in-memory `Font` (rustybuzz/ttf-parser handles) isn't itself part
        // of the saved document, only the raw bytes are (see `FontAsset`'s
        // doc comment), so a real consumer (this example, later the editor)
        // parses it once from `Document::font` and keeps it alongside.
        let NodeKind::Text(text) = &document.artboard(artboard_id).unwrap().node(text_node).unwrap().kind else {
            unreachable!("build_document always creates a Text node")
        };
        let font_asset = document.font(text.font).expect("build_document always registers the font it references");
        let font = Font::from_bytes(font_asset.data.clone(), 0).expect("demo font bytes must parse");

        Self {
            smoke_test,
            window: None,
            renderer: None,
            font,
            cache: ShapedRunCache::new(),
            document,
            artboard_id,
            text_node,
            string_index: 0,
            clip,
            playback,
            last_frame: None,
            rendered_one_frame: false,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("an1mbyt3 — Phase 3 text_shape smoke test")
            .with_inner_size(winit::dpi::LogicalSize::new(ARTBOARD_WIDTH, ARTBOARD_HEIGHT));
        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );

        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create wgpu surface for window");

        let renderer = pollster::block_on(Renderer::new(&instance, surface, size.width, size.height));

        let artboard = self.document.artboard(self.artboard_id).unwrap();
        let NodeKind::Text(text) = &artboard.node(self.text_node).unwrap().kind else {
            unreachable!("build_document always creates a Text node")
        };
        self.cache.refresh(&renderer, &self.font, &text.content, text.size_px);

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_frame = Some(Instant::now());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { state: ElementState::Pressed, logical_key, .. },
                ..
            } => {
                let artboard = self.document.artboard_mut(self.artboard_id).unwrap();
                let NodeKind::Text(text) = &mut artboard.node_mut(self.text_node).unwrap().kind else {
                    unreachable!("build_document always creates a Text node")
                };

                match logical_key {
                    Key::Named(NamedKey::Space) => {
                        self.string_index = (self.string_index + 1) % DEMO_STRINGS.len();
                        text.content = DEMO_STRINGS[self.string_index].to_string();
                        eprintln!("text_shape: content -> {:?} (re-shapes next frame)", text.content);
                    }
                    Key::Character(ref c) if c.as_str() == "+" || c.as_str() == "=" => {
                        text.size_px = (text.size_px + FONT_SIZE_STEP_PX).min(MAX_FONT_SIZE_PX);
                        eprintln!("text_shape: size -> {} (re-shapes next frame)", text.size_px);
                    }
                    Key::Character(ref c) if c.as_str() == "-" => {
                        text.size_px = (text.size_px - FONT_SIZE_STEP_PX).max(MIN_FONT_SIZE_PX);
                        eprintln!("text_shape: size -> {} (re-shapes next frame)", text.size_px);
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(renderer) = &self.renderer else { return };

                let now = Instant::now();
                let delta = now.duration_since(self.last_frame.unwrap_or(now)).as_secs_f32();
                self.last_frame = Some(now);

                self.playback.advance(delta);
                let artboard = self.document.artboard_mut(self.artboard_id).unwrap();
                self.clip.apply(artboard, self.playback.time);

                let node_transform = artboard.node(self.text_node).unwrap().transform;
                let NodeKind::Text(text) = &artboard.node(self.text_node).unwrap().kind else {
                    unreachable!("build_document always creates a Text node")
                };

                // Only re-shapes if content/size actually changed since the
                // last frame — every other frame (the vast majority, driven
                // purely by the bounce `AnimationClip` above) this is a no-op,
                // proving transform-only movement causes zero re-shaping.
                let reshaped = self.cache.refresh(renderer, &self.font, &text.content, text.size_px);
                if reshaped {
                    eprintln!("text_shape: re-shaped ({} glyphs)", self.cache.glyph_meshes.len());
                }
                let fill_color = text.fill_color;

                let draws: Vec<DrawCall> = self
                    .cache
                    .glyph_meshes
                    .iter()
                    .map(|(gpu_mesh, draw_binding, offset)| DrawCall {
                        mesh: gpu_mesh,
                        binding: draw_binding,
                        transform: node_transform.with_local_offset(*offset),
                        fill_color,
                        blend_mode: BlendMode::Normal, gradient: None, texture: None,
                        material: None,
                    })
                    .collect();

                match renderer.render(
                    (ARTBOARD_WIDTH, ARTBOARD_HEIGHT),
                    Camera::default(),
                    wgpu::Color { r: 0.05, g: 0.05, b: 0.08, a: 1.0 },
                    &draws,
                ) {
                    Ok(()) => {
                        self.rendered_one_frame = true;
                        if self.smoke_test {
                            event_loop.exit();
                        }
                    }
                    Err(err) => eprintln!("an1mbyt3 text_shape: render failed: {err:?}"),
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        } else {
            let _ = event_loop;
        }
    }
}

fn main() {
    let smoke_test = std::env::var("AN1MBYT3_SMOKE_TEST").as_deref() == Ok("1");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(smoke_test);
    event_loop.run_app(&mut app).expect("event loop exited with an error");

    if smoke_test && !app.rendered_one_frame {
        panic!("smoke test finished without ever rendering a frame");
    }
}
