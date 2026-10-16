import { t as e } from "./an1mbyt3-runtime.js";
import { useEffect as t, useImperativeHandle as n, useRef as r, useState as i } from "react";
import { jsx as a } from "react/jsx-runtime";
//#region src/lite/react.tsx
function o(e, t) {
	return t ? Object.entries(t).filter(([t, n]) => !e || !(t in e) || !Object.is(e[t], n)) : [];
}
function s(s) {
	let c = r(null), [l, u] = i(null), d = r(s);
	d.current = s;
	let f = r(void 0), p = r(void 0);
	return n(s.ref, () => l, [l]), t(() => {
		let t = c.current;
		if (!t) return;
		let n = !1, r = null, i = d.current;
		return f.current = i.data ? { ...i.data } : void 0, p.current = i.inputs ? { ...i.inputs } : void 0, e(t, {
			src: i.src,
			fit: i.fit,
			background: i.background,
			paused: i.paused,
			data: i.data,
			inputs: i.inputs
		}).then((e) => {
			if (n) return e.destroy();
			r = e, e.on("event", (e) => d.current.onEvent?.(e)), e.on("state", (e) => d.current.onStateChange?.(e)), e.on("prop", (e) => d.current.onPropChange?.(e)), u(e), d.current.onReady?.(e);
		}, console.error), () => {
			n = !0, r?.destroy(), u(null);
		};
	}, [
		s.src,
		s.fit,
		s.background
	]), t(() => {
		if (l) {
			for (let [e, t] of o(f.current, s.data)) l.set(e, t);
			f.current = s.data ? { ...s.data } : void 0;
		}
	}, [l, s.data]), t(() => {
		if (l) {
			for (let [e, t] of o(p.current, s.inputs)) l.setInput(e, t);
			p.current = s.inputs ? { ...s.inputs } : void 0;
		}
	}, [l, s.inputs]), t(() => {
		l && (s.paused ? l.pause() : l.play());
	}, [l, s.paused]), /* @__PURE__ */ a("div", {
		ref: c,
		className: s.className,
		title: s.title,
		style: {
			position: "relative",
			...s.style
		}
	});
}
//#endregion
export { s as ToastyLite };
