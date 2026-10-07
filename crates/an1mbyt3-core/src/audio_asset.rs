//! Sound — imported audio clips and the Audio Source component.
//!
//! An [`AudioAsset`] is a document-level asset like [`crate::ImageAsset`]:
//! the encoded file (MP3 / WAV / OGG / …) saved once, played by any number
//! of [`AudioSource`] components and by scripts (`ctx.playSound(name)`).
//! Decoding and playback happen in the host (the browser's Web Audio, in
//! both the editor and exported games), so the engine only stores bytes.

use serde::{Deserialize, Serialize};

use crate::ids::AudioId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAsset {
    /// Shown in the Resources panel; also the name scripts play it by.
    pub name: String,
    /// The encoded file's bytes.
    pub source: Vec<u8>,
    /// In a project folder: the file it lives in (`Assets/Sounds/x.mp3`),
    /// so saves write the bytes there instead of into the manifest.
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

/// Unity's `AudioSource`: plays a clip from a node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSource {
    pub clip: Option<AudioId>,
    /// 0..1 (above 1 amplifies).
    pub volume: f32,
    /// Playback rate: 1 = normal, 2 = an octave up (and twice as fast).
    pub pitch: f32,
    #[serde(rename = "loop")]
    pub looping: bool,
    /// Starts by itself when Playground (or an exported game) enters the
    /// scene.
    pub play_on_start: bool,
    /// Pans left/right by where the node is relative to the camera, and
    /// fades with distance (silent beyond `range` world units).
    pub spatial: bool,
    pub range: f32,
}

impl Default for AudioSource {
    fn default() -> Self {
        Self { clip: None, volume: 1.0, pitch: 1.0, looping: false, play_on_start: true, spatial: false, range: 600.0 }
    }
}

/// Guesses a file extension from an audio file's leading bytes.
pub fn audio_extension(bytes: &[u8]) -> &'static str {
    match bytes {
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'A', b'V', b'E', ..] => ".wav",
        [b'O', b'g', b'g', b'S', ..] => ".ogg",
        [b'f', b'L', b'a', b'C', ..] => ".flac",
        [b'I', b'D', b'3', ..] | [0xFF, 0xFB | 0xF3 | 0xF2, ..] => ".mp3",
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => ".m4a",
        _ => ".audio",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_audio_formats() {
        assert_eq!(audio_extension(b"RIFF\0\0\0\0WAVEfmt "), ".wav");
        assert_eq!(audio_extension(b"OggS\0"), ".ogg");
        assert_eq!(audio_extension(b"ID3\x04"), ".mp3");
        assert_eq!(audio_extension(&[0xFF, 0xFB, 0x90]), ".mp3");
    }

    #[test]
    fn audio_source_round_trips_with_loop_named_loop() {
        let source = AudioSource { looping: true, ..AudioSource::default() };
        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"loop\":true"), "{json}");
        assert_eq!(serde_json::from_str::<AudioSource>(&json).unwrap(), source);
    }
}
