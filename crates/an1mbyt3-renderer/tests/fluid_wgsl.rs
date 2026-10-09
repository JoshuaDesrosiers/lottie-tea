//! The screen-space fluid composite shader parses and validates (naga),
//! so a typo can't turn into a GPU validation panic at run time.

#[test]
fn fluid_composite_shader_validates() {
    let source = include_str!("../src/renderer/fluid3d.wgsl");
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
}
