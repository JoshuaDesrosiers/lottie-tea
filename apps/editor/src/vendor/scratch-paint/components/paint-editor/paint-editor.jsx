import paper from '@scratch/paper';
import React from 'react';
import {createPortal} from 'react-dom';
import PropTypes from 'prop-types';

import PaperCanvas from '../../containers/paper-canvas.jsx';
import ScrollableCanvas from '../../containers/scrollable-canvas.jsx';

import BitBrushMode from '../../containers/bit-brush-mode.jsx';
import BitLineMode from '../../containers/bit-line-mode.jsx';
import BitOvalMode from '../../containers/bit-oval-mode.jsx';
import BitRectMode from '../../containers/bit-rect-mode.jsx';
import BitFillMode from '../../containers/bit-fill-mode.jsx';
import BitEraserMode from '../../containers/bit-eraser-mode.jsx';
import BitSelectMode from '../../containers/bit-select-mode.jsx';
import Box from '../box/box.jsx';
import BrushMode from '../../containers/brush-mode.jsx';
import EraserMode from '../../containers/eraser-mode.jsx';
import FillColorIndicatorComponent from '../../containers/fill-color-indicator.jsx';
import FillMode from '../../containers/fill-mode.jsx';
import FixedToolsContainer from '../../containers/fixed-tools.jsx';
import LineMode from '../../containers/line-mode.jsx';
import Loupe from '../loupe/loupe.jsx';
import ModeToolsContainer from '../../containers/mode-tools.jsx';
import OvalMode from '../../containers/oval-mode.jsx';
import RectMode from '../../containers/rect-mode.jsx';
import ReshapeMode from '../../containers/reshape-mode.jsx';
import SelectMode from '../../containers/select-mode.jsx';
import StrokeColorIndicatorComponent from '../../containers/stroke-color-indicator.jsx';
import StrokeWidthIndicatorComponent from '../../containers/stroke-width-indicator.jsx';
import TextMode from '../../containers/text-mode.jsx';

import Formats from '../../lib/format';
import styles from './paint-editor.module.css';

/**
 * an1mbyt3's own request: "the only thing in that main area should be the
 * canvas" (the actual drawing surface, not scratch-paint's own cramped
 * multi-row inline chrome) plus "we should use our layout... show buttons
 * with our styling." Two different fixes for two different kinds of
 * former-chrome here:
 *
 * - The individual tool-*mode* containers (`SelectMode`/`BrushMode`/
 *   `EraserMode`/etc., and their Bit* bitmap-mode equivalents) are still
 *   rendered here, just invisibly (`className="hidden"`, plain CSS
 *   `display: none`) — they are *not* redundant chrome: each one's
 *   `componentDidMount`/`componentWillReceiveProps` is what actually
 *   instantiates and activates/deactivates the real paper.js `Tool` (mouse
 *   handling) for its mode, reacting to `state.scratchPaint.mode` regardless
 *   of *what* changed it. `PaintToolPalette.jsx` (an1mbyt3's own component,
 *   mounted separately by `PaintAssetEditor.tsx`, floating over the canvas)
 *   dispatches `changeMode(...)` directly to the same store — these hidden
 *   containers pick that up and do the actual tool-switching work exactly as
 *   before; only their own visible button markup (which nothing here still
 *   renders) is gone. Removing them from the tree entirely would silently
 *   break every tool (nothing would ever activate a paper.js Tool for the
 *   newly-selected mode).
 * - `FixedToolsContainer`/`ModeToolsContainer` (costume name/undo-redo/
 *   group-ungroup/z-order, and the current tool's contextual properties —
 *   Copy/Paste/Delete/Flip for Select, brush size for Brush, etc.) *are*
 *   genuinely redundant as originally laid out (crammed into multiple rows
 *   above the tool-mode strip) — these are still rendered here (their real
 *   Group/Flip/Copy/etc. logic needs to stay wired to `onUpdateImage`,
 *   which only reaches this deep via prop-threading from the outer
 *   `UpdateImageHOC`-wrapped container, not reachable from a sibling outside
 *   this tree), but their own *presentational* files
 *   (`components/fixed-tools/fixed-tools.jsx`,
 *   `components/mode-tools/mode-tools.jsx`) were rewritten with plain
 *   Tailwind buttons/inputs instead of scratch-paint's own styled
 *   components — and, rather than rendering inline above the canvas here
 *   (which sat behind an1mbyt3's own title bar/mode-tabs — a real bug a
 *   screenshot caught, some buttons unclickable), they're portaled
 *   (`createPortal`, `props.actionBarTarget`) out to a `<div>`
 *   `PaintAssetEditor.tsx` owns and positions clear of that chrome. Still
 *   logically nested exactly here (so `onUpdateImage`/Redux/context all
 *   keep flowing correctly) — only their rendered DOM output moves.
 */
const PaintEditorComponent = props => {
    const actionBar = (
        // No border/background/padding of its own — it portals into
        // `PaintAssetEditor.tsx`'s own already-bordered row (combined with
        // that row's Cancel/Save as Resource buttons into one strip, not
        // two stacked ones), so styling it here too would nest a box
        // inside a box.
        <div className="flex flex-1 items-center gap-3">
            <FixedToolsContainer
                canRedo={props.canRedo}
                canUndo={props.canUndo}
                name={props.name}
                onRedo={props.onRedo}
                onUndo={props.onUndo}
                onUpdateImage={props.onUpdateImage}
                onUpdateName={props.onUpdateName}
            />
            <div className="h-5 w-px bg-panel-border" />
            <FillColorIndicatorComponent onUpdateImage={props.onUpdateImage} />
            <StrokeColorIndicatorComponent onUpdateImage={props.onUpdateImage} />
            <StrokeWidthIndicatorComponent onUpdateImage={props.onUpdateImage} />
            <div className="h-5 w-px bg-panel-border" />
            <ModeToolsContainer onUpdateImage={props.onUpdateImage} />
        </div>
    );

    return (
        <div
            className={styles.editorContainer}
            dir={props.rtl ? 'rtl' : 'ltr'}
        >
            {props.canvas !== null && props.actionBarTarget ? createPortal(actionBar, props.actionBarTarget) : null}

            {/* Rendered *before* `.controlsContainer` (`PaperCanvas`)
                deliberately, not just for tidiness — React unmounts
                same-level siblings in JSX declaration order, and each of
                these containers' own `componentWillUnmount` calls into
                paper.js tool cleanup (`this.tool.deactivateTool()`) that
                needs `paper.project` to still exist. `PaperCanvas`'s own
                unmount calls `paper.remove()`, nulling that out — a real,
                hand-verified crash when it ran *first* (`SelectMode`'s
                cleanup then threw reading `.layers` off a null `project`,
                escaping every error boundary and blanking the window).
                Listing these first means they tear down while `paper` is
                still alive; `PaperCanvas`'s own teardown happens after. */}
            {props.canvas !== null ? (
                <div className="hidden" aria-hidden="true">
                    <SelectMode onUpdateImage={props.onUpdateImage} />
                    <ReshapeMode onUpdateImage={props.onUpdateImage} />
                    <BrushMode onUpdateImage={props.onUpdateImage} />
                    <EraserMode onUpdateImage={props.onUpdateImage} />
                    <FillMode onUpdateImage={props.onUpdateImage} />
                    <TextMode textArea={props.textArea} onUpdateImage={props.onUpdateImage} />
                    <LineMode onUpdateImage={props.onUpdateImage} />
                    <OvalMode onUpdateImage={props.onUpdateImage} />
                    <RectMode onUpdateImage={props.onUpdateImage} />
                    <BitBrushMode onUpdateImage={props.onUpdateImage} />
                    <BitLineMode onUpdateImage={props.onUpdateImage} />
                    <BitOvalMode onUpdateImage={props.onUpdateImage} />
                    <BitRectMode onUpdateImage={props.onUpdateImage} />
                    <TextMode isBitmap textArea={props.textArea} onUpdateImage={props.onUpdateImage} />
                    <BitFillMode onUpdateImage={props.onUpdateImage} />
                    <BitEraserMode onUpdateImage={props.onUpdateImage} />
                    <BitSelectMode onUpdateImage={props.onUpdateImage} />
                </div>
            ) : null}

            <div className={styles.controlsContainer}>
                {/* Canvas */}
                <ScrollableCanvas
                    canvas={props.canvas}
                    hideScrollbars={props.isEyeDropping}
                    style={styles.canvasContainer}
                >
                    <PaperCanvas
                        canvasRef={props.setCanvas}
                        image={props.image}
                        imageFormat={props.imageFormat}
                        imageId={props.imageId}
                        rotationCenterX={props.rotationCenterX}
                        rotationCenterY={props.rotationCenterY}
                        zoomLevelId={props.zoomLevelId}
                        onUpdateImage={props.onUpdateImage}
                    />
                    <textarea
                        className={styles.textArea}
                        ref={props.setTextArea}
                        spellCheck={false}
                    />
                    {props.isEyeDropping &&
                        props.colorInfo !== null &&
                        !props.colorInfo.hideLoupe ? (
                            <Box className={styles.colorPickerWrapper}>
                                <Loupe
                                    colorInfo={props.colorInfo}
                                    pixelRatio={paper.project.view.pixelRatio}
                                />
                            </Box>
                        ) : null
                    }
                </ScrollableCanvas>
            </div>
        </div>
    );
};

PaintEditorComponent.propTypes = {
    canRedo: PropTypes.func.isRequired,
    canUndo: PropTypes.func.isRequired,
    canvas: PropTypes.instanceOf(Element),
    colorInfo: Loupe.propTypes.colorInfo,
    format: PropTypes.oneOf(Object.keys(Formats)),
    image: PropTypes.oneOfType([
        PropTypes.string,
        PropTypes.instanceOf(HTMLImageElement)
    ]),
    imageFormat: PropTypes.string,
    imageId: PropTypes.string,
    isEyeDropping: PropTypes.bool,
    name: PropTypes.string,
    onRedo: PropTypes.func.isRequired,
    onSwitchToBitmap: PropTypes.func.isRequired,
    onSwitchToVector: PropTypes.func.isRequired,
    onUndo: PropTypes.func.isRequired,
    onUpdateImage: PropTypes.func.isRequired,
    onUpdateName: PropTypes.func.isRequired,
    onZoomIn: PropTypes.func.isRequired,
    onZoomOut: PropTypes.func.isRequired,
    onZoomReset: PropTypes.func.isRequired,
    rotationCenterX: PropTypes.number,
    rotationCenterY: PropTypes.number,
    rtl: PropTypes.bool,
    setCanvas: PropTypes.func.isRequired,
    setTextArea: PropTypes.func.isRequired,
    textArea: PropTypes.instanceOf(Element),
    zoomLevelId: PropTypes.string
};

export default PaintEditorComponent;
