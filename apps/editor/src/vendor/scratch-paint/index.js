import PaintEditor from './containers/paint-editor.jsx';
import ScratchPaintReducer from './reducers/scratch-paint-reducer';
import {setOnionSkin, setOverSkin, setPickSource} from './helper/layer';
import {brushSettings, setBrushSettings, onBrushSettings} from './helper/an1-brush';
import {currentColors, setPaintColor, startEyeDropper, pinchView, paintHistoryStep, endPaperDrag} from './helper/an1-actions';

export {
    PaintEditor as default,
    ScratchPaintReducer,
    setOnionSkin,
    setOverSkin,
    setPickSource,
    brushSettings,
    setBrushSettings,
    onBrushSettings,
    currentColors,
    setPaintColor,
    startEyeDropper,
    pinchView,
    paintHistoryStep,
    endPaperDrag
};
