//! Writes a small real `.tea` file to disk — a fixture for manually exercising
//! `an1mbyt3-cli compile`/`validate` end-to-end on an actual file, since
//! `.tea`'s JSON shape (particularly `SlotMap`'s own serialization) isn't
//! something to hand-write by guessing (see `apps/editor/src-tauri/src/dto.rs`'s
//! doc comment on why). Run with `cargo run -p an1mbyt3-format --example write_sample`.

use an1mbyt3_core::{Artboard, Document, Node, NodeKind, Path, ShapeNode};

fn main() {
    let mut document = Document::new();
    let artboard_id = document.add_artboard(Artboard::new("Main", 400.0, 400.0));
    let artboard = document.artboard_mut(artboard_id).unwrap();
    artboard.add_node(Node::new(
        "Square",
        None,
        NodeKind::Shape(ShapeNode { path: Path::rect(120.0, 120.0), fill_color: [0.85, 0.2, 0.35, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
    ));
    artboard.add_node(Node::new(
        "Accent",
        None,
        NodeKind::Shape(ShapeNode { path: Path::rect(60.0, 200.0), fill_color: [0.3, 0.55, 0.95, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None}),
    ));

    let json = an1mbyt3_format::save_tea(&document).expect("save_tea should never fail for a well-formed Document");
    std::fs::write("sample.tea", &json).expect("failed to write sample.tea");
    println!("wrote sample.tea ({} bytes)", json.len());
}
