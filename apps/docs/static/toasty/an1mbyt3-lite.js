import { n as e, t } from "./an1mbyt3-runtime.js";
//#region src/lite/index.ts
var n = class extends HTMLElement {
	static observedAttributes = [
		"src",
		"fit",
		"background",
		"paused",
		"data",
		"inputs",
		"ink"
	];
	player = null;
	box = null;
	loading = 0;
	connectedCallback() {
		this.style.display || (this.style.display = "inline-block"), this.load();
	}
	disconnectedCallback() {
		this.stopInk?.(), this.stopInk = null, this.player?.destroy(), this.player = null, this.box?.remove(), this.box = null;
	}
	attributeChangedCallback(e, t, n) {
		if (!(t === n || !this.isConnected)) {
			if (e === "src" || e === "fit" || e === "background") this.load();
			else if (this.player) {
				if (e === "paused") this.hasAttribute("paused") ? this.player.pause() : this.player.play();
				else if (e === "data") this.player.setData(this.json("data") ?? {});
				else if (e === "inputs") for (let [e, t] of Object.entries(this.json("inputs") ?? {})) this.player.setInput(e, t);
				else e === "ink" && this.applyInk();
			}
		}
	}
	stopInk = null;
	applyInk() {
		let e = this.getAttribute("ink");
		if (!e || !this.player) return;
		let t = () => {
			let t = document.createElement("span");
			t.style.color = e, this.appendChild(t);
			let n = getComputedStyle(t).color;
			t.remove();
			let r = n.match(/rgba?\(([^)]+)\)/);
			if (!r) return;
			let [i, a, o, s] = r[1].split(/[ ,/]+/).filter(Boolean).map(Number);
			this.player?.set("ink", [
				i / 255,
				a / 255,
				o / 255,
				Number.isFinite(s) ? s : 1
			]);
		};
		if (t(), !this.stopInk && e === "currentColor") {
			let e = new MutationObserver(t);
			e.observe(document.documentElement, {
				attributes: !0,
				attributeFilter: [
					"class",
					"data-theme",
					"style"
				]
			});
			let n = matchMedia("(prefers-color-scheme: dark)");
			n.addEventListener("change", t), this.stopInk = () => {
				e.disconnect(), n.removeEventListener("change", t);
			};
		}
	}
	json(e) {
		let t = this.getAttribute(e);
		if (!t) return null;
		try {
			return JSON.parse(t);
		} catch {
			return console.warn(`<toasty-lite>: "${e}" must be JSON`), null;
		}
	}
	async load() {
		let e = this.getAttribute("src");
		if (!e) return;
		let n = ++this.loading;
		this.player?.destroy(), this.player = null, this.box?.remove();
		let r = document.createElement("div");
		r.style.cssText = "width:100%;height:100%", this.box = r, this.appendChild(r);
		try {
			let i = await t(r, {
				src: e,
				fit: this.getAttribute("fit") ?? void 0,
				background: this.hasAttribute("background"),
				paused: this.hasAttribute("paused"),
				data: this.json("data") ?? void 0,
				inputs: this.json("inputs") ?? void 0
			});
			if (n !== this.loading) return i.destroy();
			this.player = i, this.applyInk(), this.dispatchEvent(new CustomEvent("toasty:ready", {
				detail: { player: i },
				bubbles: !0
			}));
		} catch (e) {
			console.error(e), this.dispatchEvent(new CustomEvent("toasty:error", {
				detail: { error: e },
				bubbles: !0
			}));
		}
	}
};
typeof customElements < "u" && !customElements.get("toasty-lite") && customElements.define("toasty-lite", n);
//#endregion
export { n as ToastyLiteElement, t as createLite, e as loadLite };
