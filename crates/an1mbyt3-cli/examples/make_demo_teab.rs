//! Generates a real, authored `.teab` file — the same "Idle" (parked left)/
//! "Active" (slid right, eased) two-state demo `an1mbyt3-renderer`'s
//! `state_machine` example and `an1mbyt3-wasm`'s `WasmPlayer::create`
//! both build in code, just written to disk this time instead of only ever
//! existing in memory.
//!
//! This exists to produce a real fixture for Phase 8's actual exit
//! criterion (AN1MBYT3_BUILD_PLAN.md §10: "`<Animation>` plays a real
//! `.teab` file in a browser with working state-machine input") —
//! `apps/web-test`'s `.teab` demo page loads whatever this writes. Nothing
//! about the document it builds is CLI-specific; this is a one-off
//! generator, not a `an1mbyt3-cli` subcommand, since "build me this
//! specific demo scene" isn't a general compiler operation.
//!
//! Run with `cargo run -p an1mbyt3-cli --example make_demo_teab -- <output-path>`
//! (defaults to `demo.teab` in the current directory if no path is given).

use glam::Vec2;
use an1mbyt3_core::{
    Animation, Artboard, ConditionDef, Document, Easing, InputDef, InputKind, Keyframe, LoopMode, Node, NodeKind, Path, PlaybackDirection,
    PropertyTrack, ShapeNode, StateDef, StateMachineDef, Track, TransitionDef,
};

fn build_demo_document() -> Document {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));

    let artboard = document.artboard_mut(artboard_id).unwrap();
    let node_id = artboard.add_node(Node::new(
        "Square",
        None,
        NodeKind::Shape(ShapeNode { path: Path::rect(120.0, 120.0), fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
    ));

    let ease_in_out = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };

    let mut idle_animation = Animation::new("Idle");
    idle_animation.duration = 0.0;
    idle_animation
        .tracks
        .push((node_id, PropertyTrack::Translation(Track::new(vec![Keyframe::new(0.0, Vec2::new(-120.0, 0.0), Easing::Linear)]))));

    let mut active_animation = Animation::new("Active");
    active_animation.duration = 0.6;
    active_animation.tracks.push((
        node_id,
        PropertyTrack::Translation(Track::new(vec![
            Keyframe::new(0.0, Vec2::new(-120.0, 0.0), ease_in_out),
            Keyframe::new(0.6, Vec2::new(120.0, 0.0), ease_in_out),
        ])),
    ));

    let idle_animation_id = artboard.add_animation(idle_animation);
    let active_animation_id = artboard.add_animation(active_animation);

    let def = StateMachineDef {
        name: "Main".into(),
        inputs: vec![InputDef { name: "toggle".into(), kind: InputKind::Trigger }],
        states: vec![
            StateDef {
                name: "Idle".into(),
                animation: idle_animation_id,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Active".into(),
                    condition: ConditionDef::Trigger { input: "toggle".into() },
                    direction: PlaybackDirection::Forward,
                    duration_seconds: 0.0,
                    interruptible: true,
                }],
                position: glam::Vec2::ZERO,
                blend: None,
                program: None,
            },
            StateDef {
                name: "Active".into(),
                animation: active_animation_id,
                loop_mode: LoopMode::Once,
                transitions: vec![TransitionDef {
                    to: "Idle".into(),
                    condition: ConditionDef::Trigger { input: "toggle".into() },
                    direction: PlaybackDirection::Forward,
                    duration_seconds: 0.0,
                    interruptible: true,
                }],
                position: glam::Vec2::ZERO,
                blend: None,
                program: None,
            },
        ],
        initial_state: "Idle".into(),
        layers: Vec::new(),
        deleted: false,
    };
    artboard.add_state_machine(def);

    document
}

fn main() {
    let output_path = std::env::args().nth(1).unwrap_or_else(|| "demo.teab".to_string());
    let document = build_demo_document();
    let bytes = an1mbyt3_format::save_teab(&document).expect("save_teab should never fail for a well-formed in-memory Document");
    std::fs::write(&output_path, &bytes).unwrap_or_else(|err| panic!("failed to write {output_path}: {err}"));
    println!("wrote {} bytes to {output_path}", bytes.len());
}
