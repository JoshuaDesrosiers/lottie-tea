// Phase 8 WASM smoke test glue — loads the wasm-pack-built `an1mbyt3-wasm`
// module, creates a `WasmPlayer` against the page's <canvas>, and drives it
// with a real `requestAnimationFrame` loop. This mirrors
// `an1mbyt3-renderer/examples/state_machine.rs`'s winit event loop, just
// with `requestAnimationFrame`/`delta_seconds` standing in for winit's
// `about_to_wait`/`Instant`, since `WasmPlayer` can't read a wall clock
// itself on wasm32-unknown-unknown (see its doc comment in `lib.rs`).
import init, { WasmPlayer } from "./pkg/an1mbyt3_wasm.js";

const statusEl = document.getElementById("status");
const canvas = document.getElementById("canvas");
const toggleButton = document.getElementById("toggle-button");

async function main() {
  // `init()` fetches and instantiates the .wasm binary itself — must
  // complete before any exported class/function is usable.
  await init();

  statusEl.textContent = "creating WGPU surface…";
  // `true` here is the pointer-tracking toggle: named "PointerX"/"PointerY"
  // inputs are kept updated from a window-level (not canvas-level)
  // `mousemove` listener (so tracking continues even once the cursor leaves
  // the canvas rectangle), for whichever state machine's `ConditionDef`s
  // choose to reference them — this demo's own two states don't bind to
  // them at all, so the square won't visibly react to the mouse; only the
  // toggle button's trigger moves it. Pass `false` for a component that
  // shouldn't pay for pointer tracking at all.
  const player = await WasmPlayer.create(canvas, true);
  statusEl.textContent = "running — click the button to toggle Idle/Active";

  toggleButton.addEventListener("click", () => {
    player.fire_trigger("toggle");
  });

  let lastTimestampMs = null;
  function frame(timestampMs) {
    if (lastTimestampMs === null) {
      lastTimestampMs = timestampMs;
    }
    const deltaSeconds = (timestampMs - lastTimestampMs) / 1000;
    lastTimestampMs = timestampMs;

    player.render(deltaSeconds);
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main().catch((err) => {
  statusEl.textContent = `error: ${err}`;
  console.error(err);
});
