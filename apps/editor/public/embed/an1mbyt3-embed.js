import { n as e, t } from "./an1mbyt3-engine.js";
//#region src/embed/runtimeVersion.ts
var n = "0.1.0";
//#endregion
//#region src/embed/index.ts
function r(e = "an1-player") {
	if (customElements.get(e)) return;
	class n extends HTMLElement {
		static observedAttributes = [
			"game",
			"wasm",
			"runtime",
			"scene",
			"data",
			"paused",
			"fit",
			"keyboard",
			"run-offscreen"
		];
		player = null;
		box = null;
		loading = 0;
		connectedCallback() {
			this.style.display || (this.style.display = "block"), this.load();
		}
		disconnectedCallback() {
			this.player?.destroy(), this.player = null, this.box?.remove(), this.box = null;
		}
		attributeChangedCallback(e, t, n) {
			t === n || !this.isConnected || (e === "game" || e === "wasm" || e === "runtime" || e === "fit" || e === "keyboard" || e === "run-offscreen" ? this.load() : this.player && this.apply(e));
		}
		apply(e) {
			let t = this.player;
			if (t) {
				if (e === "scene") {
					let e = this.getAttribute("scene");
					e && e !== t.scene && t.loadScene(e);
				} else if (e === "paused") this.hasAttribute("paused") ? t.pause() : t.play();
				else if (e === "data") {
					let e = this.readData();
					e && t.setData(e);
				}
			}
		}
		readData() {
			let e = this.getAttribute("data");
			if (!e) return null;
			try {
				return JSON.parse(e);
			} catch {
				return console.warn("<an1-player>: `data` must be JSON"), null;
			}
		}
		async load() {
			let e = this.getAttribute("game");
			if (!e) return;
			let n = ++this.loading;
			this.player?.destroy(), this.player = null, this.box || (this.box = document.createElement("div"), this.box.style.cssText = "position:relative;width:100%;height:100%;", this.appendChild(this.box));
			let r = {
				game: e,
				wasm: this.getAttribute("wasm") ?? void 0,
				runtimeUrl: this.getAttribute("runtime") ?? void 0,
				pauseWhenOffscreen: !this.hasAttribute("run-offscreen"),
				scene: this.getAttribute("scene") ?? void 0,
				fit: this.getAttribute("fit") ?? void 0,
				keyboard: this.getAttribute("keyboard") ?? void 0,
				paused: this.hasAttribute("paused"),
				data: this.readData() ?? void 0
			};
			try {
				let e = await t(this.box, r);
				if (n !== this.loading) {
					e.destroy();
					return;
				}
				this.player = e, this.dispatchEvent(new CustomEvent("an1:ready", {
					detail: {
						scenes: e.scenes,
						scene: e.scene
					},
					bubbles: !0
				}));
			} catch (e) {
				this.dispatchEvent(new CustomEvent("an1:error", {
					detail: { error: e },
					bubbles: !0
				})), console.error("<an1-player> failed to start", e);
			}
		}
	}
	customElements.define(e, n);
}
//#endregion
export { n as RUNTIME_VERSION, t as createPlayer, r as defineAn1PlayerElement, e as setRuntimeUrl };
