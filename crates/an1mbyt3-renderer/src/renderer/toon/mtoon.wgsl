// Toon material, MToon style (VRM's standard look): the lit colour and the
// shade colour split at a soft-to-hard terminator, a rim light, emission.
fn material_fragment(in: VertexOutput) -> vec4<f32> {
    let p = uniforms.material;
    let base = tn_base(in);
    let n = tn_normal(in);
    let to_eye = normalize(scene.camera_pos.xyz - in.world);
    let shade = base.rgb * p[0].rgb;
    let lit = tn_term(dot(n, -scene.sun_dir.xyz), p[0].w, p[1].w);
    var rgb = mix(shade, base.rgb, lit) * max(scene.sun_color.rgb, vec3<f32>(p[5].w));
    rgb += base.rgb * scene.ambient.rgb * 0.35;
    // Point lights: the same terminator, added on top.
    let count = u32(scene.counts.x);
    for (var i = 0u; i < MAX_POINT_LIGHTS; i = i + 1u) {
        if i >= count || p[5].y <= 0.0 {
            break;
        }
        let pl = tn_point(i, in.world);
        rgb += base.rgb * pl[1].rgb * tn_term(dot(n, pl[0].xyz), p[0].w, p[1].w) * p[5].y;
    }
    let fresnel = clamp(pow(1.0 - clamp(dot(n, to_eye), 0.0, 1.0), max(p[2].w, 0.001)) + p[3].w, 0.0, 1.0);
    rgb += p[1].rgb * fresnel;
    rgb += p[2].rgb;
    return vec4<f32>(tn_fog(rgb, in.world), base.a);
}
