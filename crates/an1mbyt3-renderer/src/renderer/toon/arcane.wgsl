// Toon material, Arcane style: a painted look. Light falls in a few flat
// bands whose edges are broken up by brush-stroke noise; shadows go cool
// (the shade colour) and get crosshatched, lit areas warm up (highlight),
// a canvas grain sits over everything and the rim light is painted on in
// strokes rather than as a smooth glow.
fn arc_bands(x: f32, bands: f32, toony: f32) -> f32 {
    let count = max(bands, 2.0);
    let y = clamp(x, 0.0, 0.9999) * count;
    let w = mix(0.5, 0.02, clamp(toony, 0.0, 1.0));
    return clamp((floor(y) + smoothstep(1.0 - w, 1.0, fract(y))) / (count - 1.0), 0.0, 1.0);
}

fn material_fragment(in: VertexOutput) -> vec4<f32> {
    let p = uniforms.material;
    let base = tn_base(in);
    let n = tn_normal(in);
    let to_eye = normalize(scene.camera_pos.xyz - in.world);
    let scale = max(p[4].x, 0.005);
    // Brush strokes: smooth noise stretched along one axis so it reads as
    // long dabs of paint, plus a finer octave for bristle texture.
    let q = in.world / scale;
    let broad = tn_noise(q * vec3<f32>(1.0, 2.2, 1.0));
    let fine = tn_noise(q * vec3<f32>(3.1, 7.5, 3.1) + 17.0);
    let brush = broad * 0.75 + fine * 0.25;
    let wobble = (brush - 0.5) * p[4].y;

    // Sun: half-Lambert, shifted, brush-broken, banded.
    let ndl = dot(n, -scene.sun_dir.xyz) * 0.5 + 0.5;
    var level = arc_bands(ndl + p[0].w + wobble, p[5].z, p[1].w);
    // Point lights brighten the band too.
    let count = u32(scene.counts.x);
    var point = vec3<f32>(0.0);
    for (var i = 0u; i < MAX_POINT_LIGHTS; i = i + 1u) {
        if i >= count || p[5].y <= 0.0 {
            break;
        }
        let pl = tn_point(i, in.world);
        let t = arc_bands(dot(n, pl[0].xyz) * 0.5 + 0.5 + wobble, p[5].z, p[1].w);
        point += pl[1].rgb * t * p[5].y;
    }

    let shadow = base.rgb * p[0].rgb;
    let light = base.rgb * (vec3<f32>(1.0) + p[3].rgb);
    var rgb = mix(shadow, light, level) * max(scene.sun_color.rgb, vec3<f32>(p[5].w));
    rgb += base.rgb * (scene.ambient.rgb * 0.3 + point);

    // Paint: each dab a slightly different value, like layered strokes.
    rgb *= 1.0 + (brush - 0.5) * 0.3 * p[4].y;

    // Ink crosshatching in the dark, in screen space (even pen lines at any
    // distance); a second set crosses in the deepest shade.
    let dark = 1.0 - level;
    if p[4].z > 0.0 && dark > 0.01 {
        let h = in.clip.xy / 6.0;
        let l1 = 1.0 - smoothstep(0.1, 0.24, abs(fract(h.x + h.y + brush * 0.8) - 0.5));
        let l2 = 1.0 - smoothstep(0.1, 0.24, abs(fract(h.x - h.y - brush * 0.8) - 0.5));
        let lines = max(l1 * smoothstep(0.35, 0.7, dark), l2 * smoothstep(0.7, 0.98, dark));
        rgb *= 1.0 - lines * p[4].z * 0.5;
    }

    // Painted rim: the fresnel edge, cut into strokes.
    let fres = clamp(pow(1.0 - clamp(dot(n, to_eye), 0.0, 1.0), max(p[2].w, 0.001)) + p[3].w, 0.0, 1.0);
    rgb += p[1].rgb * fres * smoothstep(0.35, 0.65, brush + fres * 0.4);

    // Canvas grain.
    let grain = tn_noise(vec3<f32>(in.clip.xy * 0.7, 0.0)) - 0.5;
    rgb *= 1.0 + grain * p[4].w * 0.4;

    rgb += p[2].rgb;
    return vec4<f32>(tn_fog(max(rgb, vec3<f32>(0.0)), in.world), base.a);
}
