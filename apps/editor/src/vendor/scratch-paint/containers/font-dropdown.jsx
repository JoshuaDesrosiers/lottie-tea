import paper from '@scratch/paper';
import {connect} from 'react-redux';
import bindAll from 'lodash.bindall';
import PropTypes from 'prop-types';
import React from 'react';

import FontDropdownComponent from '../components/font-dropdown/font-dropdown.jsx';
import Fonts from '../lib/fonts';
import {ensureFontReady} from '../../../game/googleFonts';
import {changeFont} from '../reducers/font';
import {getSelectedLeafItems} from '../helper/selection';
import styles from '../components/font-dropdown/font-dropdown.module.css';

class FontDropdown extends React.Component {
    constructor (props) {
        super(props);
        bindAll(this, [
            'getFontStyle',
            'getFontName',
            'handleChangeFontSerif',
            'handleChangeFontSansSerif',
            'handleChangeFontHandwriting',
            'handleChangeFontMarker',
            'handleChangeFontCurly',
            'handleChangeFontPixel',
            'handleChangeFontChinese',
            'handleChangeFontJapanese',
            'handleChangeFontKorean',
            'handleChangeFontGoogle',
            'handlePickFontGoogle',
            'handleOpenDropdown',
            'handleClickOutsideDropdown',
            'setDropdown',
            'handleChoose'
        ]);
    }
    getFontStyle (font) {
        switch (font) {
        case Fonts.SERIF:
            return styles.serif;
        case Fonts.SANS_SERIF:
            return styles.sansSerif;
        case Fonts.HANDWRITING:
            return styles.handwriting;
        case Fonts.MARKER:
            return styles.marker;
        case Fonts.CURLY:
            return styles.curly;
        case Fonts.PIXEL:
            return styles.pixel;
        case Fonts.CHINESE:
            return styles.chinese;
        case Fonts.JAPANESE:
            return styles.japanese;
        case Fonts.KOREAN:
            return styles.korean;
        default:
            return '';
        }
    }
    getFontName (font) {
        switch (font) {
        case Fonts.CHINESE:
            return '中文';
        case Fonts.KOREAN:
            return '한국어';
        case Fonts.JAPANESE:
            return '日本語';
        default:
            // A Google family is stored quoted for paper.js.
            return typeof font === 'string' ? font.replace(/^"|"$/g, '') : font;
        }
    }
    handleChangeFontSansSerif () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.SANS_SERIF);
        }
    }
    handleChangeFontSerif () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.SERIF);
        }
    }
    handleChangeFontHandwriting () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.HANDWRITING);
        }
    }
    handleChangeFontMarker () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.MARKER);
        }
    }
    handleChangeFontCurly () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.CURLY);
        }
    }
    handleChangeFontPixel () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.PIXEL);
        }
    }
    handleChangeFontChinese () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.CHINESE);
        }
    }
    handleChangeFontJapanese () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.JAPANESE);
        }
    }
    handleChangeFontKorean () {
        if (this.dropDown.isOpen()) {
            this.props.changeFont(Fonts.KOREAN);
        }
    }
    /** an1mbyt3: a Google Font (quoted family). The stylesheet is loaded
     * first so paper.js measures and draws the real face, not a fallback. */
    handleChangeFontGoogle (font) {
        if (!this.dropDown.isOpen()) return;
        this.hoveredGoogle = font;
        ensureFontReady(font.replace(/^"|"$/g, '')).then(() => {
            if (this.hoveredGoogle === font && this.dropDown && this.dropDown.isOpen()) {
                this.props.changeFont(font);
            }
        });
    }
    /** an1mbyt3: clicking a Google Font keeps it even if its hover preview
     * hadn't finished loading — close, wait for the face, then apply. */
    handlePickFontGoogle (font) {
        this.hoveredGoogle = null;
        if (this.dropDown.isOpen()) this.dropDown.handleClosePopover();
        ensureFontReady(font.replace(/^"|"$/g, '')).then(() => {
            this.props.changeFont(font);
            this.props.onUpdateImage();
        });
    }
    handleChoose () {
        if (this.dropDown.isOpen()) {
            this.dropDown.handleClosePopover();
            this.props.onUpdateImage();
        }
    }
    handleOpenDropdown () {
        this.savedFont = this.props.font;
        this.savedSelection = getSelectedLeafItems();
    }
    handleClickOutsideDropdown (e) {
        e.stopPropagation();
        this.dropDown.handleClosePopover();

        // Cancel font change
        for (const item of this.savedSelection) {
            if (item instanceof paper.PointText) {
                item.font = this.savedFont;
            }
        }

        this.props.changeFont(this.savedFont);
        this.savedFont = null;
        this.savedSelection = null;
    }
    setDropdown (element) {
        this.dropDown = element;
    }
    render () {
        return (
            <FontDropdownComponent
                componentRef={this.setDropdown}
                font={this.props.font}
                getFontName={this.getFontName}
                getFontStyle={this.getFontStyle}
                onChoose={this.handleChoose}
                onClickOutsideDropdown={this.handleClickOutsideDropdown}
                onHoverChinese={this.handleChangeFontChinese}
                onHoverCurly={this.handleChangeFontCurly}
                onHoverFont={this.handleChangeFontGoogle}
                onPickFont={this.handlePickFontGoogle}
                onHoverHandwriting={this.handleChangeFontHandwriting}
                onHoverJapanese={this.handleChangeFontJapanese}
                onHoverKorean={this.handleChangeFontKorean}
                onHoverMarker={this.handleChangeFontMarker}
                onHoverPixel={this.handleChangeFontPixel}
                onHoverSansSerif={this.handleChangeFontSansSerif}
                onHoverSerif={this.handleChangeFontSerif}
                onOpenDropdown={this.handleOpenDropdown}
            />
        );
    }
}

FontDropdown.propTypes = {
    changeFont: PropTypes.func.isRequired,
    font: PropTypes.string,
    onUpdateImage: PropTypes.func.isRequired
};

const mapStateToProps = state => ({
    font: state.scratchPaint.font
});
const mapDispatchToProps = dispatch => ({
    changeFont: font => {
        dispatch(changeFont(font));
    }
});

export default connect(
    mapStateToProps,
    mapDispatchToProps
)(FontDropdown);
