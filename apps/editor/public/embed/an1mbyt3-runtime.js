//#region src/lite/fx.ts
var e = (e) => `rgba(${Math.round(e[0] * 255)},${Math.round(e[1] * 255)},${Math.round(e[2] * 255)},${e[3]})`, t = new class {
	free = [];
	take(e, t) {
		let n = this.free.findIndex((n) => n.width === e && n.height === t), r = n >= 0 ? this.free.splice(n, 1)[0] : this.free.pop() ?? document.createElement("canvas");
		(r.width !== e || r.height !== t) && (r.width = e, r.height = t);
		let i = r.getContext("2d");
		return i.setTransform(1, 0, 0, 1, 0, 0), i.globalCompositeOperation = "source-over", i.globalAlpha = 1, i.filter = "none", i.clearRect(0, 0, e, t), r;
	}
	give(e) {
		this.free.push(e);
	}
}();
function n(e) {
	return e.getContext("2d", { willReadFrequently: !1 });
}
function r(e) {
	let r = t.take(e.width, e.height);
	return n(r).drawImage(e, 0, 0), r;
}
function i(t, i) {
	let a = r(t), o = n(a);
	return o.globalCompositeOperation = "source-in", o.fillStyle = e(i), o.fillRect(0, 0, a.width, a.height), o.globalCompositeOperation = "source-over", a;
}
function a(e, t, n) {
	let r = e * 374761393 + t * 668265263 + n * 2147483647 | 0;
	return r = Math.imul(r ^ r >>> 13, 1274126177), ((r ^ r >>> 16) >>> 0) / 4294967295;
}
function o(e, t, n, r = 0) {
	let i = Math.floor(e), o = Math.floor(t), s = e - i, c = t - o, l = (e) => e * e * (3 - 2 * e), u = (e) => (e % n + n) % n, d = a(u(i), u(o), r), f = a(u(i + 1), u(o), r), p = a(u(i), u(o + 1), r), m = a(u(i + 1), u(o + 1), r), h = l(s), g = l(c);
	return d + (f - d) * h + (p - d) * g + (d - f - p + m) * h * g;
}
function s(e, t, n, r, i = 0) {
	let a = 0, s = .5, c = n;
	for (let n = 0; n < r; n++) a += o(e * c, t * c, c, i + n * 17) * s, s *= .5, c *= 2;
	return a;
}
function c(e, t, n, r) {
	let i = 3 + Math.round(r * 3), c = Math.PI * 2;
	switch (e) {
		case "Canvas": {
			let e = t * 16, r = n * 16, a = Math.floor(e), c = Math.floor(r), l = e - a - .5, u = r - c - .5, d = a + c & 1, f = .42 + (o(c * 2.3, 1, 16) - .5) * .12, p = .42 + (o(3, a * 2.9, 16) - .5) * .12, m = Math.sqrt(Math.max(0, 1 - (u / f) ** 2)), h = Math.sqrt(Math.max(0, 1 - (l / p) ** 2)), g = d ? m : h, _ = (d ? h : m) * .55;
			return .24 + Math.max(g, _) * (.82 + .3 * o(e * .35, r * .35, 6, 5)) * .5 + (s(t, n, 22, i, 9) - .5) * .12;
		}
		case "Paper": return .4 + s(t, n, 3, i) * .18 + s(t, n * 7, 2, 3, 3) * .1 + o(t * 40, n * 40, 40, 7) * .08 * (.5 + r);
		case "Linen": {
			let e = .5 + .5 * Math.sin(t * 14 * c + o(t * 4, n * 28, 4, 1) * 2), r = .5 + .5 * Math.sin(n * 14 * c + o(t * 28, n * 4, 28, 2) * 2);
			return .35 + e * r * .25 + (e + r) * .08 + (s(t, n, 5, 3, 4) - .5) * .15;
		}
		case "Noise": return s(t, n, 4, i);
		case "Waves": {
			let e = s(t, n, 2, 3, 6) * 3;
			return .5 + Math.sin((n + Math.sin(t * c) * .08) * 4 * c + e) * .32;
		}
		case "Fluid": {
			let e = s(t, n, 2, 4, 11), r = s(t, n, 2, 4, 23);
			return .25 + s(t + e * .3, n + r * .3, 2, i, 31) * .7;
		}
		case "Marble": {
			let e = s(t, n, 2, i, 12) * 6;
			return .32 + Math.abs(Math.sin((t + n * .4) * 2 * c + e)) ** .35 * .42;
		}
		case "Concrete": {
			let e = s(t, n, 4, i, 2), r = +(o(t * 32, n * 32, 32, 8) > .8), c = +(a(Math.floor(t * 96), Math.floor(n * 96), 13) > .985);
			return .36 + e * .28 - r * .18 + c * .15;
		}
		case "Dust": {
			let e = +(a(Math.floor(t * 48), Math.floor(n * 48), 3) > .97), r = Math.floor(t * 9), i = a(r, 3, 5), s = +(i > .8 && Math.abs(t * 9 - r - i) < .02 && o(r, n * 3, 3, 4) > .3);
			return .5 - e * .35 + s * .3;
		}
		case "Brushed": return .3 + s(t, n * 16, 1, i, 21) * .45;
		default: return .5;
	}
}
var l = /* @__PURE__ */ new Map();
function u(e, t) {
	if (e.source === "Image") {
		if (!e.image) return null;
		let n = `img:${e.image.slice(0, 64)}:${e.image.length}`, r = d.get(n);
		if (r) return r.complete ? r : null;
		let i = new Image();
		return i.onload = t, i.src = e.image, d.set(n, i), null;
	}
	let n = `${e.source}:${e.detail.toFixed(2)}:${e.contrast.toFixed(2)}:${e.tint.join(",")}`, r = l.get(n);
	if (!r) {
		r = document.createElement("canvas"), r.width = r.height = 256;
		let t = r.getContext("2d"), i = t.createImageData(256, 256), a = 1 + e.contrast * 1.5;
		for (let t = 0; t < 256; t++) for (let n = 0; n < 256; n++) {
			let r = Math.min(1, Math.max(0, (c(e.source, n / 256, t / 256, e.detail) - .5) * a + .5)), o = (t * 256 + n) * 4;
			i.data[o] = r * e.tint[0] * 255, i.data[o + 1] = r * e.tint[1] * 255, i.data[o + 2] = r * e.tint[2] * 255, i.data[o + 3] = 255;
		}
		t.putImageData(i, 0, 0), l.set(n, r);
	}
	return r;
}
var d = /* @__PURE__ */ new Map();
function f(e) {
	return {
		Normal: "source-over",
		Multiply: "multiply",
		Screen: "screen",
		Overlay: "overlay",
		SoftLight: "soft-light",
		HardLight: "hard-light",
		ColorDodge: "color-dodge",
		ColorBurn: "color-burn",
		Darken: "darken",
		Lighten: "lighten",
		Difference: "difference",
		LinearLight: "hard-light"
	}[e] ?? "soft-light";
}
function p(e, i, a, o, s) {
	let c = u(i, s);
	if (!c) return;
	let l = n(e), d = r(e), p = l.createPattern(c, "repeat");
	if (p) {
		let t = i.source === "Image" ? Math.max(c.naturalWidth || 256, 1) : 256, n = [
			"Fluid",
			"Waves",
			"Noise"
		].includes(i.source) ? o * i.speed * .15 : 0, r = new DOMMatrix(a.toString()).rotate(i.rotation).scale(i.scale / t, -i.scale / t).translate((i.offset[0] + n) * t, (i.offset[1] + n * .6) * t);
		p.setTransform(r), l.save(), l.setTransform(1, 0, 0, 1, 0, 0), l.globalCompositeOperation = f(i.blend), l.globalAlpha = Math.min(1, Math.max(0, i.opacity * i.tint[3])), l.fillStyle = p, l.fillRect(0, 0, e.width, e.height), l.globalAlpha = 1, l.globalCompositeOperation = "destination-in", l.drawImage(d, 0, 0), l.restore();
	}
	t.give(d);
}
var m = null, h = null;
function g(e) {
	let t = () => {
		let t = document.createElement("canvas");
		t.width = t.height = 128;
		let n = t.getContext("2d"), r = n.createImageData(128, 128);
		for (let t = 0; t < 16384; t++) {
			let n = Math.random() * 255;
			r.data[t * 4] = n, r.data[t * 4 + 1] = e ? n : Math.random() * 255, r.data[t * 4 + 2] = e ? n : Math.random() * 255, r.data[t * 4 + 3] = 255;
		}
		return n.putImageData(r, 0, 0), t;
	};
	return e ? m ??= t() : h ??= t();
}
function _(a, o, s, c, l, u, d) {
	let f = n(a), m = o.clip || o.feather > 0 ? r(a) : null;
	if (o.background[3] > 0 && c) {
		let [t, n, r, i] = c, a = o.padding;
		f.save(), f.setTransform(s), f.globalCompositeOperation = "destination-over", f.fillStyle = e(o.background), f.beginPath();
		let l = r - t + a * 2, u = i - n + a * 2;
		f.roundRect(t - a, n - a, l, u, Math.min(o.radius, Math.min(l, u) / 2)), f.fill(), f.restore();
	}
	for (let e of o.effects) if (e.type === "shadow") {
		let n = i(a, e.color);
		f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.globalCompositeOperation = "destination-over", f.filter = `blur(${Math.max(0, e.blur * l * .5)}px)`, f.drawImage(n, e.offset[0] * l, -e.offset[1] * l), f.restore(), t.give(n);
	} else if (e.type === "stroke" && e.width > 0) {
		let n = i(a, e.color), r = e.width * l;
		f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.globalCompositeOperation = "destination-over";
		for (let e = 1; e <= 2; e++) for (let t = 0; t < 16; t++) {
			let i = t / 16 * Math.PI * 2 + e * .3;
			f.drawImage(n, Math.cos(i) * r * (e / 2), Math.sin(i) * r * (e / 2));
		}
		f.restore(), t.give(n);
	} else if (e.type === "blur" && e.radius > 0) {
		let n = r(a);
		f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.clearRect(0, 0, a.width, a.height), f.filter = `blur(${e.radius * l * .5}px)`, f.drawImage(n, 0, 0), f.restore(), t.give(n);
	} else if (e.type === "glow") {
		let n = e.tint.slice(0, 3).every((e) => e >= .999) ? r(a) : i(a, e.tint);
		f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.globalCompositeOperation = "lighter", f.filter = `blur(${e.radius * l * .5}px) brightness(${Math.max(0, e.intensity)})`, f.drawImage(n, 0, 0), f.restore(), t.give(n);
	} else if (e.type === "grain" && e.amount > 0) {
		let n = g(e.mono), i = f.createPattern(n, "repeat");
		if (i) {
			let n = Math.max(1, e.size * l), o = e.animated ? Math.floor(u * 24) : 0;
			i.setTransform(new DOMMatrix().translate(o * 37 % 128, o * 53 % 128).scale(n, n));
			let s = r(a);
			f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.globalCompositeOperation = "overlay", f.globalAlpha = Math.min(1, e.amount * 1.6), f.fillStyle = i, f.fillRect(0, 0, a.width, a.height), f.globalAlpha = 1, f.globalCompositeOperation = "destination-in", f.drawImage(s, 0, 0), f.restore(), t.give(s);
		}
	} else e.type === "gradientMap" && e.stops.length > 0 ? w(a, e) : e.type === "texture" && p(a, e, s, u, d);
	if (m && o.clip && (f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.globalCompositeOperation = "destination-in", f.drawImage(m, 0, 0), f.restore()), m && o.feather > 0) {
		let e = S(m, o.feather * l);
		f.save(), f.setTransform(1, 0, 0, 1, 0, 0), f.globalCompositeOperation = "destination-in", f.drawImage(e, 0, 0), f.restore(), t.give(e);
	}
	m && t.give(m);
}
var v = typeof CanvasRenderingContext2D < "u" && "filter" in CanvasRenderingContext2D.prototype, y = null, b = /* @__PURE__ */ new Set();
function x(e, t) {
	if (!b.has(e)) {
		y || (y = document.createElementNS("http://www.w3.org/2000/svg", "svg"), y.setAttribute("width", "0"), y.setAttribute("height", "0"), y.setAttribute("aria-hidden", "true"), y.style.cssText = "position:absolute;width:0;height:0;overflow:hidden;pointer-events:none", document.body.appendChild(y));
		let n = document.createElementNS("http://www.w3.org/2000/svg", "filter");
		n.setAttribute("id", e), n.setAttribute("color-interpolation-filters", "sRGB"), n.innerHTML = t, y.appendChild(n), b.add(e);
	}
	return `url(#${e})`;
}
function S(e, r) {
	if (!v) return T(e, r);
	let i = t.take(e.width, e.height), a = n(i);
	return a.filter = `blur(${r}px) ${x("an1-lite-feather", "<feComponentTransfer><feFuncA type=\"linear\" slope=\"2\" intercept=\"-1\"/></feComponentTransfer>")}`, a.drawImage(e, 0, 0), a.filter = "none", i;
}
function C(e) {
	let t = [...e.stops].sort((e, t) => e.position - t.position), n = /* @__PURE__ */ new Float32Array(1024);
	for (let r = 0; r < 256; r++) {
		let i = Math.min(1, Math.max(0, r / 255 + e.offset)), a = t[0].color;
		i >= t[t.length - 1].position && (a = t[t.length - 1].color);
		for (let n = 0; n < t.length - 1; n++) {
			let r = t[n], o = t[n + 1];
			if (i >= r.position && i < o.position) {
				let t = (i - r.position) / Math.max(o.position - r.position, 1e-4), n = t < .5 ? 0 : 1;
				t = n + (t * t * (3 - 2 * t) - n) * Math.min(1, Math.max(0, e.smooth)), a = r.color.map((e, n) => e + (o.color[n] - e) * t);
			}
		}
		n.set(a, r * 4);
	}
	return n;
}
function w(e, i) {
	if (!v) return E(e, i);
	let a = C(i), o = (e) => Array.from({ length: 48 }, (t, n) => a[Math.round(n / 47 * 255) * 4 + e].toFixed(3)).join(" "), s = o(0), c = o(1), l = o(2), u = 0;
	for (let e of s + c + l) u = Math.imul(u, 31) + e.charCodeAt(0) | 0;
	let d = "0.2126 0.7152 0.0722 0 0", f = x(`an1-lite-gm-${(u >>> 0).toString(36)}`, `<feColorMatrix type="matrix" values="${d} ${d} ${d} 0 0 0 1 0"/><feComponentTransfer><feFuncR type="table" tableValues="${s}"/><feFuncG type="table" tableValues="${c}"/><feFuncB type="table" tableValues="${l}"/></feComponentTransfer>`), p = r(e), m = n(e);
	m.save(), m.setTransform(1, 0, 0, 1, 0, 0), m.globalCompositeOperation = "source-atop", m.globalAlpha = Math.min(1, Math.max(0, i.amount)), m.filter = f, m.drawImage(p, 0, 0), m.restore(), t.give(p);
}
function T(e, r) {
	let i = t.take(e.width, e.height), a = n(i);
	a.drawImage(e, 0, 0), a.globalCompositeOperation = "source-in", a.fillStyle = "#fff", a.fillRect(0, 0, i.width, i.height), a.globalCompositeOperation = "source-over";
	let o = t.take(e.width, e.height), s = n(o);
	s.filter = `blur(${r}px)`, s.drawImage(i, 0, 0), s.filter = "none", t.give(i);
	let c = s.getImageData(0, 0, o.width, o.height), l = c.data;
	for (let e = 3; e < l.length; e += 4) {
		let t = Math.min(1, Math.max(0, (l[e] / 255 - .5) * 2));
		l[e] = t * t * (3 - 2 * t) * 255;
	}
	return s.putImageData(c, 0, 0), o;
}
function E(e, t) {
	let r = n(e), i = C(t), a = r.getImageData(0, 0, e.width, e.height), o = a.data, s = Math.min(1, Math.max(0, t.amount));
	for (let e = 0; e < o.length; e += 4) {
		if (o[e + 3] === 0) continue;
		let t = Math.round(.2126 * o[e] + .7152 * o[e + 1] + .0722 * o[e + 2]) * 4;
		o[e] += (i[t] * 255 - o[e]) * s, o[e + 1] += (i[t + 1] * 255 - o[e + 1]) * s, o[e + 2] += (i[t + 2] * 255 - o[e + 2]) * s;
	}
	r.putImageData(a, 0, 0);
}
function D(e) {
	return e.some((e) => e.type === "grain" && e.animated || e.type === "texture" && e.speed > 0 && [
		"Fluid",
		"Waves",
		"Noise"
	].includes(e.source));
}
//#endregion
//#region src/lite/runtime.ts
var O = /* @__PURE__ */ new Map();
function k(e) {
	let t = new URL(String(e), location.href).href, n = O.get(t);
	return n || (n = fetch(t).then((e) => {
		if (!e.ok) throw Error(`Toasty Lite: ${t} → ${e.status}`);
		return e.json();
	}), O.set(t, n)), n;
}
async function A(e, t) {
	let n = typeof t.src == "string" || t.src instanceof URL ? await k(t.src) : t.src;
	if (n.format !== "toasty-lite") throw Error("Toasty Lite: not a Toasty Lite file");
	return await L(n), new q(e, n, t);
}
var j = /* @__PURE__ */ new Set(), M = 0, N = 0;
function P(e) {
	let t = N ? Math.min(.1, (e - N) / 1e3) : 1 / 60;
	N = e;
	for (let e of j) e.frame(t);
	M = j.size ? requestAnimationFrame(P) : 0, M || (N = 0);
}
function F() {
	!M && j.size && (M = requestAnimationFrame(P));
}
var I = /* @__PURE__ */ new Set();
async function L(e) {
	for (let t of e.fonts) if (!(I.has(t.name) || typeof FontFace > "u")) {
		I.add(t.name);
		try {
			let e = Uint8Array.from(atob(t.data), (e) => e.charCodeAt(0)), n = new FontFace(t.name, e);
			await n.load(), document.fonts.add(n);
		} catch {}
	}
}
function R(e) {
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
function z(e, t) {
	let n = 0;
	return e.map((e) => {
		let r = t.slice(n, n + e.p.length);
		return n += e.p.length, {
			c: e.c,
			p: r
		};
	});
}
function B(e) {
	let t = new Path2D();
	for (let { c: n, p: r } of e) n === "M" ? t.moveTo(r[0], r[1]) : n === "L" ? t.lineTo(r[0], r[1]) : n === "C" ? t.bezierCurveTo(r[0], r[1], r[2], r[3], r[4], r[5]) : t.closePath();
	return t;
}
function V(e) {
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
function H(e, t, n, r) {
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
function U(e, t, n, r) {
	let i = new Path2D(), a = e.total;
	if (a <= 0) return i;
	let o = Math.min(t, n), s = Math.max(t, n);
	return s - o >= 1 - 1e-6 ? (H(e, 0, a, i), i) : (o = ((o + r) % 1 + 1) % 1, s = o + (Math.max(t, n) - Math.min(t, n)), s <= 1 ? H(e, o * a, s * a, i) : (H(e, o * a, a, i), H(e, 0, (s - 1) * a, i)), i);
}
function W(e, t) {
	let [n, r, i, a] = e, o = t;
	for (let e = 0; e < 6; e++) {
		let e = 1 - o, r = 3 * e * e * o * n + 3 * e * o * o * i + o * o * o - t, a = 3 * e * e * n + 6 * e * o * (i - n) + 3 * o * o * (1 - i);
		if (Math.abs(a) < 1e-6) break;
		o = Math.min(1, Math.max(0, o - r / a));
	}
	let s = 1 - o;
	return 3 * s * s * o * r + 3 * s * o * o * a + o * o * o;
}
function G(e, t) {
	if (t <= e[0].t) return e[0].v;
	let n = e[e.length - 1];
	if (t >= n.t) return n.v;
	let r = 1;
	for (; e[r].t < t;) r++;
	let i = e[r - 1], a = e[r], o = (t - i.t) / Math.max(1e-6, a.t - i.t);
	return i.e && (o = W(i.e, o)), i.v.length === a.v.length ? i.v.map((e, t) => e + (a.v[t] - e) * o) : o < 1 ? i.v : a.v;
}
var K = (e, t, n) => e.length === t.length ? e.map((e, r) => e + (t[r] - e) * n) : n < .5 ? e : t, q = class {
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
		releases: 0,
		leaving: !1
	};
	worlds = [];
	cleanups = [];
	lastPose = /* @__PURE__ */ new Map();
	clock = 0;
	fxStep = 0;
	animatedFx = !1;
	wake = () => {
		this.dirty = !0, F();
	};
	container;
	file;
	options;
	constructor(e, t, n) {
		this.container = e, this.file = t, this.options = n, this.canvas = document.createElement("canvas"), this.canvas.style.cssText = "display:block;width:100%;height:100%;touch-action:manipulation", e.appendChild(this.canvas);
		let r = this.canvas.getContext("2d");
		if (!r) throw Error("Toasty Lite: no 2D canvas");
		this.ctx = r, this.paused = !!n.paused, this.nodes = t.nodes.map((e) => {
			let t = e.path ? R(e.path) : null, n = null;
			return e.kind === "image" && e.image?.src && (n = new Image(), n.onload = () => this.wake(), n.src = e.image.src), {
				def: e,
				template: t,
				cmds: t,
				path: t ? B(t) : null,
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
				text: null,
				img: n
			};
		}), this.animatedFx = t.nodes.some((e) => e.fx && D(e.fx.effects)) || !!t.texture && D([t.texture]);
		for (let e of t.properties) this.props.set(e.name, Q(e.kind, e.default));
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
		let i = new ResizeObserver(() => this.resize());
		if (i.observe(e), this.cleanups.push(() => i.disconnect()), typeof IntersectionObserver < "u") {
			let t = new IntersectionObserver((e) => {
				this.visible = e.some((e) => e.isIntersecting), this.visible && (this.dirty = !0);
			});
			t.observe(e), this.cleanups.push(() => t.disconnect());
		}
		let a = (e) => {
			let t = this.canvas.getBoundingClientRect();
			this.pointer.x = e.clientX - t.left, this.pointer.y = e.clientY - t.top, this.pointer.inside = !0, this.dirty = !0, F();
		}, o = () => {
			this.pointer.leaving = !0, this.dirty = !0, F();
		}, s = (e) => {
			a(e), this.pointer.leaving = !1, this.pointer.down = !0, this.pointer.presses++;
		}, c = (e) => {
			a(e), this.pointer.down = !1, this.pointer.releases++;
		};
		this.canvas.addEventListener("pointermove", a), this.canvas.addEventListener("pointerleave", o), this.canvas.addEventListener("pointerdown", s), this.canvas.addEventListener("pointerup", c);
		let l = () => {
			this.pointer.down = !1, this.pointer.presses = 0, this.pointer.releases = 0, o();
		};
		this.canvas.addEventListener("pointercancel", l), this.cleanups.push(() => {
			this.canvas.removeEventListener("pointercancel", l), this.canvas.removeEventListener("pointermove", a), this.canvas.removeEventListener("pointerleave", o), this.canvas.removeEventListener("pointerdown", s), this.canvas.removeEventListener("pointerup", c);
		}), this.resize(), j.add(this), F();
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
			this.dirty = !0, F();
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
		this.paused = !1, this.dirty = !0, F();
	}
	pause() {
		this.paused = !0;
	}
	destroy() {
		j.delete(this);
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
		let r = this.file.properties.find((t) => t.name === e), i = r ? Q(r.kind, t) : t;
		ee(this.props.get(e), i) || (this.props.set(e, i), this.dirty = !0, n && this.emit("prop", {
			name: e,
			value: i
		}), F());
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
		if (r) for (let e of r.tracks) e.keys.length && n.set(`${e.node}:${e.prop}`, G(e.keys, t));
	}
	frame(e) {
		if (!this.visible) return;
		this.clock += e, this.fxStep += e;
		let t = !1;
		if (this.animatedFx && this.fxStep >= 1 / 24 && (this.fxStep = 0, t = !0), !this.paused) {
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
		!t && !n && !this.dirty || (this.dirty = !1, this.draw(), F());
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
					a && o ? e.set(t, K(a, o, i)) : o && e.set(t, o);
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
			d && !a.pressed && u(a.def.listeners?.down), (a.pressed || d) && (i > 0 || !this.pointer.down) && (u(a.def.listeners?.up), c && u(a.def.listeners?.click)), !c && a.hover && u(a.def.listeners?.exit), a.hover = c, a.pressed = l && !this.pointer.leaving;
			let f = l ? s.pressedScale : c ? s.hoverScale : 1, p = s.interactable ? l ? s.pressedColor : c ? s.hoverColor : s.normalColor : s.normalColor, m = s.fade > 0 ? Math.min(1, e / s.fade) : 1, h = a.lookScale + (f - a.lookScale) * m, g = a.lookTint.map((e, t) => e + (p[t] - e) * m);
			(Math.abs(h - a.lookScale) > 1e-4 || g.some((e, t) => Math.abs(e - a.lookTint[t]) > 1e-4)) && (t = !0), a.lookScale = h, a.lookTint = g;
		}), this.pointer.leaving && (this.pointer.leaving = !1, this.pointer.inside = !1, this.pointer.down = !1, this.dirty = !0, F()), t;
	}
	runActions(e, t) {
		for (let n of e) switch (n.type) {
			case "setInput":
				this.setInput(n.input, Z(n.value));
				break;
			case "fireTrigger":
				this.fire(n.input);
				break;
			case "toggleInput":
				this.setInput(n.input, !this.inputs.get(n.input));
				break;
			case "setProperty": {
				let e = this.props.get(n.property), t = /^[+-]\d/.test(n.value) && typeof e == "number";
				this.set(n.property, t ? e + Number(n.value) : Z(n.value));
				break;
			}
			case "event":
				this.emit("event", {
					name: n.name,
					payload: n.arg === "" ? void 0 : Z(n.arg)
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
		let e = this.ctx, n = e, { w: r, h: i, dpr: a } = this.size, o = this.file, s = this.pose();
		this.cur = s;
		let c = new Set(s.keys()), l = (e) => !c.has(e);
		o.nodes.forEach((e, t) => {
			for (let n of e.bindings ?? []) {
				let r = this.props.get(n.property);
				if (r !== void 0) {
					if (n.target === "text") this.nodes[t].text = n.format ? n.format.replace("{}", $(r)) : $(r);
					else if (n.target === "visible") l(`${t}:visible`) && s.set(`${t}:visible`, [+!!r]);
					else if ((n.target === "fill" || n.target === "strokeColor") && Array.isArray(r)) l(`${t}:${n.target}`) && s.set(`${t}:${n.target}`, r);
					else {
						let i = Number(r) * n.scale + n.offset, a = s.get(`${t}:translation`) ?? [e.x, e.y], o = s.get(`${t}:scale`) ?? [e.scaleX, e.scaleY];
						n.target === "x" ? s.set(`${t}:translation`, [i, a[1]]) : n.target === "y" ? s.set(`${t}:translation`, [a[0], i]) : n.target === "rotation" ? s.set(`${t}:rotation`, [i * Math.PI / 180]) : n.target === "scale" ? s.set(`${t}:scale`, [i, i]) : n.target === "scaleX" ? s.set(`${t}:scale`, [i, o[1]]) : n.target === "scaleY" ? s.set(`${t}:scale`, [o[0], i]) : l(`${t}:${n.target}`) && s.set(`${t}:${n.target}`, [i]);
					}
				}
			}
		}), e.setTransform(1, 0, 0, 1, 0, 0), e.clearRect(0, 0, this.canvas.width, this.canvas.height);
		let u = this.options.fit === "cover" ? Math.max : Math.min, d = o.view.zoom || 1, f = u(r / (o.width / d), i / (o.height / d)) * d * a, m = new DOMMatrix().translate(r * a / 2, i * a / 2).scale(f, -f).translate(-o.view.x, -o.view.y);
		this.options.background && o.background && (e.fillStyle = J(o.background), e.fillRect(0, 0, this.canvas.width, this.canvas.height));
		let h = [], g = [];
		o.nodes.forEach((e, t) => {
			let n = s.get(`${t}:translation`) ?? [e.x, e.y], r = s.get(`${t}:rotation`)?.[0] ?? e.rotation, i = s.get(`${t}:scale`) ?? [e.scaleX, e.scaleY], a = s.get(`${t}:anchor`) ?? [e.anchorX, e.anchorY], c = this.nodes[t].lookScale, l = new DOMMatrix().translate(n[0], n[1]).rotate(r * 180 / Math.PI).scale(i[0] * c, i[1] * c).translate(-a[0], -a[1]);
			h[t] = (e.parent == null ? m : h[e.parent]).multiply(l);
			let u = e.parent != null && o.nodes[e.parent].button?.tintChildren ? g[e.parent] : [
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
			g[t] = d.map((e, t) => e * u[t]);
		}), this.worlds = h;
		let v = o.nodes.map((e) => e.fx);
		for (let [e, t] of s) {
			let n = e.indexOf(":fx/");
			if (n < 0) continue;
			let r = Number(e.slice(0, n)), i = o.nodes[r]?.fx;
			if (!i) continue;
			let a = v[r];
			a === i && (a = v[r] = {
				...i,
				effects: i.effects.map((e) => ({ ...e }))
			});
			let s = e.slice(n + 4).split("/"), c = t.length === 1 ? t[0] : [...t];
			s.length === 1 ? a[s[0]] = c : a.effects[Number(s[0])] && (a.effects[Number(s[0])][s[1]] = c);
		}
		let y = {
			x: 0,
			y: 0
		}, b = (e) => y.x || y.y ? new DOMMatrix().translate(-y.x, -y.y).multiply(e) : e, x = (e) => Math.hypot(h[e].a, h[e].b) || 1, S = (e) => {
			let t = o.nodes[e];
			if (t.blend && !t.fx && (n.globalCompositeOperation = t.blend), t.shadow) {
				let r = x(e);
				n.shadowColor = J(t.shadow.color), n.shadowBlur = t.shadow.blur * r, n.shadowOffsetX = t.shadow.offset[0] * r, n.shadowOffsetY = -t.shadow.offset[1] * r;
			}
			return () => {
				n.globalCompositeOperation = "source-over", t.shadow && (n.shadowColor = "transparent", n.shadowBlur = 0, n.shadowOffsetX = 0, n.shadowOffsetY = 0);
			};
		}, C = [], w = (e) => {
			let t = o.nodes[e], r = this.nodes[e];
			if (!this.isShown(e)) return;
			n.setTransform(b(h[e]));
			let i = g[e];
			if (t.kind === "shape" && r.template) {
				let a = s.get(`${e}:path`);
				if (a) {
					let e = a.join(",");
					if (e !== r.pathKey) {
						let t = z(r.template, a);
						r.path = B(t), r.flat = null, r.pathKey = e, r.cmds = t;
					}
				}
				let o = r.path, c = S(e), l = s.get(`${e}:fill`) ?? t.fill;
				if (t.gradient ? n.fillStyle = X(n, t.gradient, i) : l && l[3] > 0 && (n.fillStyle = J(Y(l, i))), (t.gradient || l && l[3] > 0) && n.fill(o), t.stroke) {
					let a = s.get(`${e}:strokeWidth`)?.[0] ?? t.stroke.width, c = s.get(`${e}:strokeColor`) ?? t.stroke.color;
					if (a > 0 && (c[3] > 0 || t.stroke.gradient)) {
						n.lineWidth = a, n.lineCap = t.stroke.cap, n.lineJoin = t.stroke.join, n.strokeStyle = t.stroke.gradient ? X(n, t.stroke.gradient, i) : J(Y(c, i));
						let l = o;
						if (t.trim) {
							let n = s.get(`${e}:trimStart`)?.[0] ?? t.trim.start, i = s.get(`${e}:trimEnd`)?.[0] ?? t.trim.end, a = s.get(`${e}:trimOffset`)?.[0] ?? t.trim.offset;
							Math.abs(i - n) < 1e-4 ? l = new Path2D() : n <= 0 && i >= 1 && a === 0 || (r.flat ||= V(r.cmds ?? r.template), l = U(r.flat, n, i, a));
						}
						t.dash && t.dash.dash > 0 && (n.setLineDash([t.dash.dash, t.dash.gap]), n.lineDashOffset = -(s.get(`${e}:dashOffset`)?.[0] ?? t.dash.offset)), n.stroke(l), n.setLineDash([]);
					}
				}
				c(), t.clip && (n.save(), n.clip(o), C.push(e));
			} else if (t.kind === "image" && t.image && r.img?.complete && r.img.naturalWidth > 0) {
				let a = S(e);
				n.scale(1, -1), n.globalAlpha = i[3], n.drawImage(r.img, -t.image.width / 2, -t.image.height / 2, t.image.width, t.image.height), n.globalAlpha = 1, a();
			} else if (t.kind === "text" && t.text) {
				let a = S(e), o = s.get(`${e}:textSize`)?.[0] ?? t.text.size, c = s.get(`${e}:fill`) ?? t.text.color;
				n.scale(1, -1), n.font = `${o}px "${t.text.font}", system-ui, sans-serif`, n.textAlign = "center", n.textBaseline = "middle", n.fillStyle = J(Y(c, i)), n.fillText(r.text ?? t.text.content, 0, 0), a();
			} else t.clip && C.push(-1);
		}, T = o.nodes.map(() => 0);
		for (let e = o.nodes.length - 1; e >= 0; e--) {
			let t = e;
			for (let n = e + 1; n < o.nodes.length && this.isDescendant(n, e); n++) t = n;
			T[e] = t;
		}
		let E = (e) => {
			let t = o.nodes[e], n = this.canvas.width, r = this.canvas.height, i = v[e];
			if (!t.box || !i) return {
				x: 0,
				y: 0,
				w: n,
				h: r
			};
			let a = Math.max(0, i.padding);
			for (let e of i.effects) e.type === "shadow" ? a = Math.max(a, Math.hypot(e.offset[0], e.offset[1]) + e.blur * 1.5 + e.spread) : e.type === "stroke" ? a = Math.max(a, e.width * 1.5) : (e.type === "blur" || e.type === "glow") && (a = Math.max(a, e.radius * 1.5));
			let [s, c, l, u] = t.box, d = h[e], f = Infinity, p = Infinity, m = -Infinity, g = -Infinity;
			for (let [e, t] of [
				[s - a, c - a],
				[l + a, c - a],
				[l + a, u + a],
				[s - a, u + a]
			]) {
				let n = d.transformPoint(new DOMPoint(e, t));
				f = Math.min(f, n.x), p = Math.min(p, n.y), m = Math.max(m, n.x), g = Math.max(g, n.y);
			}
			let _ = Math.max(0, Math.floor(f) - 2), y = Math.max(0, Math.floor(p) - 2);
			return {
				x: _,
				y,
				w: Math.max(1, Math.min(n, Math.ceil(m) + 2) - _),
				h: Math.max(1, Math.min(r, Math.ceil(g) + 2) - y)
			};
		}, D = [], O = (e) => {
			if (e.kind === "clip") {
				n.restore();
				return;
			}
			let r = o.nodes[e.owner];
			_(e.layer, v[e.owner], b(h[e.owner]), r.box, x(e.owner), this.clock, this.wake), n = e.prev, y = e.prevOff, n.save(), n.setTransform(1, 0, 0, 1, 0, 0), n.globalCompositeOperation = r.blend ?? "source-over", n.drawImage(e.layer, e.rect.x - y.x, e.rect.y - y.y), n.restore(), t.give(e.layer);
		};
		for (let e = 0; e < o.nodes.length; e++) {
			for (; D.length && e > D[D.length - 1].end;) O(D.pop());
			if (o.nodes[e].fx && this.isShown(e)) {
				let r = E(e), i = t.take(r.w, r.h);
				D.push({
					kind: "fx",
					owner: e,
					end: T[e],
					layer: i,
					prev: n,
					prevOff: y,
					rect: {
						x: r.x,
						y: r.y
					}
				}), n = i.getContext("2d"), y = {
					x: r.x,
					y: r.y
				};
			}
			let r = C.length;
			w(e), C.length > r && (C[C.length - 1] === -1 ? C.pop() : D.push({
				kind: "clip",
				end: T[e]
			}));
		}
		for (; D.length;) O(D.pop());
		o.texture && p(this.canvas, o.texture, m, this.clock, this.wake);
	}
	isDescendant(e, t) {
		for (let n = this.file.nodes[e].parent; n != null; n = this.file.nodes[n].parent) if (n === t) return !0;
		return !1;
	}
};
function J(e) {
	let t = (e) => Math.round(Math.min(1, Math.max(0, e)) * 255);
	return `rgba(${t(e[0])},${t(e[1])},${t(e[2])},${Math.min(1, Math.max(0, e[3] ?? 1))})`;
}
function Y(e, t) {
	return [
		e[0] * t[0],
		e[1] * t[1],
		e[2] * t[2],
		(e[3] ?? 1) * t[3]
	];
}
function X(e, t, n) {
	let r = t.kind === "linear" ? e.createLinearGradient(t.start[0], t.start[1], t.end[0], t.end[1]) : e.createRadialGradient(t.start[0], t.start[1], 0, t.start[0], t.start[1], Math.hypot(t.end[0] - t.start[0], t.end[1] - t.start[1]) || 1);
	for (let e of t.stops) r.addColorStop(Math.min(1, Math.max(0, e.offset)), J(Y(e.color, n)));
	return r;
}
function Z(e) {
	try {
		return JSON.parse(e);
	} catch {
		return e;
	}
}
function Q(e, t) {
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
function ee(e, t) {
	return Array.isArray(e) && Array.isArray(t) ? e.length === t.length && e.every((e, n) => e === t[n]) : Object.is(e, t);
}
function $(e) {
	return String(typeof e == "number" ? Number.isInteger(e) ? e : Math.round(e * 100) / 100 : e);
}
//#endregion
export { k as n, A as t };
