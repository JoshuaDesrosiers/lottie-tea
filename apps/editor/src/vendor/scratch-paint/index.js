import PaintEditor from './containers/paint-editor.jsx';
import ScratchPaintReducer from './reducers/scratch-paint-reducer';
import {setOnionSkin, setOverSkin} from './helper/layer';
import {brushSettings, setBrushSettings, onBrushSettings} from './helper/an1-brush';

export {
    PaintEditor as default,
    ScratchPaintReducer,
    setOnionSkin,
    setOverSkin,
    brushSettings,
    setBrushSettings,
    onBrushSettings
};
