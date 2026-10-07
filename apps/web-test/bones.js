// Glue for bones.html — creates a `WasmBoneIk` against the canvas, feeds it
// `mousemove` positions (canvas-relative, via `offsetX`/`offsetY`), and
// re-renders every frame. No `delta_seconds` here (unlike main.js/WasmPlayer)
// since IK solving isn't time-integrated — see `bone_ik.rs`'s doc comment.
import init, { WasmBoneIk } from "./pkg/an1mbyt3_wasm.js";

const statusEl = document.getElementById("status");
const canvas = document.getElementById("canvas");

async function main() {
  await init();

  statusEl.textContent = "creating WGPU surface…";
  const arm = await WasmBoneIk.create(canvas);
  statusEl.textContent = "running — move your mouse over the canvas";

  // Listening on `window` (not `canvas`) so the arm keeps reaching toward
  // the cursor even once it's left the canvas rectangle — `offsetX/offsetY`
  // only exist relative to whatever element the event fired on, so a
  // canvas-scoped listener stops receiving events the moment the cursor
  // leaves it. `getBoundingClientRect()` gives the same canvas-relative
  // coordinate space `offsetX/offsetY` would, just computed manually from
  // page-relative `clientX/clientY`, extrapolated (not clamped) past the
  // canvas edges.
  window.addEventListener("mousemove", (event) => {
    const rect = canvas.getBoundingClientRect();
    arm.set_target(event.clientX - rect.left, event.clientY - rect.top);
  });

  function frame() {
    arm.render();
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main().catch((err) => {
  statusEl.textContent = `error: ${err}`;
  console.error(err);
});
