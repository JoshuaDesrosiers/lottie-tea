import classNames from 'classnames';
import PropTypes from 'prop-types';
import React from 'react';

import Button from '../button/button.jsx';
import Dropdown from '../dropdown/dropdown.jsx';
import InputGroup from '../input-group/input-group.jsx';
import Fonts from '../../lib/fonts';
import styles from './font-dropdown.module.css';
// an1mbyt3: Google Fonts in the text tool (not in upstream scratch-paint).
import {fontCatalog, loadFontPreview, POPULAR_FONTS} from '../../../../game/googleFonts';

/** How many Google families the dropdown lists at once (search for more). */
const GOOGLE_ROWS = 40;

/**
 * an1mbyt3 addition: a searchable list of Google Fonts under the built-in
 * ones. Hover previews the font on the selected text, click keeps it —
 * same as the built-in rows. A family is passed quoted (`"Press Start 2P"`)
 * because paper.js puts it straight into a CSS `font` shorthand.
 */
const GoogleFontsSection = ({onHoverFont, onPickFont}) => {
    const [query, setQuery] = React.useState('');
    const [fonts, setFonts] = React.useState(POPULAR_FONTS);
    React.useEffect(() => {
        fontCatalog().then(setFonts);
    }, []);
    const q = query.trim().toLowerCase();
    const shown = fonts.filter(f => !q || f.family.toLowerCase().includes(q)).slice(0, GOOGLE_ROWS);
    return (
        <div className={styles.googleSection}>
            <div className={styles.googleHeading}>Google Fonts</div>
            <input
                className={styles.googleSearch}
                placeholder="Search fonts…"
                value={query}
                onChange={e => setQuery(e.target.value)}
                onKeyDown={e => e.stopPropagation()}
            />
            <div className={styles.googleList}>
                {shown.map(f => {
                    loadFontPreview(f.family);
                    return (
                        <Button
                            className={classNames(styles.modMenuItem)}
                            key={f.family}
                            onClick={() => onPickFont(`"${f.family}"`)}
                            onMouseOver={() => onHoverFont(`"${f.family}"`)}
                        >
                            <span style={{fontFamily: `"${f.family}"`}}>{f.family}</span>
                        </Button>
                    );
                })}
            </div>
        </div>
    );
};

GoogleFontsSection.propTypes = {
    onPickFont: PropTypes.func.isRequired,
    onHoverFont: PropTypes.func.isRequired
};

const ModeToolsComponent = props => (
    <Dropdown
        className={classNames(styles.modUnselect, styles.fontDropdown)}
        enterExitTransitionDurationMs={60}
        popoverContent={
            <InputGroup className={styles.modContextMenu}>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverSansSerif}
                >
                    <span className={styles.sansSerif}>
                        {props.getFontName(Fonts.SANS_SERIF)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverSerif}
                >
                    <span className={styles.serif}>
                        {props.getFontName(Fonts.SERIF)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverHandwriting}
                >
                    <span className={styles.handwriting}>
                        {props.getFontName(Fonts.HANDWRITING)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverMarker}
                >
                    <span className={styles.marker}>
                        {props.getFontName(Fonts.MARKER)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverCurly}
                >
                    <span className={styles.curly}>
                        {props.getFontName(Fonts.CURLY)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverPixel}
                >
                    <span className={styles.pixel}>
                        {props.getFontName(Fonts.PIXEL)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverChinese}
                >
                    <span className={styles.chinese}>
                        {props.getFontName(Fonts.CHINESE)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverJapanese}
                >
                    <span className={styles.japanese}>
                        {props.getFontName(Fonts.JAPANESE)}
                    </span>
                </Button>
                <Button
                    className={classNames(styles.modMenuItem)}
                    onClick={props.onChoose}
                    onMouseOver={props.onHoverKorean}
                >
                    <span className={styles.korean}>
                        {props.getFontName(Fonts.KOREAN)}
                    </span>
                </Button>
                <GoogleFontsSection
                    onPickFont={props.onPickFont}
                    onHoverFont={props.onHoverFont}
                />
            </InputGroup>
        }
        ref={props.componentRef}
        tipSize={.01}
        onOpen={props.onOpenDropdown}
        onOuterAction={props.onClickOutsideDropdown}
    >
        <span
            className={classNames(props.getFontStyle(props.font), styles.displayedFontName)}
            style={props.getFontStyle(props.font) ? null : {fontFamily: props.font}}
        >
            {props.getFontName(props.font)}
        </span>
    </Dropdown>
);

ModeToolsComponent.propTypes = {
    componentRef: PropTypes.func.isRequired,
    font: PropTypes.string,
    getFontName: PropTypes.func.isRequired,
    getFontStyle: PropTypes.func.isRequired,
    onChoose: PropTypes.func.isRequired,
    onClickOutsideDropdown: PropTypes.func,
    onHoverChinese: PropTypes.func,
    onHoverFont: PropTypes.func,
    onPickFont: PropTypes.func,
    onHoverCurly: PropTypes.func,
    onHoverHandwriting: PropTypes.func,
    onHoverJapanese: PropTypes.func,
    onHoverKorean: PropTypes.func,
    onHoverMarker: PropTypes.func,
    onHoverPixel: PropTypes.func,
    onHoverSansSerif: PropTypes.func,
    onHoverSerif: PropTypes.func,
    onOpenDropdown: PropTypes.func
};
export default ModeToolsComponent;
