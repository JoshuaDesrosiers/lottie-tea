//! Live keyboard state — a Blocks program's "key [ ] pressed?" sensing
//! reporter and "when key [ ] pressed" hat both read this. Purely live
//! session state (like [`crate::state_machine::Inputs`]), never saved; a
//! host (the desktop editor's `App.tsx`, a wasm player embed, ...) is
//! responsible for forwarding real keyboard events into it via
//! [`KeyboardState::set_down`], the same "always forward, gate at the read
//! side" convention `Inputs`/`programs_enabled` already establish elsewhere
//! in this workspace — forwarding a key that no active Blocks script reads
//! is simply inert, not a validity concern.
//!
//! Keys are addressed by their raw `KeyboardEvent.key` string, lowercased
//! (`"a"`, `"space"` — actually `" "` for the space bar in the real DOM
//! `key` value, but callers are free to normalize to a friendlier name
//! before calling in; this type itself does no interpretation beyond a
//! plain string key) so authoring a block's key field is just typing/
//! picking that string, no separate keycode table to maintain here.

use std::collections::HashSet;

/// Held-down keys, plus enough of last frame's snapshot to detect a
/// down-*edge* (`just_pressed`) for [`an1mbyt3_core::EventKind::KeyPressed`]'s
/// edge-triggered hat, as distinct from [`an1mbyt3_core::Expr::KeyPressed`]'s
/// plain level-triggered "is it down right now" reporter.
#[derive(Debug, Clone, Default)]
pub struct KeyboardState {
    down: HashSet<String>,
    down_last_frame: HashSet<String>,
}

impl KeyboardState {
    pub fn set_down(&mut self, key: impl Into<String>, pressed: bool) {
        let key = key.into();
        if pressed {
            self.down.insert(key);
        } else {
            self.down.remove(&key);
        }
    }

    /// Level-triggered: `true` for as long as `key` is held.
    pub fn is_down(&self, key: &str) -> bool {
        self.down.contains(key)
    }

    /// Edge-triggered: `true` only on the first frame `key` reads as down
    /// after a frame where it didn't — call [`Self::end_frame`] once per
    /// frame (after every Blocks script has had a chance to check this) to
    /// advance the edge-detection snapshot.
    pub fn just_pressed(&self, key: &str) -> bool {
        self.down.contains(key) && !self.down_last_frame.contains(key)
    }

    /// Snapshots this frame's held keys as "last frame" for the *next*
    /// call's `just_pressed` — called once per frame, after every script
    /// has run, from `StateMachine`'s own per-frame entry point (mirrors
    /// `Inputs::clear_fired_triggers`'s "consumed once per frame, by the
    /// same caller that drives everything else" placement).
    pub fn end_frame(&mut self) {
        self.down_last_frame.clone_from(&self.down);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn just_pressed_is_true_only_on_the_first_frame_a_key_goes_down() {
        let mut keys = KeyboardState::default();
        keys.set_down("space", true);
        assert!(keys.just_pressed("space"), "first frame the key reads down must be an edge");

        keys.end_frame();
        assert!(!keys.just_pressed("space"), "still held on the next frame is not a new edge");
        assert!(keys.is_down("space"), "is_down stays true for as long as it's held");
    }

    #[test]
    fn releasing_and_pressing_again_produces_a_second_edge() {
        let mut keys = KeyboardState::default();
        keys.set_down("space", true);
        keys.end_frame();
        keys.set_down("space", false);
        keys.end_frame();
        assert!(!keys.is_down("space"));

        keys.set_down("space", true);
        assert!(keys.just_pressed("space"), "pressing again after a release is a new edge");
    }
}
