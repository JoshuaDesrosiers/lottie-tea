import {connect} from 'react-redux';
import PropTypes from 'prop-types';
import React from 'react';

import {shouldShowGroup, shouldShowUngroup} from '../../helper/group';
import {shouldShowBringForward, shouldShowSendBackward} from '../../helper/order';

import Formats, {isVector} from '../../lib/format';

import groupIcon from './icons/group.svg';
import redoIcon from './icons/redo.svg';
import sendBackIcon from './icons/send-back.svg';
import sendBackwardIcon from './icons/send-backward.svg';
import sendForwardIcon from './icons/send-forward.svg';
import sendFrontIcon from './icons/send-front.svg';
import undoIcon from './icons/undo.svg';
import ungroupIcon from './icons/ungroup.svg';

/** an1mbyt3's own restyled action bar — plain Tailwind-styled buttons
 * (matching `TopToolbar`'s own button conventions) instead of
 * scratch-paint's `Button`/`InputGroup`/`ButtonGroup`/`LabeledIconButton`/
 * `Dropdown`/`MediaQuery` responsive-dual-layout components (all removed
 * from this file — see git history/the original upstream repo for the full
 * version this replaces). Every prop, callback, and the underlying
 * `containers/fixed-tools.jsx` connected wrapper (real Group/Ungroup/
 * Z-order/Undo-Redo logic — this file only ever rendered its buttons) is
 * completely unchanged; only this leaf markup was rewritten. */
function IconButton({ icon, label, onClick, disabled }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={label}
      className={`flex h-8 w-8 items-center justify-center rounded-md transition-colors ${
        disabled ? 'cursor-default opacity-30' : 'hover:bg-overlay/10'
      }`}
    >
      <img src={icon} draggable={false} className="h-4 w-4" style={{ filter: 'brightness(0) invert(var(--icon-invert))' }} />
    </button>
  );
}

const FixedToolsComponent = props => {
    const redoDisabled = !props.canRedo();
    const undoDisabled = !props.canUndo();

    return (
        <div className="flex items-center gap-2">
            <input
                value={props.name}
                onChange={(e) => props.onUpdateName(e.target.value)}
                className="w-40 rounded border border-panel-border bg-overlay/5 px-2 py-1 text-sm text-ink outline-none focus:border-accent/60"
                placeholder="Costume name"
            />
            <div className="mx-1 h-5 w-px bg-panel-border" />
            <IconButton icon={undoIcon} label="Undo" onClick={props.onUndo} disabled={undoDisabled} />
            <IconButton icon={redoIcon} label="Redo" onClick={props.onRedo} disabled={redoDisabled} />
            {isVector(props.format) ? (
                <>
                    <div className="mx-1 h-5 w-px bg-panel-border" />
                    <IconButton icon={groupIcon} label="Group" onClick={props.onGroup} disabled={!shouldShowGroup()} />
                    <IconButton icon={ungroupIcon} label="Ungroup" onClick={props.onUngroup} disabled={!shouldShowUngroup()} />
                    <div className="mx-1 h-5 w-px bg-panel-border" />
                    <IconButton icon={sendForwardIcon} label="Forward" onClick={props.onSendForward} disabled={!shouldShowBringForward()} />
                    <IconButton icon={sendBackwardIcon} label="Backward" onClick={props.onSendBackward} disabled={!shouldShowSendBackward()} />
                    <IconButton icon={sendFrontIcon} label="Front" onClick={props.onSendToFront} disabled={!shouldShowBringForward()} />
                    <IconButton icon={sendBackIcon} label="Back" onClick={props.onSendToBack} disabled={!shouldShowSendBackward()} />
                </>
            ) : null}
        </div>
    );
};

FixedToolsComponent.propTypes = {
    canRedo: PropTypes.func.isRequired,
    canUndo: PropTypes.func.isRequired,
    format: PropTypes.oneOf(Object.keys(Formats)),
    name: PropTypes.string,
    onGroup: PropTypes.func.isRequired,
    onRedo: PropTypes.func.isRequired,
    onSendBackward: PropTypes.func.isRequired,
    onSendForward: PropTypes.func.isRequired,
    onSendToBack: PropTypes.func.isRequired,
    onSendToFront: PropTypes.func.isRequired,
    onUndo: PropTypes.func.isRequired,
    onUngroup: PropTypes.func.isRequired,
    onUpdateName: PropTypes.func.isRequired
};

const mapStateToProps = state => ({
    format: state.scratchPaint.format,
    selectedItems: state.scratchPaint.selectedItems,
    undoState: state.scratchPaint.undo
});

export default connect(
    mapStateToProps
)(FixedToolsComponent);
