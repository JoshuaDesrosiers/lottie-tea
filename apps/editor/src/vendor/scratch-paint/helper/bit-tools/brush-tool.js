import paper from '@scratch/paper';
import {getRaster, getGuideLayer} from '../layer';
import {forEachLinePoint, getBrushMark} from '../bitmap';
import {brushSettings, currentPressure, getPresetMark} from '../an1-brush';

/**
 * Tool for drawing with the bitmap brush and eraser
 */
class BrushTool extends paper.Tool {
    /**
     * @param {!Function} onUpdateImage A callback to call when the image visibly changes
     * @param {boolean} isEraser True if brush should erase
     */
    constructor (onUpdateImage, isEraser) {
        super();
        this.onUpdateImage = onUpdateImage;
        this.isEraser = isEraser;

        // We have to set these functions instead of just declaring them because
        // paper.js tools hook up the listeners in the setter functions.
        this.onMouseMove = this.handleMouseMove;
        this.onMouseDown = this.handleMouseDown;
        this.onMouseDrag = this.handleMouseDrag;
        this.onMouseUp = this.handleMouseUp;

        this.colorState = null;
        this.active = false;
        this.lastPoint = null;
        this.cursorPreview = null;
    }
    setColor (color) {
        this.color = color;
        this.tmpCanvas = getBrushMark(this.size, this.color, this.isEraser || !this.color);
    }
    setBrushSize (size) {
        // For performance, make sure this is an integer
        this.size = Math.max(1, ~~size);
        this.tmpCanvas = getBrushMark(this.size, this.color, this.isEraser || !this.color);
    }
    // Draw a brush mark at the given point.
    // an1mbyt3: the stamp follows the brush preset and pen pressure
    // (`an1-brush.js`); soft / spray presets stamp with spacing so strokes
    // build up evenly.
    draw (x, y) {
        const erasing = this.isEraser || !this.color;
        const preset = brushSettings.preset;
        const size = Math.max(1, Math.round(this.size * currentPressure()));
        if (preset === 'soft' || preset === 'spray') {
            const spacing = Math.max(1, size * (preset === 'soft' ? 0.18 : 0.35));
            if (this.lastStamp && Math.hypot(x - this.lastStamp.x, y - this.lastStamp.y) < spacing) return;
            this.lastStamp = {x, y};
        }
        const roundedUpRadius = Math.ceil(size / 2);
        const context = getRaster().getContext('2d');
        if (erasing) {
            context.globalCompositeOperation = 'destination-out';
        }
        let mark;
        if (preset === 'spray') {
            mark = document.createElement('canvas');
            mark.width = roundedUpRadius * 2;
            mark.height = roundedUpRadius * 2;
            const dots = mark.getContext('2d');
            dots.fillStyle = erasing ? 'white' : this.color;
            const count = Math.max(2, Math.round(size * size / 30));
            for (let i = 0; i < count; i++) {
                const angle = Math.random() * Math.PI * 2;
                const distance = Math.sqrt(Math.random()) * size / 2;
                dots.fillRect(~~(roundedUpRadius + Math.cos(angle) * distance), ~~(roundedUpRadius + Math.sin(angle) * distance), 1, 1);
            }
        } else if (preset === 'round') {
            mark = size === this.size ? this.tmpCanvas : getBrushMark(size, this.color, erasing);
        } else {
            mark = getPresetMark(preset, size, erasing ? 'white' : this.color, getBrushMark);
        }
        getRaster().drawImage(mark, new paper.Point(~~x - roundedUpRadius, ~~y - roundedUpRadius));
        if (erasing) {
            context.globalCompositeOperation = 'source-over';
        }
    }
    // an1mbyt3: the stabilizer — the brush tip trails the cursor by
    // `brushSettings.smoothing` (0 = exactly on it).
    stabilized (point) {
        const smoothing = Math.min(0.95, Math.max(0, brushSettings.smoothing || 0));
        if (!this.smoothed || smoothing === 0) {
            this.smoothed = point;
            return point;
        }
        this.smoothed = this.smoothed.add(point.subtract(this.smoothed).multiply(1 - smoothing));
        return this.smoothed;
    }
    updateCursorIfNeeded () {
        if (!this.size) {
            return;
        }

        // The cursor preview was unattached from the view by an outside process,
        // such as changing costumes or undo.
        if (this.cursorPreview && !this.cursorPreview.parent) {
            this.cursorPreview = null;
        }

        if (!this.cursorPreview || !(this.lastSize === this.size && this.lastColor === this.color)) {
            if (this.cursorPreview) {
                this.cursorPreview.remove();
            }

            this.tmpCanvas = getBrushMark(this.size, this.color, this.isEraser || !this.color);
            this.cursorPreview = new paper.Raster(this.tmpCanvas);
            this.cursorPreview.guide = true;
            this.cursorPreview.parent = getGuideLayer();
            this.cursorPreview.data.isHelperItem = true;
        }

        this.lastSize = this.size;
        this.lastColor = this.color;
    }
    handleMouseMove (event) {
        this.updateCursorIfNeeded();
        this.cursorPreview.position = new paper.Point(~~event.point.x, ~~event.point.y);
    }
    handleMouseDown (event) {
        if (event.event.button > 0) return; // only first mouse button
        this.active = true;

        if (this.cursorPreview) {
            this.cursorPreview.remove();
        }

        this.smoothed = event.point;
        this.lastStamp = null;
        this.draw(event.point.x, event.point.y);
        this.lastPoint = event.point;
    }
    handleMouseDrag (event) {
        if (event.event.button > 0 || !this.active) return; // only first mouse button

        const point = this.stabilized(event.point);
        forEachLinePoint(this.lastPoint, point, this.draw.bind(this));
        this.lastPoint = point;
    }
    handleMouseUp (event) {
        if (event.event.button > 0 || !this.active) return; // only first mouse button

        // Catch the stabilized tip up with where the stroke ended.
        forEachLinePoint(this.lastPoint, event.point, this.draw.bind(this));
        this.onUpdateImage();

        this.lastPoint = null;
        this.smoothed = null;
        this.active = false;

        this.updateCursorIfNeeded();
        this.cursorPreview.position = new paper.Point(~~event.point.x, ~~event.point.y);
    }
    deactivateTool () {
        this.active = false;
        this.tmpCanvas = null;
        if (this.cursorPreview) {
            this.cursorPreview.remove();
            this.cursorPreview = null;
        }
    }
}

export default BrushTool;
