//! Parses a real VRMA file when `VRMA_SAMPLE` points at one (skipped
//! otherwise — the sample isn't checked in).
#[test]
fn parses_a_real_vrma_when_given() {
    let Ok(path) = std::env::var("VRMA_SAMPLE") else { return };
    let bytes = std::fs::read(&path).expect("read sample");
    let data = an1mbyt3_geometry::parse_gltf(&bytes, &|_| None).expect("glTF");
    let json = an1mbyt3_geometry::vrm::gltf_json(&bytes);
    let clips = an1mbyt3_geometry::retarget::humanoid_clips(&data, json.as_ref()).expect("clips");
    for c in &clips {
        println!("{} {:.2}s bones={} hips={}", c.name, c.duration, c.bones.len(), c.hips.is_some());
        for k in 0..6 { let s = c.sample(c.duration * k as f32 / 6.0); for (b, q) in &s.bones { let (axis, angle) = q.to_axis_angle(); println!("  t{k} {b}: {:.1}° about {:.2?}", angle.to_degrees(), axis); } }
        let mid = c.sample(c.duration * 0.5);
        for (b, q) in &mid.bones {
            let (axis, angle) = q.to_axis_angle();
            println!("  {b}: {:.1}° about {:.2?}", angle.to_degrees(), axis);
        }
    }
    assert!(!clips.is_empty());
}
