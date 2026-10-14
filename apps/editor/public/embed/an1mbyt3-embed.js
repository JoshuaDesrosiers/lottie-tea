import { t as e } from "./an1mbyt3-engine.js";
//#region src/embed/index.ts
function t(t = "an1-player") {
	if (customElements.get(t)) return;
	class n extends HTMLElement {
		static observedAttributes = [
			"game",
			"wasm",
			"scene",
			"data",
			"paused",
			"fit",
			"keyboard"
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
			t === n || !this.isConnected || (e === "game" || e === "wasm" || e === "fit" || e === "keyboard" ? this.load() : this.player && this.apply(e));
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
			let t = this.getAttribute("game");
			if (!t) return;
			let n = ++this.loading;
			this.player?.destroy(), this.player = null, this.box || (this.box = document.createElement("div"), this.box.style.cssText = "position:relative;width:100%;height:100%;", this.appendChild(this.box));
			let r = {
				game: t,
				wasm: this.getAttribute("wasm") ?? void 0,
				scene: this.getAttribute("scene") ?? void 0,
				fit: this.getAttribute("fit") ?? void 0,
				keyboard: this.getAttribute("keyboard") ?? void 0,
				paused: this.hasAttribute("paused"),
				data: this.readData() ?? void 0
			};
			try {
				let t = await e(this.box, r);
				if (n !== this.loading) {
					t.destroy();
					return;
				}
				this.player = t, this.dispatchEvent(new CustomEvent("an1:ready", {
					detail: {
						scenes: t.scenes,
						scene: t.scene
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
	customElements.define(t, n);
}
//#endregion
export { e as createPlayer, t as defineAn1PlayerElement };
