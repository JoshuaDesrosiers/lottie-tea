// an1mbyt3: bitmap brush upgrades — presets, pen pressure, stabilizer.
//
// One shared settings object (the Paint settings bar writes it through
// `setBrushSettings`, the bitmap BrushTool reads it per stamp), plus the live
// pen pressure: paper.js tools see mouse events, so pressure is tracked from
// the window's own pointer events instead.
//
// Presets:
// - round:  scratch-paint's original mark (hard circle; tiny sizes are
//           squares for pixel art).
// - pixel:  always a hard square — crisp pixel art at any size.
// - soft:   an airbrush — a radial falloff, stamped with spacing so strokes
//           build up evenly instead of going solid.
// - spray:  random single-pixel dots inside the brush radius.

const brushSettings = {
    preset: 'round',
    /** 0 = off .. 0.95 = very laggy. How far the brush tip trails the cursor. */
    smoothing: 0,
    /** Pen pressure scales the brush size. */
    pressure: true
};

const listeners = new Set();

const setBrushSettings = function (patch) {
    Object.assign(brushSettings, patch);
    for (const listener of listeners) listener(brushSettings);
};

const onBrushSettings = function (listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
};

// ---- Pen pressure ----
let pressure = 1;
if (typeof window !== 'undefined') {
    const track = e => {
        // A mouse reports 0.5 while pressed — treat it as full size. Pens
        // (and pressure-capable touch screens) report the real value.
        pressure = e.pointerType === 'pen' || (e.pointerType === 'touch' && e.pressure > 0 && e.pressure !== 0.5) ?
            Math.max(0.05, e.pressure) :
            1;
    };
    window.addEventListener('pointerdown', track, {capture: true, passive: true});
    window.addEventListener('pointermove', track, {capture: true, passive: true});
}

/** The size multiplier for the current stamp (0.05..1). */
const currentPressure = function () {
    return brushSettings.pressure ? pressure : 1;
};

// ---- Marks ----
const markCache = new Map();

/**
 * The stamp for `preset` at diameter `size` (≥ 1, integer) in `color`. For the
 * eraser the color only matters for alpha (erasing uses destination-out).
 * @param {string} preset one of round / pixel / soft
 * @param {number} size diameter in pixels
 * @param {string} color css color
 * @param {Function} roundMark scratch-paint's own `getBrushMark`
 * @returns {HTMLCanvasElement} the mark
 */
const getPresetMark = function (preset, size, color, roundMark) {
    const key = `${preset}|${size}|${color}`;
    let mark = markCache.get(key);
    if (mark) return mark;
    if (preset === 'round' || preset === 'spray') {
        mark = roundMark(size, color, false);
    } else {
        const radius = Math.ceil(size / 2);
        mark = document.createElement('canvas');
        mark.width = radius * 2;
        mark.height = radius * 2;
        const context = mark.getContext('2d');
        context.imageSmoothingEnabled = false;
        if (preset === 'pixel') {
            context.fillStyle = color;
            context.fillRect(0, 0, size, size);
        } else {
            // Solid color, then an alpha falloff mask (a color gradient to
            // transparent would fade through black at the edges).
            context.fillStyle = color;
            context.fillRect(0, 0, radius * 2, radius * 2);
            const falloff = context.createRadialGradient(radius, radius, 0, radius, radius, size / 2);
            falloff.addColorStop(0, 'rgba(0,0,0,1)');
            falloff.addColorStop(1, 'rgba(0,0,0,0)');
            context.globalCompositeOperation = 'destination-in';
            context.globalAlpha = 0.35;
            context.fillStyle = falloff;
            context.fillRect(0, 0, radius * 2, radius * 2);
        }
    }
    if (markCache.size > 256) markCache.clear();
    markCache.set(key, mark);
    return mark;
};

export {
    brushSettings,
    setBrushSettings,
    onBrushSettings,
    currentPressure,
    getPresetMark
};
