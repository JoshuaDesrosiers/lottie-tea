//#region src/lite/runtime.ts
var e = /* @__PURE__ */ new Map();
function t(t) {
	let n = new URL(String(t), location.href).href, r = e.get(n);
	return r || (r = fetch(n).then((e) => {
		if (!e.ok) throw Error(`Toasty Lite: ${n} → ${e.status}`);
		return e.json();
	}), e.set(n, r)), r;
}
async function n(e, n) {
	let r = typeof n.src == "string" || n.src instanceof URL ? await t(n.src) : n.src;
	if (r.format !== "toasty-lite") throw Error("Toasty Lite: not a Toasty Lite file");
	return await l(r), new y(e, r, n);
}
var r = /* @__PURE__ */ new Set(), i = 0, a = 0;
function o(e) {
	let t = a ? Math.min(.1, (e - a) / 1e3) : 1 / 60;
	a = e;
	for (let e of r) e.frame(t);
	i = r.size ? requestAnimationFrame(o) : 0, i || (a = 0);
}
function s() {
	!i && r.size && (i = requestAnimationFrame(o));
}
var c = /* @__PURE__ */ new Set();
async function l(e) {
	for (let t of e.fonts) if (!(c.has(t.name) || typeof FontFace > "u")) {
		c.add(t.name);
		try {
			let e = Uint8Array.from(atob(t.data), (e) => e.charCodeAt(0)), n = new FontFace(t.name, e);
			await n.load(), document.fonts.add(n);
		} catch {}
	}
}
function u(e) {
	let t = [], n = /([MLCZ])([^MLCZ]*)/g, r;
	for (; r = n.exec(e);) {
		let e = r[2].trim() ? r[2].trim().split(/[\s,]+/).map(Number) : [];
		t.push({
			c: r[1],
			p: e
		});
	}
	return t;
}
function d(e, t) {
	let n = 0;
	return e.map((e) => {
		let r = t.slice(n, n + e.p.length);
		return n += e.p.length, {
			c: e.c,
			p: r
		};
	});
}
function f(e) {
	let t = new Path2D();
	for (let { c: n, p: r } of e) n === "M" ? t.moveTo(r[0], r[1]) : n === "L" ? t.lineTo(r[0], r[1]) : n === "C" ? t.bezierCurveTo(r[0], r[1], r[2], r[3], r[4], r[5]) : t.closePath();
	return t;
}
function p(e) {
	let t = [], n = [], r = 0, i = 0, a = 0, o = 0, s = (e) => {
		if (n.length >= 4) {
			let r = [0];
			for (let e = 2; e < n.length; e += 2) r.push(r[r.length - 1] + Math.hypot(n[e] - n[e - 2], n[e + 1] - n[e - 1]));
			t.push({
				pts: n,
				lens: r,
				closed: e
			});
		}
		n = [];
	};
	for (let { c: t, p: c } of e) if (t === "M") s(!1), a = r = c[0], o = i = c[1], n = [a, o];
	else if (t === "L") a = c[0], o = c[1], n.push(a, o);
	else if (t === "C") {
		let e = Math.max(8, Math.min(48, Math.ceil((Math.hypot(c[0] - a, c[1] - o) + Math.hypot(c[2] - c[0], c[3] - c[1]) + Math.hypot(c[4] - c[2], c[5] - c[3])) / 3)));
		for (let t = 1; t <= e; t++) {
			let r = t / e, i = 1 - r;
			n.push(i * i * i * a + 3 * i * i * r * c[0] + 3 * i * r * r * c[2] + r * r * r * c[4], i * i * i * o + 3 * i * i * r * c[1] + 3 * i * r * r * c[3] + r * r * r * c[5]);
		}
		a = c[4], o = c[5];
	} else n.length && (a !== r || o !== i) && n.push(r, i), s(!0), a = r, o = i;
	return s(!1), {
		subpaths: t,
		total: t.reduce((e, t) => e + t.lens[t.lens.length - 1], 0)
	};
}
function m(e, t, n, r) {
	let i = 0;
	for (let a of e.subpaths) {
		let e = a.lens[a.lens.length - 1], o = Math.max(t - i, 0), s = Math.min(n - i, e);
		if (s > o) {
			let e = (e) => {
				let t = 1;
				for (; t < a.lens.length - 1 && a.lens[t] < e;) t++;
				let n = a.lens[t - 1], r = a.lens[t], i = r > n ? (e - n) / (r - n) : 0;
				return [a.pts[(t - 1) * 2] + (a.pts[t * 2] - a.pts[(t - 1) * 2]) * i, a.pts[(t - 1) * 2 + 1] + (a.pts[t * 2 + 1] - a.pts[(t - 1) * 2 + 1]) * i];
			}, [t, n] = e(o);
			r.moveTo(t, n);
			for (let e = 1; e < a.lens.length; e++) a.lens[e] > o && a.lens[e] < s && r.lineTo(a.pts[e * 2], a.pts[e * 2 + 1]);
			let [i, c] = e(s);
			r.lineTo(i, c);
		}
		i += e;
	}
}
function h(e, t, n, r) {
	let i = new Path2D(), a = e.total;
	if (a <= 0) return i;
	let o = Math.min(t, n), s = Math.max(t, n);
	return s - o >= 1 - 1e-6 ? (m(e, 0, a, i), i) : (o = ((o + r) % 1 + 1) % 1, s = o + (Math.max(t, n) - Math.min(t, n)), s <= 1 ? m(e, o * a, s * a, i) : (m(e, o * a, a, i), m(e, 0, (s - 1) * a, i)), i);
}
function g(e, t) {
	let [n, r, i, a] = e, o = t;
	for (let e = 0; e < 6; e++) {
		let e = 1 - o, r = 3 * e * e * o * n + 3 * e * o * o * i + o * o * o - t, a = 3 * e * e * n + 6 * e * o * (i - n) + 3 * o * o * (1 - i);
		if (Math.abs(a) < 1e-6) break;
		o = Math.min(1, Math.max(0, o - r / a));
	}
	let s = 1 - o;
	return 3 * s * s * o * r + 3 * s * o * o * a + o * o * o;
}
function _(e, t) {
	if (t <= e[0].t) return e[0].v;
	let n = e[e.length - 1];
	if (t >= n.t) return n.v;
	let r = 1;
	for (; e[r].t < t;) r++;
	let i = e[r - 1], a = e[r], o = (t - i.t) / Math.max(1e-6, a.t - i.t);
	return i.e && (o = g(i.e, o)), i.v.length === a.v.length ? i.v.map((e, t) => e + (a.v[t] - e) * o) : o < 1 ? i.v : a.v;
}
var v = (e, t, n) => e.length === t.length ? e.map((e, r) => e + (t[r] - e) * n) : n < .5 ? e : t, y = class {
	canvas;
	ctx;
	nodes;
	props = /* @__PURE__ */ new Map();
	inputs = /* @__PURE__ */ new Map();
	triggers = /* @__PURE__ */ new Set();
	layers = [];
	listeners = /* @__PURE__ */ new Map();
	paused;
	dirty = !0;
	visible = !0;
	size = {
		w: 1,
		h: 1,
		dpr: 1
	};
	pointer = {
		x: 0,
		y: 0,
		inside: !1,
		down: !1,
		presses: 0,
		releases: 0
	};
	worlds = [];
	cleanups = [];
	lastPose = /* @__PURE__ */ new Map();
	container;
	file;
	options;
	constructor(e, t, n) {
		this.container = e, this.file = t, this.options = n, this.canvas = document.createElement("canvas"), this.canvas.style.cssText = "display:block;width:100%;height:100%;touch-action:manipulation", e.appendChild(this.canvas);
		let i = this.canvas.getContext("2d");
		if (!i) throw Error("Toasty Lite: no 2D canvas");
		this.ctx = i, this.paused = !!n.paused, this.nodes = t.nodes.map((e) => {
			let t = e.path ? u(e.path) : null;
			return {
				def: e,
				template: t,
				cmds: t,
				path: t ? f(t) : null,
				flat: null,
				pathKey: e.path ?? "",
				hover: !1,
				pressed: !1,
				lookScale: 1,
				lookTint: [
					1,
					1,
					1,
					1
				],
				visibleOverride: null,
				text: null
			};
		});
		for (let e of t.properties) this.props.set(e.name, w(e.kind, e.default));
		for (let [e, t] of Object.entries(n.data ?? {})) this.setProp(e, t, !1);
		for (let e of t.stateMachine?.inputs ?? []) e.kind !== "trigger" && this.inputs.set(e.name, e.kind === "bool" ? !1 : e.kind === "number" ? 0 : "");
		for (let [e, t] of Object.entries(n.inputs ?? {})) this.setInput(e, t);
		for (let e of t.stateMachine?.layers ?? []) {
			let t = {
				name: e.name,
				states: e.states,
				current: null,
				time: 0,
				dir: 1,
				from: null,
				fade: 0,
				fadeDuration: 0
			};
			this.layers.push(t), this.enter(t, e.states.find((t) => t.name === e.initial) ?? e.states[0] ?? null, 0, !1);
		}
		let a = new ResizeObserver(() => this.resize());
		if (a.observe(e), this.cleanups.push(() => a.disconnect()), typeof IntersectionObserver < "u") {
			let t = new IntersectionObserver((e) => {
				this.visible = e.some((e) => e.isIntersecting), this.visible && (this.dirty = !0);
			});
			t.observe(e), this.cleanups.push(() => t.disconnect());
		}
		let o = (e) => {
			let t = this.canvas.getBoundingClientRect();
			this.pointer.x = e.clientX - t.left, this.pointer.y = e.clientY - t.top, this.pointer.inside = !0, this.dirty = !0, s();
		}, c = () => {
			this.pointer.inside = !1, this.pointer.down = !1, this.dirty = !0;
		}, l = (e) => {
			o(e), this.pointer.down = !0, this.pointer.presses++;
		}, d = (e) => {
			o(e), this.pointer.down = !1, this.pointer.releases++;
		};
		this.canvas.addEventListener("pointermove", o), this.canvas.addEventListener("pointerleave", c), this.canvas.addEventListener("pointerdown", l), this.canvas.addEventListener("pointerup", d), this.cleanups.push(() => {
			this.canvas.removeEventListener("pointermove", o), this.canvas.removeEventListener("pointerleave", c), this.canvas.removeEventListener("pointerdown", l), this.canvas.removeEventListener("pointerup", d);
		}), this.resize(), r.add(this), s();
	}
	set(e, t) {
		this.setProp(e, t, !0);
	}
	setData(e) {
		for (let [t, n] of Object.entries(e)) this.setProp(t, n, !0);
	}
	get(e) {
		return this.props.get(e);
	}
	setInput(e, t) {
		let n = this.file.stateMachine?.inputs.find((t) => t.name === e);
		if (n) {
			if (n.kind === "trigger") t && this.triggers.add(e);
			else {
				let r = n.kind === "bool" ? t === !0 || t === "true" || t === 1 || t === "1" : n.kind === "number" ? Number(t) || 0 : String(t);
				this.inputs.get(e) !== r && (this.inputs.set(e, r), this.emit("input", {
					name: e,
					value: r
				}));
			}
			this.dirty = !0, s();
		}
	}
	fire(e) {
		this.setInput(e, !0);
	}
	getInput(e) {
		return this.inputs.get(e);
	}
	state(e = null) {
		return this.layers.find((t) => t.name === e)?.current?.name ?? null;
	}
	on(e, t) {
		let n = this.listeners.get(e);
		return n || this.listeners.set(e, n = /* @__PURE__ */ new Set()), n.add(t), () => n.delete(t);
	}
	play() {
		this.paused = !1, this.dirty = !0, s();
	}
	pause() {
		this.paused = !0;
	}
	destroy() {
		r.delete(this);
		for (let e of this.cleanups) e();
		this.canvas.remove();
	}
	emit(e, t) {
		for (let n of this.listeners.get(e) ?? []) try {
			n(t);
		} catch (e) {
			console.error(e);
		}
		this.container.dispatchEvent(new CustomEvent(`toasty:${e}`, {
			detail: t,
			bubbles: !0
		}));
	}
	setProp(e, t, n) {
		let r = this.file.properties.find((t) => t.name === e), i = r ? w(r.kind, t) : t;
		T(this.props.get(e), i) || (this.props.set(e, i), this.dirty = !0, n && this.emit("prop", {
			name: e,
			value: i
		}), s());
	}
	resize() {
		let e = Math.min(3, window.devicePixelRatio || 1), t = Math.max(1, this.container.clientWidth), n = Math.max(1, this.container.clientHeight);
		this.size = {
			w: t,
			h: n,
			dpr: e
		}, this.canvas.width = Math.round(t * e), this.canvas.height = Math.round(n * e), this.dirty = !0;
	}
	enter(e, t, n, r) {
		let i = e.current;
		e.from = i && n > 0 ? {
			state: i,
			time: e.time,
			dir: e.dir
		} : null, e.fade = 0, e.fadeDuration = n, e.current = t, e.dir = r ? -1 : 1;
		let a = t?.animation == null ? null : this.file.animations[t.animation];
		e.time = r && a ? a.duration : 0, t && i !== t && this.emit("state", {
			layer: e.name,
			state: t.name,
			previous: i?.name ?? null
		}), this.dirty = !0;
	}
	holds(e) {
		let t = this.inputs.get(e.input);
		switch (e.type) {
			case "bool": return t === e.value;
			case "gt": return Number(t) > Number(e.value);
			case "lt": return Number(t) < Number(e.value);
			case "enum": return t === e.value;
			case "trigger": return this.triggers.has(e.input);
		}
	}
	advance(e, t) {
		let n = !1, r = (e, n, r) => {
			let i = e.animation == null ? null : this.file.animations[e.animation];
			if (!i || i.duration <= 0) return [
				0,
				r,
				!1
			];
			let a = n + t * r, o = i.duration;
			return e.loop === "loop" ? [
				(a % o + o) % o,
				r,
				!0
			] : e.loop === "pingpong" ? a > o ? [
				o - (a - o),
				-1,
				!0
			] : a < 0 ? [
				-a,
				1,
				!0
			] : [
				a,
				r,
				!0
			] : (a = Math.min(o, Math.max(0, a)), [
				a,
				r,
				a > 0 && a < o
			]);
		};
		if (e.current) {
			let [t, i, a] = r(e.current, e.time, e.dir);
			e.time = t, e.dir = i, n = a;
		}
		if (e.from) {
			let [i, a] = r(e.from.state, e.from.time, e.from.dir);
			e.from.time = i, e.from.dir = a, e.fade += t, e.fade >= e.fadeDuration && (e.from = null), n = !0;
		}
		return n;
	}
	sample(e, t, n) {
		if (!e || e.animation == null) return;
		let r = this.file.animations[e.animation];
		if (r) for (let e of r.tracks) e.keys.length && n.set(`${e.node}:${e.prop}`, _(e.keys, t));
	}
	frame(e) {
		if (!this.visible) return;
		let t = !1;
		if (!this.paused) {
			for (let e of this.layers) {
				let t = e.current?.transitions.find((e) => this.holds(e.condition));
				if (t) {
					let n = e.states.find((e) => e.name === t.to);
					n && this.enter(e, n, t.duration, t.reverse);
				}
			}
			this.triggers.clear();
			for (let n of this.layers) t = this.advance(n, e) || t;
		}
		let n = this.updatePointer(e);
		!t && !n && !this.dirty || (this.dirty = !1, this.draw(), s());
	}
	pose() {
		let e = /* @__PURE__ */ new Map();
		for (let t of this.layers) {
			let n = /* @__PURE__ */ new Map();
			if (this.sample(t.current, t.time, n), t.from && t.fadeDuration > 0) {
				let r = /* @__PURE__ */ new Map();
				this.sample(t.from.state, t.from.time, r);
				let i = Math.min(1, t.fade / t.fadeDuration), a = /* @__PURE__ */ new Set([...r.keys(), ...n.keys()]);
				for (let t of a) {
					let a = r.get(t) ?? e.get(t) ?? this.lastPose.get(t), o = n.get(t) ?? e.get(t) ?? this.base(t);
					a && o ? e.set(t, v(a, o, i)) : o && e.set(t, o);
				}
			} else for (let [t, r] of n) e.set(t, r);
		}
		return this.lastPose = e, e;
	}
	base(e) {
		let [t, n] = e.split(":"), r = this.file.nodes[Number(t)];
		if (r) switch (n) {
			case "translation": return [r.x, r.y];
			case "rotation": return [r.rotation];
			case "scale": return [r.scaleX, r.scaleY];
			case "fill": return r.fill ?? r.text?.color;
			case "trimStart": return [r.trim?.start ?? 0];
			case "trimEnd": return [r.trim?.end ?? 1];
			case "trimOffset": return [r.trim?.offset ?? 0];
			case "strokeWidth": return [r.stroke?.width ?? 0];
			case "strokeColor": return r.stroke?.color;
			case "visible": return [+!!r.visible];
		}
	}
	updatePointer(e) {
		let t = !1, n = -1, { presses: r, releases: i } = this.pointer;
		if (this.pointer.presses = 0, this.pointer.releases = 0, this.pointer.inside) for (let e = this.nodes.length - 1; e >= 0; e--) {
			let t = this.nodes[e];
			if (!(!t.def.button?.interactable || !t.path || !this.worlds[e] || !this.isShown(e)) && (this.ctx.setTransform(this.worlds[e]), this.ctx.isPointInPath(t.path, this.pointer.x * this.size.dpr, this.pointer.y * this.size.dpr))) {
				n = e;
				break;
			}
		}
		return this.canvas.style.cursor = n >= 0 ? "pointer" : "", this.nodes.forEach((a, o) => {
			let s = a.def.button;
			if (!s) return;
			let c = o === n, l = c && this.pointer.down, u = (e) => e && this.runActions(e, o);
			c && !a.hover && u(a.def.listeners?.enter);
			let d = c && (r > 0 || l && !a.pressed);
			d && !a.pressed && u(a.def.listeners?.down), (a.pressed || d) && (i > 0 || !this.pointer.down) && (u(a.def.listeners?.up), c && u(a.def.listeners?.click)), !c && a.hover && u(a.def.listeners?.exit), a.hover = c, a.pressed = l;
			let f = l ? s.pressedScale : c ? s.hoverScale : 1, p = s.interactable ? l ? s.pressedColor : c ? s.hoverColor : s.normalColor : s.normalColor, m = s.fade > 0 ? Math.min(1, e / s.fade) : 1, h = a.lookScale + (f - a.lookScale) * m, g = a.lookTint.map((e, t) => e + (p[t] - e) * m);
			(Math.abs(h - a.lookScale) > 1e-4 || g.some((e, t) => Math.abs(e - a.lookTint[t]) > 1e-4)) && (t = !0), a.lookScale = h, a.lookTint = g;
		}), t;
	}
	runActions(e, t) {
		for (let n of e) switch (n.type) {
			case "setInput":
				this.setInput(n.input, C(n.value));
				break;
			case "fireTrigger":
				this.fire(n.input);
				break;
			case "toggleInput":
				this.setInput(n.input, !this.inputs.get(n.input));
				break;
			case "setProperty": {
				let e = this.props.get(n.property), t = /^[+-]\d/.test(n.value) && typeof e == "number";
				this.set(n.property, t ? e + Number(n.value) : C(n.value));
				break;
			}
			case "event":
				this.emit("event", {
					name: n.name,
					payload: n.arg === "" ? void 0 : C(n.arg)
				});
				break;
			case "setVisible":
				this.nodes[n.target ?? t].visibleOverride = n.visible;
				break;
			case "toggleVisible": {
				let e = n.target ?? t;
				this.nodes[e].visibleOverride = !this.isShown(e);
				break;
			}
		}
		this.dirty = !0;
	}
	isShown(e) {
		for (let t = e; t != null; t = this.file.nodes[t].parent) {
			let e = this.nodes[t], n = e.visibleOverride ?? this.cur.get(`${t}:visible`)?.[0] ?? +!!e.def.visible;
			if (typeof n == "boolean" ? !n : n < .5) return !1;
		}
		return !0;
	}
	cur = /* @__PURE__ */ new Map();
	draw() {
		let e = this.ctx, { w: t, h: n, dpr: r } = this.size, i = this.file, a = this.pose();
		this.cur = a;
		let o = new Set(a.keys()), s = (e) => !o.has(e);
		i.nodes.forEach((e, t) => {
			for (let n of e.bindings ?? []) {
				let r = this.props.get(n.property);
				if (r !== void 0) {
					if (n.target === "text") this.nodes[t].text = n.format ? n.format.replace("{}", E(r)) : E(r);
					else if (n.target === "visible") s(`${t}:visible`) && a.set(`${t}:visible`, [+!!r]);
					else if ((n.target === "fill" || n.target === "strokeColor") && Array.isArray(r)) s(`${t}:${n.target}`) && a.set(`${t}:${n.target}`, r);
					else {
						let i = Number(r) * n.scale + n.offset, o = a.get(`${t}:translation`) ?? [e.x, e.y], c = a.get(`${t}:scale`) ?? [e.scaleX, e.scaleY];
						n.target === "x" ? a.set(`${t}:translation`, [i, o[1]]) : n.target === "y" ? a.set(`${t}:translation`, [o[0], i]) : n.target === "rotation" ? a.set(`${t}:rotation`, [i * Math.PI / 180]) : n.target === "scale" ? a.set(`${t}:scale`, [i, i]) : n.target === "scaleX" ? a.set(`${t}:scale`, [i, c[1]]) : n.target === "scaleY" ? a.set(`${t}:scale`, [c[0], i]) : s(`${t}:${n.target}`) && a.set(`${t}:${n.target}`, [i]);
					}
				}
			}
		}), e.setTransform(1, 0, 0, 1, 0, 0), e.clearRect(0, 0, this.canvas.width, this.canvas.height);
		let c = this.options.fit === "cover" ? Math.max : Math.min, l = i.view.zoom || 1, u = c(t / (i.width / l), n / (i.height / l)) * l * r, m = new DOMMatrix().translate(t * r / 2, n * r / 2).scale(u, -u).translate(-i.view.x, -i.view.y);
		this.options.background && i.background && (e.fillStyle = b(i.background), e.fillRect(0, 0, this.canvas.width, this.canvas.height));
		let g = [], _ = [];
		i.nodes.forEach((e, t) => {
			let n = a.get(`${t}:translation`) ?? [e.x, e.y], r = a.get(`${t}:rotation`)?.[0] ?? e.rotation, o = a.get(`${t}:scale`) ?? [e.scaleX, e.scaleY], s = a.get(`${t}:anchor`) ?? [e.anchorX, e.anchorY], c = this.nodes[t].lookScale, l = new DOMMatrix().translate(n[0], n[1]).rotate(r * 180 / Math.PI).scale(o[0] * c, o[1] * c).translate(-s[0], -s[1]);
			g[t] = (e.parent == null ? m : g[e.parent]).multiply(l);
			let u = e.parent != null && i.nodes[e.parent].button?.tintChildren ? _[e.parent] : [
				1,
				1,
				1,
				1
			], d = e.button ? this.nodes[t].lookTint : [
				1,
				1,
				1,
				1
			];
			_[t] = d.map((e, t) => e * u[t]);
		}), this.worlds = g;
		let v = [], y = (t) => {
			let n = i.nodes[t], r = this.nodes[t];
			if (!this.isShown(t)) return;
			e.setTransform(g[t]);
			let o = _[t];
			if (n.kind === "shape" && r.template) {
				let i = a.get(`${t}:path`);
				if (i) {
					let e = i.join(",");
					if (e !== r.pathKey) {
						let t = d(r.template, i);
						r.path = f(t), r.flat = null, r.pathKey = e, r.cmds = t;
					}
				}
				let s = r.path, c = a.get(`${t}:fill`) ?? n.fill;
				if (n.gradient ? e.fillStyle = S(e, n.gradient, o) : c && c[3] > 0 && (e.fillStyle = b(x(c, o))), (n.gradient || c && c[3] > 0) && e.fill(s), n.stroke) {
					let i = a.get(`${t}:strokeWidth`)?.[0] ?? n.stroke.width, c = a.get(`${t}:strokeColor`) ?? n.stroke.color;
					if (i > 0 && (c[3] > 0 || n.stroke.gradient)) {
						e.lineWidth = i, e.lineCap = n.stroke.cap, e.lineJoin = n.stroke.join, e.strokeStyle = n.stroke.gradient ? S(e, n.stroke.gradient, o) : b(x(c, o));
						let l = s;
						if (n.trim) {
							let e = a.get(`${t}:trimStart`)?.[0] ?? n.trim.start, i = a.get(`${t}:trimEnd`)?.[0] ?? n.trim.end, o = a.get(`${t}:trimOffset`)?.[0] ?? n.trim.offset;
							Math.abs(i - e) < 1e-4 ? l = new Path2D() : e <= 0 && i >= 1 && o === 0 || (r.flat ||= p(r.cmds ?? r.template), l = h(r.flat, e, i, o));
						}
						n.dash && n.dash.dash > 0 && (e.setLineDash([n.dash.dash, n.dash.gap]), e.lineDashOffset = -(a.get(`${t}:dashOffset`)?.[0] ?? n.dash.offset)), e.stroke(l), e.setLineDash([]);
					}
				}
				n.clip && (e.save(), e.clip(s), v.push(t));
			} else if (n.kind === "text" && n.text) {
				let i = a.get(`${t}:textSize`)?.[0] ?? n.text.size, s = a.get(`${t}:fill`) ?? n.text.color;
				e.scale(1, -1), e.font = `${i}px "${n.text.font}", system-ui, sans-serif`, e.textAlign = "center", e.textBaseline = "middle", e.fillStyle = b(x(s, o)), e.fillText(r.text ?? n.text.content, 0, 0);
			} else n.clip && v.push(-1);
		}, C = i.nodes.map(() => 0);
		for (let e = i.nodes.length - 1; e >= 0; e--) {
			let t = e;
			for (let n = e + 1; n < i.nodes.length && this.isDescendant(n, e); n++) t = n;
			C[e] = t;
		}
		let w = [];
		for (let t = 0; t < i.nodes.length; t++) {
			for (; w.length && t > w[w.length - 1].end;) w.pop(), e.restore();
			let n = v.length;
			y(t), v.length > n && (v[v.length - 1] === -1 ? v.pop() : w.push({
				owner: t,
				end: C[t]
			}));
		}
		for (; w.length;) w.pop(), e.restore();
	}
	isDescendant(e, t) {
		for (let n = this.file.nodes[e].parent; n != null; n = this.file.nodes[n].parent) if (n === t) return !0;
		return !1;
	}
};
function b(e) {
	let t = (e) => Math.round(Math.min(1, Math.max(0, e)) * 255);
	return `rgba(${t(e[0])},${t(e[1])},${t(e[2])},${Math.min(1, Math.max(0, e[3] ?? 1))})`;
}
function x(e, t) {
	return [
		e[0] * t[0],
		e[1] * t[1],
		e[2] * t[2],
		(e[3] ?? 1) * t[3]
	];
}
function S(e, t, n) {
	let r = t.kind === "linear" ? e.createLinearGradient(t.start[0], t.start[1], t.end[0], t.end[1]) : e.createRadialGradient(t.start[0], t.start[1], 0, t.start[0], t.start[1], Math.hypot(t.end[0] - t.start[0], t.end[1] - t.start[1]) || 1);
	for (let e of t.stops) r.addColorStop(Math.min(1, Math.max(0, e.offset)), b(x(e.color, n)));
	return r;
}
function C(e) {
	try {
		return JSON.parse(e);
	} catch {
		return e;
	}
}
function w(e, t) {
	switch (e) {
		case "number": {
			let e = Number(t);
			return Number.isFinite(e) ? e : 0;
		}
		case "bool": return t === !0 || t === 1 || t === "true" || t === "1";
		case "text": return t == null ? "" : String(t);
		case "color":
			if (Array.isArray(t)) return [
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
function T(e, t) {
	return Array.isArray(e) && Array.isArray(t) ? e.length === t.length && e.every((e, n) => e === t[n]) : Object.is(e, t);
}
function E(e) {
	return String(typeof e == "number" ? Number.isInteger(e) ? e : Math.round(e * 100) / 100 : e);
}
//#endregion
export { t as n, n as t };
