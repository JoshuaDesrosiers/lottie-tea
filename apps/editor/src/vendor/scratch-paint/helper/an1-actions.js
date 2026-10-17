// an1mbyt3: small entry points into scratch-paint's own Redux state, for
// Paint's extra UI (the color palette and the eyedropper shortcut).
import paper from '@scratch/paper';
import {changeFillColor} from '../reducers/fill-style';
import {changeStrokeColor} from '../reducers/stroke-style';
import {activateEyeDropper} from '../reducers/eye-dropper';
import {pan, zoomOnFixedPoint} from './view';
import {updateViewBounds} from '../reducers/view-bounds';
import {redrawSelectionBox} from '../reducers/selected-items';

/**
 * The fill and outline colors new strokes / shapes use (css strings, or null
 * for transparent).
 * @param {object} store the paint editor's Redux store
 * @returns {{fill: ?string, stroke: ?string}} the colors
 */
const currentColors = function (store) {
    const color = store.getState().scratchPaint.color;
    return {fill: color.fillColor.primary, stroke: color.strokeColor.primary};
};

/**
 * Sets the fill (or outline) color for what's drawn next — what picking a
 * color in scratch-paint's color picker does for the tools.
 * @param {object} store the paint editor's Redux store
 * @param {?string} color css color, or null for transparent
 * @param {boolean} outline true for the outline color
 */
const setPaintColor = function (store, color, outline) {
    store.dispatch(outline ? changeStrokeColor(color) : changeFillColor(color));
};

/**
 * Starts scratch-paint's own eyedropper (the color picker's dropper button):
 * the next click on the canvas picks a color into the fill (or outline).
 * @param {object} store the paint editor's Redux store
 * @param {boolean} outline true to pick the outline color
 */
const startEyeDropper = function (store, outline) {
    if (!paper.tool) return;
    store.dispatch(activateEyeDropper(paper.tool, color => setPaintColor(store, color, outline)));
};

/** Deepest zoom a touch pinch may reach (scratch-paint's own +/- buttons
 * and ctrl+wheel have no ceiling; a pinch reaches absurd zooms fast). */
const MAX_PINCH_ZOOM = 32;

/**
 * One step of a two-finger touch pinch / pan on the paint canvas
 * (`input/touchGestures.ts`, wired up in `PaintAssetEditor.tsx`): pans the
 * view so the artwork follows the fingers, then zooms by `scaleDelta` about
 * the fingers' midpoint — the same `pan` / `zoomOnFixedPoint` helpers (and
 * the same Redux view-bounds + selection-box refresh) `ScrollableCanvas`'s
 * wheel handler uses for scroll and ctrl+wheel, so scrollbars, handles and
 * the crosshair all stay in step.
 * @param {object} store the paint editor's Redux store
 * @param {number} scaleDelta zoom multiplier since the last step (>1 = in)
 * @param {number} clientX the fingers' midpoint, client px
 * @param {number} clientY the fingers' midpoint, client px
 * @param {number} dx how far the midpoint moved since the last step, client px
 * @param {number} dy how far the midpoint moved since the last step, client px
 */
const pinchView = function (store, scaleDelta, clientX, clientY, dx, dy) {
    const view = paper.view;
    if (!view || !view.element) return;
    if (dx !== 0 || dy !== 0) pan(-dx / view.zoom, -dy / view.zoom);
    if (scaleDelta !== 1) {
        const rect = view.element.getBoundingClientRect();
        const fixedPoint = view.viewToProject(new paper.Point(clientX - rect.left, clientY - rect.top));
        const target = Math.min(MAX_PINCH_ZOOM, view.zoom * scaleDelta);
        // `zoomOnFixedPoint` takes an additive delta (and clamps the
        // outermost level itself).
        zoomOnFixedPoint(target - view.zoom, fixedPoint);
    }
    store.dispatch(updateViewBounds(view.matrix));
    store.dispatch(redrawSelectionBox());
};

/**
 * Paint's own undo / redo, exactly as Ctrl+Z / Ctrl+Shift+Z reach it:
 * `KeyboardShortcutsHOC` listens for `keydown` on `document`, and its
 * `onUndo`/`onRedo` are `UndoHOC`'s (which need the editor's internal
 * `onUpdateImage`, so they can't be called from out here directly). The
 * event is dispatched *non-bubbling* at `document`, so it never reaches the
 * app's own window-level Ctrl+Z listener (which would undo the scene
 * document instead).
 * @param {boolean} redo true for redo
 */
const paintHistoryStep = function (redo) {
    document.dispatchEvent(new KeyboardEvent('keydown', {
        key: redo ? 'Z' : 'z',
        ctrlKey: true,
        shiftKey: redo,
        bubbles: false,
        cancelable: true
    }));
};

/**
 * Ends paper.js's in-progress tool drag (a brush stroke, a shape, a select
 * drag) at `clientX, clientY`, as if the finger lifted there. paper.js on
 * Chromium tracks touches through `touch*` events on `document` (not
 * pointer events), so this is a synthetic `touchcancel` — paper treats it
 * as its mouseup. A plain `Event` carrying `changedTouches` (paper's
 * `DomEvent.getPoint` only reads `targetTouches` / `changedTouches[0]`)
 * rather than a real `TouchEvent`, whose `Touch` constructor Safari lacks.
 * @param {number} clientX where the stroke ends, client px
 * @param {number} clientY where the stroke ends, client px
 */
const endPaperDrag = function (clientX, clientY) {
    const event = new Event('touchcancel', {bubbles: true, cancelable: true});
    const point = {
        clientX,
        clientY,
        pageX: clientX + window.scrollX,
        pageY: clientY + window.scrollY
    };
    Object.defineProperty(event, 'touches', {value: []});
    Object.defineProperty(event, 'targetTouches', {value: []});
    Object.defineProperty(event, 'changedTouches', {value: [point]});
    document.dispatchEvent(event);
};

export {currentColors, setPaintColor, startEyeDropper, pinchView, paintHistoryStep, endPaperDrag};
