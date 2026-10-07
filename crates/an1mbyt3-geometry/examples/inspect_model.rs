//! `cargo run -p an1mbyt3-geometry --example inspect_model -- path/to/model.glb`
//! Prints what `parse_gltf` sees: primitives, textures, bounds, clips.
fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect_model <file.glb|.gltf>");
    let bytes = std::fs::read(&path).expect("read model");
    let dir = std::path::Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let model = an1mbyt3_geometry::parse_gltf(&bytes, &|uri| std::fs::read(dir.join(uri)).ok()).expect("parse");
    let tris: usize = model.primitives.iter().map(|p| p.mesh.indices.len() / 3).sum();
    println!("{path}: {} nodes, {} primitives, {tris} triangles, {} images ({} bytes), bounds {:?}", model.nodes.len(), model.primitives.len(), model.images.len(), model.images.iter().map(|i| i.len()).sum::<usize>(), model.bounds);
    for n in &model.nodes {
        println!("  node {:?} parent {:?} t {:?}", n.name, n.parent, n.translation);
    }
    for c in &model.animations {
        println!("  clip {:?} {:.2}s, {} channels", c.name, c.duration, c.channels.len());
    }
}
