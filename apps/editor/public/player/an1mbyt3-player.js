//#region src/game/googleFonts.ts
var e = (e, t, n = [400, 700]) => ({
	family: e,
	id: e.toLowerCase().replace(/\s+/g, "-"),
	category: t,
	weights: n,
	subset: "latin"
}), t = [
	e("Inter", "sans-serif"),
	e("Roboto", "sans-serif"),
	e("Open Sans", "sans-serif"),
	e("Montserrat", "sans-serif"),
	e("Poppins", "sans-serif"),
	e("Lato", "sans-serif"),
	e("Nunito", "sans-serif"),
	e("Rubik", "sans-serif"),
	e("Work Sans", "sans-serif"),
	e("Outfit", "sans-serif"),
	e("Fredoka", "sans-serif"),
	e("Russo One", "sans-serif", [400]),
	e("Oswald", "sans-serif"),
	e("Bebas Neue", "display", [400]),
	e("Anton", "sans-serif", [400]),
	e("Bangers", "display", [400]),
	e("Luckiest Guy", "display", [400]),
	e("Lilita One", "display", [400]),
	e("Bungee", "display", [400]),
	e("Black Ops One", "display", [400]),
	e("Orbitron", "sans-serif"),
	e("Audiowide", "display", [400]),
	e("Press Start 2P", "display", [400]),
	e("VT323", "monospace", [400]),
	e("Silkscreen", "display"),
	e("Pixelify Sans", "display"),
	e("Creepster", "display", [400]),
	e("Cinzel", "serif"),
	e("Playfair Display", "serif"),
	e("Merriweather", "serif"),
	e("Lora", "serif"),
	e("EB Garamond", "serif"),
	e("Pacifico", "handwriting", [400]),
	e("Lobster", "display", [400]),
	e("Caveat", "handwriting"),
	e("Permanent Marker", "handwriting", [400]),
	e("Indie Flower", "handwriting", [400]),
	e("Dancing Script", "handwriting"),
	e("Fira Code", "monospace"),
	e("JetBrains Mono", "monospace"),
	e("Space Mono", "monospace")
], n = [
	"Rounded",
	"Outlined",
	"Sharp"
];
function r(e) {
	return `Material Symbols ${e}`;
}
function i(e) {
	return !!e && /^Material Symbols /.test(e);
}
var a = /* @__PURE__ */ new Set();
function o(e) {
	if (a.has(e) || typeof document > "u") return;
	a.add(e);
	let t = document.createElement("link");
	t.rel = "stylesheet", t.href = e, t.dataset.an1Font = "1", document.head.appendChild(t);
}
function s(e, t = {}) {
	let n = encodeURIComponent(e).replace(/%20/g, "+"), r = t.weights ?? (i(e) ? null : [400, 700]);
	return `https://fonts.googleapis.com/css2?family=${n}${i(e) ? ":opsz,wght,FILL,GRAD@20..48,100..700,0..1,-50..200" : r && r.length > 1 ? `:wght@${r.join(";")}` : ""}&display=swap${t.text ? `&text=${encodeURIComponent(t.text)}` : ""}`;
}
function c(e, t) {
	let n = m(e);
	o(s(e, { weights: t ?? (n && n.weights.length < 2 ? [] : void 0) }));
}
var l = n.map((e) => `.material-symbols-${e.toLowerCase()}{font-family:'${r(e)}';font-weight:normal;font-style:normal;font-size:24px;line-height:1;letter-spacing:normal;text-transform:none;display:inline-block;white-space:nowrap;word-wrap:normal;direction:ltr;-webkit-font-feature-settings:'liga';font-feature-settings:'liga';-webkit-font-smoothing:antialiased;vertical-align:middle}`).join("\n"), u = /* @__PURE__ */ new Set([
	"arial",
	"helvetica",
	"helvetica neue",
	"times",
	"times new roman",
	"georgia",
	"verdana",
	"tahoma",
	"trebuchet ms",
	"courier",
	"courier new",
	"segoe ui",
	"impact",
	"comic sans ms",
	"consolas",
	"menlo",
	"monaco",
	"-apple-system",
	"blinkmacsystemfont"
]), d = /* @__PURE__ */ new Set([
	"serif",
	"sans-serif",
	"monospace",
	"cursive",
	"fantasy",
	"system-ui",
	"inherit",
	"initial",
	"unset",
	"ui-sans-serif",
	"ui-serif",
	"ui-monospace",
	"emoji",
	"math"
]);
function f(...e) {
	for (let t of e) if (t) {
		for (let e of t.matchAll(/font-family\s*:\s*([^;}"<]+|"[^"]*")/gi)) for (let t of e[1].split(",")) {
			let e = t.trim().replace(/^['"]|['"]$/g, "").replace(/&quot;|&#39;/g, "").trim();
			if (!e || d.has(e.toLowerCase()) || u.has(e.toLowerCase()) || e.startsWith("var(")) continue;
			let n = m(e);
			c(n?.family ?? e, n ? void 0 : []);
		}
		for (let e of t.matchAll(/material-symbols-(rounded|outlined|sharp)/gi)) c(r(e[1][0].toUpperCase() + e[1].slice(1).toLowerCase()));
	}
}
var p = null;
function m(e) {
	let n = e.toLowerCase();
	return (p ?? t).find((e) => e.family.toLowerCase() === n);
}
//#endregion
//#region src/wasm-pkg/an1mbyt3_wasm.js
var h = class e {
	static __wrap(t) {
		let n = Object.create(e.prototype);
		return n.__wbg_ptr = t, we.register(n, n.__wbg_ptr, n), n;
	}
	__destroy_into_raw() {
		let e = this.__wbg_ptr;
		return this.__wbg_ptr = 0, we.unregister(this), e;
	}
	free() {
		let e = this.__destroy_into_raw();
		G.__wbg_wasmboneik_free(e, 0);
	}
	static create(e) {
		return G.wasmboneik_create(e);
	}
	render() {
		let e = G.wasmboneik_render(this.__wbg_ptr);
		if (e[1]) throw U(e[0]);
	}
	resize(e, t) {
		G.wasmboneik_resize(this.__wbg_ptr, e, t);
	}
	set_target(e, t) {
		G.wasmboneik_set_target(this.__wbg_ptr, e, t);
	}
};
Symbol.dispose && (h.prototype[Symbol.dispose] = h.prototype.free);
var g = class e {
	static __wrap(t) {
		let n = Object.create(e.prototype);
		return n.__wbg_ptr = t, Te.register(n, n.__wbg_ptr, n), n;
	}
	__destroy_into_raw() {
		let e = this.__wbg_ptr;
		return this.__wbg_ptr = 0, Te.unregister(this), e;
	}
	free() {
		let e = this.__destroy_into_raw();
		G.__wbg_wasmeditor_free(e, 0);
	}
	add_animated_drawing(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_add_animated_drawing(this.__wbg_ptr, n, r, t);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	add_animation(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_add_animation(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	add_artboard(e, t, n) {
		let r = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_artboard(this.__wbg_ptr, r, i, t, n);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_bitmap_drawing(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = V(t, G.__wbindgen_malloc), a = W;
		return G.wasmeditor_add_bitmap_drawing(this.__wbg_ptr, n, r, i, a);
	}
	add_blank_shape_node(e, t) {
		let n = G.wasmeditor_add_blank_shape_node(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	add_bone_node(e, t, n) {
		let r = G.wasmeditor_add_bone_node(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	add_camera_node(e, t) {
		let n = G.wasmeditor_add_camera_node(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	add_component(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_component(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_component_variant(e, t, n) {
		let r = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_component_variant(this.__wbg_ptr, e, r, i, n);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_demo_hover_animation(e, t, n) {
		let r = G.wasmeditor_add_demo_hover_animation(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	add_drawing(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_add_drawing(this.__wbg_ptr, n, r, i, a);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	add_drawing_cel_image(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = V(t, G.__wbindgen_malloc), a = W;
		return G.wasmeditor_add_drawing_cel_image(this.__wbg_ptr, n, r, i, a);
	}
	add_font(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = V(t, G.__wbindgen_malloc), a = W, o = G.wasmeditor_add_font(this.__wbg_ptr, n, r, i, a);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	add_ik_target(e, t, n) {
		let r = G.wasmeditor_add_ik_target(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	add_image_node(e, t, n, r, i) {
		let a = G.wasmeditor_add_image_node(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_instance_node(e, t, n) {
		let r = G.wasmeditor_add_instance_node(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	add_keyframe(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_add_keyframe(this.__wbg_ptr, e, t, i, a, r);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	add_light2d_node(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_light2d_node(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_material(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W, r = G.wasmeditor_add_material(this.__wbg_ptr, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	add_motion_path_keyframe(e, t, n, r) {
		let i = G.wasmeditor_add_motion_path_keyframe(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	add_node3d(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_node3d(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_particle_system_node(e, t) {
		let n = G.wasmeditor_add_particle_system_node(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	add_property(e, t, n) {
		let r = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_add_property(this.__wbg_ptr, e, r, i, a, o);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	add_script(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W, r = G.wasmeditor_add_script(this.__wbg_ptr, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	add_shape_node(e, t, n) {
		let r = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_shape_node(this.__wbg_ptr, e, r, i, n);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_state(e, t, n, r, i, a) {
		let o = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = H(i, G.__wbindgen_malloc, G.__wbindgen_realloc), l = W, u = G.wasmeditor_add_state(this.__wbg_ptr, e, t, o, s, r, c, l, a);
		if (u[2]) throw U(u[1]);
		return U(u[0]);
	}
	add_state_machine(e, t, n) {
		let r = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_add_state_machine(this.__wbg_ptr, e, r, i, n);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	add_state_machine_input(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = G.wasmeditor_add_state_machine_input(this.__wbg_ptr, e, t, i, a, o, s);
		if (c[2]) throw U(c[1]);
		return U(c[0]);
	}
	add_state_machine_layer(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_add_state_machine_layer(this.__wbg_ptr, e, t, i, a, r);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	add_text_file(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_add_text_file(this.__wbg_ptr, n, r, i, a);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	add_text_node(e, t) {
		let n = G.wasmeditor_add_text_node(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	add_tilemap_node(e, t, n, r, i, a, o, s, c) {
		let l = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), u = W, d = V(r, G.__wbindgen_malloc), f = W;
		return G.wasmeditor_add_tilemap_node(this.__wbg_ptr, e, t, l, u, d, f, i, a, o, s, c);
	}
	add_trail_node(e, t) {
		let n = G.wasmeditor_add_trail_node(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	add_transition(e, t, n, r, i, a, o, s, c) {
		let l = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), u = W, d = H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), f = W, p = H(a, G.__wbindgen_malloc, G.__wbindgen_realloc), m = W, h = G.wasmeditor_add_transition(this.__wbg_ptr, e, t, l, u, d, f, i, p, m, o, s, c);
		if (h[2]) throw U(h[1]);
		return U(h[0]);
	}
	add_ui_node(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_add_ui_node(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	apply_node_writes(e, t) {
		let n = Xe(t, G.__wbindgen_malloc), r = W, i = G.wasmeditor_apply_node_writes(this.__wbg_ptr, e, n, r);
		if (i[1]) throw U(i[0]);
	}
	apply_prefab_instance(e, t) {
		let n = G.wasmeditor_apply_prefab_instance(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	audio_bytes(e) {
		let t = G.wasmeditor_audio_bytes(this.__wbg_ptr, e);
		if (t[3]) throw U(t[2]);
		var n = N(t[0], t[1]).slice();
		return G.__wbindgen_free(t[0], t[1] * 1, 1), n;
	}
	audio_list() {
		let e = G.wasmeditor_audio_list(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	bind_sprite_skin(e, t, n, r, i) {
		let a = G.wasmeditor_bind_sprite_skin(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	bring_node_to_front(e, t) {
		let n = G.wasmeditor_bring_node_to_front(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	burst_particles_3d(e, t, n) {
		let r = G.wasmeditor_burst_particles_3d(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return r[0] !== 0;
	}
	cloth_drag(e, t, n) {
		let r = G.wasmeditor_cloth_drag(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	cloth_grab(e, t, n, r) {
		let i = G.wasmeditor_cloth_grab(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return i[0] !== 0;
	}
	cloth_release(e) {
		let t = G.wasmeditor_cloth_release(this.__wbg_ptr, e);
		if (t[1]) throw U(t[0]);
	}
	cloth_reset(e) {
		let t = G.wasmeditor_cloth_reset(this.__wbg_ptr, e);
		if (t[1]) throw U(t[0]);
	}
	cloth_tear(e, t, n, r) {
		let i = G.wasmeditor_cloth_tear(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return i[0] >>> 0;
	}
	cloth_wind(e, t) {
		var n = B(t) ? 0 : Ye(t, G.__wbindgen_malloc), r = W;
		let i = G.wasmeditor_cloth_wind(this.__wbg_ptr, e, n, r);
		if (i[1]) throw U(i[0]);
	}
	commit_bone_pose(e, t, n) {
		let r = Ye(n, G.__wbindgen_malloc), i = W, a = G.wasmeditor_commit_bone_pose(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	commit_keyframe_time(e, t, n, r, i, a) {
		let o = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = G.wasmeditor_commit_keyframe_time(this.__wbg_ptr, e, t, o, s, r, i, a);
		if (c[2]) throw U(c[1]);
		return U(c[0]);
	}
	commit_node_anchor(e, t, n, r, i, a, o, s, c, l) {
		let u = G.wasmeditor_commit_node_anchor(this.__wbg_ptr, e, t, n, r, i, a, o, s, c, l);
		if (u[2]) throw U(u[1]);
		return U(u[0]);
	}
	commit_node_rotation(e, t, n, r, i) {
		let a = G.wasmeditor_commit_node_rotation(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	commit_node_scale(e, t, n, r, i, a, o, s, c, l, u) {
		let d = G.wasmeditor_commit_node_scale(this.__wbg_ptr, e, t, n, r, i, a, o, s, c, l, u);
		if (d[2]) throw U(d[1]);
		return U(d[0]);
	}
	commit_node_translation(e, t, n, r, i, a) {
		let o = G.wasmeditor_commit_node_translation(this.__wbg_ptr, e, t, n, r, i, a);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	static create(e) {
		return G.wasmeditor_create(e);
	}
	create_prefab(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_create_prefab(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	default_component(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W, r = G.wasmeditor_default_component(this.__wbg_ptr, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	delete_animation(e, t) {
		let n = G.wasmeditor_delete_animation(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	delete_artboard(e) {
		let t = G.wasmeditor_delete_artboard(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_audio(e) {
		let t = G.wasmeditor_delete_audio(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_component(e) {
		let t = G.wasmeditor_delete_component(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_drawing(e) {
		let t = G.wasmeditor_delete_drawing(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_material(e) {
		let t = G.wasmeditor_delete_material(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_node(e, t) {
		let n = G.wasmeditor_delete_node(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	delete_nodes(e, t) {
		let n = G.wasmeditor_delete_nodes(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	delete_prefab(e) {
		let t = G.wasmeditor_delete_prefab(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_property(e) {
		let t = G.wasmeditor_delete_property(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_script(e) {
		let t = G.wasmeditor_delete_script(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	delete_state(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_delete_state(this.__wbg_ptr, e, t, i, a, r);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	delete_state_machine(e, t) {
		let n = G.wasmeditor_delete_state_machine(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	delete_state_machine_layer(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_delete_state_machine_layer(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	delete_text_file(e) {
		let t = G.wasmeditor_delete_text_file(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	destroy_node(e, t) {
		let n = G.wasmeditor_destroy_node(this.__wbg_ptr, e, t);
		if (n[1]) throw U(n[0]);
	}
	document_instance() {
		return G.wasmeditor_document_instance(this.__wbg_ptr);
	}
	document_revision() {
		return G.wasmeditor_document_revision(this.__wbg_ptr);
	}
	document_summary() {
		let e = G.wasmeditor_document_summary(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	document_summary_json() {
		let e, t;
		try {
			let i = G.wasmeditor_document_summary_json(this.__wbg_ptr);
			var n = i[0], r = i[1];
			if (i[3]) throw n = 0, r = 0, U(i[2]);
			return e = n, t = r, L(n, r);
		} finally {
			G.__wbindgen_free(e, t, 1);
		}
	}
	drawing_content(e) {
		let t = G.wasmeditor_drawing_content(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	drawing_svg(e) {
		let t = G.wasmeditor_drawing_svg(this.__wbg_ptr, e);
		if (t[3]) throw U(t[2]);
		let n;
		return t[0] !== 0 && (n = L(t[0], t[1]), G.__wbindgen_free(t[0], t[1] * 1, 1)), n;
	}
	duplicate_nodes(e, t) {
		let n = G.wasmeditor_duplicate_nodes(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	emit_particles(e, t, n) {
		let r = G.wasmeditor_emit_particles(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	fire_trigger(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W;
		G.wasmeditor_fire_trigger(this.__wbg_ptr, t, n);
	}
	frame_stats() {
		let e = G.wasmeditor_frame_stats(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	get_image_source(e) {
		let t = G.wasmeditor_get_image_source(this.__wbg_ptr, e);
		if (t[3]) throw U(t[2]);
		var n = N(t[0], t[1]).slice();
		return G.__wbindgen_free(t[0], t[1] * 1, 1), n;
	}
	glass_intact(e, t) {
		let n = G.wasmeditor_glass_intact(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return n[0];
	}
	glass_raycast(e, t, n, r, i, a, o, s) {
		let c = G.wasmeditor_glass_raycast(this.__wbg_ptr, e, t, n, r, i, a, o, s);
		if (c[2]) throw U(c[1]);
		return U(c[0]);
	}
	glass_reset(e, t) {
		let n = G.wasmeditor_glass_reset(this.__wbg_ptr, e, t);
		if (n[1]) throw U(n[0]);
	}
	glass_shatter(e, t, n, r, i, a, o, s, c, l) {
		let u = G.wasmeditor_glass_shatter(this.__wbg_ptr, e, t, n, r, i, a, o, s, c, l);
		if (u[2]) throw U(u[1]);
		return U(u[0]);
	}
	group_nodes(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_group_nodes(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	import_audio(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = V(t, G.__wbindgen_malloc), a = W, o = G.wasmeditor_import_audio(this.__wbg_ptr, n, r, i, a);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	import_image(e, t, n) {
		let r = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = V(n, G.__wbindgen_malloc), o = W;
		return G.wasmeditor_import_image(this.__wbg_ptr, e, r, i, a, o);
	}
	import_image_asset(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = V(t, G.__wbindgen_malloc), a = W;
		return G.wasmeditor_import_image_asset(this.__wbg_ptr, n, r, i, a);
	}
	import_model(e, t, n, r, i, a) {
		let o = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = V(n, G.__wbindgen_malloc), l = W, u = Ze(r, G.__wbindgen_malloc), d = W, f = V(i, G.__wbindgen_malloc), p = W, m = Je(a, G.__wbindgen_malloc), h = W, g = G.wasmeditor_import_model(this.__wbg_ptr, e, o, s, c, l, u, d, f, p, m, h);
		if (g[2]) throw U(g[1]);
		return U(g[0]);
	}
	import_svg(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_import_svg(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	instantiate_prefab(e, t, n, r, i) {
		let a = G.wasmeditor_instantiate_prefab(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	load_3d_demo() {
		return G.wasmeditor_load_3d_demo(this.__wbg_ptr);
	}
	load_boom_demo() {
		return G.wasmeditor_load_boom_demo(this.__wbg_ptr);
	}
	load_cloth_demo() {
		return G.wasmeditor_load_cloth_demo(this.__wbg_ptr);
	}
	load_dungeon_demo() {
		return G.wasmeditor_load_dungeon_demo(this.__wbg_ptr);
	}
	load_fps_demo() {
		return G.wasmeditor_load_fps_demo(this.__wbg_ptr);
	}
	load_geometry_dash_demo(e) {
		let t = G.wasmeditor_load_geometry_dash_demo(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	load_goo_demo() {
		return G.wasmeditor_load_goo_demo(this.__wbg_ptr);
	}
	load_goo_shapes_demo() {
		return G.wasmeditor_load_goo_shapes_demo(this.__wbg_ptr);
	}
	load_html_demo() {
		return G.wasmeditor_load_html_demo(this.__wbg_ptr);
	}
	load_lan_demo() {
		return G.wasmeditor_load_lan_demo(this.__wbg_ptr);
	}
	load_leer_demo() {
		return G.wasmeditor_load_leer_demo(this.__wbg_ptr);
	}
	load_liquid_demo() {
		return G.wasmeditor_load_liquid_demo(this.__wbg_ptr);
	}
	load_match3_demo() {
		return G.wasmeditor_load_match3_demo(this.__wbg_ptr);
	}
	load_ninja_demo() {
		return G.wasmeditor_load_ninja_demo(this.__wbg_ptr);
	}
	load_physics3d_demo() {
		return G.wasmeditor_load_physics3d_demo(this.__wbg_ptr);
	}
	load_physics_demo() {
		let e = G.wasmeditor_load_physics_demo(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	load_project(e, t, n, r) {
		let i = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = Ze(t, G.__wbindgen_malloc), s = W, c = Je(n, G.__wbindgen_malloc), l = W, u = V(r, G.__wbindgen_malloc), d = W;
		return G.wasmeditor_load_project(this.__wbg_ptr, i, a, o, s, c, l, u, d);
	}
	load_responsive_demo() {
		return G.wasmeditor_load_responsive_demo(this.__wbg_ptr);
	}
	load_rig_demo() {
		return G.wasmeditor_load_rig_demo(this.__wbg_ptr);
	}
	load_shatter_demo() {
		return G.wasmeditor_load_shatter_demo(this.__wbg_ptr);
	}
	load_stress_demo() {
		return G.wasmeditor_load_stress_demo(this.__wbg_ptr);
	}
	load_tactics_demo() {
		return G.wasmeditor_load_tactics_demo(this.__wbg_ptr);
	}
	load_tea(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W;
		return G.wasmeditor_load_tea(this.__wbg_ptr, t, n);
	}
	load_teab(e) {
		let t = V(e, G.__wbindgen_malloc), n = W;
		return G.wasmeditor_load_teab(this.__wbg_ptr, t, n);
	}
	load_torture_demo() {
		return G.wasmeditor_load_torture_demo(this.__wbg_ptr);
	}
	load_trees_demo() {
		return G.wasmeditor_load_trees_demo(this.__wbg_ptr);
	}
	load_ui_demo() {
		return G.wasmeditor_load_ui_demo(this.__wbg_ptr);
	}
	load_voxel_demo() {
		return G.wasmeditor_load_voxel_demo(this.__wbg_ptr);
	}
	make_ragdoll(e, t, n, r, i) {
		let a = G.wasmeditor_make_ragdoll(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	material_graph(e) {
		let t = G.wasmeditor_material_graph(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	material_status(e) {
		let t = G.wasmeditor_material_status(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	model_clips(e) {
		let t = G.wasmeditor_model_clips(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	move_node_forward(e, t) {
		let n = G.wasmeditor_move_node_forward(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	node_local_bounds(e, t) {
		let n = G.wasmeditor_node_local_bounds(this.__wbg_ptr, e, t);
		if (n[3]) throw U(n[2]);
		var r = k(n[0], n[1]).slice();
		return G.__wbindgen_free(n[0], n[1] * 4, 4), r;
	}
	physics3d_add_force(e, t, n, r, i, a) {
		let o = G.wasmeditor_physics3d_add_force(this.__wbg_ptr, e, t, n, r, i, a);
		if (o[1]) throw U(o[0]);
	}
	physics3d_angular_velocity(e, t) {
		let n = G.wasmeditor_physics3d_angular_velocity(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	physics3d_raycast(e, t, n, r, i, a, o, s, c) {
		let l = G.wasmeditor_physics3d_raycast(this.__wbg_ptr, e, t, n, r, i, a, o, s, c);
		if (l[2]) throw U(l[1]);
		return U(l[0]);
	}
	physics3d_set_velocity(e, t, n, r, i, a) {
		let o = G.wasmeditor_physics3d_set_velocity(this.__wbg_ptr, e, t, n, r, i, a);
		if (o[1]) throw U(o[0]);
	}
	physics3d_velocity(e, t) {
		let n = G.wasmeditor_physics3d_velocity(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	physics_add_force(e, t, n, r, i) {
		let a = G.wasmeditor_physics_add_force(this.__wbg_ptr, e, t, n, r, i);
		if (a[1]) throw U(a[0]);
	}
	physics_angular_velocity(e, t) {
		let n = G.wasmeditor_physics_angular_velocity(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	physics_events(e) {
		let t = G.wasmeditor_physics_events(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	physics_raycast(e, t, n, r, i, a, o) {
		let s = G.wasmeditor_physics_raycast(this.__wbg_ptr, e, t, n, r, i, a, o);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	physics_set_angular_velocity(e, t, n) {
		let r = G.wasmeditor_physics_set_angular_velocity(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	physics_set_velocity(e, t, n, r) {
		let i = G.wasmeditor_physics_set_velocity(this.__wbg_ptr, e, t, n, r);
		if (i[1]) throw U(i[0]);
	}
	physics_velocity(e, t) {
		let n = G.wasmeditor_physics_velocity(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	pick_node_at(e, t, n) {
		let r = G.wasmeditor_pick_node_at(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	place_drawing(e, t, n, r, i) {
		let a = G.wasmeditor_place_drawing(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	playground_view(e) {
		let t = G.wasmeditor_playground_view(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	prefab_instance_root(e, t) {
		let n = G.wasmeditor_prefab_instance_root(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	preview_body_type(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_preview_body_type(this.__wbg_ptr, e, t, i, a, r);
		if (o[1]) throw U(o[0]);
	}
	preview_bone_aim(e, t, n, r) {
		let i = G.wasmeditor_preview_bone_aim(this.__wbg_ptr, e, t, n, r);
		if (i[1]) throw U(i[0]);
	}
	preview_bone_ik(e, t, n, r, i) {
		let a = G.wasmeditor_preview_bone_ik(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	preview_break_joint(e, t) {
		let n = G.wasmeditor_preview_break_joint(this.__wbg_ptr, e, t);
		if (n[1]) throw U(n[0]);
	}
	preview_component_field(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), c = W, l = H(i, G.__wbindgen_malloc, G.__wbindgen_realloc), u = W, d = G.wasmeditor_preview_component_field(this.__wbg_ptr, e, t, a, o, s, c, l, u);
		if (d[2]) throw U(d[1]);
		return d[0] !== 0;
	}
	preview_fill_color(e, t, n) {
		let r = G.wasmeditor_preview_fill_color(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_keyframe_time(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_preview_keyframe_time(this.__wbg_ptr, e, t, a, o, r, i);
		if (s[1]) throw U(s[0]);
	}
	preview_light(e, t, n, r) {
		let i = G.wasmeditor_preview_light(this.__wbg_ptr, e, t, n, r);
		if (i[1]) throw U(i[0]);
	}
	preview_node3d_animation(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = H(i, G.__wbindgen_malloc, G.__wbindgen_realloc), c = W, l = G.wasmeditor_preview_node3d_animation(this.__wbg_ptr, e, t, a, o, r, s, c);
		if (l[1]) throw U(l[0]);
	}
	preview_node3d_transform(e, t, n) {
		let r = Ye(n, G.__wbindgen_malloc), i = W, a = G.wasmeditor_preview_node3d_transform(this.__wbg_ptr, e, t, r, i);
		if (a[1]) throw U(a[0]);
	}
	preview_node_anchor(e, t, n, r, i, a) {
		let o = G.wasmeditor_preview_node_anchor(this.__wbg_ptr, e, t, n, r, i, a);
		if (o[1]) throw U(o[0]);
	}
	preview_node_image(e, t, n) {
		let r = G.wasmeditor_preview_node_image(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_node_path(e, t, n) {
		let r = G.wasmeditor_preview_node_path(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_node_rotation(e, t, n) {
		let r = G.wasmeditor_preview_node_rotation(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_node_scale(e, t, n, r, i, a) {
		let o = G.wasmeditor_preview_node_scale(this.__wbg_ptr, e, t, n, r, i, a);
		if (o[1]) throw U(o[0]);
	}
	preview_node_translation(e, t, n, r) {
		let i = G.wasmeditor_preview_node_translation(this.__wbg_ptr, e, t, n, r);
		if (i[1]) throw U(i[0]);
	}
	preview_node_visible(e, t, n) {
		let r = G.wasmeditor_preview_node_visible(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_node_z_index(e, t, n) {
		let r = G.wasmeditor_preview_node_z_index(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_shape_path(e, t, n) {
		let r = G.wasmeditor_preview_shape_path(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	preview_text_content(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_preview_text_content(this.__wbg_ptr, e, t, r, i);
		if (a[1]) throw U(a[0]);
	}
	preview_tilemap_tiles(e, t, n) {
		let r = Je(n, G.__wbindgen_malloc), i = W, a = G.wasmeditor_preview_tilemap_tiles(this.__wbg_ptr, e, t, r, i);
		if (a[1]) throw U(a[0]);
	}
	static project_asset_paths(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W, r = G.wasmeditor_project_asset_paths(t, n);
		if (r[3]) throw U(r[2]);
		var i = je(r[0], r[1]);
		return G.__wbindgen_free(r[0], r[1] * 4, 4), i;
	}
	project_file_index(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W, r = G.wasmeditor_project_file_index(this.__wbg_ptr, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	redo() {
		let e = G.wasmeditor_redo(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	remove_keyframe(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_remove_keyframe(this.__wbg_ptr, e, t, i, a, r);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	remove_motion_path_track(e, t) {
		let n = G.wasmeditor_remove_motion_path_track(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	remove_transition(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_remove_transition(this.__wbg_ptr, e, t, a, o, r, i);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	rename_animation(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_rename_animation(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	rename_artboard(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_rename_artboard(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	rename_audio(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_rename_audio(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	rename_drawing(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_rename_drawing(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	rename_material(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_rename_material(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	rename_node(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_rename_node(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	rename_prefab(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_rename_prefab(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	rename_state_machine(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_rename_state_machine(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	rename_text_file(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_rename_text_file(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	render(e, t, n, r, i, a, o, s, c, l, u, d) {
		let f = G.wasmeditor_render(this.__wbg_ptr, e, t, n, r, i, a, o, s, c, l, u, d);
		if (f[1]) throw U(f[0]);
	}
	reparent_node(e, t, n) {
		let r = G.wasmeditor_reparent_node(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	reparent_node_before(e, t, n, r) {
		let i = G.wasmeditor_reparent_node_before(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	replay_particle_system(e, t) {
		let n = G.wasmeditor_replay_particle_system(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	resize(e, t) {
		G.wasmeditor_resize(this.__wbg_ptr, e, t);
	}
	resize_tilemap(e, t, n, r) {
		let i = G.wasmeditor_resize_tilemap(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	revert_prefab_instance(e, t) {
		let n = G.wasmeditor_revert_prefab_instance(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	save_project(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W, r = G.wasmeditor_save_project(this.__wbg_ptr, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	save_tea() {
		let e, t;
		try {
			let i = G.wasmeditor_save_tea(this.__wbg_ptr);
			var n = i[0], r = i[1];
			if (i[3]) throw n = 0, r = 0, U(i[2]);
			return e = n, t = r, L(n, r);
		} finally {
			G.__wbindgen_free(e, t, 1);
		}
	}
	save_teab() {
		let e = G.wasmeditor_save_teab(this.__wbg_ptr);
		if (e[3]) throw U(e[2]);
		var t = N(e[0], e[1]).slice();
		return G.__wbindgen_free(e[0], e[1] * 1, 1), t;
	}
	screen_info_now() {
		let e = G.wasmeditor_screen_info_now(this.__wbg_ptr);
		var t = k(e[0], e[1]).slice();
		return G.__wbindgen_free(e[0], e[1] * 4, 4), t;
	}
	script_dynamic(e) {
		let t = G.wasmeditor_script_dynamic(this.__wbg_ptr, e);
		if (t[3]) throw U(t[2]);
		var n = k(t[0], t[1]).slice();
		return G.__wbindgen_free(t[0], t[1] * 4, 4), n;
	}
	script_revision(e) {
		let t = G.wasmeditor_script_revision(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return t[0] >>> 0;
	}
	script_scene(e) {
		let t = G.wasmeditor_script_scene(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	send_node_backward(e, t) {
		let n = G.wasmeditor_send_node_backward(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	send_node_to_back(e, t) {
		let n = G.wasmeditor_send_node_to_back(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_active_animation(e, t) {
		let n = G.wasmeditor_set_active_animation(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_active_artboard(e) {
		let t = G.wasmeditor_set_active_artboard(this.__wbg_ptr, e);
		if (t[2]) throw U(t[1]);
		return U(t[0]);
	}
	set_active_camera(e, t) {
		let n = G.wasmeditor_set_active_camera(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_active_state_machine(e, t) {
		let n = G.wasmeditor_set_active_state_machine(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_animate_mode_active(e) {
		G.wasmeditor_set_animate_mode_active(this.__wbg_ptr, e);
	}
	set_animation_duration(e, t, n) {
		let r = G.wasmeditor_set_animation_duration(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_artboard_gravity(e, t, n) {
		let r = G.wasmeditor_set_artboard_gravity(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_artboard_gravity_3d(e, t, n, r) {
		let i = G.wasmeditor_set_artboard_gravity_3d(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	set_artboard_resolution_scale(e, t) {
		let n = G.wasmeditor_set_artboard_resolution_scale(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_artboard_screen(e, t) {
		let n = G.wasmeditor_set_artboard_screen(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_artboard_size(e, t, n) {
		let r = G.wasmeditor_set_artboard_size(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_audio_file(e, t) {
		var n = B(t) ? 0 : H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		let i = G.wasmeditor_set_audio_file(this.__wbg_ptr, e, n, r);
		if (i[1]) throw U(i[0]);
	}
	set_bone_length(e, t, n) {
		let r = G.wasmeditor_set_bone_length(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_camera(e, t, n) {
		let r = G.wasmeditor_set_camera(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_component_default_fill_color(e, t) {
		let n = G.wasmeditor_set_component_default_fill_color(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_environment_3d(e, t, n, r, i, a, o) {
		G.wasmeditor_set_environment_3d(this.__wbg_ptr, e, t, n, r, i, a, o);
	}
	set_export_transparent(e) {
		G.wasmeditor_set_export_transparent(this.__wbg_ptr, e);
	}
	set_fill_color(e, t, n) {
		let r = G.wasmeditor_set_fill_color(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_gradient(e, t, n) {
		let r = G.wasmeditor_set_gradient(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_group_mask(e, t, n) {
		let r = G.wasmeditor_set_group_mask(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_group_y_sort(e, t, n) {
		let r = G.wasmeditor_set_group_y_sort(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_image_file(e, t) {
		var n = B(t) ? 0 : H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		let i = G.wasmeditor_set_image_file(this.__wbg_ptr, e, n, r);
		if (i[1]) throw U(i[0]);
	}
	set_image_processing(e, t) {
		return G.wasmeditor_set_image_processing(this.__wbg_ptr, e, t);
	}
	set_instance_fill_color_override(e, t, n) {
		let r = G.wasmeditor_set_instance_fill_color_override(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_instance_variant(e, t, n) {
		let r = G.wasmeditor_set_instance_variant(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_key_down(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		G.wasmeditor_set_key_down(this.__wbg_ptr, n, r, t);
	}
	set_keyframe_easing(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_set_keyframe_easing(this.__wbg_ptr, e, t, a, o, r, i);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	set_light2d(e, t, n) {
		let r = G.wasmeditor_set_light2d(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_material_graph(e, t) {
		let n = G.wasmeditor_set_material_graph(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_mode(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_set_mode(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	set_motion_path_guide(e, t, n) {
		let r = G.wasmeditor_set_motion_path_guide(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node3d(e, t, n) {
		let r = G.wasmeditor_set_node3d(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node_anchor(e, t, n, r) {
		let i = G.wasmeditor_set_node_anchor(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	set_node_behaviors(e, t, n) {
		let r = G.wasmeditor_set_node_behaviors(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node_classes(e, t, n) {
		let r = G.wasmeditor_set_node_classes(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node_clip_children(e, t, n) {
		let r = G.wasmeditor_set_node_clip_children(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node_id(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_set_node_id(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	set_node_rotation(e, t, n) {
		let r = G.wasmeditor_set_node_rotation(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node_scale(e, t, n, r) {
		let i = G.wasmeditor_set_node_scale(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	set_node_translation(e, t, n, r) {
		let i = G.wasmeditor_set_node_translation(this.__wbg_ptr, e, t, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	set_node_unlit(e, t, n) {
		let r = G.wasmeditor_set_node_unlit(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_node_visible(e, t, n) {
		let r = G.wasmeditor_set_node_visible(this.__wbg_ptr, e, t, n);
		if (r[1]) throw U(r[0]);
	}
	set_node_z_index(e, t, n) {
		let r = G.wasmeditor_set_node_z_index(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_particle_system(e, t, n) {
		let r = G.wasmeditor_set_particle_system(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_post_effects(e, t) {
		let n = G.wasmeditor_set_post_effects(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_preview_screen(e, t) {
		G.wasmeditor_set_preview_screen(this.__wbg_ptr, e, t);
	}
	set_script_fields(e, t) {
		let n = G.wasmeditor_set_script_fields(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_scrub_time(e, t) {
		let n = G.wasmeditor_set_scrub_time(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	set_selected_node(e) {
		let t = G.wasmeditor_set_selected_node(this.__wbg_ptr, e);
		if (t[1]) throw U(t[0]);
	}
	set_shadow_caster(e, t, n) {
		let r = G.wasmeditor_set_shadow_caster(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_shape_blend_mode(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_set_shape_blend_mode(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	set_shape_image(e, t, n) {
		let r = G.wasmeditor_set_shape_image(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_shape_path(e, t, n) {
		let r = G.wasmeditor_set_shape_path(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_shape_stroke(e, t, n) {
		let r = G.wasmeditor_set_shape_stroke(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_state_blend(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_set_state_blend(this.__wbg_ptr, e, t, a, o, r, i);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	set_state_machine_active(e) {
		G.wasmeditor_set_state_machine_active(this.__wbg_ptr, e);
	}
	set_state_machine_bool_input(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		G.wasmeditor_set_state_machine_bool_input(this.__wbg_ptr, n, r, t);
	}
	set_state_machine_enum_input(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
		G.wasmeditor_set_state_machine_enum_input(this.__wbg_ptr, n, r, i, a);
	}
	set_state_machine_initial_state(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_set_state_machine_initial_state(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	set_state_machine_layer_initial_state(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = G.wasmeditor_set_state_machine_layer_initial_state(this.__wbg_ptr, e, t, i, a, o, s);
		if (c[2]) throw U(c[1]);
		return U(c[0]);
	}
	set_state_machine_layer_mask(e, t, n, r) {
		let i = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = G.wasmeditor_set_state_machine_layer_mask(this.__wbg_ptr, e, t, i, a, r);
		if (o[2]) throw U(o[1]);
		return U(o[0]);
	}
	set_state_machine_number_input(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		G.wasmeditor_set_state_machine_number_input(this.__wbg_ptr, n, r, t);
	}
	set_state_machine_programs_enabled(e) {
		G.wasmeditor_set_state_machine_programs_enabled(this.__wbg_ptr, e);
	}
	set_state_position(e, t, n, r, i, a) {
		let o = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = G.wasmeditor_set_state_position(this.__wbg_ptr, e, t, o, s, r, i, a);
		if (c[2]) throw U(c[1]);
		return U(c[0]);
	}
	set_state_program(e, t, n, r, i) {
		let a = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_set_state_program(this.__wbg_ptr, e, t, a, o, r, i);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	set_text_content(e, t, n) {
		let r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W, a = G.wasmeditor_set_text_content(this.__wbg_ptr, e, t, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	set_text_file_text(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_set_text_file_text(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return i[0] !== 0;
	}
	set_text_font(e, t, n) {
		let r = G.wasmeditor_set_text_font(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_text_size(e, t, n) {
		let r = G.wasmeditor_set_text_size(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_tile(e, t, n, r, i) {
		let a = G.wasmeditor_set_tile(this.__wbg_ptr, e, t, n, r, B(i) ? 2 ** 53 - 1 : i >>> 0);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	set_tilemap_cell_size(e, t, n) {
		let r = G.wasmeditor_set_tilemap_cell_size(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_trail(e, t, n) {
		let r = G.wasmeditor_set_trail(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	set_transition(e, t, n, r, i, a, o, s, c) {
		let l = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), u = W, d = H(a, G.__wbindgen_malloc, G.__wbindgen_realloc), f = W, p = G.wasmeditor_set_transition(this.__wbg_ptr, e, t, l, u, r, i, d, f, o, s, c);
		if (p[2]) throw U(p[1]);
		return U(p[0]);
	}
	shader_node_catalog() {
		let e = G.wasmeditor_shader_node_catalog(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	solve_two_bone_ik(e, t, n, r, i) {
		let a = G.wasmeditor_solve_two_bone_ik(this.__wbg_ptr, e, t, n, r, i);
		if (a[2]) throw U(a[1]);
		return U(a[0]);
	}
	spawn_prefab(e, t, n, r, i) {
		let a = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), o = W, s = G.wasmeditor_spawn_prefab(this.__wbg_ptr, e, a, o, n, r, i);
		if (s[2]) throw U(s[1]);
		return U(s[0]);
	}
	state_machine_summary() {
		let e = G.wasmeditor_state_machine_summary(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	summary_fingerprint() {
		return G.wasmeditor_summary_fingerprint(this.__wbg_ptr);
	}
	take_project_file_bytes(e) {
		let t = G.wasmeditor_take_project_file_bytes(this.__wbg_ptr, e);
		var n = N(t[0], t[1]).slice();
		return G.__wbindgen_free(t[0], t[1] * 1, 1), n;
	}
	tree_regrow(e) {
		let t = G.wasmeditor_tree_regrow(this.__wbg_ptr, e);
		if (t[1]) throw U(t[0]);
	}
	tree_shake(e, t) {
		let n = G.wasmeditor_tree_shake(this.__wbg_ptr, e, t);
		if (n[1]) throw U(n[0]);
	}
	undo() {
		let e = G.wasmeditor_undo(this.__wbg_ptr);
		if (e[2]) throw U(e[1]);
		return U(e[0]);
	}
	unpack_prefab_instance(e, t) {
		let n = G.wasmeditor_unpack_prefab_instance(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	update_animated_drawing(e, t) {
		let n = G.wasmeditor_update_animated_drawing(this.__wbg_ptr, e, t);
		if (n[2]) throw U(n[1]);
		return U(n[0]);
	}
	update_bitmap_drawing(e, t) {
		let n = V(t, G.__wbindgen_malloc), r = W;
		return G.wasmeditor_update_bitmap_drawing(this.__wbg_ptr, e, n, r);
	}
	update_drawing(e, t) {
		let n = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = G.wasmeditor_update_drawing(this.__wbg_ptr, e, n, r);
		if (i[2]) throw U(i[1]);
		return U(i[0]);
	}
	update_property(e, t, n, r) {
		let i = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W, o = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), l = W, u = G.wasmeditor_update_property(this.__wbg_ptr, e, i, a, o, s, c, l);
		if (u[2]) throw U(u[1]);
		return U(u[0]);
	}
	update_state(e, t, n, r, i, a) {
		let o = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), s = W, c = H(i, G.__wbindgen_malloc, G.__wbindgen_realloc), l = W, u = G.wasmeditor_update_state(this.__wbg_ptr, e, t, o, s, r, c, l, a);
		if (u[2]) throw U(u[1]);
		return U(u[0]);
	}
	view3d_drag(e, t, n, r, i, a) {
		let o = Ye(t, G.__wbindgen_malloc), s = W, c = G.wasmeditor_view3d_drag(this.__wbg_ptr, e, o, s, n, r, i, a);
		var l = k(c[0], c[1]).slice();
		return G.__wbindgen_free(c[0], c[1] * 4, 4), l;
	}
	view3d_frame(e, t) {
		let n = G.wasmeditor_view3d_frame(this.__wbg_ptr, e, t);
		if (n[1]) throw U(n[0]);
	}
	view3d_hover(e, t, n, r) {
		let i = G.wasmeditor_view3d_hover(this.__wbg_ptr, e, t, n, r);
		if (i[1]) throw U(i[0]);
	}
	view3d_orbit(e, t) {
		G.wasmeditor_view3d_orbit(this.__wbg_ptr, e, t);
	}
	view3d_pan(e, t) {
		G.wasmeditor_view3d_pan(this.__wbg_ptr, e, t);
	}
	view3d_pick(e, t, n) {
		let r = G.wasmeditor_view3d_pick(this.__wbg_ptr, e, t, n);
		if (r[2]) throw U(r[1]);
		return U(r[0]);
	}
	view3d_select(e) {
		let t = G.wasmeditor_view3d_select(this.__wbg_ptr, e);
		if (t[1]) throw U(t[0]);
	}
	view3d_set_enabled(e) {
		G.wasmeditor_view3d_set_enabled(this.__wbg_ptr, e);
	}
	view3d_zoom(e) {
		G.wasmeditor_view3d_zoom(this.__wbg_ptr, e);
	}
	voxel_chunk_set(e, t, n, r, i) {
		let a = V(i, G.__wbindgen_malloc), o = W, s = G.wasmeditor_voxel_chunk_set(this.__wbg_ptr, e, t, n, r, a, o);
		if (s[1]) throw U(s[0]);
	}
	voxel_chunk_unload(e, t, n, r) {
		let i = G.wasmeditor_voxel_chunk_unload(this.__wbg_ptr, e, t, n, r);
		if (i[1]) throw U(i[0]);
	}
	voxel_set(e, t, n, r, i, a) {
		let o = G.wasmeditor_voxel_set(this.__wbg_ptr, e, t, n, r, i, a);
		if (o[2]) throw U(o[1]);
		return o[0] !== 0;
	}
	voxels_set(e, t, n) {
		let r = V(n, G.__wbindgen_malloc), i = W, a = G.wasmeditor_voxels_set(this.__wbg_ptr, e, t, r, i);
		if (a[1]) throw U(a[0]);
	}
};
Symbol.dispose && (g.prototype[Symbol.dispose] = g.prototype.free);
var _ = class e {
	static __wrap(t) {
		let n = Object.create(e.prototype);
		return n.__wbg_ptr = t, Ee.register(n, n.__wbg_ptr, n), n;
	}
	__destroy_into_raw() {
		let e = this.__wbg_ptr;
		return this.__wbg_ptr = 0, Ee.unregister(this), e;
	}
	free() {
		let e = this.__destroy_into_raw();
		G.__wbg_wasmplayer_free(e, 0);
	}
	static create(e, t) {
		return G.wasmplayer_create(e, t);
	}
	fire_trigger(e) {
		let t = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), n = W;
		G.wasmplayer_fire_trigger(this.__wbg_ptr, t, n);
	}
	load_teab(e) {
		let t = V(e, G.__wbindgen_malloc), n = W;
		return G.wasmplayer_load_teab(this.__wbg_ptr, t, n);
	}
	pause() {
		G.wasmplayer_pause(this.__wbg_ptr);
	}
	play() {
		G.wasmplayer_play(this.__wbg_ptr);
	}
	render(e) {
		let t = G.wasmplayer_render(this.__wbg_ptr, e);
		if (t[1]) throw U(t[0]);
	}
	resize(e, t) {
		G.wasmplayer_resize(this.__wbg_ptr, e, t);
	}
	seek(e) {
		G.wasmplayer_seek(this.__wbg_ptr, e);
	}
	set_bool_input(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		G.wasmplayer_set_bool_input(this.__wbg_ptr, n, r, t);
	}
	set_enum_input(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W, i = H(t, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
		G.wasmplayer_set_enum_input(this.__wbg_ptr, n, r, i, a);
	}
	set_key_down(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		G.wasmplayer_set_key_down(this.__wbg_ptr, n, r, t);
	}
	set_number_input(e, t) {
		let n = H(e, G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
		G.wasmplayer_set_number_input(this.__wbg_ptr, n, r, t);
	}
	state_machine_names() {
		let e = G.wasmplayer_state_machine_names(this.__wbg_ptr);
		var t = je(e[0], e[1]);
		return G.__wbindgen_free(e[0], e[1] * 4, 4), t;
	}
};
Symbol.dispose && (_.prototype[Symbol.dispose] = _.prototype.free);
function v() {
	return {
		__proto__: null,
		"./an1mbyt3_wasm_bg.js": {
			__proto__: null,
			__wbg_Error_408e67f47ca7b58b: function(e, t) {
				return Error(L(e, t));
			},
			__wbg_Number_3890faa6d3ff057d: function(e) {
				return Number(e);
			},
			__wbg_String_8564e559799eccda: function(e, t) {
				let n = H(String(t), G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
				F().setInt32(e + 4, r, !0), F().setInt32(e + 0, n, !0);
			},
			__wbg_Window_afcc911b2f9c92e2: function(e) {
				return e.Window;
			},
			__wbg_WorkerGlobalScope_5d19ebc889ff397e: function(e) {
				return e.WorkerGlobalScope;
			},
			__wbg___wbindgen_bigint_get_as_i64_c4ecf48528083721: function(e, t) {
				let n = t, r = typeof n == "bigint" ? n : void 0;
				F().setBigInt64(e + 8, B(r) ? BigInt(0) : r, !0), F().setInt32(e + 0, !B(r), !0);
			},
			__wbg___wbindgen_boolean_get_c9c83ebd41b34df3: function(e) {
				let t = e, n = typeof t == "boolean" ? t : void 0;
				return B(n) ? 16777215 : +!!n;
			},
			__wbg___wbindgen_debug_string_a57024b9c6e4a48b: function(e, t) {
				let n = H(Oe(t), G.__wbindgen_malloc, G.__wbindgen_realloc), r = W;
				F().setInt32(e + 4, r, !0), F().setInt32(e + 0, n, !0);
			},
			__wbg___wbindgen_in_ac983077f137f2e6: function(e, t) {
				return e in t;
			},
			__wbg___wbindgen_is_bigint_8ffbbef442139384: function(e) {
				return typeof e == "bigint";
			},
			__wbg___wbindgen_is_function_5e4570eb24ffa122: function(e) {
				return typeof e == "function";
			},
			__wbg___wbindgen_is_null_7d13f41e1a2d5140: function(e) {
				return e === null;
			},
			__wbg___wbindgen_is_object_a2790eb24c211ea0: function(e) {
				let t = e;
				return typeof t == "object" && !!t;
			},
			__wbg___wbindgen_is_string_e6f02f0ea5f20a32: function(e) {
				return typeof e == "string";
			},
			__wbg___wbindgen_is_undefined_6cff064c44e0d823: function(e) {
				return e === void 0;
			},
			__wbg___wbindgen_jsval_eq_0a18949a61670320: function(e, t) {
				return e === t;
			},
			__wbg___wbindgen_jsval_loose_eq_acf2776254a8d832: function(e, t) {
				return e == t;
			},
			__wbg___wbindgen_number_get_136b9679cab35cfb: function(e, t) {
				let n = t, r = typeof n == "number" ? n : void 0;
				F().setFloat64(e + 8, B(r) ? 0 : r, !0), F().setInt32(e + 0, !B(r), !0);
			},
			__wbg___wbindgen_string_get_d154f1e671052120: function(e, t) {
				let n = t, r = typeof n == "string" ? n : void 0;
				var i = B(r) ? 0 : H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
				F().setInt32(e + 4, a, !0), F().setInt32(e + 0, i, !0);
			},
			__wbg___wbindgen_throw_bb96b2010945f0bc: function(e, t) {
				throw Error(L(e, t));
			},
			__wbg__wbg_cb_unref_be22cc64ae6946a0: function(e) {
				e._wbg_cb_unref();
			},
			__wbg_activeTexture_8e65ac2e8d488478: function(e, t) {
				e.activeTexture(t >>> 0);
			},
			__wbg_activeTexture_fd6262686afdbe2f: function(e, t) {
				e.activeTexture(t >>> 0);
			},
			__wbg_addEventListener_3b8edc02c33d9f77: function() {
				return z(function(e, t, n, r) {
					e.addEventListener(L(t, n), r);
				}, arguments);
			},
			__wbg_attachShader_26751604f00d1f1b: function(e, t, n) {
				e.attachShader(t, n);
			},
			__wbg_attachShader_61baa58641ea664a: function(e, t, n) {
				e.attachShader(t, n);
			},
			__wbg_beginQuery_444a51812fdbf958: function(e, t, n) {
				e.beginQuery(t >>> 0, n);
			},
			__wbg_beginRenderPass_aa22c432e793359a: function() {
				return z(function(e, t) {
					return e.beginRenderPass(t);
				}, arguments);
			},
			__wbg_bindAttribLocation_1e182a50e1556784: function(e, t, n, r, i) {
				e.bindAttribLocation(t, n >>> 0, L(r, i));
			},
			__wbg_bindAttribLocation_9cc5ab15df1d042d: function(e, t, n, r, i) {
				e.bindAttribLocation(t, n >>> 0, L(r, i));
			},
			__wbg_bindBufferRange_5a8d28ef662d8746: function(e, t, n, r, i, a) {
				e.bindBufferRange(t >>> 0, n >>> 0, r, i, a);
			},
			__wbg_bindBuffer_1fb12d083d2a22af: function(e, t, n) {
				e.bindBuffer(t >>> 0, n);
			},
			__wbg_bindBuffer_31cb159ab5dc5ba7: function(e, t, n) {
				e.bindBuffer(t >>> 0, n);
			},
			__wbg_bindFramebuffer_32ce672324ce8a16: function(e, t, n) {
				e.bindFramebuffer(t >>> 0, n);
			},
			__wbg_bindFramebuffer_e620067056f9316f: function(e, t, n) {
				e.bindFramebuffer(t >>> 0, n);
			},
			__wbg_bindRenderbuffer_765cffe23b9c36f7: function(e, t, n) {
				e.bindRenderbuffer(t >>> 0, n);
			},
			__wbg_bindRenderbuffer_9b313332bd7aa049: function(e, t, n) {
				e.bindRenderbuffer(t >>> 0, n);
			},
			__wbg_bindSampler_28b0a4c34c6f96d4: function(e, t, n) {
				e.bindSampler(t >>> 0, n);
			},
			__wbg_bindTexture_4c54ffb64c33564f: function(e, t, n) {
				e.bindTexture(t >>> 0, n);
			},
			__wbg_bindTexture_6fe86367f6be8f59: function(e, t, n) {
				e.bindTexture(t >>> 0, n);
			},
			__wbg_bindVertexArrayOES_96a4898652eac0d8: function(e, t) {
				e.bindVertexArrayOES(t);
			},
			__wbg_bindVertexArray_0185d931d681d806: function(e, t) {
				e.bindVertexArray(t);
			},
			__wbg_blendColor_402572bc445d3ac3: function(e, t, n, r, i) {
				e.blendColor(t, n, r, i);
			},
			__wbg_blendColor_af92968fedc595b1: function(e, t, n, r, i) {
				e.blendColor(t, n, r, i);
			},
			__wbg_blendEquationSeparate_5ab35e46e7f48717: function(e, t, n) {
				e.blendEquationSeparate(t >>> 0, n >>> 0);
			},
			__wbg_blendEquationSeparate_9ad084e8266b8e3c: function(e, t, n) {
				e.blendEquationSeparate(t >>> 0, n >>> 0);
			},
			__wbg_blendEquation_4bab539169e7e865: function(e, t) {
				e.blendEquation(t >>> 0);
			},
			__wbg_blendEquation_502ed4c6af5bf8ee: function(e, t) {
				e.blendEquation(t >>> 0);
			},
			__wbg_blendFuncSeparate_2e4d259caaba517e: function(e, t, n, r, i) {
				e.blendFuncSeparate(t >>> 0, n >>> 0, r >>> 0, i >>> 0);
			},
			__wbg_blendFuncSeparate_66688b15ecc6529c: function(e, t, n, r, i) {
				e.blendFuncSeparate(t >>> 0, n >>> 0, r >>> 0, i >>> 0);
			},
			__wbg_blendFunc_b7f382e97db2fd5b: function(e, t, n) {
				e.blendFunc(t >>> 0, n >>> 0);
			},
			__wbg_blendFunc_d908118bbb181928: function(e, t, n) {
				e.blendFunc(t >>> 0, n >>> 0);
			},
			__wbg_blitFramebuffer_20b32de88a3097b1: function(e, t, n, r, i, a, o, s, c, l, u) {
				e.blitFramebuffer(t, n, r, i, a, o, s, c, l >>> 0, u >>> 0);
			},
			__wbg_bufferData_1dd2939db2d88d82: function(e, t, n, r) {
				e.bufferData(t >>> 0, n, r >>> 0);
			},
			__wbg_bufferData_69a44ade0864ba2b: function(e, t, n, r) {
				e.bufferData(t >>> 0, n, r >>> 0);
			},
			__wbg_bufferData_6c10d3e07ec9a2a9: function(e, t, n, r) {
				e.bufferData(t >>> 0, n, r >>> 0);
			},
			__wbg_bufferData_d359d1c797b8e8b7: function(e, t, n, r) {
				e.bufferData(t >>> 0, n, r >>> 0);
			},
			__wbg_bufferSubData_4f6063d50303b61d: function(e, t, n, r) {
				e.bufferSubData(t >>> 0, n, r);
			},
			__wbg_bufferSubData_64b69f468a0d3048: function(e, t, n, r) {
				e.bufferSubData(t >>> 0, n, r);
			},
			__wbg_call_1c5886ab9c57d1c7: function() {
				return z(function(e, t) {
					return e.call(t);
				}, arguments);
			},
			__wbg_call_35dba3c747ad7521: function() {
				return z(function(e, t, n) {
					return e.call(t, n);
				}, arguments);
			},
			__wbg_clearBufferfv_ccbb43fb098f1912: function(e, t, n, r, i) {
				e.clearBufferfv(t >>> 0, n, k(r, i));
			},
			__wbg_clearBufferiv_8b1c68299632478f: function(e, t, n, r, i) {
				e.clearBufferiv(t >>> 0, n, A(r, i));
			},
			__wbg_clearBufferuiv_cd72147d09d432e8: function(e, t, n, r, i) {
				e.clearBufferuiv(t >>> 0, n, M(r, i));
			},
			__wbg_clearDepth_887000180cc9eb2e: function(e, t) {
				e.clearDepth(t);
			},
			__wbg_clearDepth_c4897278afd894a9: function(e, t) {
				e.clearDepth(t);
			},
			__wbg_clearStencil_3d39149452a2f872: function(e, t) {
				e.clearStencil(t);
			},
			__wbg_clearStencil_96978923f9c6fb1f: function(e, t) {
				e.clearStencil(t);
			},
			__wbg_clear_20f7614cd20df101: function(e, t) {
				e.clear(t >>> 0);
			},
			__wbg_clear_332f205d7e52df87: function(e, t) {
				e.clear(t >>> 0);
			},
			__wbg_clientWaitSync_8800b42d1c534e00: function(e, t, n, r) {
				return e.clientWaitSync(t, n >>> 0, r >>> 0);
			},
			__wbg_clientX_73f0b294f91b259a: function(e) {
				return e.clientX;
			},
			__wbg_clientY_a4c1ea57bd41430f: function(e) {
				return e.clientY;
			},
			__wbg_colorMask_5646450fe1f1b723: function(e, t, n, r, i) {
				e.colorMask(t !== 0, n !== 0, r !== 0, i !== 0);
			},
			__wbg_colorMask_8fca508f44773327: function(e, t, n, r, i) {
				e.colorMask(t !== 0, n !== 0, r !== 0, i !== 0);
			},
			__wbg_compileShader_4ede19e4fc1bebce: function(e, t) {
				e.compileShader(t);
			},
			__wbg_compileShader_ac457ada9042f08e: function(e, t) {
				e.compileShader(t);
			},
			__wbg_compressedTexSubImage2D_0968a85385b7c463: function(e, t, n, r, i, a, o, s, c, l) {
				e.compressedTexSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c, l);
			},
			__wbg_compressedTexSubImage2D_45987d7f0210d36f: function(e, t, n, r, i, a, o, s, c) {
				e.compressedTexSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c);
			},
			__wbg_compressedTexSubImage2D_a39446fce0a68ad9: function(e, t, n, r, i, a, o, s, c) {
				e.compressedTexSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c);
			},
			__wbg_compressedTexSubImage3D_3da84908295b8ec3: function(e, t, n, r, i, a, o, s, c, l, u, d) {
				e.compressedTexSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u, d);
			},
			__wbg_compressedTexSubImage3D_c0bc017057e3942a: function(e, t, n, r, i, a, o, s, c, l, u) {
				e.compressedTexSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u);
			},
			__wbg_configure_0e4789c0f6b35c8e: function() {
				return z(function(e, t) {
					e.configure(t);
				}, arguments);
			},
			__wbg_copyBufferSubData_e5dc2aab90456f99: function(e, t, n, r, i, a) {
				e.copyBufferSubData(t >>> 0, n >>> 0, r, i, a);
			},
			__wbg_copyTexSubImage2D_188da734d1c8aa07: function(e, t, n, r, i, a, o, s, c) {
				e.copyTexSubImage2D(t >>> 0, n, r, i, a, o, s, c);
			},
			__wbg_copyTexSubImage2D_84d99fa40fabace0: function(e, t, n, r, i, a, o, s, c) {
				e.copyTexSubImage2D(t >>> 0, n, r, i, a, o, s, c);
			},
			__wbg_copyTexSubImage3D_89064e67340a38b3: function(e, t, n, r, i, a, o, s, c, l) {
				e.copyTexSubImage3D(t >>> 0, n, r, i, a, o, s, c, l);
			},
			__wbg_copyTextureToBuffer_ed6e67a77ecb768d: function() {
				return z(function(e, t, n, r) {
					e.copyTextureToBuffer(t, n, r);
				}, arguments);
			},
			__wbg_createBindGroupLayout_49a7e2b3d076afcf: function() {
				return z(function(e, t) {
					return e.createBindGroupLayout(t);
				}, arguments);
			},
			__wbg_createBindGroup_655c6e6c0258530e: function(e, t) {
				return e.createBindGroup(t);
			},
			__wbg_createBuffer_0726dd2ab09ea1d2: function() {
				return z(function(e, t) {
					return e.createBuffer(t);
				}, arguments);
			},
			__wbg_createBuffer_44b37c222efbd326: function(e) {
				let t = e.createBuffer();
				return B(t) ? 0 : O(t);
			},
			__wbg_createBuffer_af6c411fe2b091f8: function(e) {
				let t = e.createBuffer();
				return B(t) ? 0 : O(t);
			},
			__wbg_createCommandEncoder_ec1f40f0cb4d09df: function(e, t) {
				return e.createCommandEncoder(t);
			},
			__wbg_createFramebuffer_4dc2fb6bd93463a5: function(e) {
				let t = e.createFramebuffer();
				return B(t) ? 0 : O(t);
			},
			__wbg_createFramebuffer_e6d8917bf9291c65: function(e) {
				let t = e.createFramebuffer();
				return B(t) ? 0 : O(t);
			},
			__wbg_createPipelineLayout_2c8cd4528b06c108: function(e, t) {
				return e.createPipelineLayout(t);
			},
			__wbg_createProgram_2ebbd17565e0ede7: function(e) {
				let t = e.createProgram();
				return B(t) ? 0 : O(t);
			},
			__wbg_createProgram_81b37242eadef893: function(e) {
				let t = e.createProgram();
				return B(t) ? 0 : O(t);
			},
			__wbg_createQuery_5ef5edffbd3a678d: function(e) {
				let t = e.createQuery();
				return B(t) ? 0 : O(t);
			},
			__wbg_createRenderPipeline_cf98d4d699bfb03c: function() {
				return z(function(e, t) {
					return e.createRenderPipeline(t);
				}, arguments);
			},
			__wbg_createRenderbuffer_be624f81e06a0cfd: function(e) {
				let t = e.createRenderbuffer();
				return B(t) ? 0 : O(t);
			},
			__wbg_createRenderbuffer_cd2638d5dda9c277: function(e) {
				let t = e.createRenderbuffer();
				return B(t) ? 0 : O(t);
			},
			__wbg_createSampler_c8ffb3c8d565f704: function(e, t) {
				return e.createSampler(t);
			},
			__wbg_createSampler_f1aedbf47c21745a: function(e) {
				let t = e.createSampler();
				return B(t) ? 0 : O(t);
			},
			__wbg_createShaderModule_2e44fc7677c6288b: function(e, t) {
				return e.createShaderModule(t);
			},
			__wbg_createShader_9a8e5f335caac850: function(e, t) {
				let n = e.createShader(t >>> 0);
				return B(n) ? 0 : O(n);
			},
			__wbg_createShader_f8638cf4c19a1d2d: function(e, t) {
				let n = e.createShader(t >>> 0);
				return B(n) ? 0 : O(n);
			},
			__wbg_createTexture_1bac74c999b8a48e: function() {
				return z(function(e, t) {
					return e.createTexture(t);
				}, arguments);
			},
			__wbg_createTexture_42c791197006c64a: function(e) {
				let t = e.createTexture();
				return B(t) ? 0 : O(t);
			},
			__wbg_createTexture_c74740f68b5c2a93: function(e) {
				let t = e.createTexture();
				return B(t) ? 0 : O(t);
			},
			__wbg_createVertexArrayOES_f7e8c94194c4e075: function(e) {
				let t = e.createVertexArrayOES();
				return B(t) ? 0 : O(t);
			},
			__wbg_createVertexArray_abd18ded26b75653: function(e) {
				let t = e.createVertexArray();
				return B(t) ? 0 : O(t);
			},
			__wbg_createView_ceaf2f5881adbd34: function() {
				return z(function(e, t) {
					return e.createView(t);
				}, arguments);
			},
			__wbg_cullFace_053fc24c214cae86: function(e, t) {
				e.cullFace(t >>> 0);
			},
			__wbg_cullFace_94e1cd382e8b654f: function(e, t) {
				e.cullFace(t >>> 0);
			},
			__wbg_deleteBuffer_42bd497a20b76d88: function(e, t) {
				e.deleteBuffer(t);
			},
			__wbg_deleteBuffer_50f20219abee4d05: function(e, t) {
				e.deleteBuffer(t);
			},
			__wbg_deleteFramebuffer_073235a01c2a0a28: function(e, t) {
				e.deleteFramebuffer(t);
			},
			__wbg_deleteFramebuffer_07fcc16563d17920: function(e, t) {
				e.deleteFramebuffer(t);
			},
			__wbg_deleteProgram_0191056307686073: function(e, t) {
				e.deleteProgram(t);
			},
			__wbg_deleteProgram_ee7f1925cb856dc2: function(e, t) {
				e.deleteProgram(t);
			},
			__wbg_deleteQuery_4624acbf9cbfc6e2: function(e, t) {
				e.deleteQuery(t);
			},
			__wbg_deleteRenderbuffer_570117534d9608a1: function(e, t) {
				e.deleteRenderbuffer(t);
			},
			__wbg_deleteRenderbuffer_ba4a805dfac20358: function(e, t) {
				e.deleteRenderbuffer(t);
			},
			__wbg_deleteSampler_527e8d31f81669d9: function(e, t) {
				e.deleteSampler(t);
			},
			__wbg_deleteShader_2558228a4ef7373e: function(e, t) {
				e.deleteShader(t);
			},
			__wbg_deleteShader_413961eb94f5c67c: function(e, t) {
				e.deleteShader(t);
			},
			__wbg_deleteSync_11f80510355180d6: function(e, t) {
				e.deleteSync(t);
			},
			__wbg_deleteTexture_0ccd278d6db819ff: function(e, t) {
				e.deleteTexture(t);
			},
			__wbg_deleteTexture_aadf9716c394d7be: function(e, t) {
				e.deleteTexture(t);
			},
			__wbg_deleteVertexArrayOES_e43a9a425587d52b: function(e, t) {
				e.deleteVertexArrayOES(t);
			},
			__wbg_deleteVertexArray_106030034355d246: function(e, t) {
				e.deleteVertexArray(t);
			},
			__wbg_depthFunc_6c6f948417f5bde4: function(e, t) {
				e.depthFunc(t >>> 0);
			},
			__wbg_depthFunc_bb3152f635a60ff2: function(e, t) {
				e.depthFunc(t >>> 0);
			},
			__wbg_depthMask_4e0075e07739355b: function(e, t) {
				e.depthMask(t !== 0);
			},
			__wbg_depthMask_bdc57b9e64c6b4d8: function(e, t) {
				e.depthMask(t !== 0);
			},
			__wbg_depthRange_0acaf3031a92d51d: function(e, t, n) {
				e.depthRange(t, n);
			},
			__wbg_depthRange_e2d0a59942d33efd: function(e, t, n) {
				e.depthRange(t, n);
			},
			__wbg_description_02485704e69b1e7f: function(e, t) {
				let n = t.description, r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W;
				F().setInt32(e + 4, i, !0), F().setInt32(e + 0, r, !0);
			},
			__wbg_destroy_fe937f756bf8df37: function(e) {
				e.destroy();
			},
			__wbg_disableVertexAttribArray_98752beca840c3da: function(e, t) {
				e.disableVertexAttribArray(t >>> 0);
			},
			__wbg_disableVertexAttribArray_aee51b7f1a8ef4cc: function(e, t) {
				e.disableVertexAttribArray(t >>> 0);
			},
			__wbg_disable_2ad210ba5315372a: function(e, t) {
				e.disable(t >>> 0);
			},
			__wbg_disable_bb1df5a6c75eaecd: function(e, t) {
				e.disable(t >>> 0);
			},
			__wbg_document_ac38448dbfd31a57: function(e) {
				let t = e.document;
				return B(t) ? 0 : O(t);
			},
			__wbg_done_669171204c3dcae2: function(e) {
				return e.done;
			},
			__wbg_drawArraysInstancedANGLE_cb3b87925641d5b9: function(e, t, n, r, i) {
				e.drawArraysInstancedANGLE(t >>> 0, n, r, i);
			},
			__wbg_drawArraysInstanced_45317b22bbf7ffe8: function(e, t, n, r, i) {
				e.drawArraysInstanced(t >>> 0, n, r, i);
			},
			__wbg_drawArrays_02c354e377984441: function(e, t, n, r) {
				e.drawArrays(t >>> 0, n, r);
			},
			__wbg_drawArrays_b2004a40c212065c: function(e, t, n, r) {
				e.drawArrays(t >>> 0, n, r);
			},
			__wbg_drawBuffersWEBGL_0b4935290cba977e: function(e, t) {
				e.drawBuffersWEBGL(t);
			},
			__wbg_drawBuffers_f07f796e50bb0077: function(e, t) {
				e.drawBuffers(t);
			},
			__wbg_drawElementsInstancedANGLE_8179cb41f5862831: function(e, t, n, r, i, a) {
				e.drawElementsInstancedANGLE(t >>> 0, n, r >>> 0, i, a);
			},
			__wbg_drawElementsInstanced_07717eeb890435e9: function(e, t, n, r, i, a) {
				e.drawElementsInstanced(t >>> 0, n, r >>> 0, i, a);
			},
			__wbg_drawIndexed_d31913e79d58fbac: function(e, t, n, r, i, a) {
				e.drawIndexed(t >>> 0, n >>> 0, r >>> 0, i, a >>> 0);
			},
			__wbg_draw_6877f98847e1e36c: function(e, t, n, r, i) {
				e.draw(t >>> 0, n >>> 0, r >>> 0, i >>> 0);
			},
			__wbg_enableVertexAttribArray_90f1a9f570379c36: function(e, t) {
				e.enableVertexAttribArray(t >>> 0);
			},
			__wbg_enableVertexAttribArray_b072ffcbe4f26e2b: function(e, t) {
				e.enableVertexAttribArray(t >>> 0);
			},
			__wbg_enable_17346ff3b2257cae: function(e, t) {
				e.enable(t >>> 0);
			},
			__wbg_enable_db1e433ea267f29b: function(e, t) {
				e.enable(t >>> 0);
			},
			__wbg_endQuery_0434371d408e59b7: function(e, t) {
				e.endQuery(t >>> 0);
			},
			__wbg_end_f99ebed53d4e198a: function(e) {
				e.end();
			},
			__wbg_entries_7774d489e1da5f4f: function(e) {
				return Object.entries(e);
			},
			__wbg_error_757e9472f8410341: function(e, t) {
				let n, r;
				try {
					n = e, r = t, console.error(L(e, t));
				} finally {
					G.__wbindgen_free(n, r, 1);
				}
			},
			__wbg_fenceSync_57ab30f550e5a5a2: function(e, t, n) {
				let r = e.fenceSync(t >>> 0, n >>> 0);
				return B(r) ? 0 : O(r);
			},
			__wbg_finish_4d91de5e927dd13f: function(e, t) {
				return e.finish(t);
			},
			__wbg_finish_6e06b68ab68cd9f6: function(e) {
				return e.finish();
			},
			__wbg_finish_81c066eb195fc8a9: function(e) {
				e.finish();
			},
			__wbg_finish_94865fee5c90da6b: function(e) {
				e.finish();
			},
			__wbg_flush_2a8fa6766a4f3ada: function(e) {
				e.flush();
			},
			__wbg_flush_918ffb9cfebcbaab: function(e) {
				e.flush();
			},
			__wbg_framebufferRenderbuffer_5736a8553be94035: function(e, t, n, r, i) {
				e.framebufferRenderbuffer(t >>> 0, n >>> 0, r >>> 0, i);
			},
			__wbg_framebufferRenderbuffer_e0c873b9f296443d: function(e, t, n, r, i) {
				e.framebufferRenderbuffer(t >>> 0, n >>> 0, r >>> 0, i);
			},
			__wbg_framebufferTexture2D_8584b49a205ffe5b: function(e, t, n, r, i, a) {
				e.framebufferTexture2D(t >>> 0, n >>> 0, r >>> 0, i, a);
			},
			__wbg_framebufferTexture2D_9abab99d6209666a: function(e, t, n, r, i, a) {
				e.framebufferTexture2D(t >>> 0, n >>> 0, r >>> 0, i, a);
			},
			__wbg_framebufferTextureLayer_e236352620170c5a: function(e, t, n, r, i, a) {
				e.framebufferTextureLayer(t >>> 0, n >>> 0, r, i, a);
			},
			__wbg_framebufferTextureMultiviewOVR_9b89dd83134856d3: function(e, t, n, r, i, a, o) {
				e.framebufferTextureMultiviewOVR(t >>> 0, n >>> 0, r, i, a, o);
			},
			__wbg_frontFace_188579d7bba462b1: function(e, t) {
				e.frontFace(t >>> 0);
			},
			__wbg_frontFace_19294c82ae89fa71: function(e, t) {
				e.frontFace(t >>> 0);
			},
			__wbg_getBoundingClientRect_8165472d0fc91753: function(e) {
				return e.getBoundingClientRect();
			},
			__wbg_getBufferSubData_d1d7ad69c40ea085: function(e, t, n, r) {
				e.getBufferSubData(t >>> 0, n, r);
			},
			__wbg_getContext_123ddade3a0fb2f5: function() {
				return z(function(e, t, n, r) {
					let i = e.getContext(L(t, n), r);
					return B(i) ? 0 : O(i);
				}, arguments);
			},
			__wbg_getContext_53c8c42beb820370: function() {
				return z(function(e, t, n, r) {
					let i = e.getContext(L(t, n), r);
					return B(i) ? 0 : O(i);
				}, arguments);
			},
			__wbg_getContext_71c33f14b63da593: function() {
				return z(function(e, t, n) {
					let r = e.getContext(L(t, n));
					return B(r) ? 0 : O(r);
				}, arguments);
			},
			__wbg_getContext_c5236e0057b35024: function() {
				return z(function(e, t, n) {
					let r = e.getContext(L(t, n));
					return B(r) ? 0 : O(r);
				}, arguments);
			},
			__wbg_getCurrentTexture_20714d1bd9051cab: function() {
				return z(function(e) {
					return e.getCurrentTexture();
				}, arguments);
			},
			__wbg_getExtension_8e8c3be603d4f5ce: function() {
				return z(function(e, t, n) {
					let r = e.getExtension(L(t, n));
					return B(r) ? 0 : O(r);
				}, arguments);
			},
			__wbg_getIndexedParameter_fa6cca29d50de787: function() {
				return z(function(e, t, n) {
					return e.getIndexedParameter(t >>> 0, n >>> 0);
				}, arguments);
			},
			__wbg_getMappedRange_d0bf3141224111b6: function() {
				return z(function(e, t, n) {
					return e.getMappedRange(t, n);
				}, arguments);
			},
			__wbg_getParameter_19325d4aa1b66856: function() {
				return z(function(e, t) {
					return e.getParameter(t >>> 0);
				}, arguments);
			},
			__wbg_getParameter_7ddbe9f9606f6a80: function() {
				return z(function(e, t) {
					return e.getParameter(t >>> 0);
				}, arguments);
			},
			__wbg_getPreferredCanvasFormat_8b57039d1801a506: function(e) {
				let t = e.getPreferredCanvasFormat();
				return (D.indexOf(t) + 1 || 102) - 1;
			},
			__wbg_getProgramInfoLog_50a07d12dddd0da6: function(e, t, n) {
				let r = t.getProgramInfoLog(n);
				var i = B(r) ? 0 : H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
				F().setInt32(e + 4, a, !0), F().setInt32(e + 0, i, !0);
			},
			__wbg_getProgramInfoLog_72665662cf78b5a2: function(e, t, n) {
				let r = t.getProgramInfoLog(n);
				var i = B(r) ? 0 : H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
				F().setInt32(e + 4, a, !0), F().setInt32(e + 0, i, !0);
			},
			__wbg_getProgramParameter_1f5cceb73030e823: function(e, t, n) {
				return e.getProgramParameter(t, n >>> 0);
			},
			__wbg_getProgramParameter_41e1ea6f52a71ba5: function(e, t, n) {
				return e.getProgramParameter(t, n >>> 0);
			},
			__wbg_getQueryParameter_fa2ce36cfdedc862: function(e, t, n) {
				return e.getQueryParameter(t, n >>> 0);
			},
			__wbg_getShaderInfoLog_337a0567e83283d1: function(e, t, n) {
				let r = t.getShaderInfoLog(n);
				var i = B(r) ? 0 : H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
				F().setInt32(e + 4, a, !0), F().setInt32(e + 0, i, !0);
			},
			__wbg_getShaderInfoLog_663a9b136ab42b32: function(e, t, n) {
				let r = t.getShaderInfoLog(n);
				var i = B(r) ? 0 : H(r, G.__wbindgen_malloc, G.__wbindgen_realloc), a = W;
				F().setInt32(e + 4, a, !0), F().setInt32(e + 0, i, !0);
			},
			__wbg_getShaderParameter_95d4ad40668ee798: function(e, t, n) {
				return e.getShaderParameter(t, n >>> 0);
			},
			__wbg_getShaderParameter_9e9aa18598294f3b: function(e, t, n) {
				return e.getShaderParameter(t, n >>> 0);
			},
			__wbg_getSupportedExtensions_63e3eaba880055c5: function(e) {
				let t = e.getSupportedExtensions();
				return B(t) ? 0 : O(t);
			},
			__wbg_getSupportedProfiles_7cd826b4eff5e8fc: function(e) {
				let t = e.getSupportedProfiles();
				return B(t) ? 0 : O(t);
			},
			__wbg_getSyncParameter_3eb3ecefa061c5ee: function(e, t, n) {
				return e.getSyncParameter(t, n >>> 0);
			},
			__wbg_getUniformBlockIndex_78264d4d94f8252d: function(e, t, n, r) {
				return e.getUniformBlockIndex(t, L(n, r));
			},
			__wbg_getUniformLocation_11fd99fee70965dc: function(e, t, n, r) {
				let i = e.getUniformLocation(t, L(n, r));
				return B(i) ? 0 : O(i);
			},
			__wbg_getUniformLocation_c493d2f5f1a6213d: function(e, t, n, r) {
				let i = e.getUniformLocation(t, L(n, r));
				return B(i) ? 0 : O(i);
			},
			__wbg_get_36debceb6d43d7a1: function(e, t) {
				let n = e[t >>> 0];
				return B(n) ? 0 : O(n);
			},
			__wbg_get_c0c8f8d7da0c03dd: function(e, t) {
				return e[t >>> 0];
			},
			__wbg_get_d173c0308df22d37: function() {
				return z(function(e, t) {
					return Reflect.get(e, t);
				}, arguments);
			},
			__wbg_get_unchecked_e20b893aeafc3fca: function(e, t) {
				return e[t >>> 0];
			},
			__wbg_get_with_ref_key_6412cf3094599694: function(e, t) {
				return e[t];
			},
			__wbg_gpu_2ccc250735d24a2a: function(e) {
				return e.gpu;
			},
			__wbg_height_e56f6fb197710e09: function(e) {
				return e.height;
			},
			__wbg_includes_a4b83ade703cb80b: function(e, t, n) {
				return e.includes(t, n);
			},
			__wbg_info_cf0d9a286850cd24: function(e) {
				return e.info;
			},
			__wbg_instanceof_ArrayBuffer_993d02d2d254cad1: function(e) {
				let t;
				try {
					t = e instanceof ArrayBuffer;
				} catch {
					t = !1;
				}
				return t;
			},
			__wbg_instanceof_HtmlCanvasElement_327e7f7530c72bbd: function(e) {
				let t;
				try {
					t = e instanceof HTMLCanvasElement;
				} catch {
					t = !1;
				}
				return t;
			},
			__wbg_instanceof_Map_9a4d6ead180ae3a9: function(e) {
				let t;
				try {
					t = e instanceof Map;
				} catch {
					t = !1;
				}
				return t;
			},
			__wbg_instanceof_Uint8Array_f935dbb0aa7cdeed: function(e) {
				let t;
				try {
					t = e instanceof Uint8Array;
				} catch {
					t = !1;
				}
				return t;
			},
			__wbg_instanceof_WebGl2RenderingContext_e27143c72f888655: function(e) {
				let t;
				try {
					t = e instanceof WebGL2RenderingContext;
				} catch {
					t = !1;
				}
				return t;
			},
			__wbg_instanceof_Window_5625ff9937037a38: function(e) {
				let t;
				try {
					t = e instanceof Window;
				} catch {
					t = !1;
				}
				return t;
			},
			__wbg_invalidateFramebuffer_9a711eeb3940aba0: function() {
				return z(function(e, t, n) {
					e.invalidateFramebuffer(t >>> 0, n);
				}, arguments);
			},
			__wbg_isArray_6339f732981044bf: function(e) {
				return Array.isArray(e);
			},
			__wbg_isFallbackAdapter_8ccb967428491dcb: function(e) {
				return e.isFallbackAdapter;
			},
			__wbg_isSafeInteger_f3d6cd19ccfe4512: function(e) {
				return Number.isSafeInteger(e);
			},
			__wbg_is_86be747e88e872fb: function(e, t) {
				return Object.is(e, t);
			},
			__wbg_iterator_5cebbb86e33c6dd6: function() {
				return Symbol.iterator;
			},
			__wbg_label_7ed42f25f841996b: function(e, t) {
				let n = t.label, r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W;
				F().setInt32(e + 4, i, !0), F().setInt32(e + 0, r, !0);
			},
			__wbg_left_5acbc01043b87b48: function(e) {
				return e.left;
			},
			__wbg_length_36bd29c6848c2144: function(e) {
				return e.length;
			},
			__wbg_length_ecfa2c63d3d0d82c: function(e) {
				return e.length;
			},
			__wbg_limits_20c6f56636df7d38: function(e) {
				return e.limits;
			},
			__wbg_limits_328c61cd41512420: function(e) {
				return e.limits;
			},
			__wbg_linkProgram_124252d16ea0ef40: function(e, t) {
				e.linkProgram(t);
			},
			__wbg_linkProgram_dd3cfc19950a354c: function(e, t) {
				e.linkProgram(t);
			},
			__wbg_mapAsync_52b01fa9e8f765fd: function(e, t, n, r) {
				return e.mapAsync(t >>> 0, n, r);
			},
			__wbg_matchMedia_0e2963d34f3ddd40: function() {
				return z(function(e, t, n) {
					let r = e.matchMedia(L(t, n));
					return B(r) ? 0 : O(r);
				}, arguments);
			},
			__wbg_matches_72427e51457a4411: function(e) {
				return e.matches;
			},
			__wbg_maxBindGroupsPlusVertexBuffers_33e5006b23e20478: function(e) {
				return e.maxBindGroupsPlusVertexBuffers;
			},
			__wbg_maxBindGroups_f6d26f3a67826666: function(e) {
				return e.maxBindGroups;
			},
			__wbg_maxBindingsPerBindGroup_edab2e8dabbf6060: function(e) {
				return e.maxBindingsPerBindGroup;
			},
			__wbg_maxBufferSize_bbc69284c14aa7da: function(e) {
				return e.maxBufferSize;
			},
			__wbg_maxColorAttachmentBytesPerSample_63ebe4f81de2f34c: function(e) {
				return e.maxColorAttachmentBytesPerSample;
			},
			__wbg_maxColorAttachments_aed8c38beabf3a5c: function(e) {
				return e.maxColorAttachments;
			},
			__wbg_maxComputeInvocationsPerWorkgroup_2d964564c37f1c65: function(e) {
				return e.maxComputeInvocationsPerWorkgroup;
			},
			__wbg_maxComputeWorkgroupSizeX_a3e3206570da184f: function(e) {
				return e.maxComputeWorkgroupSizeX;
			},
			__wbg_maxComputeWorkgroupSizeY_dffa4a62244b7563: function(e) {
				return e.maxComputeWorkgroupSizeY;
			},
			__wbg_maxComputeWorkgroupSizeZ_976ebcb760f6d07d: function(e) {
				return e.maxComputeWorkgroupSizeZ;
			},
			__wbg_maxComputeWorkgroupStorageSize_2e8dbece6e532e2a: function(e) {
				return e.maxComputeWorkgroupStorageSize;
			},
			__wbg_maxComputeWorkgroupsPerDimension_bb7d36b4d20c80f4: function(e) {
				return e.maxComputeWorkgroupsPerDimension;
			},
			__wbg_maxDynamicStorageBuffersPerPipelineLayout_1ca859cb96a414e0: function(e) {
				return e.maxDynamicStorageBuffersPerPipelineLayout;
			},
			__wbg_maxDynamicUniformBuffersPerPipelineLayout_e968f2c8cd8f4d46: function(e) {
				return e.maxDynamicUniformBuffersPerPipelineLayout;
			},
			__wbg_maxInterStageShaderVariables_138ac882c4d6a3d3: function(e) {
				return e.maxInterStageShaderVariables;
			},
			__wbg_maxSampledTexturesPerShaderStage_bb3e6b2698321fa6: function(e) {
				return e.maxSampledTexturesPerShaderStage;
			},
			__wbg_maxSamplersPerShaderStage_98c00a1829fa414b: function(e) {
				return e.maxSamplersPerShaderStage;
			},
			__wbg_maxStorageBufferBindingSize_e500e31f479e669e: function(e) {
				return e.maxStorageBufferBindingSize;
			},
			__wbg_maxStorageBuffersPerShaderStage_eb663f6d7521b6a7: function(e) {
				return e.maxStorageBuffersPerShaderStage;
			},
			__wbg_maxStorageTexturesPerShaderStage_bb3ad93b53e618c0: function(e) {
				return e.maxStorageTexturesPerShaderStage;
			},
			__wbg_maxTextureArrayLayers_2a56d05fb261c99a: function(e) {
				return e.maxTextureArrayLayers;
			},
			__wbg_maxTextureDimension1D_84590c1d4770d319: function(e) {
				return e.maxTextureDimension1D;
			},
			__wbg_maxTextureDimension2D_7f2b5c8b2727e3fc: function(e) {
				return e.maxTextureDimension2D;
			},
			__wbg_maxTextureDimension3D_7f3babddf55c32a6: function(e) {
				return e.maxTextureDimension3D;
			},
			__wbg_maxUniformBufferBindingSize_d80a09e23c0b284c: function(e) {
				return e.maxUniformBufferBindingSize;
			},
			__wbg_maxUniformBuffersPerShaderStage_0b8b2de676fa740e: function(e) {
				return e.maxUniformBuffersPerShaderStage;
			},
			__wbg_maxVertexAttributes_a693dd921316649b: function(e) {
				return e.maxVertexAttributes;
			},
			__wbg_maxVertexBufferArrayStride_f256d91f281076cb: function(e) {
				return e.maxVertexBufferArrayStride;
			},
			__wbg_maxVertexBuffers_70ab564b25d5ac20: function(e) {
				return e.maxVertexBuffers;
			},
			__wbg_minStorageBufferOffsetAlignment_3248ed00dcdbf79f: function(e) {
				return e.minStorageBufferOffsetAlignment;
			},
			__wbg_minUniformBufferOffsetAlignment_3b9fa4caae03e903: function(e) {
				return e.minUniformBufferOffsetAlignment;
			},
			__wbg_navigator_6cfdd5fa246d910f: function(e) {
				return e.navigator;
			},
			__wbg_navigator_e5c345298a9609cd: function(e) {
				return e.navigator;
			},
			__wbg_new_116be93542d39019: function() {
				return [];
			},
			__wbg_new_227d7c05414eb861: function() {
				return /* @__PURE__ */ Error();
			},
			__wbg_new_652118cdee90118f: function() {
				return z(function(e, t) {
					return new OffscreenCanvas(e >>> 0, t >>> 0);
				}, arguments);
			},
			__wbg_new_77cc4f4f472aeb81: function(e) {
				return new Uint8Array(e);
			},
			__wbg_new_cdf041679ded4c5f: function() {
				return /* @__PURE__ */ new Map();
			},
			__wbg_new_ebe3e0f6837f0879: function() {
				return {};
			},
			__wbg_new_typed_ad9b105a7be50737: function() {
				return {};
			},
			__wbg_new_typed_cceaf62d8d95e9f2: function(e, t) {
				try {
					var n = {
						a: e,
						b: t
					};
					return new Promise((e, t) => {
						let r = n.a;
						n.a = 0;
						try {
							return ee(r, n.b, e, t);
						} finally {
							n.a = r;
						}
					});
				} finally {
					n.a = 0;
				}
			},
			__wbg_new_with_byte_offset_and_length_ff6e927f8d72f0c3: function(e, t, n) {
				return new Uint8Array(e, t >>> 0, n >>> 0);
			},
			__wbg_next_42cf16ee0dafc9e2: function() {
				return z(function(e) {
					return e.next();
				}, arguments);
			},
			__wbg_next_8f26b64fa5e9f64b: function(e) {
				return e.next;
			},
			__wbg_now_2283802bfbda617e: function(e) {
				return e.now();
			},
			__wbg_of_0c6464fa8d2aa86d: function(e) {
				return [e];
			},
			__wbg_onSubmittedWorkDone_270d6b5a45520e79: function(e) {
				return e.onSubmittedWorkDone();
			},
			__wbg_parse_817cdcbb0323649b: function() {
				return z(function(e, t) {
					return JSON.parse(L(e, t));
				}, arguments);
			},
			__wbg_performance_821a3767f0dce300: function(e) {
				let t = e.performance;
				return B(t) ? 0 : O(t);
			},
			__wbg_pixelStorei_11bdfb5bc6a39d28: function(e, t, n) {
				e.pixelStorei(t >>> 0, n);
			},
			__wbg_pixelStorei_86481a168d6e225e: function(e, t, n) {
				e.pixelStorei(t >>> 0, n);
			},
			__wbg_polygonOffset_2b8b141e8cc17c10: function(e, t, n) {
				e.polygonOffset(t, n);
			},
			__wbg_polygonOffset_94b427c5130ab6c2: function(e, t, n) {
				e.polygonOffset(t, n);
			},
			__wbg_prototypesetcall_de8e0d9553586985: function(e, t, n) {
				Uint8Array.prototype.set.call(N(e, t), n);
			},
			__wbg_push_adb0107829f02d75: function(e, t) {
				return e.push(t);
			},
			__wbg_queryCounterEXT_b0cddcdfb28830df: function(e, t, n) {
				e.queryCounterEXT(t, n >>> 0);
			},
			__wbg_querySelectorAll_9b6a612499ecb916: function() {
				return z(function(e, t, n) {
					return e.querySelectorAll(L(t, n));
				}, arguments);
			},
			__wbg_querySelector_2c472eddb417c6b3: function() {
				return z(function(e, t, n) {
					let r = e.querySelector(L(t, n));
					return B(r) ? 0 : O(r);
				}, arguments);
			},
			__wbg_queueMicrotask_ac694eae12e92dfb: function(e) {
				queueMicrotask(e);
			},
			__wbg_queueMicrotask_be5fe34a8f4cad4d: function(e) {
				return e.queueMicrotask;
			},
			__wbg_queue_adce34608fd0c893: function(e) {
				return e.queue;
			},
			__wbg_readBuffer_2de0b72ac08915c8: function(e, t) {
				e.readBuffer(t >>> 0);
			},
			__wbg_readPixels_0033d2834b498dda: function() {
				return z(function(e, t, n, r, i, a, o, s) {
					e.readPixels(t, n, r, i, a >>> 0, o >>> 0, s);
				}, arguments);
			},
			__wbg_readPixels_0e3230bf7a891882: function() {
				return z(function(e, t, n, r, i, a, o, s) {
					e.readPixels(t, n, r, i, a >>> 0, o >>> 0, s);
				}, arguments);
			},
			__wbg_readPixels_8f8bde9ee420ba35: function() {
				return z(function(e, t, n, r, i, a, o, s) {
					e.readPixels(t, n, r, i, a >>> 0, o >>> 0, s);
				}, arguments);
			},
			__wbg_removeEventListener_aa653c6b402cc27e: function() {
				return z(function(e, t, n, r) {
					e.removeEventListener(L(t, n), r);
				}, arguments);
			},
			__wbg_renderbufferStorageMultisample_a9f65ef0cc53fb37: function(e, t, n, r, i, a) {
				e.renderbufferStorageMultisample(t >>> 0, n, r >>> 0, i, a);
			},
			__wbg_renderbufferStorage_33c57e600b175bd6: function(e, t, n, r, i) {
				e.renderbufferStorage(t >>> 0, n >>> 0, r, i);
			},
			__wbg_renderbufferStorage_3fb6d5a0f3e07d46: function(e, t, n, r, i) {
				e.renderbufferStorage(t >>> 0, n >>> 0, r, i);
			},
			__wbg_requestAdapter_2e6718811c735a57: function(e, t) {
				return e.requestAdapter(t);
			},
			__wbg_requestAdapter_fedd76261c649e55: function(e) {
				return e.requestAdapter();
			},
			__wbg_requestDevice_ab46d0519ea1cc34: function(e, t) {
				return e.requestDevice(t);
			},
			__wbg_resolve_020f95d838c6ef25: function(e) {
				return Promise.resolve(e);
			},
			__wbg_samplerParameterf_d7f38ba3194c43ba: function(e, t, n, r) {
				e.samplerParameterf(t, n >>> 0, r);
			},
			__wbg_samplerParameteri_3d8994d9967c6803: function(e, t, n, r) {
				e.samplerParameteri(t, n >>> 0, r);
			},
			__wbg_scissor_2f02706fbca6e98a: function(e, t, n, r, i) {
				e.scissor(t, n, r, i);
			},
			__wbg_scissor_cdfb84de20f004b6: function(e, t, n, r, i) {
				e.scissor(t, n, r, i);
			},
			__wbg_setBindGroup_268fd1714fff0ef5: function() {
				return z(function(e, t, n, r, i, a, o) {
					e.setBindGroup(t >>> 0, n, M(r, i), a, o >>> 0);
				}, arguments);
			},
			__wbg_setBindGroup_f0de6cb2c7dbfc2c: function(e, t, n) {
				e.setBindGroup(t >>> 0, n);
			},
			__wbg_setIndexBuffer_2531a9103450445e: function(e, t, n, r) {
				e.setIndexBuffer(t, le[n], r);
			},
			__wbg_setIndexBuffer_7f3cf667b4d71566: function(e, t, n, r, i) {
				e.setIndexBuffer(t, le[n], r, i);
			},
			__wbg_setPipeline_c41bf46790f27f9e: function(e, t) {
				e.setPipeline(t);
			},
			__wbg_setStencilReference_66c74be6232bb9da: function(e, t) {
				e.setStencilReference(t >>> 0);
			},
			__wbg_setVertexBuffer_1e448859663dd400: function(e, t, n, r) {
				e.setVertexBuffer(t >>> 0, n, r);
			},
			__wbg_setVertexBuffer_7cf533d694e747f3: function(e, t, n, r, i) {
				e.setVertexBuffer(t >>> 0, n, r, i);
			},
			__wbg_set_014226dfeca53178: function(e, t, n) {
				return e.set(t, n);
			},
			__wbg_set_6be42768c690e380: function(e, t, n) {
				e[t] = n;
			},
			__wbg_set_8155bb79a948541b: function() {
				return z(function(e, t, n) {
					return Reflect.set(e, t, n);
				}, arguments);
			},
			__wbg_set_862c439a342a8818: function(e, t, n) {
				e.set(t, n >>> 0);
			},
			__wbg_set_a80955eb93b145c6: function(e, t, n) {
				e[t >>> 0] = n;
			},
			__wbg_set_a_88262a42340d0b1c: function(e, t) {
				e.a = t;
			},
			__wbg_set_access_9a5092f05dc45fad: function(e, t) {
				e.access = ge[t];
			},
			__wbg_set_address_mode_u_9e2695575a219e33: function(e, t) {
				e.addressModeU = w[t];
			},
			__wbg_set_address_mode_v_f479b2e6cccbcac4: function(e, t) {
				e.addressModeV = w[t];
			},
			__wbg_set_address_mode_w_46273e153230180d: function(e, t) {
				e.addressModeW = w[t];
			},
			__wbg_set_alpha_bfd2df62e7bc581b: function(e, t) {
				e.alpha = t;
			},
			__wbg_set_alpha_mode_df805952892caa9c: function(e, t) {
				e.alphaMode = re[t];
			},
			__wbg_set_alpha_to_coverage_enabled_8b5dc2b0a225b3b2: function(e, t) {
				e.alphaToCoverageEnabled = t !== 0;
			},
			__wbg_set_array_layer_count_7312f0f31af94e7c: function(e, t) {
				e.arrayLayerCount = t >>> 0;
			},
			__wbg_set_array_stride_f64_27ffaf4fffd74e61: function(e, t) {
				e.arrayStride = t;
			},
			__wbg_set_aspect_0d453bca3d012f02: function(e, t) {
				e.aspect = ve[t];
			},
			__wbg_set_aspect_4962514fe99e68e6: function(e, t) {
				e.aspect = ve[t];
			},
			__wbg_set_attributes_7537844a7e6dafdc: function(e, t, n) {
				e.attributes = j(t, n);
			},
			__wbg_set_b_c47befe0af3261eb: function(e, t) {
				e.b = t;
			},
			__wbg_set_base_array_layer_f176bb9f1b37b342: function(e, t) {
				e.baseArrayLayer = t >>> 0;
			},
			__wbg_set_base_mip_level_1df145d9f8db32a9: function(e, t) {
				e.baseMipLevel = t >>> 0;
			},
			__wbg_set_beginning_of_pass_write_index_e9f5d016947893bd: function(e, t) {
				e.beginningOfPassWriteIndex = t >>> 0;
			},
			__wbg_set_bind_group_layouts_5a9cfea401c020ab: function(e, t, n) {
				e.bindGroupLayouts = j(t, n);
			},
			__wbg_set_binding_155b0440b4307793: function(e, t) {
				e.binding = t >>> 0;
			},
			__wbg_set_binding_f74df3510792aba1: function(e, t) {
				e.binding = t >>> 0;
			},
			__wbg_set_blend_7493c2066c3e9970: function(e, t) {
				e.blend = t;
			},
			__wbg_set_buffer_9c01e3b6d6765ea2: function(e, t) {
				e.buffer = t;
			},
			__wbg_set_buffer_c3410572051920ba: function(e, t) {
				e.buffer = t;
			},
			__wbg_set_buffer_ef7f75306cf663ed: function(e, t) {
				e.buffer = t;
			},
			__wbg_set_buffers_7d0d8f507699e956: function(e, t, n) {
				e.buffers = j(t, n);
			},
			__wbg_set_bytes_per_row_c54ca96953f35774: function(e, t) {
				e.bytesPerRow = t >>> 0;
			},
			__wbg_set_bytes_per_row_d69b88eee3929c07: function(e, t) {
				e.bytesPerRow = t >>> 0;
			},
			__wbg_set_clear_value_gpu_color_dict_6211425789c76e59: function(e, t) {
				e.clearValue = t;
			},
			__wbg_set_code_b4f37f81f45b5b25: function(e, t, n) {
				e.code = L(t, n);
			},
			__wbg_set_color_83aa977526e88cbb: function(e, t) {
				e.color = t;
			},
			__wbg_set_color_attachments_581fdb3310e4abfa: function(e, t, n) {
				e.colorAttachments = j(t, n);
			},
			__wbg_set_compare_cd9b62cdb92eb580: function(e, t) {
				e.compare = ae[t];
			},
			__wbg_set_compare_f36b34abfaa08ccb: function(e, t) {
				e.compare = ae[t];
			},
			__wbg_set_count_069a4eac409bac55: function(e, t) {
				e.count = t >>> 0;
			},
			__wbg_set_cull_mode_fc649853947a3d0c: function(e, t) {
				e.cullMode = oe[t];
			},
			__wbg_set_depth_bias_clamp_1c0d695df7f092e5: function(e, t) {
				e.depthBiasClamp = t;
			},
			__wbg_set_depth_bias_d7cd16096242a657: function(e, t) {
				e.depthBias = t;
			},
			__wbg_set_depth_bias_slope_scale_c4e52ec743ef55ba: function(e, t) {
				e.depthBiasSlopeScale = t;
			},
			__wbg_set_depth_clear_value_beda3ec5b1a5c43a: function(e, t) {
				e.depthClearValue = t;
			},
			__wbg_set_depth_compare_0c8631eb2eae98e3: function(e, t) {
				e.depthCompare = ae[t];
			},
			__wbg_set_depth_fail_op_668155ae33d3c06f: function(e, t) {
				e.depthFailOp = he[t];
			},
			__wbg_set_depth_load_op_511c513eab4e56a9: function(e, t) {
				e.depthLoadOp = ue[t];
			},
			__wbg_set_depth_or_array_layers_89371305ed0bd962: function(e, t) {
				e.depthOrArrayLayers = t >>> 0;
			},
			__wbg_set_depth_read_only_7f41a74741c144ec: function(e, t) {
				e.depthReadOnly = t !== 0;
			},
			__wbg_set_depth_stencil_97506c7bea4f53da: function(e, t) {
				e.depthStencil = t;
			},
			__wbg_set_depth_stencil_attachment_73b79e8b4e948222: function(e, t) {
				e.depthStencilAttachment = t;
			},
			__wbg_set_depth_store_op_c89f33b39b43361c: function(e, t) {
				e.depthStoreOp = _e[t];
			},
			__wbg_set_depth_write_enabled_ce89750042940350: function(e, t) {
				e.depthWriteEnabled = t !== 0;
			},
			__wbg_set_device_e275d1d4f3c9eb74: function(e, t) {
				e.device = t;
			},
			__wbg_set_dimension_868eee80f4b90011: function(e, t) {
				e.dimension = ye[t];
			},
			__wbg_set_dimension_e325282e613ca0a4: function(e, t) {
				e.dimension = xe[t];
			},
			__wbg_set_dst_factor_ec7407f19be1aff9: function(e, t) {
				e.dstFactor = te[t];
			},
			__wbg_set_end_of_pass_write_index_0d546e46b86ea069: function(e, t) {
				e.endOfPassWriteIndex = t >>> 0;
			},
			__wbg_set_entries_86a29dd6291c95e7: function(e, t, n) {
				e.entries = j(t, n);
			},
			__wbg_set_entries_a12aca1e458b0456: function(e, t, n) {
				e.entries = j(t, n);
			},
			__wbg_set_entry_point_207540f042015ce5: function(e, t, n) {
				e.entryPoint = L(t, n);
			},
			__wbg_set_entry_point_e87e79251dd3144f: function(e, t, n) {
				e.entryPoint = L(t, n);
			},
			__wbg_set_external_texture_386483d8dd82ab56: function(e, t) {
				e.externalTexture = t;
			},
			__wbg_set_fail_op_92f716dbc88b6973: function(e, t) {
				e.failOp = he[t];
			},
			__wbg_set_format_1fcaa7d60546b490: function(e, t) {
				e.format = D[t];
			},
			__wbg_set_format_2c1414a817c213f8: function(e, t) {
				e.format = D[t];
			},
			__wbg_set_format_533f9ffa7eef563d: function(e, t) {
				e.format = D[t];
			},
			__wbg_set_format_5d2f25cc93654ecc: function(e, t) {
				e.format = Se[t];
			},
			__wbg_set_format_5ff53724ed6cedf2: function(e, t) {
				e.format = D[t];
			},
			__wbg_set_format_815efd4dc4817bbb: function(e, t) {
				e.format = D[t];
			},
			__wbg_set_format_e52bdcca880d2c8e: function(e, t) {
				e.format = D[t];
			},
			__wbg_set_fragment_8b780f00a0b0e6f3: function(e, t) {
				e.fragment = t;
			},
			__wbg_set_front_face_28ffdf524eedce5b: function(e, t) {
				e.frontFace = ce[t];
			},
			__wbg_set_g_5983abfc46e0cf4e: function(e, t) {
				e.g = t;
			},
			__wbg_set_has_dynamic_offset_62bc230bdb7c54d0: function(e, t) {
				e.hasDynamicOffset = t !== 0;
			},
			__wbg_set_height_14335c4047cf9c1b: function(e, t) {
				e.height = t >>> 0;
			},
			__wbg_set_height_ca39bd9597314f83: function(e, t) {
				e.height = t >>> 0;
			},
			__wbg_set_height_d72f2b76484a44de: function(e, t) {
				e.height = t >>> 0;
			},
			__wbg_set_label_08d9be3e4719c226: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_17eb9fe3a02f62b0: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_48e6b787d256f621: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_547d0d4aec39fbe9: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_5ee7427342869829: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_60ad96c811e0d109: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_72bb4f41ef0cb893: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_79387decda299036: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_9556af8b5cda3c9d: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_d010f237b26f2c55: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_e16e2dbe51349c7f: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_e3944e54881b8c50: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_label_e922700240417ab5: function(e, t, n) {
				e.label = L(t, n);
			},
			__wbg_set_layout_50ab727f44b38f26: function(e, t) {
				e.layout = t;
			},
			__wbg_set_layout_913d53c17194c989: function(e, t) {
				e.layout = t;
			},
			__wbg_set_layout_gpu_auto_layout_mode_aeba193938b47882: function(e, t) {
				e.layout = T[t];
			},
			__wbg_set_load_op_99661da6c4eab9b0: function(e, t) {
				e.loadOp = ue[t];
			},
			__wbg_set_lod_max_clamp_dd2d9f9f052f4f44: function(e, t) {
				e.lodMaxClamp = t;
			},
			__wbg_set_lod_min_clamp_6d20c97916baeb93: function(e, t) {
				e.lodMinClamp = t;
			},
			__wbg_set_mag_filter_b5adebc99cb938e1: function(e, t) {
				e.magFilter = se[t];
			},
			__wbg_set_mapped_at_creation_81b586dc90a50347: function(e, t) {
				e.mappedAtCreation = t !== 0;
			},
			__wbg_set_mask_70a8a59ce09e5997: function(e, t) {
				e.mask = t >>> 0;
			},
			__wbg_set_max_anisotropy_2beada0e2db62c45: function(e, t) {
				e.maxAnisotropy = t;
			},
			__wbg_set_min_binding_size_f64_5005a6904cdf43da: function(e, t) {
				e.minBindingSize = t;
			},
			__wbg_set_min_filter_c72f17375e135f0a: function(e, t) {
				e.minFilter = se[t];
			},
			__wbg_set_mip_level_count_534caaa7e68e68b8: function(e, t) {
				e.mipLevelCount = t >>> 0;
			},
			__wbg_set_mip_level_count_776c8c218b65bc08: function(e, t) {
				e.mipLevelCount = t >>> 0;
			},
			__wbg_set_mip_level_f7ac79e8c54f59ad: function(e, t) {
				e.mipLevel = t >>> 0;
			},
			__wbg_set_mipmap_filter_5bf66195a3639700: function(e, t) {
				e.mipmapFilter = de[t];
			},
			__wbg_set_mode_9990b3393ba469ae: function(e, t) {
				e.mode = ie[t];
			},
			__wbg_set_module_d0e2098713606cae: function(e, t) {
				e.module = t;
			},
			__wbg_set_module_f02e076ca7e7daf8: function(e, t) {
				e.module = t;
			},
			__wbg_set_multisample_37ddafe88b5cd466: function(e, t) {
				e.multisample = t;
			},
			__wbg_set_multisampled_7913fd7183272840: function(e, t) {
				e.multisampled = t !== 0;
			},
			__wbg_set_offset_f64_28c24dc15000932e: function(e, t) {
				e.offset = t;
			},
			__wbg_set_offset_f64_89f0ce01a689839e: function(e, t) {
				e.offset = t;
			},
			__wbg_set_offset_f64_b562d1367e34ef93: function(e, t) {
				e.offset = t;
			},
			__wbg_set_offset_f64_fa66068813376ca3: function(e, t) {
				e.offset = t;
			},
			__wbg_set_operation_62ce44e1728c4047: function(e, t) {
				e.operation = E[t];
			},
			__wbg_set_origin_gpu_origin_3d_dict_631c04520718091f: function(e, t) {
				e.origin = t;
			},
			__wbg_set_pass_op_cf02fa088d6352a7: function(e, t) {
				e.passOp = he[t];
			},
			__wbg_set_power_preference_8fdca0b7af640d49: function(e, t) {
				e.powerPreference = fe[t];
			},
			__wbg_set_primitive_43c23761a55b4088: function(e, t) {
				e.primitive = t;
			},
			__wbg_set_query_set_41de86d2401aee04: function(e, t) {
				e.querySet = t;
			},
			__wbg_set_r_c6f4c68f4804d655: function(e, t) {
				e.r = t;
			},
			__wbg_set_required_features_1baf274a8669db60: function(e, t, n) {
				e.requiredFeatures = j(t, n);
			},
			__wbg_set_required_limits_871ed33c68613dcb: function(e, t) {
				e.requiredLimits = t;
			},
			__wbg_set_resolve_target_gpu_texture_view_b19a4f2debf79b96: function(e, t) {
				e.resolveTarget = t;
			},
			__wbg_set_resource_5ae7b5e67924f234: function(e, t) {
				e.resource = t;
			},
			__wbg_set_resource_gpu_buffer_binding_e5dbca063e7cb67b: function(e, t) {
				e.resource = t;
			},
			__wbg_set_resource_gpu_texture_view_eb46c355d51ad7e5: function(e, t) {
				e.resource = t;
			},
			__wbg_set_rows_per_image_5011f97318ee71af: function(e, t) {
				e.rowsPerImage = t >>> 0;
			},
			__wbg_set_rows_per_image_59a813ac5006e10e: function(e, t) {
				e.rowsPerImage = t >>> 0;
			},
			__wbg_set_sample_count_eb86a8b18545b54f: function(e, t) {
				e.sampleCount = t >>> 0;
			},
			__wbg_set_sample_type_c32e1dfff94e63eb: function(e, t) {
				e.sampleType = be[t];
			},
			__wbg_set_sampler_c0e1258543a33bce: function(e, t) {
				e.sampler = t;
			},
			__wbg_set_shader_location_7e1832a74f912217: function(e, t) {
				e.shaderLocation = t >>> 0;
			},
			__wbg_set_size_f64_6bcd40704bf4cfdc: function(e, t) {
				e.size = t;
			},
			__wbg_set_size_f64_8b8f6bba5d678162: function(e, t) {
				e.size = t;
			},
			__wbg_set_size_gpu_extent_3d_dict_7e42e1c98fa36434: function(e, t) {
				e.size = t;
			},
			__wbg_set_src_factor_9bfe84af9b7b5cac: function(e, t) {
				e.srcFactor = te[t];
			},
			__wbg_set_stencil_back_85b22f1db5b1940a: function(e, t) {
				e.stencilBack = t;
			},
			__wbg_set_stencil_clear_value_42be608809151e2a: function(e, t) {
				e.stencilClearValue = t >>> 0;
			},
			__wbg_set_stencil_front_525526164a798a44: function(e, t) {
				e.stencilFront = t;
			},
			__wbg_set_stencil_load_op_31838c036993098a: function(e, t) {
				e.stencilLoadOp = ue[t];
			},
			__wbg_set_stencil_read_mask_5cc26495e8b3ae82: function(e, t) {
				e.stencilReadMask = t >>> 0;
			},
			__wbg_set_stencil_read_only_bf1d0c1897e25c62: function(e, t) {
				e.stencilReadOnly = t !== 0;
			},
			__wbg_set_stencil_store_op_e6be1cbc3a8fc210: function(e, t) {
				e.stencilStoreOp = _e[t];
			},
			__wbg_set_stencil_write_mask_d9cb40ec4b4bee5b: function(e, t) {
				e.stencilWriteMask = t >>> 0;
			},
			__wbg_set_step_mode_a97bb24714da41a9: function(e, t) {
				e.stepMode = Ce[t];
			},
			__wbg_set_storage_texture_939a097db4b18bd4: function(e, t) {
				e.storageTexture = t;
			},
			__wbg_set_store_op_b5fdf672436f13f3: function(e, t) {
				e.storeOp = _e[t];
			},
			__wbg_set_strip_index_format_9f787be6c5fc9e87: function(e, t) {
				e.stripIndexFormat = le[t];
			},
			__wbg_set_targets_c38bd200c836d66f: function(e, t, n) {
				e.targets = j(t, n);
			},
			__wbg_set_texture_1f64653a5d2d7b4d: function(e, t) {
				e.texture = t;
			},
			__wbg_set_texture_9dcedde1bb31eda6: function(e, t) {
				e.texture = t;
			},
			__wbg_set_timestamp_writes_98bed1a8bbc6682d: function(e, t) {
				e.timestampWrites = t;
			},
			__wbg_set_tone_mapping_b3464f1baa4cff92: function(e, t) {
				e.toneMapping = t;
			},
			__wbg_set_topology_da25f2cc5af203d2: function(e, t) {
				e.topology = pe[t];
			},
			__wbg_set_type_ccf8472d40abcddf: function(e, t) {
				e.type = me[t];
			},
			__wbg_set_type_d09829f59932a0fc: function(e, t) {
				e.type = ne[t];
			},
			__wbg_set_unclipped_depth_04524a2b44e1e3c1: function(e, t) {
				e.unclippedDepth = t !== 0;
			},
			__wbg_set_usage_a137f82ca163b0a9: function(e, t) {
				e.usage = t >>> 0;
			},
			__wbg_set_usage_b2a2935f37bf3d08: function(e, t) {
				e.usage = t >>> 0;
			},
			__wbg_set_usage_ba5b0f8b333ab325: function(e, t) {
				e.usage = t >>> 0;
			},
			__wbg_set_usage_ddd42599bbba7779: function(e, t) {
				e.usage = t >>> 0;
			},
			__wbg_set_vertex_0be5d146f9ff6f36: function(e, t) {
				e.vertex = t;
			},
			__wbg_set_view_dimension_0df554032f1f3a85: function(e, t) {
				e.viewDimension = xe[t];
			},
			__wbg_set_view_dimension_4818d4c18ce5815e: function(e, t) {
				e.viewDimension = xe[t];
			},
			__wbg_set_view_formats_4347dc8363331086: function(e, t, n) {
				e.viewFormats = j(t, n);
			},
			__wbg_set_view_formats_5797d2fff3c11808: function(e, t, n) {
				e.viewFormats = j(t, n);
			},
			__wbg_set_view_gpu_texture_view_9b2d86b6b99d9fd9: function(e, t) {
				e.view = t;
			},
			__wbg_set_view_gpu_texture_view_c0f35f8857c25206: function(e, t) {
				e.view = t;
			},
			__wbg_set_visibility_9570b037224c4cc2: function(e, t) {
				e.visibility = t >>> 0;
			},
			__wbg_set_width_36ef6630b22fc519: function(e, t) {
				e.width = t >>> 0;
			},
			__wbg_set_width_661c95ea46b71eba: function(e, t) {
				e.width = t >>> 0;
			},
			__wbg_set_width_9f685402c2cbee70: function(e, t) {
				e.width = t >>> 0;
			},
			__wbg_set_write_mask_d45279e56abbfcb5: function(e, t) {
				e.writeMask = t >>> 0;
			},
			__wbg_set_x_876d592971db129a: function(e, t) {
				e.x = t >>> 0;
			},
			__wbg_set_y_2b1f5ac0dd5586a5: function(e, t) {
				e.y = t >>> 0;
			},
			__wbg_set_z_ef005d82bc9d24e3: function(e, t) {
				e.z = t >>> 0;
			},
			__wbg_shaderSource_7d3f360b4b626db7: function(e, t, n, r) {
				e.shaderSource(t, L(n, r));
			},
			__wbg_shaderSource_dcba4cd3379b35bd: function(e, t, n, r) {
				e.shaderSource(t, L(n, r));
			},
			__wbg_stack_3b0d974bbf31e44f: function(e, t) {
				let n = t.stack, r = H(n, G.__wbindgen_malloc, G.__wbindgen_realloc), i = W;
				F().setInt32(e + 4, i, !0), F().setInt32(e + 0, r, !0);
			},
			__wbg_static_accessor_GLOBAL_THIS_466428f93b4eaa76: function() {
				let e = typeof globalThis > "u" ? null : globalThis;
				return B(e) ? 0 : O(e);
			},
			__wbg_static_accessor_GLOBAL_c7aea38d4de089bc: function() {
				let e = typeof global > "u" ? null : global;
				return B(e) ? 0 : O(e);
			},
			__wbg_static_accessor_SELF_42d4fae05e59267a: function() {
				let e = typeof self > "u" ? null : self;
				return B(e) ? 0 : O(e);
			},
			__wbg_static_accessor_WINDOW_e0db14a0eba6a812: function() {
				let e = typeof window > "u" ? null : window;
				return B(e) ? 0 : O(e);
			},
			__wbg_stencilFuncSeparate_1455ac65895207da: function(e, t, n, r, i) {
				e.stencilFuncSeparate(t >>> 0, n >>> 0, r, i >>> 0);
			},
			__wbg_stencilFuncSeparate_55627e589746e09f: function(e, t, n, r, i) {
				e.stencilFuncSeparate(t >>> 0, n >>> 0, r, i >>> 0);
			},
			__wbg_stencilMaskSeparate_85d929ff95496631: function(e, t, n) {
				e.stencilMaskSeparate(t >>> 0, n >>> 0);
			},
			__wbg_stencilMaskSeparate_8e37bf59a93afc15: function(e, t, n) {
				e.stencilMaskSeparate(t >>> 0, n >>> 0);
			},
			__wbg_stencilMask_020d2d7ea8e4f640: function(e, t) {
				e.stencilMask(t >>> 0);
			},
			__wbg_stencilMask_967f16a89bfd056a: function(e, t) {
				e.stencilMask(t >>> 0);
			},
			__wbg_stencilOpSeparate_1f45c75c83dad8d5: function(e, t, n, r, i) {
				e.stencilOpSeparate(t >>> 0, n >>> 0, r >>> 0, i >>> 0);
			},
			__wbg_stencilOpSeparate_33a6764dd0ce6e24: function(e, t, n, r, i) {
				e.stencilOpSeparate(t >>> 0, n >>> 0, r >>> 0, i >>> 0);
			},
			__wbg_subgroupMaxSize_1527c5f7a8fe91bb: function(e) {
				return e.subgroupMaxSize;
			},
			__wbg_subgroupMinSize_d6c5ad4bddc828e9: function(e) {
				return e.subgroupMinSize;
			},
			__wbg_submit_ce44115121cd166c: function(e, t, n) {
				e.submit(j(t, n));
			},
			__wbg_texImage2D_053488112c3d702f: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texImage2D_2854247ff7d047a1: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texImage2D_44740302c934daf1: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texImage3D_d23f7d2f9e66b916: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u) {
					e.texImage3D(t >>> 0, n, r, i, a, o, s, c >>> 0, l >>> 0, u);
				}, arguments);
			},
			__wbg_texImage3D_faae3ea3f2969ecc: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u) {
					e.texImage3D(t >>> 0, n, r, i, a, o, s, c >>> 0, l >>> 0, u);
				}, arguments);
			},
			__wbg_texParameteri_2bc38aa8e9964d77: function(e, t, n, r) {
				e.texParameteri(t >>> 0, n >>> 0, r);
			},
			__wbg_texParameteri_dd4f56c2acbbe859: function(e, t, n, r) {
				e.texParameteri(t >>> 0, n >>> 0, r);
			},
			__wbg_texStorage2D_d473a12d49d7deee: function(e, t, n, r, i, a) {
				e.texStorage2D(t >>> 0, n, r >>> 0, i, a);
			},
			__wbg_texStorage3D_3ceb25ba9ad4b7ac: function(e, t, n, r, i, a, o) {
				e.texStorage3D(t >>> 0, n, r >>> 0, i, a, o);
			},
			__wbg_texSubImage2D_1b383b66dfe35010: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_205cfbaea80e77e6: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_606540d3e650e0bb: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_62ae3d4b2700f7cd: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_6eb05d8f455f99ba: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_a035d2307e014a73: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_ad5a64d8f68a2d0d: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage2D_cb9ad676165c5da5: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l) {
					e.texSubImage2D(t >>> 0, n, r, i, a, o, s >>> 0, c >>> 0, l);
				}, arguments);
			},
			__wbg_texSubImage3D_09e44c66b4ac6bc6: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_texSubImage3D_16678785ac62fd6b: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_texSubImage3D_3ee8764dfdcb6746: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_texSubImage3D_53489be691cee78d: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_texSubImage3D_73d365baf8dad003: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_texSubImage3D_8a2331639ee1ee0e: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_texSubImage3D_9b0bd9fd73d7bb1c: function() {
				return z(function(e, t, n, r, i, a, o, s, c, l, u, d) {
					e.texSubImage3D(t >>> 0, n, r, i, a, o, s, c, l >>> 0, u >>> 0, d);
				}, arguments);
			},
			__wbg_then_7026b513a94278a8: function(e, t) {
				return e.then(t);
			},
			__wbg_then_72819b8d4e081fb5: function(e, t, n) {
				return e.then(t, n);
			},
			__wbg_top_66d56bb4eca7d9c5: function(e) {
				return e.top;
			},
			__wbg_unconfigure_0a07a0a40de8988d: function(e) {
				e.unconfigure();
			},
			__wbg_uniform1f_e92095ce29c38424: function(e, t, n) {
				e.uniform1f(t, n);
			},
			__wbg_uniform1f_e93503bc589b432d: function(e, t, n) {
				e.uniform1f(t, n);
			},
			__wbg_uniform1i_235dff1d94e0df95: function(e, t, n) {
				e.uniform1i(t, n);
			},
			__wbg_uniform1i_d5db9c3184abbd04: function(e, t, n) {
				e.uniform1i(t, n);
			},
			__wbg_uniform1ui_8bbaaa1161bfd433: function(e, t, n) {
				e.uniform1ui(t, n >>> 0);
			},
			__wbg_uniform2fv_1443080aaf9c1077: function(e, t, n, r) {
				e.uniform2fv(t, k(n, r));
			},
			__wbg_uniform2fv_b039f28911c30526: function(e, t, n, r) {
				e.uniform2fv(t, k(n, r));
			},
			__wbg_uniform2iv_9648a06d054a25aa: function(e, t, n, r) {
				e.uniform2iv(t, A(n, r));
			},
			__wbg_uniform2iv_e0496dc424dc25ec: function(e, t, n, r) {
				e.uniform2iv(t, A(n, r));
			},
			__wbg_uniform2uiv_935dfb31f50dfbe3: function(e, t, n, r) {
				e.uniform2uiv(t, M(n, r));
			},
			__wbg_uniform3fv_025760367cc4eed3: function(e, t, n, r) {
				e.uniform3fv(t, k(n, r));
			},
			__wbg_uniform3fv_b985d45f54156d3b: function(e, t, n, r) {
				e.uniform3fv(t, k(n, r));
			},
			__wbg_uniform3iv_193b7a0e1ae9ac9a: function(e, t, n, r) {
				e.uniform3iv(t, A(n, r));
			},
			__wbg_uniform3iv_63e82687b07e66fc: function(e, t, n, r) {
				e.uniform3iv(t, A(n, r));
			},
			__wbg_uniform3uiv_ccd86b78a5fb3077: function(e, t, n, r) {
				e.uniform3uiv(t, M(n, r));
			},
			__wbg_uniform4f_61192d516e9bede4: function(e, t, n, r, i, a) {
				e.uniform4f(t, n, r, i, a);
			},
			__wbg_uniform4f_d9bb623add5d2541: function(e, t, n, r, i, a) {
				e.uniform4f(t, n, r, i, a);
			},
			__wbg_uniform4fv_c39527800fc76c8e: function(e, t, n, r) {
				e.uniform4fv(t, k(n, r));
			},
			__wbg_uniform4fv_fcff56a650906708: function(e, t, n, r) {
				e.uniform4fv(t, k(n, r));
			},
			__wbg_uniform4iv_197c2f54a8dfb5c2: function(e, t, n, r) {
				e.uniform4iv(t, A(n, r));
			},
			__wbg_uniform4iv_9e6e36f0e1d1f84d: function(e, t, n, r) {
				e.uniform4iv(t, A(n, r));
			},
			__wbg_uniform4uiv_73fc9e298d02c948: function(e, t, n, r) {
				e.uniform4uiv(t, M(n, r));
			},
			__wbg_uniformBlockBinding_057177606c8b522f: function(e, t, n, r) {
				e.uniformBlockBinding(t, n >>> 0, r >>> 0);
			},
			__wbg_uniformMatrix2fv_013723900a9cb65c: function(e, t, n, r, i) {
				e.uniformMatrix2fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix2fv_fb61eccac67a8218: function(e, t, n, r, i) {
				e.uniformMatrix2fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix2x3fv_de8b00219f47ffb4: function(e, t, n, r, i) {
				e.uniformMatrix2x3fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix2x4fv_e659cc34e95fee5e: function(e, t, n, r, i) {
				e.uniformMatrix2x4fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix3fv_3e548032fc28c3e2: function(e, t, n, r, i) {
				e.uniformMatrix3fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix3fv_72ca83d3393e0364: function(e, t, n, r, i) {
				e.uniformMatrix3fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix3x2fv_8598636e806d318d: function(e, t, n, r, i) {
				e.uniformMatrix3x2fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix3x4fv_277fbf38db85e612: function(e, t, n, r, i) {
				e.uniformMatrix3x4fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix4fv_20161efad644f822: function(e, t, n, r, i) {
				e.uniformMatrix4fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix4fv_8689fd0481ac5ab4: function(e, t, n, r, i) {
				e.uniformMatrix4fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix4x2fv_e91bd4e774f6266d: function(e, t, n, r, i) {
				e.uniformMatrix4x2fv(t, n !== 0, k(r, i));
			},
			__wbg_uniformMatrix4x3fv_a829c88dfd0c29d3: function(e, t, n, r, i) {
				e.uniformMatrix4x3fv(t, n !== 0, k(r, i));
			},
			__wbg_unmap_adaf93276fdf9aaf: function(e) {
				e.unmap();
			},
			__wbg_useProgram_1c047de878f20b72: function(e, t) {
				e.useProgram(t);
			},
			__wbg_useProgram_9edff145e073d3b1: function(e, t) {
				e.useProgram(t);
			},
			__wbg_value_1e2369fab29b420e: function(e) {
				return e.value;
			},
			__wbg_vertexAttribDivisorANGLE_581f060f68a0c850: function(e, t, n) {
				e.vertexAttribDivisorANGLE(t >>> 0, n >>> 0);
			},
			__wbg_vertexAttribDivisor_f910af52b19ce382: function(e, t, n) {
				e.vertexAttribDivisor(t >>> 0, n >>> 0);
			},
			__wbg_vertexAttribIPointer_54e6be6fa5e39567: function(e, t, n, r, i, a) {
				e.vertexAttribIPointer(t >>> 0, n, r >>> 0, i, a);
			},
			__wbg_vertexAttribPointer_7bc186aca7721b90: function(e, t, n, r, i, a, o) {
				e.vertexAttribPointer(t >>> 0, n, r >>> 0, i !== 0, a, o);
			},
			__wbg_vertexAttribPointer_b0838f8618a8c446: function(e, t, n, r, i, a, o) {
				e.vertexAttribPointer(t >>> 0, n, r >>> 0, i !== 0, a, o);
			},
			__wbg_viewport_07bb1829f0fe2245: function(e, t, n, r, i) {
				e.viewport(t, n, r, i);
			},
			__wbg_viewport_dfe81d333ce7be86: function(e, t, n, r, i) {
				e.viewport(t, n, r, i);
			},
			__wbg_wasmboneik_new: function(e) {
				return h.__wrap(e);
			},
			__wbg_wasmeditor_new: function(e) {
				return g.__wrap(e);
			},
			__wbg_wasmplayer_new: function(e) {
				return _.__wrap(e);
			},
			__wbg_width_1952934caca67137: function(e) {
				return e.width;
			},
			__wbg_writeBuffer_8b5bd251a89198bc: function() {
				return z(function(e, t, n, r, i, a, o) {
					e.writeBuffer(t, n, N(r, i), a, o);
				}, arguments);
			},
			__wbg_writeTexture_53ba204c494b042c: function() {
				return z(function(e, t, n, r, i, a) {
					e.writeTexture(t, N(n, r), i, a);
				}, arguments);
			},
			__wbindgen_cast_0000000000000001: function(e, t) {
				return qe(e, t, b);
			},
			__wbindgen_cast_0000000000000002: function(e, t) {
				return qe(e, t, x);
			},
			__wbindgen_cast_0000000000000003: function(e, t) {
				return qe(e, t, y);
			},
			__wbindgen_cast_0000000000000004: function(e, t) {
				return qe(e, t, S);
			},
			__wbindgen_cast_0000000000000005: function(e, t) {
				return qe(e, t, C);
			},
			__wbindgen_cast_0000000000000006: function(e) {
				return e;
			},
			__wbindgen_cast_0000000000000007: function(e) {
				return e;
			},
			__wbindgen_cast_0000000000000008: function(e, t) {
				return k(e, t);
			},
			__wbindgen_cast_0000000000000009: function(e, t) {
				return ke(e, t);
			},
			__wbindgen_cast_000000000000000a: function(e, t) {
				return A(e, t);
			},
			__wbindgen_cast_000000000000000b: function(e, t) {
				return Ae(e, t);
			},
			__wbindgen_cast_000000000000000c: function(e, t) {
				return Me(e, t);
			},
			__wbindgen_cast_000000000000000d: function(e, t) {
				return M(e, t);
			},
			__wbindgen_cast_000000000000000e: function(e, t) {
				return N(e, t);
			},
			__wbindgen_cast_000000000000000f: function(e, t) {
				return L(e, t);
			},
			__wbindgen_cast_0000000000000010: function(e) {
				return BigInt.asUintN(64, e);
			},
			__wbindgen_init_externref_table: function() {
				let e = G.__wbindgen_externrefs, t = e.grow(4);
				e.set(0, void 0), e.set(t + 0, void 0), e.set(t + 1, null), e.set(t + 2, !0), e.set(t + 3, !1);
			}
		}
	};
}
function y(e, t, n) {
	G.wasm_bindgen__convert__closures_____invoke__h36fbaf31d1926eb4(e, t, n);
}
function b(e, t, n) {
	let r = G.wasm_bindgen__convert__closures_____invoke__h8a34eac7d22ddaa9(e, t, n);
	if (r[1]) throw U(r[0]);
}
function x(e, t, n) {
	let r = G.wasm_bindgen__convert__closures_____invoke__h38ba885d5c890306(e, t, n);
	if (r[1]) throw U(r[0]);
}
function S(e, t, n) {
	let r = G.wasm_bindgen__convert__closures_____invoke__h38ba885d5c890306_3(e, t, n);
	if (r[1]) throw U(r[0]);
}
function C(e, t, n) {
	let r = G.wasm_bindgen__convert__closures_____invoke__h38ba885d5c890306_4(e, t, n);
	if (r[1]) throw U(r[0]);
}
function ee(e, t, n, r) {
	G.wasm_bindgen__convert__closures_____invoke__h851bff43d8d0d8b2(e, t, n, r);
}
var w = [
	"clamp-to-edge",
	"repeat",
	"mirror-repeat"
], T = ["auto"], te = [
	"zero",
	"one",
	"src",
	"one-minus-src",
	"src-alpha",
	"one-minus-src-alpha",
	"dst",
	"one-minus-dst",
	"dst-alpha",
	"one-minus-dst-alpha",
	"src-alpha-saturated",
	"constant",
	"one-minus-constant",
	"src1",
	"one-minus-src1",
	"src1-alpha",
	"one-minus-src1-alpha"
], E = [
	"add",
	"subtract",
	"reverse-subtract",
	"min",
	"max"
], ne = [
	"uniform",
	"storage",
	"read-only-storage"
], re = ["opaque", "premultiplied"], ie = ["standard", "extended"], ae = [
	"never",
	"less",
	"equal",
	"less-equal",
	"greater",
	"not-equal",
	"greater-equal",
	"always"
], oe = [
	"none",
	"front",
	"back"
], se = ["nearest", "linear"], ce = ["ccw", "cw"], le = ["uint16", "uint32"], ue = ["load", "clear"], de = ["nearest", "linear"], fe = ["low-power", "high-performance"], pe = [
	"point-list",
	"line-list",
	"line-strip",
	"triangle-list",
	"triangle-strip"
], me = [
	"filtering",
	"non-filtering",
	"comparison"
], he = [
	"keep",
	"zero",
	"replace",
	"invert",
	"increment-clamp",
	"decrement-clamp",
	"increment-wrap",
	"decrement-wrap"
], ge = [
	"write-only",
	"read-only",
	"read-write"
], _e = ["store", "discard"], ve = [
	"all",
	"stencil-only",
	"depth-only"
], ye = [
	"1d",
	"2d",
	"3d"
], D = /* @__PURE__ */ "r8unorm.r8snorm.r8uint.r8sint.r16unorm.r16snorm.r16uint.r16sint.r16float.rg8unorm.rg8snorm.rg8uint.rg8sint.r32uint.r32sint.r32float.rg16unorm.rg16snorm.rg16uint.rg16sint.rg16float.rgba8unorm.rgba8unorm-srgb.rgba8snorm.rgba8uint.rgba8sint.bgra8unorm.bgra8unorm-srgb.rgb9e5ufloat.rgb10a2uint.rgb10a2unorm.rg11b10ufloat.rg32uint.rg32sint.rg32float.rgba16unorm.rgba16snorm.rgba16uint.rgba16sint.rgba16float.rgba32uint.rgba32sint.rgba32float.stencil8.depth16unorm.depth24plus.depth24plus-stencil8.depth32float.depth32float-stencil8.bc1-rgba-unorm.bc1-rgba-unorm-srgb.bc2-rgba-unorm.bc2-rgba-unorm-srgb.bc3-rgba-unorm.bc3-rgba-unorm-srgb.bc4-r-unorm.bc4-r-snorm.bc5-rg-unorm.bc5-rg-snorm.bc6h-rgb-ufloat.bc6h-rgb-float.bc7-rgba-unorm.bc7-rgba-unorm-srgb.etc2-rgb8unorm.etc2-rgb8unorm-srgb.etc2-rgb8a1unorm.etc2-rgb8a1unorm-srgb.etc2-rgba8unorm.etc2-rgba8unorm-srgb.eac-r11unorm.eac-r11snorm.eac-rg11unorm.eac-rg11snorm.astc-4x4-unorm.astc-4x4-unorm-srgb.astc-5x4-unorm.astc-5x4-unorm-srgb.astc-5x5-unorm.astc-5x5-unorm-srgb.astc-6x5-unorm.astc-6x5-unorm-srgb.astc-6x6-unorm.astc-6x6-unorm-srgb.astc-8x5-unorm.astc-8x5-unorm-srgb.astc-8x6-unorm.astc-8x6-unorm-srgb.astc-8x8-unorm.astc-8x8-unorm-srgb.astc-10x5-unorm.astc-10x5-unorm-srgb.astc-10x6-unorm.astc-10x6-unorm-srgb.astc-10x8-unorm.astc-10x8-unorm-srgb.astc-10x10-unorm.astc-10x10-unorm-srgb.astc-12x10-unorm.astc-12x10-unorm-srgb.astc-12x12-unorm.astc-12x12-unorm-srgb".split("."), be = [
	"float",
	"unfilterable-float",
	"depth",
	"sint",
	"uint"
], xe = [
	"1d",
	"2d",
	"2d-array",
	"cube",
	"cube-array",
	"3d"
], Se = /* @__PURE__ */ "uint8.uint8x2.uint8x4.sint8.sint8x2.sint8x4.unorm8.unorm8x2.unorm8x4.snorm8.snorm8x2.snorm8x4.uint16.uint16x2.uint16x4.sint16.sint16x2.sint16x4.unorm16.unorm16x2.unorm16x4.snorm16.snorm16x2.snorm16x4.float16.float16x2.float16x4.float32.float32x2.float32x3.float32x4.uint32.uint32x2.uint32x3.uint32x4.sint32.sint32x2.sint32x3.sint32x4.unorm10-10-10-2.unorm8x4-bgra".split("."), Ce = ["vertex", "instance"], we = typeof FinalizationRegistry > "u" ? {
	register: () => {},
	unregister: () => {}
} : new FinalizationRegistry((e) => G.__wbg_wasmboneik_free(e, 1)), Te = typeof FinalizationRegistry > "u" ? {
	register: () => {},
	unregister: () => {}
} : new FinalizationRegistry((e) => G.__wbg_wasmeditor_free(e, 1)), Ee = typeof FinalizationRegistry > "u" ? {
	register: () => {},
	unregister: () => {}
} : new FinalizationRegistry((e) => G.__wbg_wasmplayer_free(e, 1));
function O(e) {
	let t = G.__externref_table_alloc();
	return G.__wbindgen_externrefs.set(t, e), t;
}
var De = typeof FinalizationRegistry > "u" ? {
	register: () => {},
	unregister: () => {}
} : new FinalizationRegistry((e) => G.__wbindgen_destroy_closure(e.a, e.b));
function Oe(e) {
	let t = typeof e;
	if (t == "number" || t == "boolean" || e == null) return `${e}`;
	if (t == "string") return `"${e}"`;
	if (t == "symbol") {
		let t = e.description;
		return t == null ? "Symbol" : `Symbol(${t})`;
	}
	if (t == "function") {
		let t = e.name;
		return typeof t == "string" && t.length > 0 ? `Function(${t})` : "Function";
	}
	if (Array.isArray(e)) {
		let t = e.length, n = "[";
		t > 0 && (n += Oe(e[0]));
		for (let r = 1; r < t; r++) n += ", " + Oe(e[r]);
		return n += "]", n;
	}
	let n = /\[object ([^\]]+)\]/.exec(toString.call(e)), r;
	if (n && n.length > 1) r = n[1];
	else return toString.call(e);
	if (r == "Object") try {
		return "Object(" + JSON.stringify(e) + ")";
	} catch {
		return "Object";
	}
	return e instanceof Error ? `${e.name}: ${e.message}\n${e.stack}` : r;
}
function k(e, t) {
	return e >>>= 0, Ne().subarray(e / 4, e / 4 + t);
}
function ke(e, t) {
	return e >>>= 0, Le().subarray(e / 2, e / 2 + t);
}
function A(e, t) {
	return e >>>= 0, ze().subarray(e / 4, e / 4 + t);
}
function Ae(e, t) {
	return e >>>= 0, Ve().subarray(e / 1, e / 1 + t);
}
function je(e, t) {
	e >>>= 0;
	let n = F(), r = [];
	for (let i = e; i < e + 4 * t; i += 4) r.push(G.__wbindgen_externrefs.get(n.getUint32(i, !0)));
	return G.__externref_drop_slice(e, t), r;
}
function j(e, t) {
	e >>>= 0;
	let n = F(), r = [];
	for (let i = e; i < e + 4 * t; i += 4) r.push(G.__wbindgen_externrefs.get(n.getUint32(i, !0)));
	return r;
}
function Me(e, t) {
	return e >>>= 0, Ue().subarray(e / 2, e / 2 + t);
}
function M(e, t) {
	return e >>>= 0, Ge().subarray(e / 4, e / 4 + t);
}
function N(e, t) {
	return e >>>= 0, R().subarray(e / 1, e / 1 + t);
}
var P = null;
function F() {
	return (P === null || P.buffer.detached === !0 || P.buffer.detached === void 0 && P.buffer !== G.memory.buffer) && (P = new DataView(G.memory.buffer)), P;
}
var I = null;
function Ne() {
	return (I === null || I.byteLength === 0) && (I = new Float32Array(G.memory.buffer)), I;
}
var Pe = null;
function Fe() {
	return (Pe === null || Pe.byteLength === 0) && (Pe = new Float64Array(G.memory.buffer)), Pe;
}
var Ie = null;
function Le() {
	return (Ie === null || Ie.byteLength === 0) && (Ie = new Int16Array(G.memory.buffer)), Ie;
}
var Re = null;
function ze() {
	return (Re === null || Re.byteLength === 0) && (Re = new Int32Array(G.memory.buffer)), Re;
}
var Be = null;
function Ve() {
	return (Be === null || Be.byteLength === 0) && (Be = new Int8Array(G.memory.buffer)), Be;
}
function L(e, t) {
	return tt(e >>> 0, t);
}
var He = null;
function Ue() {
	return (He === null || He.byteLength === 0) && (He = new Uint16Array(G.memory.buffer)), He;
}
var We = null;
function Ge() {
	return (We === null || We.byteLength === 0) && (We = new Uint32Array(G.memory.buffer)), We;
}
var Ke = null;
function R() {
	return (Ke === null || Ke.byteLength === 0) && (Ke = new Uint8Array(G.memory.buffer)), Ke;
}
function z(e, t) {
	try {
		return e.apply(this, t);
	} catch (e) {
		let t = O(e);
		G.__wbindgen_exn_store(t);
	}
}
function B(e) {
	return e == null;
}
function qe(e, t, n) {
	let r = {
		a: e,
		b: t,
		cnt: 1
	}, i = (...e) => {
		r.cnt++;
		let t = r.a;
		r.a = 0;
		try {
			return n(t, r.b, ...e);
		} finally {
			r.a = t, i._wbg_cb_unref();
		}
	};
	return i._wbg_cb_unref = () => {
		--r.cnt === 0 && (G.__wbindgen_destroy_closure(r.a, r.b), r.a = 0, De.unregister(r));
	}, De.register(i, r, r), i;
}
function Je(e, t) {
	let n = t(e.length * 4, 4) >>> 0;
	return Ge().set(e, n / 4), W = e.length, n;
}
function V(e, t) {
	let n = t(e.length * 1, 1) >>> 0;
	return R().set(e, n / 1), W = e.length, n;
}
function Ye(e, t) {
	let n = t(e.length * 4, 4) >>> 0;
	return Ne().set(e, n / 4), W = e.length, n;
}
function Xe(e, t) {
	let n = t(e.length * 8, 8) >>> 0;
	return Fe().set(e, n / 8), W = e.length, n;
}
function Ze(e, t) {
	let n = t(e.length * 4, 4) >>> 0;
	for (let t = 0; t < e.length; t++) {
		let r = O(e[t]);
		F().setUint32(n + 4 * t, r, !0);
	}
	return W = e.length, n;
}
function H(e, t, n) {
	if (n === void 0) {
		let n = nt.encode(e), r = t(n.length, 1) >>> 0;
		return R().subarray(r, r + n.length).set(n), W = n.length, r;
	}
	let r = e.length, i = t(r, 1) >>> 0, a = R(), o = 0;
	for (; o < r; o++) {
		let t = e.charCodeAt(o);
		if (t > 127) break;
		a[i + o] = t;
	}
	if (o !== r) {
		o !== 0 && (e = e.slice(o)), i = n(i, r, r = o + e.length * 3, 1) >>> 0;
		let t = R().subarray(i + o, i + r), a = nt.encodeInto(e, t);
		o += a.written, i = n(i, r, o, 1) >>> 0;
	}
	return W = o, i;
}
function U(e) {
	let t = G.__wbindgen_externrefs.get(e);
	return G.__externref_table_dealloc(e), t;
}
var Qe = new TextDecoder("utf-8", {
	ignoreBOM: !0,
	fatal: !0
});
Qe.decode();
var $e = 2146435072, et = 0;
function tt(e, t) {
	return et += t, et >= $e && (Qe = new TextDecoder("utf-8", {
		ignoreBOM: !0,
		fatal: !0
	}), Qe.decode(), et = t), Qe.decode(R().subarray(e, e + t));
}
var nt = new TextEncoder();
"encodeInto" in nt || (nt.encodeInto = function(e, t) {
	let n = nt.encode(e);
	return t.set(n), {
		read: e.length,
		written: n.length
	};
});
var W = 0, G;
function rt(e, t) {
	return G = e.exports, P = null, I = null, Pe = null, Ie = null, Re = null, Be = null, He = null, We = null, Ke = null, G.__wbindgen_start(), G;
}
async function it(e, t) {
	if (typeof Response == "function" && e instanceof Response) {
		if (!e.ok) throw Error(`failed to fetch Wasm: ${e.status} ${e.statusText} fetching '${e.url}'`);
		if (typeof WebAssembly.instantiateStreaming == "function") try {
			return await WebAssembly.instantiateStreaming(e, t);
		} catch (t) {
			if (n(e.type) && e.headers.get("Content-Type") !== "application/wasm") console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", t);
			else throw t;
		}
		let r = await e.arrayBuffer();
		return await WebAssembly.instantiate(r, t);
	}
	{
		let n = await WebAssembly.instantiate(e, t);
		return n instanceof WebAssembly.Instance ? {
			instance: n,
			module: e
		} : n;
	}
	function n(e) {
		switch (e) {
			case "basic":
			case "cors":
			case "default": return !0;
		}
		return !1;
	}
}
async function at(e) {
	if (G !== void 0) return G;
	e !== void 0 && (Object.getPrototypeOf(e) === Object.prototype ? {module_or_path: e} = e : console.warn("using deprecated parameters for the initialization function; pass a single object instead")), e === void 0 && (e = "an1mbyt3_wasm_bg.wasm");
	let t = v();
	(typeof e == "string" || typeof Request == "function" && e instanceof Request || typeof URL == "function" && e instanceof URL) && (e = fetch(e));
	let { instance: n, module: r } = await it(await e, t);
	return rt(n, r);
}
//#endregion
//#region src/ipc/wasmEditor.ts
var ot = null, st = null;
function ct(e) {
	st = e;
}
function lt() {
	return ot ||= (st ? at({ module_or_path: st }) : at()).then(() => void 0), ot;
}
async function ut(e) {
	return await lt(), g.create(e);
}
function dt(e, t, n, r) {
	return e.burst_particles_3d(t, n, r);
}
function ft(e, t, n) {
	return e.duplicate_nodes(t, n);
}
function pt(e, t) {
	return e.set_active_artboard(t);
}
function mt(e, t, n) {
	return e.physics3d_velocity(t, n);
}
function ht(e, t, n) {
	return e.physics3d_angular_velocity(t, n);
}
function gt(e, t, n, r, i, a, o) {
	e.physics3d_set_velocity(t, n, r, i, a, o);
}
function _t(e, t, n, r, i, a, o) {
	e.physics3d_add_force(t, n, r, i, a, o);
}
function vt(e, t, n, r, i, a) {
	return e.physics3d_raycast(t, n[0], n[1], n[2], r[0], r[1], r[2], i, a);
}
function yt(e, t, n, r, i, a, o) {
	return e.glass_shatter(t, n, r[0], r[1], r[2], i[0], i[1], i[2], a, o);
}
function bt(e, t, n, r, i) {
	return e.glass_raycast(t, n[0], n[1], n[2], r[0], r[1], r[2], i);
}
function xt(e, t, n) {
	return e.physics_velocity(t, n);
}
function St(e, t, n, r, i) {
	e.physics_set_velocity(t, n, r, i);
}
function Ct(e, t, n) {
	return e.physics_angular_velocity(t, n);
}
function wt(e, t, n, r) {
	e.physics_set_angular_velocity(t, n, r);
}
function Tt(e, t, n, r, i, a) {
	e.physics_add_force(t, n, r, i, a);
}
function Et(e, t, n, r, i, a, o, s) {
	return e.physics_raycast(t, n, r, i, a, o, s);
}
function Dt(e, t) {
	return e.physics_events(t);
}
function Ot(e, t, n, r) {
	e.emit_particles(t, n, r);
}
function kt(e, t) {
	return e.playground_view(t);
}
function At(e, t, n, r) {
	e.preview_tilemap_tiles(t, n, Int32Array.from(r));
}
function jt(e, t, n, r, i) {
	e.preview_light(t, n, r, i);
}
function Mt(e, t, n, r) {
	e.preview_text_content(t, n, r);
}
function Nt(e, t, n, r) {
	e.preview_node_path(t, n, r);
}
function Pt(e, t, n) {
	return e.delete_node(t, n);
}
async function Ft(e, t) {
	return await e.load_teab(t);
}
function It(e, t, n, r, i) {
	e.preview_node_translation(t, n, r, i);
}
function Lt(e, t, n) {
	return e.set_mode(t, n);
}
function Rt(e) {
	let t = e.screen_info_now();
	return {
		width: t[0] ?? 0,
		height: t[1] ?? 0,
		screenWidth: t[2] ?? 0,
		screenHeight: t[3] ?? 0,
		compact: t[4] === 1,
		portrait: t[5] === 1
	};
}
function zt(e) {
	return e.audio_list();
}
function Bt(e, t, n, r, i, a) {
	e.preview_node3d_animation(t, n, r, i, a);
}
function Vt(e, t, n, r) {
	e.voxels_set(t, n, r);
}
function Ht(e, t, n, r, i, a) {
	e.voxel_chunk_set(t, n, r, i, a);
}
function Ut(e, t, n, r, i) {
	e.voxel_chunk_unload(t, n, r, i);
}
function Wt(e, t, n, r, i, a, o) {
	return e.voxel_set(t, n, r, i, a, o);
}
function Gt(e, t, n, r) {
	e.preview_node3d_transform(t, n, Float32Array.from(r));
}
function Kt(e, t, n) {
	e.preview_break_joint(t, n);
}
function qt(e, t, n, r, i) {
	e.preview_body_type(t, n, r, i);
}
function Jt(e, t, n, r) {
	e.preview_node_rotation(t, n, r);
}
function Yt(e, t, n) {
	let r = e.node_local_bounds(t, n);
	return [
		r[0] ?? 0,
		r[1] ?? 0,
		r[2] ?? 0,
		r[3] ?? 0
	];
}
function Xt(e, t, n, r, i, a, o) {
	e.preview_node_scale(t, n, r, i, a, o);
}
function Zt(e, t, n, r, i, a, o) {
	e.preview_node_anchor(t, n, r, i, a, o);
}
function Qt(e, t) {
	e.fire_trigger(t);
}
function $t(e) {
	return e.state_machine_summary();
}
function en(e, t, n) {
	e.set_state_machine_bool_input(t, n);
}
function tn(e, t, n) {
	e.set_state_machine_number_input(t, n);
}
function nn(e, t, n) {
	e.set_state_machine_enum_input(t, n);
}
function rn(e, t, n) {
	e.set_key_down(t, n);
}
function an(e, t, n, r, i, a) {
	return e.spawn_prefab(t, n, r, i, a);
}
function on(e, t, n) {
	e.destroy_node(t, n);
}
//#endregion
//#region src/game/props.ts
var sn = [];
function cn(e) {
	sn = [...e ?? []];
}
function ln(e, t) {
	switch (e) {
		case "Number": {
			let e = typeof t == "number" ? t : Number(t);
			return Number.isFinite(e) ? e : 0;
		}
		case "Bool": return t === !0 || t === 1 || t === "true" || t === "1" || t === "on";
		case "Text": return t == null ? "" : String(t);
		case "Color":
			if (Array.isArray(t) && t.length >= 3) return [
				Number(t[0]),
				Number(t[1]),
				Number(t[2]),
				t.length > 3 ? Number(t[3]) : 1
			];
			if (typeof t == "string" && /^#?[0-9a-f]{6}$/i.test(t)) {
				let e = t.replace("#", "");
				return [
					parseInt(e.slice(0, 2), 16) / 255,
					parseInt(e.slice(2, 4), 16) / 255,
					parseInt(e.slice(4, 6), 16) / 255,
					1
				];
			}
			return [
				1,
				1,
				1,
				1
			];
	}
}
function un(e) {
	try {
		return ln(e.kind, JSON.parse(e.default));
	} catch {
		return ln(e.kind, e.default);
	}
}
var K = new class {
	sceneKey = "";
	values = /* @__PURE__ */ new Map();
	kinds = /* @__PURE__ */ new Map();
	version = 0;
	ensureScene(e) {
		let t = `${e.idx}.${e.version}`;
		if (t === this.sceneKey) {
			for (let t of sn) t.artboard.idx === e.idx && t.artboard.version === e.version && !this.kinds.has(t.name) && (this.define(t), this.version++);
			return;
		}
		this.sceneKey = t, this.values.clear(), this.kinds.clear();
		for (let t of sn) t.artboard.idx === e.idx && t.artboard.version === e.version && this.define(t);
		this.version++;
	}
	define(e) {
		this.kinds.set(e.name, e.kind), this.values.set(e.name, un(e));
	}
	reset() {
		this.sceneKey = "", this.version++;
	}
	has(e) {
		return this.kinds.has(e);
	}
	get(e) {
		return this.values.get(e);
	}
	kind(e) {
		return this.kinds.get(e);
	}
	set(e, t) {
		let n = this.kinds.get(e);
		if (!n) return !1;
		let r = ln(n, t), i = this.values.get(e);
		return JSON.stringify(i) !== JSON.stringify(r) && (this.values.set(e, r), this.version++), !0;
	}
	apply(e, t) {
		let n = this.kinds.get(e);
		if (!n) return !1;
		if (n === "Number" && /^[+-]\s*\d/.test(t.trim())) return this.set(e, Number(this.get(e) ?? 0) + Number(t.replace(/\s/g, "")));
		if (n === "Bool" && t.trim() === "!") return this.set(e, !this.get(e));
		let r = t;
		try {
			r = JSON.parse(t);
		} catch {}
		return this.set(e, r);
	}
	names() {
		return [...this.kinds.keys()];
	}
	proxy(e) {
		return new Proxy({}, {
			get: (e, t) => typeof t == "string" ? this.get(t) : void 0,
			set: (t, n, r) => (typeof n == "string" && !this.set(n, r) && e(n), !0),
			has: (e, t) => typeof t == "string" && this.has(t),
			ownKeys: () => this.names(),
			getOwnPropertyDescriptor: (e, t) => typeof t == "string" && this.has(t) ? {
				enumerable: !0,
				configurable: !0,
				value: this.get(t)
			} : void 0
		});
	}
}();
function dn(e, t = "") {
	let n = typeof e == "number" ? String(Math.round(e * 1e3) / 1e3) : Array.isArray(e) ? e.map((e) => Math.round(Number(e) * 100) / 100).join(", ") : String(e ?? "");
	return t && t.includes("{}") ? t.split("{}").join(n) : n;
}
var fn = /* @__PURE__ */ new Map();
function pn(e) {
	let t = /* @__PURE__ */ new Map();
	for (let n of e ?? []) t.set(n.path, n.text);
	fn = t;
}
function mn(e) {
	return e.replace(/\\/g, "/").split("/").map((e) => e.trim()).filter((e) => e && e !== "." && e !== "..").join("/");
}
function hn(e) {
	return e?.startsWith("@file:") ? mn(e.slice(6)) : null;
}
function gn(e) {
	let t = hn(e);
	return t === null ? e : fn.get(t) ?? null;
}
//#endregion
//#region src/game/audio.ts
var _n = (e) => `${e.idx}:${e.version}`;
function vn(e, t, n) {
	let r = Math.max(1, n), i = Math.hypot(e, t);
	return {
		pan: Math.max(-1, Math.min(1, e / (r * .5))),
		gain: Math.max(0, Math.min(1, 1 - i / r))
	};
}
var q = new class {
	ctx = null;
	master = null;
	masterVolume = 1;
	buffers = /* @__PURE__ */ new Map();
	voices = /* @__PURE__ */ new Set();
	unlocked = !1;
	constructor() {
		if (typeof window > "u") return;
		let e = () => {
			this.unlocked = !0;
			let e = this.context();
			e && e.state === "suspended" && e.resume();
		};
		window.addEventListener("pointerdown", e, { capture: !0 }), window.addEventListener("keydown", e, { capture: !0 });
	}
	context() {
		return this.ctx ? this.ctx : typeof AudioContext > "u" ? null : (this.ctx = new AudioContext(), this.master = this.ctx.createGain(), this.master.gain.value = this.masterVolume, this.master.connect(this.ctx.destination), this.unlocked && this.ctx.state === "suspended" && this.ctx.resume(), this.ctx);
	}
	load(e, t) {
		let n = _n(t), r = this.buffers.get(n);
		if (!r) {
			let i = this.context();
			if (!i) return Promise.resolve(null);
			let a = e.audio_bytes(t);
			r = a.length ? i.decodeAudioData(a.slice().buffer).catch((e) => (console.warn("audio: could not decode a sound", e), null)) : Promise.resolve(null), this.buffers.set(n, r);
		}
		return r;
	}
	invalidate() {
		this.buffers.clear();
	}
	voice(e, t, n, r, i, a = !1) {
		let o = e.createGain();
		o.gain.value = t;
		let s = e.createStereoPanner();
		s.pan.value = n, o.connect(s).connect(this.master);
		let c = {
			node: r,
			spatial: a,
			range: i,
			baseVolume: t,
			gain: o,
			panner: s,
			ended: !1,
			stop: () => {},
			setVolume: (e) => {
				c.baseVolume = e, c.spatial || (o.gain.value = e);
			},
			setPitch: () => {},
			get playing() {
				return !c.ended;
			}
		};
		return this.voices.add(c), c;
	}
	finish(e) {
		if (!e.ended) {
			e.ended = !0, this.voices.delete(e);
			try {
				e.panner.disconnect();
			} catch {}
		}
	}
	play(e, t, n = {}) {
		let r = this.context();
		if (!r) return null;
		let i = this.voice(r, n.volume ?? 1, n.pan ?? 0, n.node, n.range ?? 600, !!n.spatial && !!n.node), a = null, o = n.pitch ?? 1, s = !1;
		return i.stop = () => {
			s = !0;
			try {
				a?.stop();
			} catch {}
			this.finish(i);
		}, i.setPitch = (e) => {
			o = e, a && (a.playbackRate.value = e);
		}, this.load(e, t).then((e) => {
			if (s || !e) {
				e || this.finish(i);
				return;
			}
			a = r.createBufferSource(), a.buffer = e, a.loop = !!n.loop, a.playbackRate.value = o, a.connect(i.gain), a.onended = () => this.finish(i), a.start();
		}), i;
	}
	tone(e = {}) {
		let t = this.context();
		if (!t) return null;
		let n = Math.max(.01, e.duration ?? .15), r = e.volume ?? .3, i = Math.max(.001, e.attack ?? .005), a = Math.max(.001, e.release ?? n * .6), o = this.voice(t, 1, e.pan ?? 0, void 0, 1), s = t.currentTime, c = t.createGain();
		c.gain.setValueAtTime(0, s), c.gain.linearRampToValueAtTime(r, s + i), c.gain.setValueAtTime(r, Math.max(s + i, s + n - a)), c.gain.linearRampToValueAtTime(0, s + n), c.connect(o.gain);
		let l;
		if (e.type === "noise") {
			let e = Math.ceil(t.sampleRate * n), r = t.createBuffer(1, e, t.sampleRate), i = r.getChannelData(0);
			for (let t = 0; t < e; t++) i[t] = Math.random() * 2 - 1;
			let a = t.createBufferSource();
			a.buffer = r, l = a;
		} else {
			let r = t.createOscillator();
			r.type = e.type ?? "square";
			let i = e.freq ?? 440;
			r.frequency.setValueAtTime(i, s), e.to !== void 0 && e.to !== i && r.frequency.exponentialRampToValueAtTime(Math.max(1, e.to), s + n), l = r;
		}
		return l.connect(c), l.onended = () => this.finish(o), l.start(s), l.stop(s + n + .02), o.stop = () => {
			try {
				l.stop();
			} catch {}
			this.finish(o);
		}, o;
	}
	update(e, t, n) {
		for (let r of this.voices) {
			if (!r.spatial || !r.node) continue;
			let i = n(r.node);
			if (!i) continue;
			let a = vn(i.x - e, i.y - t, r.range);
			r.panner.pan.value = a.pan, r.gain.gain.value = r.baseVolume * a.gain;
		}
	}
	stopNodeSounds(e) {
		for (let t of [...this.voices]) t.node && (e === void 0 || t.node === e) && t.stop();
	}
	stopAll() {
		for (let e of [...this.voices]) e.stop();
	}
	setMasterVolume(e) {
		this.masterVolume = Math.max(0, e), this.master && (this.master.gain.value = this.masterVolume);
	}
}(), J = 31, yn = class {
	id;
	elementId;
	name;
	classes;
	kind;
	parent;
	textContent;
	path;
	behaviors;
	audioSource;
	button;
	slider;
	scrollbar;
	scrollView;
	toggle;
	html;
	bounds;
	mirror;
	index;
	constructor(e, t, n) {
		this.mirror = e, this.index = t, this.id = n.id, this.elementId = n.elementId, this.name = n.name, this.classes = n.classes, this.kind = n.kind, this.parent = n.parent, this.textContent = n.textContent, this.path = n.path, this.audioSource = n.audioSource ?? null, this.button = n.button ?? null, this.slider = n.slider ?? null, this.scrollbar = n.scrollbar ?? null, this.scrollView = n.scrollView ?? null, this.toggle = n.toggle ?? null, this.html = n.html ?? null, this.bounds = n.bounds ?? [
			0,
			0,
			0,
			0
		], this.behaviors = n.behaviors ?? [];
	}
	transformAt(e) {
		let t = this.mirror.block, n = this.index * J + e;
		return {
			translation: {
				x: t[n],
				y: t[n + 1]
			},
			rotation: t[n + 2],
			scale: {
				x: t[n + 3],
				y: t[n + 4]
			},
			anchor: {
				x: t[n + 5],
				y: t[n + 6]
			}
		};
	}
	get transform() {
		return this.transformAt(0);
	}
	get worldTransform() {
		return this.transformAt(7);
	}
	get visible() {
		return this.mirror.block[this.index * J + 14] === 1;
	}
	get zIndex() {
		return this.mirror.block[this.index * J + 15];
	}
	get fillColor() {
		let e = this.mirror.block, t = this.index * J + 16;
		return Number.isNaN(e[t]) ? null : [
			e[t],
			e[t + 1],
			e[t + 2],
			e[t + 3]
		];
	}
	get image() {
		let e = this.mirror.block, t = this.index * J + 20;
		return Number.isNaN(e[t]) ? null : {
			idx: e[t],
			version: e[t + 1]
		};
	}
	get transform3d() {
		let e = this.mirror.block, t = this.index * J + 22;
		return Number.isNaN(e[t]) ? null : {
			position: [
				e[t],
				e[t + 1],
				e[t + 2]
			],
			rotation: [
				e[t + 3],
				e[t + 4],
				e[t + 5]
			],
			scale: [
				e[t + 6],
				e[t + 7],
				e[t + 8]
			]
		};
	}
}, bn = class {
	block = /* @__PURE__ */ new Float32Array();
	nodes = [];
	images = [];
	sceneKey = "";
	revision = -1;
	update(e, t) {
		let n = `${t.idx}:${t.version}`, r = e.script_revision(t);
		if (n !== this.sceneKey || r !== this.revision) {
			let r = e.script_scene(t);
			this.sceneKey = n, this.revision = r.revision, this.nodes = r.nodes.map((e, t) => new yn(this, t, e)), this.images = r.images;
		}
		return this.block = e.script_dynamic(t), this.nodes;
	}
}, Y = (e) => `${e.idx}:${e.version}`, xn = (e) => Math.max(0, Math.min(1, e));
function Sn(e, t) {
	let n = e.worldTransform, r = t.x - n.translation.x, i = t.y - n.translation.y, a = Math.cos(-n.rotation), o = Math.sin(-n.rotation);
	return {
		x: (r * a - i * o) / (n.scale.x || 1) + n.anchor.x,
		y: (r * o + i * a) / (n.scale.y || 1) + n.anchor.y
	};
}
var Cn = (e, t) => t.x >= e[0] && t.x <= e[2] && t.y >= e[1] && t.y <= e[3], X = (e, t) => Cn(e.bounds, Sn(e, t));
function wn(e) {
	return {
		vertical: e === "BottomToTop" || e === "TopToBottom",
		reversed: e === "RightToLeft" || e === "TopToBottom"
	};
}
var Tn = class {
	states = /* @__PURE__ */ new Map();
	session = null;
	wasDown = !1;
	state(e) {
		let t = this.states.get(Y(e));
		return {
			hovered: !!t?.hovered,
			pressed: !!t?.pressed,
			clicked: !!t?.clicked,
			changed: !!t?.changed,
			value: t?.value ?? 0,
			isOn: !!t?.isOn,
			scroll: t?.scroll ?? 0
		};
	}
	set(e, t) {
		let n = this.states.get(Y(e));
		n && (t.value !== void 0 && (n.value = t.value), t.isOn !== void 0 && (n.isOn = t.isOn), t.scroll !== void 0 && (n.scroll = t.scroll, n.velocity = 0));
	}
	stateFor(e, t = 0, n = !1) {
		let r = Y(e.id), i = this.states.get(r);
		return i || (i = {
			hovered: !1,
			pressed: !1,
			clicked: !1,
			changed: !1,
			value: t,
			isOn: n,
			scroll: 0,
			velocity: 0,
			drag: null,
			sizeOverride: null,
			baseColor: e.fillColor,
			baseImage: e.image,
			baseScale: {
				x: e.transform.scale.x,
				y: e.transform.scale.y
			},
			color: e.fillColor,
			scale: 1,
			look: "",
			image: "",
			layout: "",
			seen: `${t}|${n}`
		}, this.states.set(r, i)), i;
	}
	markChanged(e) {
		let t = `${e.value}|${e.isOn}`;
		e.changed = t !== e.seen, e.seen = t;
	}
	applyStyle(e, t, n, r, i, a, o = !0) {
		let s = i !== n.look;
		if (n.look = i, r.transition === "ColorTint" && n.baseColor) {
			let o = i === "highlighted" ? r.highlightedColor : i === "pressed" ? r.pressedColor : i === "disabled" ? r.disabledColor : r.normalColor, c = r.colorMultiplier, l = n.baseColor, u = [
				l[0] * o[0] * c,
				l[1] * o[1] * c,
				l[2] * o[2] * c,
				l[3] * o[3]
			].map((e) => Math.max(0, Math.min(1, e))), d = n.color ?? u, f = r.fadeDuration > 0 ? Math.min(1, e.dt / r.fadeDuration) : 1, p = d.map((e, t) => e + (u[t] - e) * f), m = p.some((e, t) => Math.abs(e - d[t]) > 1e-4) || s;
			if (n.color = p, m && (e.setFill(t.id, p), r.tintChildren)) {
				let t = p.map((e, t) => l[t] > 1e-4 ? e / l[t] : 1);
				for (let n of a) {
					let r = this.stateFor(n);
					r.baseColor && e.setFill(n.id, r.baseColor.map((e, n) => Math.min(1, e * t[n])));
				}
			}
		}
		if (r.transition === "SpriteSwap") {
			let a = (i === "highlighted" ? r.highlightedImage : i === "pressed" ? r.pressedImage : i === "disabled" ? r.disabledImage : null) ?? n.baseImage, o = a ? Y(a) : "";
			a && o !== n.image && (n.image = o, e.setImage(t.id, a));
		}
		if (o && r.transition !== "None") {
			let a = i === "pressed" ? r.pressedScale : i === "highlighted" ? r.highlightedScale : 1, o = Math.min(1, e.dt / Math.max(.04, r.fadeDuration)), c = n.scale + (a - n.scale) * o;
			(Math.abs(c - n.scale) > 1e-4 || s && Math.abs(a - n.scale) > 1e-4) && (n.scale = Math.abs(c - a) < .001 ? a : c, e.setScale(t.id, n.baseScale.x * n.scale, n.baseScale.y * n.scale, t.transform.translation.x, t.transform.translation.y));
		}
	}
	update(e, t, n) {
		n !== this.session && (this.states.clear(), this.session = n);
		let r = e.down && !this.wasDown, i = !e.down && this.wasDown;
		if (this.wasDown = e.down, !t.some((e) => e.button || e.toggle || e.slider || e.scrollbar || e.scrollView)) return;
		let a = new Map(t.map((e) => [Y(e.id), e])), o = /* @__PURE__ */ new Map();
		for (let e of t) {
			if (!e.parent) continue;
			let t = Y(e.parent), n = o.get(t);
			n ? n.push(e) : o.set(t, [e]);
		}
		let s = (e) => o.get(Y(e.id)) ?? [], c = (e, t) => s(e).find((e) => e.name === t), l = (t) => {
			let n = t.parent ? a.get(Y(t.parent)) : void 0;
			for (; n;) {
				if (n.scrollView && !X(n, e.mouse)) return !1;
				n = n.parent ? a.get(Y(n.parent)) : void 0;
			}
			return !0;
		}, u = (e, t) => e ? t.pressed ? "pressed" : t.hovered ? "highlighted" : "normal" : "disabled", d = (e, t, n) => {
			e.clicked = !1, t ? (r && n && (e.pressed = !0), i && (e.clicked = e.pressed && n, e.pressed = !1)) : e.pressed = !1, e.hovered = n;
		}, f = /* @__PURE__ */ new Map();
		for (let e of t) if (e.toggle && e.toggle.group > 0) {
			let t = f.get(e.toggle.group);
			t ? t.push(e) : f.set(e.toggle.group, [e]);
		}
		for (let n of t) if (n.visible) {
			if (n.button) {
				let t = n.button, r = this.stateFor(n);
				d(r, t.interactable, t.interactable && l(n) && X(n, e.mouse)), this.applyStyle(e, n, r, t.style, u(t.interactable, r), s(n));
			}
			if (n.toggle) {
				let t = n.toggle, r = this.stateFor(n, 0, t.isOn), i = t.interactable && l(n) && (X(n, e.mouse) || s(n).some((t) => t.visible && X(t, e.mouse)));
				if (d(r, t.interactable, i), r.clicked && (t.group > 0 && r.isOn && !t.allowSwitchOff || (r.isOn = !r.isOn)), t.group > 0 && r.isOn) for (let e of f.get(t.group) ?? []) {
					if (e === n) continue;
					let t = this.stateFor(e, 0, e.toggle.isOn);
					t.isOn && (r.clicked || r.seen !== `${r.value}|${r.isOn}`) && (t.isOn = !1);
				}
				this.applyStyle(e, n, r, t.style, u(t.interactable, r), []);
			}
			if (n.slider) {
				let t = n.slider, i = this.stateFor(n, t.value);
				i.clicked = !1;
				let { vertical: a, reversed: o } = wn(t.direction), [s, d, f, p] = n.bounds, m = a ? d : s, h = a ? p : f, g = Sn(n, e.mouse), _ = c(n, "Handle"), v = t.interactable && l(n) && (Cn(n.bounds, g) || !!_ && X(_, e.mouse));
				if (t.interactable && r && v && (i.pressed = !0), e.down || (i.pressed = !1), i.hovered = v, i.pressed) {
					let e = xn(((a ? g.y : g.x) - m) / Math.max(1e-4, h - m));
					o && (e = 1 - e);
					let n = t.min + e * (t.max - t.min);
					t.wholeNumbers && (n = Math.round(n)), i.value = n;
				}
				let y = t.max - t.min, b = Math.abs(y) < 1e-6 ? 0 : xn((i.value - t.min) / y);
				o && (b = 1 - b);
				let x = m + b * (h - m), S = c(n, "Fill");
				if (S) {
					let t = this.stateFor(S), n = o ? h : m, r = Math.max(1e-4, Math.abs(x - n) / Math.max(1e-4, h - m)), i = (n + x) / 2, s = `${r.toFixed(4)}|${i.toFixed(2)}`;
					t.layout !== s && (t.layout = s, a ? e.setScale(S.id, t.baseScale.x, t.baseScale.y * r, S.transform.translation.x, i) : e.setScale(S.id, t.baseScale.x * r, t.baseScale.y, i, S.transform.translation.y));
				}
				if (_) {
					let n = this.stateFor(_);
					this.applyStyle(e, _, n, t.style, u(t.interactable, i), []);
					let r = a ? _.transform.translation.x : x, o = a ? x : _.transform.translation.y, s = `${r.toFixed(2)}|${o.toFixed(2)}`;
					(n.layout !== s || Math.abs(r - _.transform.translation.x) > .001 || Math.abs(o - _.transform.translation.y) > .001) && (n.layout = s, e.setTranslation(_.id, r, o));
				}
			}
			if (n.scrollbar && this.updateScrollbar(e, n, c(n, "Handle"), r, l(n)), n.scrollView) {
				let t = n.scrollView, i = this.stateFor(n), a = c(n, "Content");
				if (!a) continue;
				let o = n.bounds, s = e.liveBounds(a.id), u = t.vertical || !t.horizontal, d = u ? o[3] - o[1] : o[2] - o[0], f = u ? s[3] - s[1] : s[2] - s[0], p = Math.max(0, f - d), m = Sn(n, e.mouse), h = l(n) && Cn(o, m);
				i.hovered = h;
				let g = c(n, "Scrollbar"), _ = g ? this.states.get(Y(g.id)) : void 0, v = !!g && X(g, e.mouse);
				if (h && (e.wheel.y || e.wheel.x) && (i.scroll += (u ? e.wheel.y : e.wheel.x) / 100 * t.wheelStep, i.velocity = 0), r && h && !v && (i.drag = {
					mouse: u ? m.y : m.x,
					scroll: i.scroll,
					grab: 0
				}), i.drag && e.down) {
					let t = u ? m.y : m.x, n = i.drag.scroll + (u ? t - i.drag.mouse : i.drag.mouse - t);
					i.velocity = (n - i.scroll) / Math.max(e.dt, .001), i.scroll = n;
				} else i.drag = null, t.inertia && Math.abs(i.velocity) > 1 ? (i.scroll += i.velocity * e.dt, i.velocity *= .04 ** e.dt) : i.velocity = 0;
				_?.pressed && (i.scroll = _.value * p), (i.scroll <= 0 || i.scroll >= p) && (i.velocity = 0), i.scroll = Math.max(0, Math.min(p, i.scroll));
				let y = u ? a.transform.translation.x : o[0] - s[0] - i.scroll, b = u ? o[3] - s[3] + i.scroll : a.transform.translation.y;
				(Math.abs(y - a.transform.translation.x) > .001 || Math.abs(b - a.transform.translation.y) > .001) && e.setTranslation(a.id, y, b), _ && !_.pressed && (_.value = p > 0 ? i.scroll / p : 0, _.sizeOverride = f > 0 ? Math.min(1, d / f) : 1), i.value = p > 0 ? i.scroll / p : 0;
			}
		}
		for (let n of t) {
			let t = this.states.get(Y(n.id));
			if (t) {
				if (n.toggle) {
					let r = c(n, "Checkmark");
					r && r.visible !== t.isOn && e.setVisible(r.id, t.isOn);
				}
				(n.toggle || n.slider || n.scrollbar || n.scrollView) && this.markChanged(t);
			}
		}
	}
	updateScrollbar(e, t, n, r, i) {
		let a = t.scrollbar, o = this.stateFor(t, a.value), { vertical: s, reversed: c } = wn(a.direction), [l, u, d, f] = t.bounds, p = s ? u : l, m = Math.max(1e-4, (s ? f : d) - p), h = m * Math.max(.05, Math.min(1, o.sizeOverride ?? a.size)), g = m - h, _ = Sn(t, e.mouse), v = s ? _.y : _.x, y = (e) => p + h / 2 + (c ? 1 - e : e) * g, b = (e) => {
			let t = g > 1e-4 ? xn((e - p - h / 2) / g) : 0;
			return c ? 1 - t : t;
		}, x = y(xn(o.value)), S = a.interactable && i && Cn(t.bounds, _);
		if (a.interactable && r && S && (o.pressed = !0, o.drag = {
			mouse: v,
			scroll: 0,
			grab: Math.abs(v - x) <= h / 2 ? v - x : 0
		}), e.down || (o.pressed = !1, o.drag = null), o.hovered = S, o.pressed && o.drag) {
			let e = b(v - o.drag.grab);
			a.steps > 1 && (e = Math.round(e * (a.steps - 1)) / (a.steps - 1)), o.value = e;
		}
		if (!n) return;
		let C = this.stateFor(n), ee = s ? n.bounds[3] - n.bounds[1] : n.bounds[2] - n.bounds[0], w = ee > 1e-4 ? h / ee : 1, T = y(xn(o.value)), te = `${w.toFixed(4)}|${T.toFixed(2)}`;
		C.layout !== te && (C.layout = te, s ? e.setScale(n.id, C.baseScale.x, C.baseScale.y * w, n.transform.translation.x, T) : e.setScale(n.id, C.baseScale.x * w, C.baseScale.y, T, n.transform.translation.y)), this.applyStyle(e, n, C, a.style, a.interactable ? o.pressed ? "pressed" : S ? "highlighted" : "normal" : "disabled", [], !1);
	}
}, En = (e) => `${e.idx}:${e.version}`, Dn = new class {
	container = null;
	live = /* @__PURE__ */ new Map();
	handlers = null;
	interactive = !1;
	canvas = null;
	attach(e, t) {
		if (this.canvas = t, this.container?.parentElement === e) return;
		this.detach();
		let n = document.createElement("div");
		n.className = "an1-html-overlay", n.style.cssText = "position:absolute;inset:0;overflow:hidden;pointer-events:none;z-index:1;", e.appendChild(n), this.container = n;
	}
	detach() {
		this.clear(), this.container?.remove(), this.container = null;
	}
	clear() {
		for (let e of this.live.values()) e.host.remove();
		this.live.clear();
	}
	cssPerPhysical() {
		let e = this.canvas;
		return e && e.width > 0 ? e.clientWidth / e.width : 1 / (window.devicePixelRatio || 1);
	}
	root(e) {
		return this.live.get(En(e))?.root ?? null;
	}
	update(e, t, n, r) {
		let i = this.container;
		if (!i) return;
		this.handlers = r, this.interactive = n;
		let a = i.clientWidth, o = i.clientHeight, s = /* @__PURE__ */ new Set();
		for (let c of e) {
			let e = En(c.id);
			s.add(e);
			let u = this.live.get(e);
			if (!u) {
				let t = document.createElement("div");
				t.style.cssText = "position:absolute;left:0;top:0;transform-origin:50% 50%;";
				let n = t.attachShadow({ mode: "open" });
				n.addEventListener("click", (e) => this.handleClick(e)), n.addEventListener("input", (e) => this.handleInput(e)), n.addEventListener("change", (e) => this.handleInput(e)), i.appendChild(t), u = {
					host: t,
					root: n,
					content: ""
				}, this.live.set(e, u);
			}
			let { config: d } = c, p = gn(d.html) ?? `<div style="font:12px system-ui;color:#f87171;padding:8px">Missing file: ${d.html.slice(6)}</div>`, m = gn(d.css) ?? "", h = `${d.width}x${d.height}\n${m}\n${p}`, g = h !== u.content;
			g && (u.content = h, f(p, m), u.root.innerHTML = `<style>:host{all:initial;display:block;width:${d.width}px;height:${d.height}px;}\n${l}\n${m}</style>${p}`);
			let _ = r?.getProp;
			if (_ && (g || u.propsVersion !== r.propsVersion)) {
				u.propsVersion = r.propsVersion;
				for (let e of u.root.querySelectorAll("[data-bind]")) {
					let t = dn(_(e.dataset.bind ?? ""), e.dataset.format ?? "");
					e.textContent !== t && (e.textContent = t);
				}
				for (let e of u.root.querySelectorAll("[data-prop]")) {
					if (u.root.activeElement === e) continue;
					let t = _(e.dataset.prop ?? "");
					e.type === "checkbox" ? e.checked = !!t : String(e.value) !== String(t ?? "") && (e.value = String(t ?? ""));
				}
			}
			let v = u.host;
			v.style.display = c.visible ? "block" : "none", v.style.width = `${d.width}px`, v.style.height = `${d.height}px`, v.style.pointerEvents = n && d.interactive ? "auto" : "none", v.style.zIndex = String(Math.round(c.zIndex ?? 0));
			let y = a / 2 + (c.translation.x - t.panX) * t.pxPerWorld, b = o / 2 - (c.translation.y - t.panY) * t.pxPerWorld, x = t.pxPerWorld;
			v.style.transform = `translate(${y - d.width / 2}px, ${b - d.height / 2}px) rotate(${-c.rotation}rad) scale(${c.scale.x * x}, ${c.scale.y * x})`;
		}
		for (let [e, t] of this.live) s.has(e) || (t.host.remove(), this.live.delete(e));
	}
	handleClick(e) {
		if (!this.interactive || !this.handlers) return;
		let t = (e.target?.closest?.("[data-trigger]"))?.getAttribute("data-trigger");
		t && this.handlers.fireTrigger(t);
	}
	handleInput(e) {
		if (!this.interactive || !this.handlers) return;
		let t = e.target, n = t?.getAttribute?.("data-prop");
		if (t && n && this.handlers.setProp) {
			this.handlers.setProp(n, t instanceof HTMLInputElement && t.type === "checkbox" ? t.checked : t.value);
			return;
		}
		let r = t?.getAttribute?.("data-input");
		!t || !r || (t instanceof HTMLInputElement && t.type === "checkbox" ? this.handlers.setInput(r, t.checked) : t instanceof HTMLInputElement && (t.type === "range" || t.type === "number") ? this.handlers.setInput(r, Number(t.value)) : this.handlers.setInput(r, t.value));
	}
}(), On = "an1mbyt3.save.", kn = {
	get(e) {
		try {
			let t = localStorage.getItem(On + e);
			return t === null ? null : JSON.parse(t);
		} catch {
			return null;
		}
	},
	set(e, t) {
		try {
			localStorage.setItem(On + e, JSON.stringify(t));
		} catch (t) {
			console.warn(`ctx.storage.set('${e}') failed`, t);
		}
	},
	remove(e) {
		try {
			localStorage.removeItem(On + e);
		} catch {}
	},
	keys() {
		try {
			let e = [];
			for (let t = 0; t < localStorage.length; t++) {
				let n = localStorage.key(t);
				n?.startsWith(On) && e.push(n.slice(14));
			}
			return e;
		} catch {
			return [];
		}
	}
};
async function An(e, t = {}, n) {
	return window.__TAURI_INTERNALS__.invoke(e, t, n);
}
//#endregion
//#region src/game/net.ts
var jn = 7777, Mn = () => typeof window < "u" && "__TAURI_INTERNALS__" in window, Nn = new class {
	status = "offline";
	error = null;
	me = null;
	hostId = null;
	players = [];
	hosting = null;
	messages = [];
	events = [];
	socket = null;
	inbox = [];
	pendingEvents = [];
	session = 0;
	get isHost() {
		return this.me !== null && this.me === this.hostId;
	}
	get canHost() {
		return Mn();
	}
	beginFrame() {
		this.messages = this.inbox, this.events = this.pendingEvents, this.inbox = [], this.pendingEvents = [];
	}
	nameOf(e) {
		return this.players.find((t) => t.id === e)?.name ?? "?";
	}
	host(e) {
		let t = Math.round(e?.port ?? 7777), n = e?.name ?? "Host";
		this.leave();
		let r = this.session;
		if (!this.canHost) {
			this.fail("Hosting needs the desktop app (or run an1mbyt3-server) — this copy can only join");
			return;
		}
		this.status = "connecting", An("net_host", { port: t }).then((e) => {
			r === this.session && (this.hosting = e, this.connect(`ws://127.0.0.1:${e.port}`, n, r));
		}).catch((e) => {
			r === this.session && this.fail(String(e));
		});
	}
	join(e, t) {
		this.leave();
		let n = e.trim();
		if (!n) {
			this.fail("Type an address to join, like 192.168.1.20:7777");
			return;
		}
		/^wss?:\/\//.test(n) || (n = "ws://" + n), /:\d+(\/|$)/.test(n.replace(/^wss?:\/\//, "")) || (n = n.replace(/\/?$/, `:${jn}`)), this.status = "connecting", this.connect(n, t?.name ?? "Player", this.session);
	}
	leave() {
		this.session++;
		let e = this.socket;
		if (this.socket = null, e) {
			e.onclose = e.onmessage = e.onerror = null;
			try {
				e.close();
			} catch {}
		}
		this.hosting && Mn() && An("net_stop").catch(() => {});
		let t = this.status === "online";
		this.hosting = null, this.status = "offline", this.error = null, this.me = this.hostId = null, this.players = [], t && this.pendingEvents.push({ type: "disconnected" });
	}
	send(e, t) {
		this.status !== "online" || !this.socket || this.socket.send(JSON.stringify(t === void 0 ? {
			t: "send",
			data: e
		} : {
			t: "send",
			data: e,
			to: t
		}));
	}
	reset() {
		this.leave(), this.inbox = [], this.pendingEvents = [], this.messages = [], this.events = [];
	}
	fail(e) {
		this.status = "error", this.error = e;
	}
	connect(e, t, n) {
		let r;
		try {
			r = new WebSocket(e);
		} catch (t) {
			this.fail(`Can't connect to ${e}: ${t}`);
			return;
		}
		this.socket = r, r.onopen = () => r.send(JSON.stringify({
			t: "hello",
			name: t
		})), r.onerror = () => {
			n === this.session && this.status === "connecting" && this.fail(`Couldn't reach ${e.replace(/^ws:\/\//, "")} — is a game hosted there?`);
		}, r.onclose = () => {
			if (n !== this.session) return;
			let t = this.status;
			this.socket = null, this.me = this.hostId = null, this.players = [], t === "online" ? (this.pendingEvents.push({ type: "disconnected" }), this.fail("Disconnected from the host")) : t === "connecting" && this.fail(this.error ?? `Couldn't reach ${e.replace(/^ws:\/\//, "")}`);
		}, r.onmessage = (e) => {
			if (n !== this.session) return;
			let t;
			try {
				t = JSON.parse(String(e.data));
			} catch {
				return;
			}
			switch (t.t) {
				case "welcome":
					this.me = t.id ?? null, this.hostId = t.host ?? null, this.players = t.players ?? [], this.status = "online", this.error = null, this.me !== null && this.pendingEvents.push({
						type: "connected",
						id: this.me
					});
					break;
				case "join":
					t.id !== void 0 && (this.players = [...this.players.filter((e) => e.id !== t.id), {
						id: t.id,
						name: t.name ?? "Player"
					}].sort((e, t) => e.id - t.id), this.pendingEvents.push({
						type: "join",
						id: t.id,
						name: t.name ?? "Player"
					}));
					break;
				case "leave":
					if (t.id !== void 0) {
						let e = this.nameOf(t.id);
						this.players = this.players.filter((e) => e.id !== t.id), this.pendingEvents.push({
							type: "leave",
							id: t.id,
							name: e
						});
					}
					break;
				case "host":
					t.id !== void 0 && (this.hostId = t.id, this.pendingEvents.push({
						type: "host",
						id: t.id
					}));
					break;
				case "msg": t.from !== void 0 && this.inbox.push({
					from: t.from,
					data: t.data
				});
			}
		};
	}
}(), Pn = 600, Fn = "an1script://";
function In(e, t = 0) {
	if (typeof e == "string") return t === 0 ? e : JSON.stringify(e);
	if (e === void 0) return "undefined";
	if (e === null || typeof e == "number" || typeof e == "boolean" || typeof e == "bigint") return String(e);
	if (typeof e == "function") return `ƒ ${e.name || "anonymous"}()`;
	if (e instanceof Error) return `${e.name}: ${e.message}`;
	if (t > 2) return Array.isArray(e) ? "[…]" : "{…}";
	if (Array.isArray(e)) return `[${e.slice(0, 20).map((e) => In(e, t + 1)).join(", ")}${e.length > 20 ? `, … ${e.length - 20} more` : ""}]`;
	if (typeof e == "object") try {
		return `{ ${Object.entries(e).slice(0, 12).map(([e, n]) => `${e}: ${In(n, t + 1)}`).join(", ")}${Object.keys(e).length > 12 ? ", …" : ""} }`;
	} catch {
		return String(e);
	}
	return String(e);
}
function Ln(e) {
	let t = (e instanceof Error ? e.stack ?? "" : "").split("\n").filter((e) => e.includes(Fn)), n = t[0]?.match(/:(\d+):(\d+)\)?\s*$/), r = t.map((e) => e.replace(/:(\d+):(\d+)(\)?)\s*$/, (e, t, n, r) => `:${Number(t) - 3}:${n}${r}`).trim());
	return n ? {
		line: Number(n[1]) - 3,
		column: Number(n[2]),
		stack: r.join("\n")
	} : { stack: r.join("\n") || void 0 };
}
function Rn(e, t, n) {
	let r = e.split("\n");
	for (let e = 1; e <= r.length; e++) try {
		Function(...n, r.slice(0, e).join("\n"));
	} catch (n) {
		if (n instanceof Error && n.message === t) return e;
	}
}
var zn = class {
	entries = [];
	listeners = /* @__PURE__ */ new Set();
	nextId = 1;
	broken = /* @__PURE__ */ new Map();
	notifyQueued = !1;
	unseenErrors = 0;
	version = 0;
	subscribe(e) {
		return this.listeners.add(e), () => this.listeners.delete(e);
	}
	list() {
		return this.entries;
	}
	add(e, t, n = {}, r) {
		let i = performance.now(), a = this.entries[this.entries.length - 1], o = (r) => r.level === e && r.message === t && r.origin.scene === n.scene && r.origin.state === n.state && r.origin.layer === n.layer && r.origin.line === n.line, s = o(a ?? {}) ? a : e === "error" ? this.entries.find(o) : void 0;
		if (s) s.count++, s.time = i;
		else {
			this.entries.push({
				id: this.nextId++,
				level: e,
				message: t,
				origin: n,
				stack: r,
				count: 1,
				time: i
			}), this.entries.length > Pn && this.entries.splice(0, this.entries.length - Pn);
			let a = Bn(n), o = a ? `[${a}] ${t}` : t;
			e === "error" ? console.error(o, ...r ? ["\n" + r] : []) : e === "warn" ? console.warn(o) : console.log(o);
		}
		e === "error" && !s && this.unseenErrors++, this.changed();
	}
	clear() {
		this.entries = [], this.unseenErrors = 0, this.changed();
	}
	markSeen() {
		this.unseenErrors !== 0 && (this.unseenErrors = 0, this.changed());
	}
	markBroken(e, t) {
		this.broken.set(e, t);
	}
	isBroken(e) {
		return this.broken.has(e);
	}
	resetBroken() {
		this.broken.clear();
	}
	changed() {
		this.version++, !this.notifyQueued && (this.notifyQueued = !0, requestAnimationFrame(() => {
			this.notifyQueued = !1;
			for (let e of this.listeners) e();
		}));
	}
};
function Bn(e) {
	let t = [
		e.scene,
		e.layer ?? void 0,
		e.state
	].filter(Boolean).join(" › "), n = e.line ? `:${e.line}${e.column ? `:${e.column}` : ""}` : "";
	return [e.kind && !t ? e.kind : "", t + n].filter(Boolean).join(" ");
}
var Z = new zn();
typeof window < "u" && (window.__an1Console = Z);
function Vn(e) {
	let t = (t) => (...n) => Z.add(t, n.map((e) => In(e)).join(" "), {
		...e(),
		kind: "Script"
	});
	return {
		log: t("log"),
		info: t("log"),
		debug: t("log"),
		warn: t("warn"),
		error: t("error")
	};
}
//#endregion
//#region src/game/events.ts
var Hn = /* @__PURE__ */ new Map();
function Un(e, t) {
	typeof t == "function" && Hn.set(e, t);
}
function Wn(e) {
	if (e.trim() !== "") try {
		return JSON.parse(e);
	} catch {
		return e;
	}
}
function Gn(e) {
	return e === "ToggleVisible" ? "Toggle visible" : "SetVisible" in e ? e.SetVisible.visible ? "Show" : "Hide" : "SetNodeField" in e ? `Set ${e.SetNodeField.field} = ${e.SetNodeField.value}` : "SetComponentField" in e ? `Set ${e.SetComponentField.component}.${e.SetComponentField.field} = ${e.SetComponentField.value}` : "CallMethod" in e ? `${e.CallMethod.component}.${e.CallMethod.method}(${e.CallMethod.arg})` : "FireTrigger" in e ? `Fire trigger ${e.FireTrigger.input}` : "SetInput" in e ? `Set input ${e.SetInput.input} = ${e.SetInput.value}` : "LoadScene" in e ? `Load scene ${e.LoadScene.scene}` : "PlaySound" in e ? `Play sound ${e.PlaySound.sound}` : "SetProperty" in e ? `Set ${e.SetProperty.property} = ${e.SetProperty.value}` : "CallFunction" in e ? `Call ${e.CallFunction.function}(${e.CallFunction.arg})` : "?";
}
function Kn(e, t, n) {
	for (let r of e) {
		let e = r.target ?? t, i = r.action, a = (e) => Z.add("warn", `On Click: ${Gn(i)} — ${e}`, {
			...n.origin,
			kind: "Event"
		});
		try {
			if (i !== "ToggleVisible" && !("FireTrigger" in i) && !("SetInput" in i) && !("LoadScene" in i) && !("PlaySound" in i) && !("SetProperty" in i) && !("CallFunction" in i) && n.nodeName(e) === null) {
				a("its target node no longer exists");
				continue;
			}
			if (i === "ToggleVisible") n.setVisible(e, !n.isVisible(e));
			else if ("SetVisible" in i) n.setVisible(e, i.SetVisible.visible);
			else if ("SetNodeField" in i) n.setNodeField(e, i.SetNodeField.field, Wn(i.SetNodeField.value)) || a(`${n.nodeName(e)} has no field "${i.SetNodeField.field}"`);
			else if ("SetComponentField" in i) {
				let { component: t, field: r, value: o } = i.SetComponentField;
				n.setComponentField(e, t, r, o) || a(`${n.nodeName(e)} has no ${t} with a "${r}" that takes ${o}`);
			} else if ("CallMethod" in i) {
				let { component: t, method: r, arg: o } = i.CallMethod;
				n.callMethod(e, t, r, o) || a(`${n.nodeName(e)} has no ${t} to call ${r} on`);
			} else if ("FireTrigger" in i) n.fireTrigger(i.FireTrigger.input);
			else if ("SetInput" in i) n.setInput(i.SetInput.input, i.SetInput.value);
			else if ("LoadScene" in i) n.loadScene(i.LoadScene.scene);
			else if ("PlaySound" in i) n.playSound(i.PlaySound.sound);
			else if ("SetProperty" in i) n.setProperty(i.SetProperty.property, i.SetProperty.value) || a(`this scene has no property "${i.SetProperty.property}"`);
			else if ("CallFunction" in i) {
				let e = Hn.get(i.CallFunction.function);
				e ? e(Wn(i.CallFunction.arg)) : a(`no script registered "${i.CallFunction.function}" (use ctx.on('${i.CallFunction.function}', () => { … }) in a script)`);
			}
		} catch (e) {
			Z.add("error", `On Click: ${Gn(i)} threw ${e instanceof Error ? `${e.name}: ${e.message}` : String(e)}`, {
				...n.origin,
				kind: "Event"
			});
		}
	}
}
//#endregion
//#region src/scriptRuntime.ts
var Q = (e) => Array.isArray(e) ? [
	e[0],
	e[1],
	e[2]
] : [
	e.x,
	e.y,
	e.z
], qn = [], Jn = null;
function Yn(e) {
	return Jn ||= new Map(qn.map((e) => [`${e.id.idx}:${e.id.version}`, e])), Jn.get(`${e.idx}:${e.version}`) ?? null;
}
function Xn(e, t) {
	let n = (qn === e.nodes ? Yn(t) : e.nodes.find((e) => e.id.idx === t.idx && e.id.version === t.version))?.behaviors.find((e) => e.enabled && e.behavior.kind === "Rigidbody2D")?.behavior;
	return n && n.kind === "Rigidbody2D" ? n.bodyType : null;
}
var Zn = 180 / Math.PI, Qn = Math.PI / 180, $ = /* @__PURE__ */ new WeakMap();
function $n(e, t, n, r, i, a) {
	let o = {
		x: i.x,
		y: i.y,
		rotation: i.rotationRadians * Zn,
		scaleX: i.scaleX,
		scaleY: i.scaleY,
		anchorX: i.anchorX,
		anchorY: i.anchorY,
		visible: i.visible,
		fillColor: i.fillColor,
		textContent: i.textContent,
		image: i.image,
		path: i.path,
		zIndex: i.zIndex
	}, s = { id: e }, c = {
		id: t,
		name: n,
		classes: r
	};
	$.set(c, e);
	let l = {
		intensity: null,
		radius: null
	}, u = null, d = { key: null }, f = () => {
		let t = Yn(e)?.transform3d ?? null;
		return u && u.at === a.time ? u.value : t;
	}, p = (e) => e ? {
		x: e[0],
		y: e[1],
		z: e[2]
	} : null, m = (e) => {
		let t = f();
		if (!t) return;
		let n = {
			...t,
			...e
		};
		u = {
			at: a.time,
			value: n
		}, a.setTransform3D(s.id, [
			...n.position,
			...n.rotation,
			...n.scale
		]);
	}, h = () => Yn(e), g = (e, t) => {
		let n = h()?.worldTransform;
		if (!n) return {
			x: e,
			y: t
		};
		let r = (e - n.anchor.x) * n.scale.x, i = (t - n.anchor.y) * n.scale.y, a = Math.cos(n.rotation), o = Math.sin(n.rotation);
		return {
			x: n.translation.x + r * a - i * o,
			y: n.translation.y + r * o + i * a
		};
	}, _ = { type: Xn(a, s.id) };
	return Object.defineProperties(c, {
		x: {
			enumerable: !0,
			get: () => o.x,
			set: (e) => {
				o.x = e, a.setTranslation(s.id, o.x, o.y);
			}
		},
		y: {
			enumerable: !0,
			get: () => o.y,
			set: (e) => {
				o.y = e, a.setTranslation(s.id, o.x, o.y);
			}
		},
		rotation: {
			enumerable: !0,
			get: () => o.rotation,
			set: (e) => {
				o.rotation = e, a.setRotationDegrees(s.id, e);
			}
		},
		scaleX: {
			enumerable: !0,
			get: () => o.scaleX,
			set: (e) => {
				o.scaleX = e, a.setScale(s.id, o.scaleX, o.scaleY, o.x, o.y);
			}
		},
		scaleY: {
			enumerable: !0,
			get: () => o.scaleY,
			set: (e) => {
				o.scaleY = e, a.setScale(s.id, o.scaleX, o.scaleY, o.x, o.y);
			}
		},
		anchorX: {
			enumerable: !0,
			get: () => o.anchorX,
			set: (e) => {
				o.anchorX = e, a.setAnchor(s.id, o.anchorX, o.anchorY, o.x, o.y);
			}
		},
		anchorY: {
			enumerable: !0,
			get: () => o.anchorY,
			set: (e) => {
				o.anchorY = e, a.setAnchor(s.id, o.anchorX, o.anchorY, o.x, o.y);
			}
		},
		visible: {
			enumerable: !0,
			get: () => o.visible,
			set: (e) => {
				o.visible = e, a.setVisible(s.id, e);
			}
		},
		fillColor: {
			enumerable: !0,
			get: () => o.fillColor,
			set: (e) => {
				o.fillColor = e, e && a.setFillColor(s.id, e[0], e[1], e[2], e[3]);
			}
		},
		textContent: {
			enumerable: !0,
			get: () => o.textContent,
			set: (e) => {
				o.textContent = e, e !== null && a.setTextContent(s.id, e);
			}
		},
		image: {
			enumerable: !0,
			get: () => o.image,
			set: (e) => {
				o.image = e, e !== null && a.setImage(s.id, e);
			}
		},
		path: {
			enumerable: !0,
			get: () => o.path,
			set: (e) => {
				o.path = e, e !== null && a.setPath(s.id, e);
			}
		},
		zIndex: {
			enumerable: !0,
			get: () => o.zIndex,
			set: (e) => {
				e !== o.zIndex && (o.zIndex = e, a.setZIndex(s.id, e));
			}
		},
		emit: {
			enumerable: !1,
			value: (e) => a.emitParticles(s.id, Math.max(0, Math.round(e)))
		},
		destroy: {
			enumerable: !1,
			value: () => a.destroyNode(s.id)
		},
		part: {
			enumerable: !1,
			writable: !0,
			value: () => null
		},
		worldX: {
			enumerable: !0,
			get: () => g(0, 0).x
		},
		worldY: {
			enumerable: !0,
			get: () => g(0, 0).y
		},
		worldRotation: {
			enumerable: !0,
			get: () => (h()?.worldTransform.rotation ?? 0) * Zn
		},
		worldPoint: {
			enumerable: !1,
			value: (e, t) => g(e, t)
		},
		bodyType: {
			enumerable: !0,
			get: () => _.type,
			set: (e) => {
				!e || e === _.type || (_.type = e, a.setBodyType(s.id, e, !1));
			}
		},
		ragdoll: {
			enumerable: !1,
			value: (e) => {
				let t = e ? "Dynamic" : "Kinematic";
				_.type !== null && (_.type = t), a.setBodyType(s.id, t, !0);
			}
		},
		breakJoint: {
			enumerable: !1,
			value: () => a.breakJoint(s.id)
		},
		position3: {
			enumerable: !0,
			get: () => p(f()?.position),
			set: (e) => e && m({ position: [
				e.x,
				e.y,
				e.z
			] })
		},
		rotation3: {
			enumerable: !0,
			get: () => p(f()?.rotation),
			set: (e) => e && m({ rotation: [
				e.x,
				e.y,
				e.z
			] })
		},
		scale3: {
			enumerable: !0,
			get: () => p(f()?.scale),
			set: (e) => e && m({ scale: [
				e.x,
				e.y,
				e.z
			] })
		},
		clicked: {
			enumerable: !0,
			get: () => a.uiState(s.id).clicked
		},
		hovered: {
			enumerable: !0,
			get: () => a.uiState(s.id).hovered
		},
		pressed: {
			enumerable: !0,
			get: () => a.uiState(s.id).pressed
		},
		changed: {
			enumerable: !0,
			get: () => a.uiState(s.id).changed
		},
		dom: {
			enumerable: !1,
			get: () => a.htmlRoot(s.id)
		},
		value: {
			enumerable: !0,
			get: () => a.uiState(s.id).value,
			set: (e) => a.setUiState(s.id, { value: Number(e) })
		},
		isOn: {
			enumerable: !0,
			get: () => a.uiState(s.id).isOn,
			set: (e) => a.setUiState(s.id, { isOn: !!e })
		},
		scroll: {
			enumerable: !0,
			get: () => a.uiState(s.id).scroll,
			set: (e) => a.setUiState(s.id, { scroll: Number(e) })
		},
		playAudio: {
			enumerable: !1,
			value: (e) => a.playAudioSource(s.id, e)
		},
		stopAudio: {
			enumerable: !1,
			value: () => a.stopAudioSource(s.id)
		},
		setVoxels: {
			enumerable: !1,
			value: (e) => a.setVoxels(s.id, e instanceof Uint8Array ? e : Uint8Array.from(e))
		},
		shakeTree: {
			enumerable: !1,
			value: (e = 1) => a.tree.shake(s.id, e)
		},
		regrowTree: {
			enumerable: !1,
			value: () => a.tree.regrow(s.id)
		},
		grabCloth: {
			enumerable: !1,
			value: (e, t, n = 40) => a.cloth.grab(s.id, e, t, n)
		},
		dragCloth: {
			enumerable: !1,
			value: (e, t) => a.cloth.drag(s.id, e, t)
		},
		releaseCloth: {
			enumerable: !1,
			value: () => a.cloth.release(s.id)
		},
		tearCloth: {
			enumerable: !1,
			value: (e, t, n = 12) => a.cloth.tear(s.id, e, t, n)
		},
		setClothWind: {
			enumerable: !1,
			value: (e, t = 0) => a.cloth.wind(s.id, e === null ? null : [e, t])
		},
		resetCloth: {
			enumerable: !1,
			value: () => a.cloth.reset(s.id)
		},
		shatter3: {
			enumerable: !1,
			value: (e, t, n = {}) => a.glass.shatter(s.id, Q(e), Q(t), n.radius ?? .25, n.floor === null ? -Infinity : n.floor ?? 0)
		},
		resetGlass: {
			enumerable: !1,
			value: () => a.glass.reset(s.id)
		},
		glassIntact: {
			enumerable: !1,
			get: () => a.glass.intact(s.id)
		},
		setVoxelChunk: {
			enumerable: !1,
			value: (e, t, n) => a.setVoxelChunk(s.id, e | 0, t | 0, n instanceof Uint8Array ? n : Uint8Array.from(n))
		},
		unloadVoxelChunk: {
			enumerable: !1,
			value: (e, t) => a.unloadVoxelChunk(s.id, e | 0, t | 0)
		},
		setVoxel: {
			enumerable: !1,
			value: (e, t, n, r) => a.setVoxel(s.id, Math.floor(e), Math.floor(t), Math.floor(n), r | 0)
		},
		playAnimation3: {
			enumerable: !1,
			value: (e, t) => {
				let n = t?.loop ?? !0, r = t?.overlay ?? "", i = `${e}|${n}|${r}`;
				d.key !== i && (d.key = i, a.setAnimation3D(s.id, e, n, r));
			}
		},
		burst3: {
			enumerable: !1,
			value: (e = 50) => a.burstParticles3D(s.id, Math.max(1, Math.round(e)))
		},
		lookAt3: {
			enumerable: !1,
			value: (e, t, n) => {
				let r = f();
				if (!r) return;
				let [i, a, o] = r.position, s = e - i, c = t - a, l = n - o, u = Math.atan2(-s, -l) * Zn, d = Math.atan2(c, Math.hypot(s, l)) * Zn;
				m({ rotation: [
					d,
					u,
					r.rotation[2]
				] });
			}
		},
		setTiles: {
			enumerable: !1,
			value: (e) => a.setTiles(s.id, e)
		},
		lightIntensity: {
			enumerable: !0,
			get: () => l.intensity,
			set: (e) => {
				e !== l.intensity && (l.intensity = e, a.setLight(s.id, e, -1));
			}
		},
		lightRadius: {
			enumerable: !0,
			get: () => l.radius,
			set: (e) => {
				e !== l.radius && (l.radius = e, a.setLight(s.id, -1, e));
			}
		},
		velocity: {
			enumerable: !0,
			get: () => {
				let e = a.physics.velocity(s.id);
				return e ? {
					x: e[0],
					y: e[1]
				} : null;
			},
			set: (e) => {
				if (!e) return;
				let [t, n] = Array.isArray(e) ? e : [e.x, e.y];
				a.physics.setVelocity(s.id, t, n);
			}
		},
		angularVelocity: {
			enumerable: !0,
			get: () => a.physics.angularVelocity(s.id),
			set: (e) => {
				e !== null && a.physics.setAngularVelocity(s.id, e);
			}
		},
		addForce: {
			enumerable: !1,
			value: (e, t) => a.physics.addForce(s.id, e, t, !1)
		},
		addImpulse: {
			enumerable: !1,
			value: (e, t) => a.physics.addForce(s.id, e, t, !0)
		},
		velocity3: {
			enumerable: !0,
			get: () => {
				let e = a.physics.velocity3(s.id);
				return e ? {
					x: e[0],
					y: e[1],
					z: e[2]
				} : null;
			},
			set: (e) => {
				e && a.physics.setVelocity3(s.id, 0, ...Q(e));
			}
		},
		angularVelocity3: {
			enumerable: !0,
			get: () => {
				let e = a.physics.angularVelocity3(s.id);
				return e ? {
					x: e[0],
					y: e[1],
					z: e[2]
				} : null;
			},
			set: (e) => {
				e && a.physics.setVelocity3(s.id, 1, ...Q(e));
			}
		},
		addForce3: {
			enumerable: !1,
			value: (e, t, n) => a.physics.addForce3(s.id, 0, e, t, n)
		},
		addImpulse3: {
			enumerable: !1,
			value: (e, t, n) => a.physics.addForce3(s.id, 1, e, t, n)
		},
		addTorque3: {
			enumerable: !1,
			value: (e, t, n) => a.physics.addForce3(s.id, 2, e, t, n)
		}
	}), c;
}
function er(e, t) {
	return $n(e.id, e.elementId, e.name, e.classes, {
		x: e.transform.translation.x,
		y: e.transform.translation.y,
		rotationRadians: e.transform.rotation,
		scaleX: e.transform.scale.x,
		scaleY: e.transform.scale.y,
		anchorX: e.transform.anchor.x,
		anchorY: e.transform.anchor.y,
		visible: e.visible,
		fillColor: e.fillColor,
		textContent: e.textContent,
		image: e.image,
		path: e.path,
		zIndex: e.zIndex
	}, t);
}
function tr(e, t) {
	let n = $.get(e);
	if (!n) return null;
	let r = t.spawnClone(n);
	return r ? $n(r, e.id, e.name, e.classes, {
		x: e.x,
		y: e.y,
		rotationRadians: e.rotation * Qn,
		scaleX: e.scaleX,
		scaleY: e.scaleY,
		anchorX: e.anchorX,
		anchorY: e.anchorY,
		visible: e.visible,
		fillColor: e.fillColor,
		textContent: e.textContent,
		image: e.image,
		path: e.path,
		zIndex: e.zIndex
	}, t) : null;
}
function nr(e, t) {
	let n = $.get(e);
	n && t.deleteNode(n);
}
function rr(e, t) {
	return e + t;
}
function ir(e, t) {
	return e - t;
}
function ar(e, t) {
	return e * t;
}
function or(e, t) {
	return t === 0 ? 0 : e / t;
}
function sr(e, t) {
	return +(e > t);
}
function cr(e, t) {
	return +(e < t);
}
function lr(e, t) {
	return +(e === t);
}
function ur(e, t) {
	return +(e !== 0 && t !== 0);
}
function dr(e, t) {
	return +(e !== 0 || t !== 0);
}
function fr(e) {
	return +(e === 0);
}
function pr(e, t, n) {
	return e === 0 ? n : t;
}
function mr(e, t, n) {
	return e + (t - e) * Math.min(Math.max(n, 0), 1);
}
function hr(e, t, n) {
	return Math.min(Math.max(e, Math.min(t, n)), Math.max(t, n));
}
function gr(e) {
	let t = Math.imul(e >>> 0, 2654435761) >>> 0;
	return t = (t ^ t >>> 15) >>> 0, t = Math.imul(t, 2246822507) >>> 0, t = (t ^ t >>> 13) >>> 0, t / 4294967295;
}
var _r = {
	add: rr,
	subtract: ir,
	multiply: ar,
	divide: or,
	greaterThan: sr,
	lessThan: cr,
	equals: lr,
	and: ur,
	or: dr,
	not: fr,
	select: pr,
	lerp: mr,
	clamp: hr,
	random: gr
}, vr = [
	"ctx",
	"getCompByID",
	"getCompByClass",
	"spawnClone",
	"deleteNode",
	"helpers",
	"console",
	"print"
], yr = /* @__PURE__ */ new Map();
function br(e, t) {
	let n = t + "\n" + e, r = yr.get(n);
	if (!r) {
		let i = `const { add, subtract, multiply, divide, greaterThan, lessThan, equals, and, or, not, select, lerp, clamp, random } = helpers;\n${e}\n//# sourceURL=${t}`;
		r = Function(...vr, i), yr.set(n, r);
	}
	return r;
}
function xr(e, t) {
	let n = {
		scene: t.scene,
		layer: t.layer,
		state: t.state
	}, r = `${Fn}${encodeURIComponent(t.scene ?? "")}/${t.layer ? encodeURIComponent(t.layer) : "-"}/${encodeURIComponent(t.state ?? "")}`, i = r + "\n" + e;
	if (Z.isBroken(i)) return;
	let a;
	try {
		a = br(e, r);
	} catch (t) {
		let r = t instanceof Error ? `${t.name}: ${t.message}` : String(t), a = t instanceof Error ? Rn(e, t.message, vr) : void 0;
		Z.add("error", `${r} — this program won't run until it's fixed`, {
			...n,
			line: a,
			kind: "Script"
		}), Z.markBroken(i, r);
		return;
	}
	qn !== t.nodes && (qn = t.nodes, Jn = null);
	let o = /* @__PURE__ */ new Map(), s = null, c = (e) => {
		let n = o.get(e);
		return n || (n = er(e, t), o.set(e, n)), n;
	}, l = (e) => {
		if (!s) {
			s = /* @__PURE__ */ new Map();
			for (let e of t.nodes) s.has(e.elementId) || s.set(e.elementId, e);
		}
		let n = s.get(e);
		return n ? c(n) : null;
	}, u = (e) => t.nodes.filter((t) => t.classes.includes(e)).map((e) => er(e, t)), d = (e) => {
		let n = t.nodes.find((t) => t.id.idx === e.idx && t.id.version === e.version);
		return n ? c(n) : null;
	};
	t.raycast = (e, n, r, i, a = Number.MAX_VALUE, o = null) => {
		let s = o ? $.get(o) ?? null : null, c = t.physics.raycast(e, n, r, i, a, s);
		return c ? {
			ref: d(c.node),
			x: c.x,
			y: c.y,
			normalX: c.normalX,
			normalY: c.normalY,
			distance: c.distance
		} : null;
	}, t.raycast3 = (e, n, r = Number.MAX_VALUE, i = null) => {
		let a = i ? $.get(i) ?? null : null, o = t.physics.raycast3(Q(e), Q(n), r, a);
		return o ? {
			ref: d(o.node),
			x: o.x,
			y: o.y,
			z: o.z,
			normalX: o.normalX,
			normalY: o.normalY,
			normalZ: o.normalZ,
			distance: o.distance
		} : null;
	}, t.glassRaycast3 = (e, n, r = Number.MAX_VALUE) => {
		let i = t.glass.raycast(Q(e), Q(n), r);
		return i ? {
			ref: d(i.node),
			x: i.x,
			y: i.y,
			z: i.z,
			normalX: i.normalX,
			normalY: i.normalY,
			normalZ: i.normalZ,
			distance: i.distance
		} : null;
	}, t.spawn = (e, n, r, i = null) => {
		let a = i ? $.get(i) ?? null : null, o = t.spawnPrefab(e, a, n, r);
		if (!o || o.length === 0) return null;
		let s = o.map((e) => {
			let n = e.transform;
			return $n(e.id, "", e.name, [], {
				x: n.translation.x,
				y: n.translation.y,
				rotationRadians: n.rotation,
				scaleX: n.scale.x,
				scaleY: n.scale.y,
				anchorX: n.anchor.x,
				anchorY: n.anchor.y,
				visible: !0,
				fillColor: e.fillColor,
				textContent: null,
				image: null,
				path: null,
				zIndex: 0
			}, t);
		}), c = s[0];
		return c.part = (e) => s.find((t) => t.name === e) ?? null, c;
	}, t.collisions = t.physics.events.map((e) => ({
		a: d(e.a),
		b: d(e.b),
		started: e.started,
		trigger: e.trigger
	}));
	let f = Vn(() => n);
	try {
		a(t, l, u, (e) => tr(e, t), (e) => nr(e, t), _r, f, f.log);
	} catch (e) {
		let t = Ln(e), r = e instanceof Error ? `${e.name}: ${e.message}` : `Uncaught ${In(e)}`;
		Z.add("error", r, {
			...n,
			line: t.line,
			column: t.column,
			kind: "Script"
		}, t.stack);
	}
}
//#endregion
//#region src/game/scriptHost.ts
var Sr = {
	" ": "space",
	ArrowUp: "up",
	ArrowDown: "down",
	ArrowLeft: "left",
	ArrowRight: "right"
};
function Cr(e) {
	return Sr[e] ?? e.toLowerCase();
}
function wr(e, t) {
	for (let [n, r] of [
		[1, "mouseleft"],
		[2, "mouseright"],
		[4, "mousemiddle"]
	]) t.buttons & n ? e.add(r) : e.delete(r);
}
var Tr = class {
	start = null;
	tapped = !1;
	swiped = null;
	down(e) {
		this.start = {
			x: e.clientX,
			y: e.clientY,
			t: performance.now()
		};
	}
	up(e) {
		let t = this.start;
		if (this.start = null, !t) return;
		let n = e.clientX - t.x, r = e.clientY - t.y, i = Math.hypot(n, r), a = performance.now() - t.t;
		if (i < 12 && a < 350) this.tapped = !0;
		else if (i >= 30 && a < 600) {
			let e = Math.abs(n) > Math.abs(r) ? n > 0 ? "right" : "left" : r > 0 ? "down" : "up";
			this.swiped = {
				dir: e,
				dx: n,
				dy: r
			};
		}
	}
	take() {
		let e = {
			tap: this.tapped,
			swipe: this.swiped
		};
		return this.tapped = !1, this.swiped = null, e;
	}
}, Er = class {
	mirror = new bn();
	writes = new Dr();
	baseState = null;
	baseElapsed = 0;
	layerClocks = {};
	tick(e, t) {
		let n = !1;
		e.activeState === this.baseState ? this.baseElapsed += t : (this.baseState = e.activeState, this.baseElapsed = 0, n = !0);
		for (let [n, r] of Object.entries(e.layerActiveStates ?? {})) {
			let e = this.layerClocks[n];
			!e || e.state !== r ? this.layerClocks[n] = {
				state: r,
				elapsed: 0
			} : e.elapsed += t;
		}
		return n;
	}
	ui = new Tn();
	updateUi(e, t) {
		let { editor: n, scene: r } = e;
		this.ui.update({
			mouse: e.mouseWorld,
			down: e.keysDown.has("mouse"),
			wheel: e.wheel ?? {
				x: 0,
				y: 0
			},
			dt: e.deltaSeconds,
			liveBounds: (e) => Yt(n, r.id, e),
			setFill: (e, t) => this.writes.push(6, e, t[0], t[1], t[2], t[3]),
			setImage: (e, t) => this.writes.push(5, e, t.idx, t.version),
			setVisible: (e, t) => this.writes.push(4, e, +!!t),
			setTranslation: (e, t, n) => this.writes.push(1, e, t, n),
			setScale: (e, t, i, a, o) => {
				this.writes.flush(), Xt(n, r.id, e, a, o, t, i);
			}
		}, t, e.globals), this.updateHtml(e, t);
	}
	updateHtml(e, t) {
		let { editor: n, scene: r } = e, i = [];
		for (let e of t) {
			if (!e.html) continue;
			let t = e.worldTransform;
			i.push({
				id: e.id,
				config: e.html,
				translation: t.translation,
				rotation: t.rotation,
				scale: t.scale,
				visible: e.visible,
				zIndex: e.zIndex
			});
		}
		let a = kt(n, r.id), o = a ? {
			panX: a[0],
			panY: a[1],
			pxPerWorld: a[2] * Dn.cssPerPhysical()
		} : {
			panX: e.camera.panX,
			panY: e.camera.panY,
			pxPerWorld: e.camera.zoom
		}, s = new Map((e.machine?.inputs ?? []).map((e) => [e.name, e.kind]));
		Dn.update(i, o, !0, {
			getProp: (e) => K.get(e),
			setProp: (e, t) => {
				K.set(e, t) || Z.add("warn", `HTML UI: data-prop="${e}" — this scene has no property "${e}"`, {
					scene: r.name,
					kind: "HTML UI"
				});
			},
			propsVersion: K.version,
			fireTrigger: (e) => Qt(n, e),
			setInput: (e, t) => {
				let r = s.get(e);
				r === "Bool" ? en(n, e, !!t) : r === "Number" ? tn(n, e, Number(t)) : r === "Enum" ? nn(n, e, String(t)) : r === "Trigger" ? Qt(n, e) : console.warn(`HTML UI: state machine has no input "${e}"`);
			}
		});
	}
	audioSession = null;
	audioScene = "";
	soundNames = null;
	soundId(e, t) {
		let n = () => {
			let e = t.toLowerCase();
			return this.soundNames?.get(e) ?? this.soundNames?.get(e.replace(/\.[a-z0-9]+$/, "")) ?? null;
		}, r = n();
		if (!r) {
			this.soundNames = /* @__PURE__ */ new Map();
			for (let t of zt(e)) {
				let e = t.name.toLowerCase();
				this.soundNames.set(e, t.id), this.soundNames.set(e.replace(/\.[a-z0-9]+$/, ""), t.id);
			}
			r = n();
		}
		return r;
	}
	playSource(e, t, n, r) {
		let i = t.find((e) => e.id.idx === n.idx && e.id.version === n.version)?.audioSource;
		return i?.clip ? q.play(e, i.clip, {
			volume: r?.volume ?? i.volume,
			pitch: r?.pitch ?? i.pitch,
			loop: i.loop,
			node: `${n.idx}:${n.version}`,
			spatial: i.spatial,
			range: i.range
		}) : null;
	}
	updateAudio(e, t) {
		let { editor: n, scene: r } = e, i = `${r.id.idx}:${r.id.version}`;
		if (e.globals !== this.audioSession || i !== this.audioScene) {
			e.globals === this.audioSession ? q.stopNodeSounds() : q.stopAll(), this.audioSession = e.globals, this.audioScene = i, this.soundNames = null;
			for (let e of t) e.audioSource?.playOnStart && e.audioSource.clip && this.playSource(n, t, e.id);
		}
		let a = kt(n, r.id), o = a ? a[0] : e.camera.panX, s = a ? a[1] : e.camera.panY, c = null;
		q.update(o, s, (e) => {
			c ||= new Map(t.map((e) => [`${e.id.idx}:${e.id.version}`, e]));
			let n = c.get(e)?.worldTransform;
			return n ? {
				x: n.translation.x,
				y: n.translation.y
			} : null;
		});
	}
	run(e, t) {
		let { editor: n, machine: r, scene: i } = t;
		Nn.beginFrame();
		let a = [], o = r?.states.find((t) => t.name === e.activeState);
		o?.program?.kind === "Code" && a.push({
			layer: null,
			state: o.name,
			source: o.program.value,
			time: this.baseElapsed
		});
		for (let t of r?.layers ?? []) {
			let n = e.layerActiveStates?.[t.name], r = t.states.find((e) => e.name === n);
			r?.program?.kind === "Code" && a.push({
				layer: t.name,
				state: r.name,
				source: r.program.value,
				time: this.layerClocks[t.name]?.elapsed ?? 0
			});
		}
		let s = i.id, c = this.mirror.update(n, s);
		this.writes.target(n, s), this.updateAudio(t, c), this.updateUi(t, c), K.ensureScene(i.id);
		let l = this.runEvents(t, c);
		if (this.applyBindings(t, c), a.length === 0) return this.writes.flush(), l;
		let u = null, d = this.writes;
		d.target(n, s);
		let f = (e) => (d.flush(), e()), p = Dt(n, s), m = new Map((r?.inputs ?? []).map((e) => [e.name, e.kind])), h = (e, t) => {
			let r = m.get(e);
			r === "Bool" ? en(n, e, !!t) : r === "Number" ? tn(n, e, Number(t)) : r === "Enum" ? nn(n, e, String(t)) : r === "Trigger" ? t && Qt(n, e) : console.warn(`ctx.setInput: state machine has no input "${e}"`);
		}, g = Rt(n);
		for (let e of a) {
			let r = gn(e.source);
			if (r === null) {
				Z.add("error", `Missing script file: ${e.source.slice(6)}`, {
					scene: i.name,
					layer: e.layer,
					state: e.state,
					kind: "Script"
				});
				continue;
			}
			xr(r, {
				time: e.time,
				screen: g,
				layer: e.layer,
				state: e.state,
				setInput: h,
				fireTrigger: (e) => h(e, !0),
				deltaSeconds: t.deltaSeconds,
				mouseX: t.mouseWorld.x,
				mouseY: t.mouseWorld.y,
				nodes: c,
				images: this.mirror.images,
				keysDown: t.keysDown,
				camera: t.camera,
				setCamera: t.setCamera,
				scene: i.name,
				loadScene: (e) => {
					u = e;
				},
				globals: t.globals,
				storage: kn,
				net: Nn,
				props: K.proxy((t) => Z.add("warn", `ctx.props.${t} — this scene has no property "${t}" (add it in the Inspector's Properties, with nothing selected)`, {
					scene: i.name,
					state: e.state,
					layer: e.layer,
					kind: "Script"
				})),
				on: (e, t) => Un(e, t),
				setTranslation: (e, t, n) => d.push(1, e, t, n),
				setRotationDegrees: (e, t) => d.push(2, e, t),
				setScale: (e, t, r, i, a) => f(() => Xt(n, s, e, i, a, t, r)),
				setAnchor: (e, t, r, i, a) => f(() => Zt(n, s, e, t, r, i, a)),
				setFillColor: (e, t, n, r, i) => d.push(6, e, t, n, r, i),
				setTextContent: (e, t) => f(() => Mt(n, s, e, t)),
				setImage: (e, t) => d.push(5, e, t.idx, t.version),
				setPath: (e, t) => f(() => Nt(n, s, e, t)),
				tap: t.gestures.tap,
				swipe: t.gestures.swipe,
				mouseDX: t.mouseDelta.x,
				mouseDY: t.mouseDelta.y,
				wheelX: t.wheel?.x ?? 0,
				wheelY: t.wheel?.y ?? 0,
				pointerLocked: t.pointerLocked,
				requestPointerLock: t.requestPointerLock,
				exitPointerLock: t.exitPointerLock,
				setZIndex: (e, t) => d.push(3, e, t),
				setTiles: (e, t) => f(() => At(n, s, e, t)),
				setBodyType: (e, t, r) => f(() => qt(n, s, e, t, r)),
				breakJoint: (e) => f(() => Kt(n, s, e)),
				setTransform3D: (e, t) => f(() => Gt(n, s, e, t)),
				setAnimation3D: (e, t, r, i) => f(() => Bt(n, s, e, t, r, i)),
				burstParticles3D: (e, t) => f(() => dt(n, s, e, t)),
				setVoxels: (e, t) => f(() => Vt(n, s, e, t)),
				setVoxel: (e, t, r, i, a) => f(() => Wt(n, s, e, t, r, i, a)),
				setVoxelChunk: (e, t, r, i) => f(() => Ht(n, s, e, t, r, i)),
				tree: {
					shake: (e, t) => f(() => n.tree_shake(e, t)),
					regrow: (e) => f(() => n.tree_regrow(e))
				},
				cloth: {
					grab: (e, t, r, i) => f(() => n.cloth_grab(e, t, r, i)),
					drag: (e, t, r) => f(() => n.cloth_drag(e, t, r)),
					release: (e) => f(() => n.cloth_release(e)),
					tear: (e, t, r, i) => f(() => n.cloth_tear(e, t, r, i)),
					wind: (e, t) => f(() => n.cloth_wind(e, t ? Float32Array.from(t) : void 0)),
					reset: (e) => f(() => n.cloth_reset(e))
				},
				unloadVoxelChunk: (e, t, r) => f(() => Ut(n, s, e, t, r)),
				glass: {
					shatter: (e, t, r, i, a) => f(() => yt(n, s, e, t, r, i, a)),
					raycast: (e, t, r) => f(() => bt(n, s, e, t, r)),
					reset: (e) => f(() => n.glass_reset(s, e)),
					intact: (e) => f(() => n.glass_intact(s, e))
				},
				playSound: (e, t) => {
					let r = this.soundId(n, e);
					return r ? q.play(n, r, t) : (console.warn(`ctx.playSound: no sound named "${e}"`), null);
				},
				playTone: (e) => q.tone(e),
				setEnvironment3: (e) => {
					if (!e) return f(() => n.set_environment_3d(0, 0, 0, -1, 0, 0, 0));
					let t = e.fog ?? [
						.5,
						.5,
						.5
					], r = e.ambient ?? [
						.3,
						.33,
						.4
					];
					f(() => n.set_environment_3d(t[0], t[1], t[2], Math.max(0, e.fogDensity ?? 0), r[0], r[1], r[2]));
				},
				stopAllSounds: () => q.stopAll(),
				setMasterVolume: (e) => q.setMasterVolume(e),
				playAudioSource: (e, t) => this.playSource(n, c, e, t),
				uiState: (e) => this.ui.state(e),
				htmlRoot: (e) => Dn.root(e),
				setUiState: (e, t) => this.ui.set(e, t),
				stopAudioSource: (e) => q.stopNodeSounds(`${e.idx}:${e.version}`),
				setLight: (e, t, r) => f(() => jt(n, s, e, t, r)),
				emitParticles: (e, t) => f(() => Ot(n, s, e, t)),
				physics: {
					velocity: (e) => f(() => xt(n, s, e)),
					setVelocity: (e, t, r) => f(() => St(n, s, e, t, r)),
					angularVelocity: (e) => f(() => Ct(n, s, e)),
					setAngularVelocity: (e, t) => f(() => wt(n, s, e, t)),
					addForce: (e, t, r, i) => f(() => Tt(n, s, e, t, r, i)),
					raycast: (e, t, r, i, a, o) => f(() => Et(n, s, e, t, r, i, a, o)),
					velocity3: (e) => f(() => mt(n, s, e)),
					angularVelocity3: (e) => f(() => ht(n, s, e)),
					setVelocity3: (e, t, r, i, a) => f(() => gt(n, s, e, t, r, i, a)),
					addForce3: (e, t, r, i, a) => f(() => _t(n, s, e, t, r, i, a)),
					raycast3: (e, t, r, i) => f(() => vt(n, s, e, t, r, i)),
					events: p
				},
				raycast: () => null,
				raycast3: () => null,
				glassRaycast3: () => null,
				collisions: [],
				setVisible: (e, t) => d.push(4, e, +!!t),
				spawnClone: (e) => f(() => ft(n, s, [e]).newNodes[0] ?? null),
				spawn: () => null,
				spawnPrefab: (e, t, r, i) => f(() => an(n, s, e, t, r, i)),
				destroyNode: (e) => f(() => on(n, s, e)),
				deleteNode: (e) => {
					f(() => Pt(n, s, e));
				}
			});
		}
		return d.flush(), u ?? l;
	}
	runEvents(e, t) {
		let { editor: n, scene: r } = e, i = null, a = new Map(t.map((e) => [`${e.id.idx}.${e.id.version}`, e])), o = (e) => a.get(`${e.idx}.${e.version}`) ?? null, s = new Map((e.machine?.inputs ?? []).map((e) => [e.name, e.kind]));
		for (let e of t) {
			if (!e.button) continue;
			let a = e.behaviors.find((e) => e.enabled && e.behavior.kind === "UiEvents")?.behavior;
			if (!a || a.onClick.length === 0 || !this.ui.state(e.id).clicked) continue;
			let c = this.writes, l = (e, t) => e.behaviors.some((e) => e.enabled && e.behavior.kind === t);
			Kn(a.onClick, e.id, {
				origin: {
					scene: r.name,
					state: e.name
				},
				nodeName: (e) => o(e)?.name ?? null,
				isVisible: (e) => o(e)?.visible ?? !1,
				setVisible: (e, t) => c.push(4, e, +!!t),
				setNodeField: (e, t, i) => {
					let a = o(e);
					if (!a) return !1;
					let s = a.transform, l = Number(i);
					switch (c.flush(), t) {
						case "x": return It(n, r.id, e, l, s.translation.y), !0;
						case "y": return It(n, r.id, e, s.translation.x, l), !0;
						case "rotation": return Jt(n, r.id, e, l), !0;
						case "scaleX": return Xt(n, r.id, e, s.translation.x, s.translation.y, l, s.scale.y), !0;
						case "scaleY": return Xt(n, r.id, e, s.translation.x, s.translation.y, s.scale.x, l), !0;
						case "text": return Mt(n, r.id, e, String(i ?? "")), !0;
						case "fillColor": {
							let t = ln("Color", i);
							return c.push(6, e, t[0], t[1], t[2], t[3]), !0;
						}
						case "visible": return c.push(4, e, +!!i), !0;
					}
					return !1;
				},
				setComponentField: (e, t, i, a) => (c.flush(), n.preview_component_field(r.id, e, t, i, a)),
				callMethod: (e, i, a, s) => {
					let u = o(e);
					if (!u || !l(u, i)) return !1;
					c.flush();
					let d = Number(s);
					switch (`${i}.${a}`) {
						case "ParticleSystem.emit": return Ot(n, r.id, e, Math.max(1, Math.round(Number.isFinite(d) && d > 0 ? d : 10))), !0;
						case "AudioSource.play": return this.playSource(n, t, e), !0;
						case "AudioSource.stop": return q.stopNodeSounds(`${e.idx}:${e.version}`), !0;
						case "ParticleSystem3D.burst": return dt(n, r.id, e, Math.max(1, Math.round(Number.isFinite(d) && d > 0 ? d : 50))), !0;
						case "Tree2D.shake": return n.tree_shake(e, Number.isFinite(d) && d > 0 ? d : 1), !0;
						case "Tree2D.regrow": return n.tree_regrow(e), !0;
						case "Cloth2D.reset": return n.cloth_reset(e), !0;
						case "Rigidbody2D.impulse": {
							let [t, i] = s.split(",").map((e) => Number(e.trim()) || 0);
							return Tt(n, r.id, e, t ?? 0, i ?? 0, !0), !0;
						}
						case "UiToggle.toggle": return this.ui.set(e, { isOn: !this.ui.state(e).isOn }), !0;
					}
					return !1;
				},
				fireTrigger: (e) => Qt(n, e),
				setInput: (e, t) => {
					let i = s.get(e);
					i === "Bool" ? en(n, e, t === "true" || t === "1") : i === "Number" ? tn(n, e, Number(t)) : i === "Enum" ? nn(n, e, t) : i === "Trigger" ? Qt(n, e) : Z.add("warn", `On Click: the state machine has no input "${e}"`, {
						scene: r.name,
						kind: "Event"
					});
				},
				loadScene: (e) => {
					i = e;
				},
				playSound: (e) => {
					let t = this.soundId(n, e);
					t ? q.play(n, t) : Z.add("warn", `On Click: no sound named "${e}"`, {
						scene: r.name,
						kind: "Event"
					});
				},
				setProperty: (e, t) => K.apply(e, t)
			});
		}
		return i;
	}
	bindingsAt = "";
	applyBindings(e, t) {
		let { editor: n, scene: r } = e, i = t.filter((e) => e.behaviors.some((e) => e.enabled && e.behavior.kind === "PropertyBindings")), a = `${K.version}|${i.length}|${r.id.idx}`;
		if (i.length === 0 || a === this.bindingsAt) return;
		this.bindingsAt = a;
		let o = this.writes;
		for (let e of i) {
			let t = e.behaviors.find((e) => e.enabled && e.behavior.kind === "PropertyBindings").behavior, i = e.transform, a = null, s = null;
			for (let c of t.bindings) {
				if (!K.has(c.property)) continue;
				let t = K.get(c.property), l = Number(t) * (c.scale ?? 1) + (c.offset ?? 0), u = c.target;
				if (u === "Text") {
					let i = typeof t == "number" && ((c.scale ?? 1) !== 1 || (c.offset ?? 0) !== 0) ? l : t;
					o.flush(), Mt(n, r.id, e.id, dn(i, c.format));
				} else if (u === "X") a = [l, (a ?? [i.translation.x, i.translation.y])[1]];
				else if (u === "Y") a = [(a ?? [i.translation.x, i.translation.y])[0], l];
				else if (u === "Rotation") o.flush(), Jt(n, r.id, e.id, l);
				else if (u === "ScaleX") s = [l, (s ?? [i.scale.x, i.scale.y])[1]];
				else if (u === "ScaleY") s = [(s ?? [i.scale.x, i.scale.y])[0], l];
				else if (u === "Scale") s = [l, l];
				else if (u === "Visible") o.push(4, e.id, +!!t);
				else if (u === "FillColor") {
					let n = ln("Color", t);
					o.push(6, e.id, n[0], n[1], n[2], n[3]);
				} else if (typeof u == "object" && "ComponentField" in u) {
					o.flush();
					let i = typeof t == "number" ? l : t;
					n.preview_component_field(r.id, e.id, u.ComponentField.component, u.ComponentField.field, JSON.stringify(i));
				}
			}
			a && o.push(1, e.id, a[0], a[1]), s && (o.flush(), Xt(n, r.id, e.id, a?.[0] ?? i.translation.x, a?.[1] ?? i.translation.y, s[0], s[1]));
		}
	}
}, Dr = class {
	buf = /* @__PURE__ */ new Float64Array(1792);
	len = 0;
	editor = null;
	artboard = null;
	target(e, t) {
		this.artboard && (this.artboard.idx !== t.idx || this.artboard.version !== t.version) && (this.len = 0), this.editor = e, this.artboard = t;
	}
	push(e, t, n = 0, r = 0, i = 0, a = 0) {
		if (this.len + 7 > this.buf.length) {
			let e = new Float64Array(this.buf.length * 2);
			e.set(this.buf), this.buf = e;
		}
		let o = this.len;
		this.buf[o] = e, this.buf[o + 1] = t.idx, this.buf[o + 2] = t.version, this.buf[o + 3] = n, this.buf[o + 4] = r, this.buf[o + 5] = i, this.buf[o + 6] = a, this.len += 7;
	}
	flush() {
		if (this.len === 0 || !this.editor || !this.artboard) return;
		let e = this.buf.subarray(0, this.len);
		this.len = 0, this.editor.apply_node_writes(this.artboard, e);
	}
}, Or = [
	"logic",
	"ik",
	"physics",
	"prep",
	"scene",
	"gpu",
	"sync",
	"scripts"
], kr = {
	logic: "anim/logic",
	ik: "ik",
	physics: "physics",
	prep: "prep",
	scene: "scene",
	gpu: "gpu submit",
	sync: "doc sync",
	scripts: "scripts"
}, Ar = class {
	enabled = !1;
	el = null;
	ema = Object.fromEntries(Or.map((e) => [e, 0]));
	frameMs = 16.7;
	worstMs = 0;
	counts = {
		nodes: 0,
		draws: 0,
		lights: 0,
		bodies: 0,
		particles: 0
	};
	lastPaint = 0;
	lastWorstReset = 0;
	jsMarks = {};
	add(e, t) {
		this.enabled && (this.jsMarks[e] = (this.jsMarks[e] ?? 0) + t);
	}
	toggle(e = document.body) {
		if (this.enabled = !this.enabled, this.enabled && !this.el) {
			let t = document.createElement("div");
			t.style.cssText = "position:fixed;top:8px;right:8px;z-index:9999;pointer-events:none;padding:6px 8px;border-radius:6px;background:rgba(10,8,14,.82);color:#e8e2f0;font:11px/1.35 ui-monospace,Consolas,monospace;white-space:pre;min-width:176px", e.appendChild(t), this.el = t;
		}
		this.el && (this.el.style.display = this.enabled ? "block" : "none");
	}
	endFrame(e, t, n) {
		if (!this.enabled) return;
		let r = e.frame_stats(), i = {
			logic: r.logicMs,
			ik: r.ikMs,
			physics: r.physicsMs,
			prep: r.prepMs,
			scene: r.sceneMs,
			gpu: r.gpuSubmitMs,
			sync: this.jsMarks.sync ?? 0,
			scripts: this.jsMarks.scripts ?? 0
		};
		this.jsMarks = {};
		let a = .1;
		for (let e of Or) this.ema[e] += (i[e] - this.ema[e]) * a;
		if (t > 0 && t < 1e3 && (this.frameMs += (t - this.frameMs) * a, n - this.lastWorstReset > 2e3 && (this.worstMs = 0, this.lastWorstReset = n), this.worstMs = Math.max(this.worstMs, t)), this.counts = r, n - this.lastPaint > 250 && this.el) {
			this.lastPaint = n;
			let e = Or.reduce((e, t) => e + this.ema[t], 0), t = 1e3 / Math.max(1, this.frameMs), r = (e) => "█".repeat(Math.min(20, Math.round(e / 16.7 * 20))), i = [
				`${t.toFixed(0).padStart(3)} fps   frame ${this.frameMs.toFixed(1)}ms`,
				`worst ${this.worstMs.toFixed(1)}ms   cpu ${e.toFixed(1)}ms`,
				"",
				...Or.map((e) => `${kr[e].padEnd(10)} ${this.ema[e].toFixed(2).padStart(6)} ${r(this.ema[e])}`),
				"",
				`nodes ${this.counts.nodes}  draws ${this.counts.draws}`,
				`lights ${this.counts.lights}  bodies ${this.counts.bodies}`,
				`particles ${this.counts.particles}`
			];
			this.el.textContent = i.join("\n");
		}
	}
};
//#endregion
//#region src/game/screen.ts
function jr(e, t, n) {
	let r = Math.max(1, e.width), i = Math.max(1, e.height), a = Math.max(1, t.width), o = Math.max(1, t.height), s = Math.max(0, Math.min(1, n.matchWidthOrHeight)), c = Math.log(a / r), l = Math.log(o / i), u = Math.max(1e-4, Math.exp(c + (l - c) * s));
	return {
		width: a / u,
		height: o / u
	};
}
//#endregion
//#region src/player/main.ts
function Mr(e) {
	let t = document.getElementById(e);
	if (!t?.textContent) throw Error(`missing ${e}`);
	return t.textContent.trim();
}
function Nr(e) {
	let t = atob(e), n = new Uint8Array(new ArrayBuffer(t.length));
	for (let e = 0; e < t.length; e++) n[e] = t.charCodeAt(e);
	return n;
}
function Pr(e) {
	let t = document.getElementById("an1-status");
	t && (t.textContent = e, t.style.display = e ? "flex" : "none");
}
async function Fr() {
	let e = JSON.parse(Mr("an1-config"));
	document.title = e.title, ct(Nr(Mr("an1-wasm")));
	let t = document.getElementById("an1-canvas"), n = document.createElement("div");
	n.style.cssText = "position:relative;flex:none;line-height:0;", t.parentElement?.insertBefore(n, t), n.appendChild(t), Dn.attach(n, t);
	let r = {
		width: 1,
		height: 1
	}, i = {
		width: 1,
		height: 1
	}, a = await ut(t), o = null, s = null, c = (e, t) => e.idx === t.idx && e.version === t.version, l = () => o?.artboards.find((e) => s && c(e.id, s)) ?? o?.artboards[0] ?? null, u = () => {
		let e = window.devicePixelRatio || 1, n = window.visualViewport, o = Math.max(1, n?.width ?? window.innerWidth), s = Math.max(1, n?.height ?? window.innerHeight), u = l(), d = u?.nodes.find((e) => u.activeCamera && c(e.id, u.activeCamera))?.camera ?? null, f = Math.max(.01, d?.zoom ?? 1), p = (u?.width ?? o) / f, m = (u?.height ?? s) / f;
		if (u?.screen.mode === "Responsive") {
			let e = jr({
				width: p,
				height: m
			}, {
				width: o,
				height: s
			}, u.screen);
			p = e.width, m = e.height;
		}
		i = {
			width: p,
			height: m
		};
		let h = Math.min(o / p, s / m);
		r = {
			width: p * h,
			height: m * h
		};
		let g = !!d?.pixelPerfect.enabled, _ = g ? Math.max(1, Math.floor(h * e)) : h * e, v = Math.max(1, Math.round(p * _)), y = Math.max(1, Math.round(m * _));
		t.width = v, t.height = y, t.style.width = `${r.width}px`, t.style.height = `${r.height}px`, t.style.imageRendering = g ? "pixelated" : "auto", a.resize(v, y);
	};
	window.addEventListener("resize", u), window.visualViewport?.addEventListener("resize", u), window.addEventListener("orientationchange", () => setTimeout(u, 200)), o = await Ft(a, Nr(Mr("an1-game"))), pn(o?.textFiles), cn(o?.properties);
	let d = (e) => {
		o = pt(a, e), o = Lt(a, e, "Playground"), s = e, u();
	}, f = o, p = f.artboards.find((t) => t.name === e.startScene) ?? f.artboards[0];
	if (!p) {
		Pr("This game has no scenes.");
		return;
	}
	d(p.id), Pr("");
	let m = /* @__PURE__ */ new Set(), h = {
		x: 0,
		y: 0
	}, g = {
		x: 0,
		y: 0
	}, _ = !1, v = (e, t) => {
		let n = Cr(e.key);
		[
			"space",
			"up",
			"down",
			"left",
			"right"
		].includes(n) && e.preventDefault(), t ? m.add(n) : m.delete(n), rn(a, n, t);
	};
	window.addEventListener("keydown", (e) => v(e, !0)), window.addEventListener("keyup", (e) => v(e, !1)), window.addEventListener("blur", () => m.clear());
	let y = new Tr();
	window.addEventListener("pointermove", (e) => {
		g = {
			x: g.x + e.movementX,
			y: g.y + e.movementY
		}, h = {
			x: e.clientX,
			y: e.clientY
		}, m.has("mouse") && wr(m, e);
	}), t.addEventListener("pointerdown", (e) => {
		h = {
			x: e.clientX,
			y: e.clientY
		}, y.down(e), m.add("mouse"), wr(m, e), _ && document.pointerLockElement !== t && t.requestPointerLock();
	}), window.addEventListener("pointerup", (e) => {
		m.has("mouse") && y.up(e), wr(m, e), e.buttons || m.delete("mouse");
	}), window.addEventListener("pointercancel", () => {
		for (let e of [
			"mouse",
			"mouseleft",
			"mouseright",
			"mousemiddle"
		]) m.delete(e);
	}), t.addEventListener("contextmenu", (e) => e.preventDefault());
	let b = {
		x: 0,
		y: 0
	};
	t.addEventListener("wheel", (e) => {
		e.preventDefault(), b = {
			x: b.x + e.deltaX,
			y: b.y + e.deltaY
		};
	}, { passive: !1 });
	let x = (e, t, n, o, s) => {
		let c = kt(a, e), l = c ? {
			x: c[0],
			y: c[1]
		} : E.pan, u = c ? i.width / Math.max(1, o) : 1 / Math.max(1e-6, E.zoom * (o / Math.max(1, r.width)));
		return {
			x: l.x + (t - o / 2) * u,
			y: l.y - (n - s / 2) * u
		};
	}, S = new Er(), C = new Ar();
	new URLSearchParams(location.search).has("stats") && C.toggle(), window.addEventListener("keydown", (e) => {
		(e.key === "F3" || e.key === "`") && (e.preventDefault(), C.toggle());
	});
	let ee = {}, w = null, T = 0, te = (e) => ({
		pan: {
			x: 0,
			y: 0
		},
		zoom: Math.min(r.width / Math.max(1, e.width), r.height / Math.max(1, e.height))
	}), E = {
		pan: {
			x: 0,
			y: 0
		},
		zoom: 1
	}, ne = !1, re = (e) => {
		try {
			let n = w === null ? 0 : Math.min((e - w) / 1e3, .1);
			w = e;
			let r = window.devicePixelRatio || 1, i = l();
			i && !ne && (E = te(i)), a.render(n, E.pan.x, E.pan.y, E.zoom * r, -1, r, 0, 0, 0, 0, 0, 0);
			let u = performance.now(), f = $t(a);
			S.tick(f, n);
			let p = o;
			C.add("sync", performance.now() - u);
			let v = p?.artboards.find((e) => s && c(e.id, s));
			if (v) {
				let e = v.stateMachines.find((e) => v.activeStateMachine && c(e.id, v.activeStateMachine)), r = t.getBoundingClientRect(), i = performance.now(), o = S.run(f, {
					editor: a,
					deltaSeconds: n,
					scene: {
						id: v.id,
						name: v.name
					},
					machine: e,
					mouseWorld: x(v.id, h.x - r.left, h.y - r.top, r.width, r.height),
					mouseDelta: g,
					wheel: b,
					gestures: y.take(),
					keysDown: m,
					pointerLocked: document.pointerLockElement === t,
					requestPointerLock: () => {
						_ = !0;
					},
					exitPointerLock: () => {
						_ = !1, document.pointerLockElement && document.exitPointerLock();
					},
					camera: {
						panX: E.pan.x,
						panY: E.pan.y,
						zoom: E.zoom
					},
					setCamera: (e, t, n) => {
						E = {
							pan: {
								x: e,
								y: t
							},
							zoom: n
						}, ne = !0;
					},
					globals: ee
				});
				if (C.add("scripts", performance.now() - i), o !== null) {
					let e = p?.artboards.find((e) => e.name === o);
					e ? d(e.id) : console.warn(`ctx.loadScene: no scene named "${o}"`);
				}
			}
			g = {
				x: 0,
				y: 0
			}, b = {
				x: 0,
				y: 0
			}, C.endFrame(a, n * 1e3, e);
		} catch (e) {
			T++, (T <= 3 || T % 600 == 0) && console.error(`frame threw (#${T}) — skipping it`, e);
		} finally {
			requestAnimationFrame(re);
		}
	};
	requestAnimationFrame(re);
}
Fr().catch((e) => {
	console.error(e), Pr(`Couldn't start the game: ${e instanceof Error ? e.message : String(e)}`);
});
//#endregion
