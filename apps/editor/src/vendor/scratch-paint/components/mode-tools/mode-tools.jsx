import {connect} from 'react-redux';
import PropTypes from 'prop-types';
import React from 'react';

import {changeBrushSize} from '../../reducers/brush-mode';
import {changeBrushSize as changeEraserSize} from '../../reducers/eraser-mode';
import {changeBitBrushSize} from '../../reducers/bit-brush-size';
import {changeBitEraserSize} from '../../reducers/bit-eraser-size';
import {setShapesFilled} from '../../reducers/fill-bitmap-shapes';

import FontDropdown from '../../containers/font-dropdown.jsx';
import Modes from '../../lib/modes';
import Formats, {isBitmap, isVector} from '../../lib/format';

import copyIcon from './icons/copy.svg';
import pasteIcon from './icons/paste.svg';
import deleteIcon from './icons/delete.svg';

import bitBrushIcon from '../bit-brush-mode/brush.svg';
import bitEraserIcon from '../bit-eraser-mode/eraser.svg';
import bitLineIcon from '../bit-line-mode/line.svg';
import brushIcon from '../brush-mode/brush.svg';
import curvedPointIcon from './icons/curved-point.svg';
import eraserIcon from '../eraser-mode/eraser.svg';
import flipHorizontalIcon from './icons/flip-horizontal.svg';
import flipVerticalIcon from './icons/flip-vertical.svg';
import straightPointIcon from './icons/straight-point.svg';
import bitOvalIcon from '../bit-oval-mode/oval.svg';
import bitRectIcon from '../bit-rect-mode/rectangle.svg';
import bitOvalOutlinedIcon from '../bit-oval-mode/oval-outlined.svg';
import bitRectOutlinedIcon from '../bit-rect-mode/rectangle-outlined.svg';

import {MAX_STROKE_WIDTH} from '../../reducers/stroke-width';

/** an1mbyt3's own restyled per-mode properties bar — see
 * `components/fixed-tools/fixed-tools.jsx`'s own doc comment for why this
 * exists (plain Tailwind buttons/inputs instead of scratch-paint's own
 * styled components) — same deal here: every branch below renders the
 * exact same props/callbacks the original did, `containers/mode-tools.jsx`
 * (the real Copy/Paste/Delete/Flip/curve-toggle logic) is untouched. */
function IconBtn({ icon, label, onClick, disabled, highlighted }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={label}
      className={`flex h-8 w-8 items-center justify-center rounded-md transition-colors ${
        disabled ? 'cursor-default opacity-30' : highlighted ? 'bg-accent/20' : 'hover:bg-overlay/10'
      }`}
    >
      <img src={icon} draggable={false} className="h-4 w-4" style={{ filter: 'brightness(0) invert(var(--icon-invert))' }} />
    </button>
  );
}

function Slider({ icon, label, value, max, onChange }) {
  return (
    <div className="flex items-center gap-2 px-1" title={label}>
      <img src={icon} draggable={false} className="h-4 w-4" style={{ filter: 'brightness(0) invert(var(--icon-invert))' }} />
      <input
        type="range"
        min={1}
        max={max}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-24 accent-accent"
      />
      <span className="w-6 text-right text-xs tabular-nums text-ink-dim">{value}</span>
    </div>
  );
}

const ModeToolsComponent = props => {
    switch (props.mode) {
    case Modes.BRUSH:
        /* falls through */
    case Modes.BIT_BRUSH:
        /* falls through */
    case Modes.BIT_LINE:
    {
        const currentIcon = isVector(props.format) ? brushIcon :
            props.mode === Modes.BIT_LINE ? bitLineIcon : bitBrushIcon;
        const currentBrushValue = isBitmap(props.format) ? props.bitBrushSize : props.brushValue;
        const changeFunction = isBitmap(props.format) ? props.onBitBrushSliderChange : props.onBrushSliderChange;
        return (
            <Slider
                icon={currentIcon}
                label={props.mode === Modes.BIT_LINE ? 'Thickness' : 'Size'}
                value={currentBrushValue}
                max={MAX_STROKE_WIDTH}
                onChange={changeFunction}
            />
        );
    }
    case Modes.BIT_ERASER:
        /* falls through */
    case Modes.ERASER:
    {
        const currentIcon = isVector(props.format) ? eraserIcon : bitEraserIcon;
        const currentEraserValue = isBitmap(props.format) ? props.bitEraserSize : props.eraserValue;
        const changeFunction = isBitmap(props.format) ? props.onBitEraserSliderChange : props.onEraserSliderChange;
        return (
            <Slider
                icon={currentIcon}
                label="Eraser size"
                value={currentEraserValue}
                max={MAX_STROKE_WIDTH}
                onChange={changeFunction}
            />
        );
    }
    case Modes.RESHAPE:
        return (
            <div className="flex items-center gap-1">
                <IconBtn icon={curvedPointIcon} label="Curved" onClick={props.onCurvePoints} disabled={!props.hasSelectedUncurvedPoints} />
                <IconBtn icon={straightPointIcon} label="Pointed" onClick={props.onPointPoints} disabled={!props.hasSelectedUnpointedPoints} />
                <div className="mx-1 h-5 w-px bg-panel-border" />
                <IconBtn icon={deleteIcon} label="Delete" onClick={props.onDelete} />
            </div>
        );
    case Modes.BIT_SELECT:
        /* falls through */
    case Modes.SELECT:
        return (
            <div className="flex items-center gap-1">
                <IconBtn icon={copyIcon} label="Copy" onClick={props.onCopyToClipboard} />
                <IconBtn icon={pasteIcon} label="Paste" onClick={props.onPasteFromClipboard} disabled={!(props.clipboardItems.length > 0)} />
                <div className="mx-1 h-5 w-px bg-panel-border" />
                <IconBtn icon={deleteIcon} label="Delete" onClick={props.onDelete} />
                <div className="mx-1 h-5 w-px bg-panel-border" />
                <IconBtn icon={flipHorizontalIcon} label="Flip Horizontal" onClick={props.onFlipHorizontal} />
                <IconBtn icon={flipVerticalIcon} label="Flip Vertical" onClick={props.onFlipVertical} />
            </div>
        );
    case Modes.BIT_TEXT:
        /* falls through */
    case Modes.TEXT:
        return (
            <div className="flex items-center">
                <FontDropdown onUpdateImage={props.onUpdateImage} />
            </div>
        );
    case Modes.BIT_RECT:
        /* falls through */
    case Modes.BIT_OVAL:
    {
        const fillIcon = props.mode === Modes.BIT_RECT ? bitRectIcon : bitOvalIcon;
        const outlineIcon = props.mode === Modes.BIT_RECT ? bitRectOutlinedIcon : bitOvalOutlinedIcon;
        return (
            <div className="flex items-center gap-1">
                <IconBtn icon={fillIcon} label="Filled" onClick={props.onFillShapes} highlighted={props.fillBitmapShapes} />
                <IconBtn icon={outlineIcon} label="Outlined" onClick={props.onOutlineShapes} highlighted={!props.fillBitmapShapes} />
                {props.fillBitmapShapes ? null : (
                    <Slider icon={bitBrushIcon} label="Thickness" value={props.bitBrushSize} max={MAX_STROKE_WIDTH} onChange={props.onBitBrushSliderChange} />
                )}
            </div>
        );
    }
    default:
        // Leave empty for now, if mode not supported
        return null;
    }
};

ModeToolsComponent.propTypes = {
    bitBrushSize: PropTypes.number,
    bitEraserSize: PropTypes.number,
    brushValue: PropTypes.number,
    clipboardItems: PropTypes.arrayOf(PropTypes.array),
    eraserValue: PropTypes.number,
    fillBitmapShapes: PropTypes.bool,
    format: PropTypes.oneOf(Object.keys(Formats)),
    hasSelectedUncurvedPoints: PropTypes.bool,
    hasSelectedUnpointedPoints: PropTypes.bool,
    mode: PropTypes.string.isRequired,
    onBitBrushSliderChange: PropTypes.func.isRequired,
    onBitEraserSliderChange: PropTypes.func.isRequired,
    onBrushSliderChange: PropTypes.func.isRequired,
    onCopyToClipboard: PropTypes.func.isRequired,
    onCurvePoints: PropTypes.func.isRequired,
    onDelete: PropTypes.func.isRequired,
    onEraserSliderChange: PropTypes.func,
    onFillShapes: PropTypes.func.isRequired,
    onFlipHorizontal: PropTypes.func.isRequired,
    onFlipVertical: PropTypes.func.isRequired,
    onOutlineShapes: PropTypes.func.isRequired,
    onPasteFromClipboard: PropTypes.func.isRequired,
    onPointPoints: PropTypes.func.isRequired,
    onUpdateImage: PropTypes.func.isRequired
};

const mapStateToProps = state => ({
    mode: state.scratchPaint.mode,
    format: state.scratchPaint.format,
    fillBitmapShapes: state.scratchPaint.fillBitmapShapes,
    bitBrushSize: state.scratchPaint.bitBrushSize,
    bitEraserSize: state.scratchPaint.bitEraserSize,
    brushValue: state.scratchPaint.brushMode.brushSize,
    clipboardItems: state.scratchPaint.clipboard.items,
    eraserValue: state.scratchPaint.eraserMode.brushSize
});
const mapDispatchToProps = dispatch => ({
    onBrushSliderChange: brushSize => {
        dispatch(changeBrushSize(brushSize));
    },
    onBitBrushSliderChange: bitBrushSize => {
        dispatch(changeBitBrushSize(bitBrushSize));
    },
    onBitEraserSliderChange: eraserSize => {
        dispatch(changeBitEraserSize(eraserSize));
    },
    onEraserSliderChange: eraserSize => {
        dispatch(changeEraserSize(eraserSize));
    },
    onFillShapes: () => {
        dispatch(setShapesFilled(true));
    },
    onOutlineShapes: () => {
        dispatch(setShapesFilled(false));
    }
});

export default connect(
    mapStateToProps,
    mapDispatchToProps
)(ModeToolsComponent);
