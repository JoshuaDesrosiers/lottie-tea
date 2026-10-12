//! The post-process shaders must parse and validate (naga, the front end
//! wgpu uses) — a broken effect branch otherwise only shows as a GPU error
//! at runtime.

fn validate(name: &str, source: &str) {
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(source)));
}

#[test]
fn post_shaders_validate() {
    validate("post_uber.wgsl", include_str!("../src/renderer/post_uber.wgsl"));
    validate("post_process.wgsl", include_str!("../src/post_process.wgsl"));
    validate("post_bloom.wgsl", include_str!("../src/renderer/post_bloom.wgsl"));
}
