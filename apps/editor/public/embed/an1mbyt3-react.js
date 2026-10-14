import { t as e } from "./an1mbyt3-engine.js";
import { useEffect as t, useImperativeHandle as n, useRef as r, useState as i } from "react";
import { jsx as a } from "react/jsx-runtime";
//#region src/embed/react.tsx
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
		return f.current = i.data ? { ...i.data } : void 0, p.current = void 0, e(t, {
			game: i.game,
			wasm: i.wasm,
			scene: i.scene,
			data: i.data,
			paused: i.paused,
			fit: i.fit,
			keyboard: i.keyboard,
			background: i.background
		}).then((e) => {
			if (n) {
				e.destroy();
				return;
			}
			r = e, e.on("event", (e) => d.current.onEvent?.(e)), e.on("state", (e) => d.current.onStateChange?.(e)), e.on("scene", (e) => d.current.onSceneChange?.(e)), e.on("prop", (e) => d.current.onPropChange?.(e)), e.on("error", (e) => d.current.onError?.(e.error));
			for (let [t, n] of o(void 0, d.current.inputs)) e.setInput(t, n);
			p.current = d.current.inputs ? { ...d.current.inputs } : void 0, u(e), d.current.onReady?.(e);
		}).catch((e) => {
			n ? console.error(e) : d.current.onError?.(e);
		}), () => {
			n = !0, r?.destroy(), u(null);
		};
	}, [
		s.game,
		s.wasm,
		s.fit,
		s.keyboard,
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
		l && s.scene && s.scene !== l.scene && l.loadScene(s.scene);
	}, [l, s.scene]), t(() => {
		l && (s.paused ? l.pause() : l.play());
	}, [l, s.paused]), /* @__PURE__ */ a("div", {
		ref: c,
		className: s.className,
		style: {
			position: "relative",
			overflow: "hidden",
			...s.style
		}
	});
}
function c() {
	let [e, t] = i(null);
	return [t, e];
}
//#endregion
export { s as An1Player, c as useAn1Player };
