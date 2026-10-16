//! **Toasty Lite** export: one scene as a small, self-describing JSON file
//! that the Lite web runtime (`apps/editor/src/lite`) draws with Canvas 2D.
//!
//! The full engine (wasm + WebGPU) runs games; it's big and runs one
//! player per page. Lite is the Rive-style counterpart for UI animation —
//! icons, buttons, toggles, loaders, illustrations: tiny, starts instantly,
//! and any number of them can run on one page. It covers the part of a
//! scene that UI animation needs:
//!
//! - vector **shapes** (fill, linear / radial gradient, stroke with caps,
//!   joins, dashes and **trim path**), **groups** and **text**, in a
//!   transform hierarchy with anchors;
//! - **animations** — position, rotation, scale, fill colour, path morphs
//!   and the animatable fields Lite understands (trim, dash offset, stroke
//!   width / colour, visibility), with linear or cubic-bezier easing;
//! - the **state machine** — inputs, every layer, conditions, crossfades,
//!   loop modes;
//! - **UI Button** hover / press looks, **On Click** and **Pointer
//!   Listeners** actions (set input, fire trigger, set property, call
//!   function → an event for the page), scene **Properties** and
//!   **Property Bindings** (text / position / scale / visible / colour).
//!
//! Scripts, physics, particles, images and 3D are full-engine features and
//! are left out (the export lists what it skipped in `skipped`).
//!
//! Coordinates are exported exactly as authored (world units, **y up**,
//! the active camera's centre at the middle of the canvas); the runtime
//! flips y when it draws.

use std::collections::HashMap;

use an1mbyt3_core::{
    ActionKind, ArtboardId, BindTarget, ConditionDef, Document, EventAction, GradientDef, GradientKind, InputKind, LoopMode, NodeBehavior, NodeId, NodeKind, Path, PathCommand, PlaybackDirection,
    PropKind, PropertyTrack, StateDef, StrokeCap, StrokeJoin,
};
use an1mbyt3_core::Easing;
use serde_json::{json, Map, Value};

/// The file format tag and version the runtime checks.
pub const LITE_FORMAT: &str = "toasty-lite";
pub const LITE_VERSION: u32 = 1;

/// Exports `artboard` (or the first scene) as a Toasty Lite JSON document.
pub fn export_lite(document: &Document, artboard: Option<ArtboardId>) -> Option<Value> {
    let (artboard_id, board) = match artboard {
        Some(id) => (id, document.artboard(id)?),
        None => document.artboards().next()?,
    };
    let mut skipped: Vec<String> = Vec::new();

    // ---- nodes, in draw order (depth-first over sorted children) ----
    let mut order: Vec<NodeId> = Vec::new();
    let mut stack: Vec<NodeId> = board.sorted_children(None).into_iter().rev().collect();
    while let Some(id) = stack.pop() {
        if board.is_effectively_deleted(id) {
            continue;
        }
        order.push(id);
        stack.extend(board.sorted_children(Some(id)).into_iter().rev());
    }
    let index: HashMap<NodeId, usize> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    // The camera frames the view: its position is the canvas centre.
    let mut view = json!({ "x": 0.0, "y": 0.0, "zoom": 1.0 });
    let mut background = Value::Null;
    if let Some(camera) = board.active_camera().and_then(|id| board.node(id).map(|n| (id, n))) {
        if let NodeKind::Camera(c) = &camera.1.kind {
            let t = board.world_transform(camera.0);
            view = json!({ "x": t.translation.x, "y": t.translation.y, "zoom": c.zoom });
        }
        for entry in &camera.1.behaviors {
            if let NodeBehavior::Camera(c) = &entry.behavior {
                let t = board.world_transform(camera.0);
                view = json!({ "x": t.translation.x, "y": t.translation.y, "zoom": c.zoom });
                background = json!(c.background_color);
            }
        }
    }

    let mut fonts_used: Vec<an1mbyt3_core::FontId> = Vec::new();
    let mut nodes = Vec::new();
    for id in &order {
        let node = board.node(*id).expect("ordered ids exist");
        let t = &node.transform;
        let mut out = Map::new();
        out.insert("id".into(), json!(node.id));
        out.insert("name".into(), json!(node.name));
        out.insert("parent".into(), node.parent.and_then(|p| index.get(&p)).map_or(Value::Null, |i| json!(i)));
        out.insert("visible".into(), json!(node.visible));
        out.insert("x".into(), json!(t.translation.x));
        out.insert("y".into(), json!(t.translation.y));
        out.insert("rotation".into(), json!(t.rotation));
        out.insert("scaleX".into(), json!(t.scale.x));
        out.insert("scaleY".into(), json!(t.scale.y));
        out.insert("anchorX".into(), json!(t.anchor.x));
        out.insert("anchorY".into(), json!(t.anchor.y));
        if node.clip_children {
            out.insert("clip".into(), json!(true));
        }
        match &node.kind {
            NodeKind::Shape(shape) => {
                out.insert("kind".into(), json!("shape"));
                out.insert("path".into(), json!(path_data(&shape.path)));
                out.insert("fill".into(), json!(shape.fill_color));
                if let Some(g) = &shape.gradient {
                    out.insert("gradient".into(), gradient(g));
                }
                if let Some(stroke) = &shape.stroke {
                    let mut s = json!({
                        "width": stroke.width,
                        "color": stroke.color,
                        "cap": match stroke.cap { StrokeCap::Butt => "butt", StrokeCap::Square => "square", StrokeCap::Round => "round" },
                        "join": match stroke.join { StrokeJoin::Miter => "miter", StrokeJoin::Round => "round", StrokeJoin::Bevel => "bevel" },
                    });
                    if let Some(g) = &stroke.gradient {
                        s["gradient"] = gradient(g);
                    }
                    out.insert("stroke".into(), s);
                }
            }
            NodeKind::Text(text) => {
                out.insert("kind".into(), json!("text"));
                if !fonts_used.contains(&text.font) {
                    fonts_used.push(text.font);
                }
                let font = document.font(text.font).map(|f| f.name.clone()).unwrap_or_default();
                out.insert("text".into(), json!({ "content": text.content, "font": font, "size": text.size_px, "color": text.fill_color }));
            }
            NodeKind::Group(_) | NodeKind::Camera(_) | NodeKind::Bone(_) => {
                out.insert("kind".into(), json!("group"));
            }
            other => {
                out.insert("kind".into(), json!("group"));
                skipped.push(format!("{} ({})", node.name, kind_name(other)));
            }
        }
        // Components Lite understands; the rest are noted as skipped.
        let mut listeners = Map::new();
        for entry in node.behaviors.iter().filter(|e| e.enabled) {
            match &entry.behavior {
                NodeBehavior::StrokeStyle2D(s) => {
                    out.insert("trim".into(), json!({ "start": s.trim_start, "end": s.trim_end, "offset": s.trim_offset }));
                    if s.dash > 0.0 {
                        out.insert("dash".into(), json!({ "dash": s.dash, "gap": s.gap, "offset": s.dash_offset }));
                    }
                }
                NodeBehavior::UiButton(b) => {
                    let st = &b.style;
                    out.insert(
                        "button".into(),
                        json!({
                            "interactable": b.interactable,
                            "transition": format!("{:?}", st.transition),
                            "normalColor": st.normal_color,
                            "hoverColor": st.highlighted_color,
                            "pressedColor": st.pressed_color,
                            "hoverScale": st.highlighted_scale,
                            "pressedScale": st.pressed_scale,
                            "fade": st.fade_duration,
                            "tintChildren": st.tint_children,
                        }),
                    );
                }
                NodeBehavior::UiEvents(e) => {
                    listeners.insert("click".into(), actions(&e.on_click, &index));
                }
                NodeBehavior::UiListeners(l) => {
                    listeners.insert("enter".into(), actions(&l.on_enter, &index));
                    listeners.insert("exit".into(), actions(&l.on_exit, &index));
                    listeners.insert("down".into(), actions(&l.on_down, &index));
                    listeners.insert("up".into(), actions(&l.on_up, &index));
                }
                NodeBehavior::PropertyBindings(b) => {
                    let list: Vec<Value> = b
                        .bindings
                        .iter()
                        .filter_map(|bind| {
                            let target = match &bind.target {
                                BindTarget::Text => "text",
                                BindTarget::X => "x",
                                BindTarget::Y => "y",
                                BindTarget::Rotation => "rotation",
                                BindTarget::ScaleX => "scaleX",
                                BindTarget::ScaleY => "scaleY",
                                BindTarget::Scale => "scale",
                                BindTarget::Visible => "visible",
                                BindTarget::FillColor => "fill",
                                BindTarget::ComponentField { component, field } => return lite_component_field(component, field).map(|p| json!({ "property": bind.property, "target": p, "format": bind.format, "scale": bind.scale, "offset": bind.offset })),
                            };
                            Some(json!({ "property": bind.property, "target": target, "format": bind.format, "scale": bind.scale, "offset": bind.offset }))
                        })
                        .collect();
                    out.insert("bindings".into(), json!(list));
                }
                NodeBehavior::Camera(_) => {}
                other => skipped.push(format!("{}: {}", node.name, behavior_name(other))),
            }
        }
        if !listeners.is_empty() {
            out.insert("listeners".into(), Value::Object(listeners));
        }
        nodes.push(Value::Object(out));
    }

    // ---- animations ----
    let mut anim_index: HashMap<an1mbyt3_core::AnimationId, usize> = HashMap::new();
    let mut animations = Vec::new();
    for (aid, anim) in board.animations().filter(|(_, a)| !a.deleted) {
        let mut tracks = Vec::new();
        for (node, track) in &anim.tracks {
            let Some(&n) = index.get(node) else { continue };
            let (prop, keys): (&str, Vec<Value>) = match track {
                PropertyTrack::Translation(t) => ("translation", t.keyframes.iter().map(|k| key(k.time, &[k.value.x, k.value.y], &k.easing)).collect()),
                PropertyTrack::Rotation(t) => ("rotation", t.keyframes.iter().map(|k| key(k.time, &[k.value], &k.easing)).collect()),
                PropertyTrack::Scale(t) => ("scale", t.keyframes.iter().map(|k| key(k.time, &[k.value.x, k.value.y], &k.easing)).collect()),
                PropertyTrack::FillColor(t) => ("fill", t.keyframes.iter().map(|k| key(k.time, &k.value, &k.easing)).collect()),
                PropertyTrack::Path(t) => (
                    "path",
                    t.keyframes
                        .iter()
                        .map(|k| {
                            let mut v = key(k.time, &path_numbers(&k.value), &k.easing);
                            v["d"] = json!(path_data(&k.value));
                            v
                        })
                        .collect(),
                ),
                PropertyTrack::Field { path, track } => {
                    let Some(prop) = lite_field(path) else {
                        skipped.push(format!("animation \"{}\": field {path}", anim.name));
                        continue;
                    };
                    (prop, track.keyframes.iter().map(|k| key(k.time, &k.value, &k.easing)).collect())
                }
                PropertyTrack::MotionPath { .. } => {
                    skipped.push(format!("animation \"{}\": motion path", anim.name));
                    continue;
                }
            };
            tracks.push(json!({ "node": n, "prop": prop, "keys": keys }));
        }
        anim_index.insert(aid, animations.len());
        animations.push(json!({ "name": anim.name, "duration": anim.duration, "tracks": tracks }));
    }

    // ---- state machine (the first live one) ----
    let state_machine = board.state_machines().find(|(_, m)| !m.deleted).map(|(_, m)| {
        let mut states = |list: &[StateDef]| -> Vec<Value> {
            list.iter()
                .map(|s| {
                    if s.program.is_some() {
                        skipped.push(format!("state \"{}\": its program (scripts need the full engine)", s.name));
                    }
                    json!({
                        "name": s.name,
                        "animation": anim_index.get(&s.animation),
                        "loop": match s.loop_mode { LoopMode::Once => "once", LoopMode::Loop => "loop", LoopMode::PingPong => "pingpong" },
                        "transitions": s.transitions.iter().map(|t| json!({
                            "to": t.to,
                            "condition": condition(&t.condition),
                            "duration": t.duration_seconds,
                            "reverse": matches!(t.direction, PlaybackDirection::Reverse),
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect()
        };
        let mut layers = vec![json!({ "name": Value::Null, "initial": m.initial_state, "states": states(&m.states) })];
        for layer in &m.layers {
            layers.push(json!({ "name": layer.name, "initial": layer.initial_state, "states": states(&layer.states) }));
        }
        json!({
            "name": m.name,
            "inputs": m.inputs.iter().map(|i| json!({ "name": i.name, "kind": match i.kind { InputKind::Bool => "bool", InputKind::Number => "number", InputKind::Enum => "enum", InputKind::Trigger => "trigger" } })).collect::<Vec<_>>(),
            "layers": layers,
        })
    });

    let properties: Vec<Value> = document
        .properties()
        .filter(|(_, p)| p.artboard == artboard_id && !p.deleted)
        .map(|(_, p)| {
            let default: Value = serde_json::from_str(&p.default).unwrap_or(Value::String(p.default.clone()));
            json!({ "name": p.name, "kind": match p.kind { PropKind::Number => "number", PropKind::Bool => "bool", PropKind::Text => "text", PropKind::Color => "color" }, "default": default })
        })
        .collect();

    let fonts: Vec<Value> = fonts_used.iter().filter_map(|id| document.font(*id)).map(|f| json!({ "name": f.name, "data": base64(&f.data) })).collect();

    Some(json!({
        "format": LITE_FORMAT,
        "version": LITE_VERSION,
        "scene": board.name,
        "width": board.width,
        "height": board.height,
        "view": view,
        "background": background,
        "nodes": nodes,
        "animations": animations,
        "stateMachine": state_machine,
        "properties": properties,
        "fonts": fonts,
        "skipped": skipped,
    }))
}

/// `{ t, v, e }`: time, value (1–4 numbers), easing (`null` = linear, or
/// the cubic-bezier `[x1, y1, x2, y2]`) of the segment leaving this key.
fn key(time: f32, value: &[f32], easing: &Easing) -> Value {
    let e = match easing {
        Easing::Linear => Value::Null,
        Easing::CubicBezier { x1, y1, x2, y2 } => json!([x1, y1, x2, y2]),
    };
    json!({ "t": time, "v": value, "e": e })
}

/// The Lite name for an animatable field path, if Lite draws it.
fn lite_field(path: &str) -> Option<&'static str> {
    Some(match path {
        "behavior/StrokeStyle2D/trimStart" => "trimStart",
        "behavior/StrokeStyle2D/trimEnd" => "trimEnd",
        "behavior/StrokeStyle2D/trimOffset" => "trimOffset",
        "behavior/StrokeStyle2D/dashOffset" => "dashOffset",
        "kind/Shape/stroke/width" => "strokeWidth",
        "kind/Shape/stroke/color" => "strokeColor",
        "kind/Text/size_px" => "textSize",
        "kind/Text/fill_color" => "fill",
        "node/visible" => "visible",
        "transform/anchor" => "anchor",
        _ => return None,
    })
}

fn lite_component_field(component: &str, field: &str) -> Option<&'static str> {
    if component == "Shape" && field == "strokeColor" {
        return Some("strokeColor");
    }
    lite_field(&format!("behavior/{component}/{field}"))
}

/// SVG path data for a path.
pub fn path_data(path: &Path) -> String {
    let mut d = String::new();
    let n = |v: f32| {
        let r = (v * 1000.0).round() / 1000.0;
        if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
    };
    for c in &path.commands {
        if !d.is_empty() {
            d.push(' ');
        }
        match c {
            PathCommand::MoveTo(p) => d.push_str(&format!("M{} {}", n(p.x), n(p.y))),
            PathCommand::LineTo(p) => d.push_str(&format!("L{} {}", n(p.x), n(p.y))),
            PathCommand::CubicTo { control1, control2, to } => d.push_str(&format!("C{} {} {} {} {} {}", n(control1.x), n(control1.y), n(control2.x), n(control2.y), n(to.x), n(to.y))),
            PathCommand::Close => d.push('Z'),
        }
    }
    d
}

/// A path's coordinates in command order (what a path morph interpolates).
fn path_numbers(path: &Path) -> Vec<f32> {
    let mut out = Vec::new();
    for c in &path.commands {
        match c {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => out.extend([p.x, p.y]),
            PathCommand::CubicTo { control1, control2, to } => out.extend([control1.x, control1.y, control2.x, control2.y, to.x, to.y]),
            PathCommand::Close => {}
        }
    }
    out
}

fn gradient(g: &GradientDef) -> Value {
    json!({
        "kind": match g.kind { GradientKind::Linear => "linear", _ => "radial" },
        "start": [g.start.x, g.start.y],
        "end": [g.end.x, g.end.y],
        "stops": g.stops.iter().map(|s| json!({ "offset": s.offset, "color": s.color })).collect::<Vec<_>>(),
    })
}

fn condition(c: &ConditionDef) -> Value {
    match c {
        ConditionDef::BoolIs { input, value } => json!({ "type": "bool", "input": input, "value": value }),
        ConditionDef::NumberGreaterThan { input, threshold } => json!({ "type": "gt", "input": input, "value": threshold }),
        ConditionDef::NumberLessThan { input, threshold } => json!({ "type": "lt", "input": input, "value": threshold }),
        ConditionDef::EnumIs { input, value } => json!({ "type": "enum", "input": input, "value": value }),
        ConditionDef::Trigger { input } => json!({ "type": "trigger", "input": input }),
    }
}

fn actions(list: &[EventAction], index: &HashMap<NodeId, usize>) -> Value {
    let target = |a: &EventAction| a.target.and_then(|t| index.get(&t)).map_or(Value::Null, |i| json!(i));
    json!(list
        .iter()
        .map(|a| match &a.action {
            ActionKind::SetInput { input, value } => json!({ "type": "setInput", "input": input, "value": value }),
            ActionKind::FireTrigger { input } => json!({ "type": "fireTrigger", "input": input }),
            ActionKind::ToggleInput { input } => json!({ "type": "toggleInput", "input": input }),
            ActionKind::SetProperty { property, value } => json!({ "type": "setProperty", "property": property, "value": value }),
            ActionKind::CallFunction { function, arg } => json!({ "type": "event", "name": function, "arg": arg }),
            ActionKind::SetVisible { visible } => json!({ "type": "setVisible", "target": target(a), "visible": visible }),
            ActionKind::ToggleVisible => json!({ "type": "toggleVisible", "target": target(a) }),
            other => json!({ "type": "unsupported", "action": format!("{other:?}") }),
        })
        .collect::<Vec<_>>())
}

fn kind_name(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Image(_) => "image",
        NodeKind::ParticleSystem(_) => "particles",
        NodeKind::Tilemap(_) => "tilemap",
        NodeKind::Instance(_) => "component instance",
        _ => "unsupported node",
    }
}

fn behavior_name(b: &NodeBehavior) -> String {
    let debug = format!("{b:?}");
    debug.split(['(', ' ', '{']).next().unwrap_or("component").to_string()
}

/// Standard base64 (fonts travel inside the JSON).
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use an1mbyt3_core::{Command, Node, ShapeNode};

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn exports_a_shape_with_its_path() {
        let mut document = Document::new();
        let Command::SetArtboardDeleted { artboard, .. } = document.apply(Command::AddArtboard { name: "Icon".into(), width: 64.0, height: 64.0 }) else { unreachable!() };
        let shape = Node::new("Box", None, NodeKind::Shape(ShapeNode { path: Path::rect(10.0, 10.0), fill_color: [1.0, 0.0, 0.0, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
        document.artboard_mut(artboard).unwrap().add_node(shape);
        let lite = export_lite(&document, Some(artboard)).unwrap();
        assert_eq!(lite["format"], LITE_FORMAT);
        let node = &lite["nodes"][0];
        assert_eq!(node["kind"], "shape");
        assert_eq!(node["path"], "M-5 -5 L5 -5 L5 5 L-5 5 Z");
    }
}
