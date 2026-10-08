// an1mbyt3: small entry points into scratch-paint's own Redux state, for
// Paint's extra UI (the color palette and the eyedropper shortcut).
import paper from '@scratch/paper';
import {changeFillColor} from '../reducers/fill-style';
import {changeStrokeColor} from '../reducers/stroke-style';
import {activateEyeDropper} from '../reducers/eye-dropper';

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

export {currentColors, setPaintColor, startEyeDropper};
