//! Playback state: play/pause/seek/loop, decoupled from any specific track or
//! node (AN1MBYT3_BUILD_PLAN.md §5's "playback, looping" — one `Playback`
//! instance can drive as many tracks as share the same timeline).

use serde::{Deserialize, Serialize};

pub use an1mbyt3_core::LoopMode;

/// Tracks elapsed time through an animation and how it advances/loops.
///
/// Deliberately holds no reference to a [`crate::Track`] or a document node —
/// keeping playback state (time/playing/loop behavior) separate from *what*
/// gets sampled at that time is what lets one `Playback` drive multiple
/// tracks in lockstep, which state machines (Phase 6) will need.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Playback {
    pub time: f32,
    pub playing: bool,
    pub speed: f32,
    pub loop_mode: LoopMode,
    duration: f32,
    /// `true` when the last `advance` was moving backward under `PingPong` —
    /// tracked here (not derived) because ping-pong direction is state, not a
    /// pure function of `time` alone (time `0.3` could be heading either way).
    reversed: bool,
}

impl Playback {
    pub fn new(duration: f32, loop_mode: LoopMode) -> Self {
        Self {
            time: 0.0,
            playing: false,
            speed: 1.0,
            loop_mode,
            duration,
            reversed: false,
        }
    }

    pub fn duration(&self) -> f32 {
        self.duration
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Jumps directly to `time`, clamped to `[0, duration]`. Does not change
    /// `playing` — matches how scrubbing a timeline in an editor works
    /// (AN1MBYT3_BUILD_PLAN.md §10 Phase 3's editor timeline will call this).
    pub fn seek(&mut self, time: f32) {
        self.time = time.clamp(0.0, self.duration);
    }

    /// Advances playback by `delta_seconds` of wall-clock time (already
    /// scaled by `speed` internally). A no-op while paused.
    pub fn advance(&mut self, delta_seconds: f32) {
        if !self.playing || self.duration <= 0.0 {
            return;
        }

        let step = delta_seconds * self.speed * if self.reversed { -1.0 } else { 1.0 };
        let mut time = self.time + step;

        match self.loop_mode {
            LoopMode::Once => {
                // Gated on `step`'s sign (not just `time`'s position) so that
                // a zero-delta `advance` landing exactly on a boundary
                // doesn't spuriously stop playback — this matters
                // specifically for a state machine's reversed transition
                // (`state_machine.rs`'s `PlaybackDirection::Reverse`), which
                // seeks a fresh `Playback` to exactly `self.duration` before
                // its first `advance` call; without this guard, that first
                // call (even with `delta_seconds: 0.0`, e.g. the same frame
                // the transition itself happens) would see `time >=
                // self.duration` and immediately set `playing = false`,
                // freezing the reversed clip before it ever moved backward.
                // A real nonzero-delta step crossing a boundary still stops
                // exactly as before.
                if step > 0.0 && time >= self.duration {
                    time = self.duration;
                    self.playing = false;
                } else if step < 0.0 && time <= 0.0 {
                    time = 0.0;
                    self.playing = false;
                }
            }
            LoopMode::Loop => {
                time = time.rem_euclid(self.duration);
            }
            LoopMode::PingPong => {
                if time >= self.duration {
                    time = self.duration - (time - self.duration);
                    self.reversed = true;
                } else if time <= 0.0 {
                    time = -time;
                    self.reversed = false;
                }
            }
        }

        self.time = time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_playback_does_not_advance() {
        let mut playback = Playback::new(1.0, LoopMode::Loop);
        playback.advance(0.5);
        assert_eq!(playback.time, 0.0);
    }

    #[test]
    fn once_stops_at_duration() {
        let mut playback = Playback::new(1.0, LoopMode::Once);
        playback.play();
        playback.advance(2.0);
        assert_eq!(playback.time, 1.0);
        assert!(!playback.playing);
    }

    #[test]
    fn zero_delta_advance_at_a_boundary_does_not_freeze_once_playback() {
        // The exact scenario `state_machine.rs`'s reversed transitions rely
        // on: a fresh `Once` `Playback` seeked to its own boundary (here
        // simulating a reverse-transition's `seek(duration)`), then advanced
        // with `delta_seconds: 0.0` — as happens the same frame a state
        // machine's `advance` both transitions state *and* calls
        // `Playback::advance` for that same frame. This must not set
        // `playing = false`, or the clip would never move on subsequent
        // real-delta frames.
        let mut playback = Playback::new(1.0, LoopMode::Once);
        playback.seek(1.0);
        playback.speed = -1.0;
        playback.play();

        playback.advance(0.0);
        assert_eq!(playback.time, 1.0);
        assert!(playback.playing, "a zero-delta advance sitting exactly on a boundary must not stop playback");

        playback.advance(0.5);
        assert_eq!(playback.time, 0.5);
        assert!(playback.playing);
    }

    #[test]
    fn loop_wraps_around() {
        let mut playback = Playback::new(1.0, LoopMode::Loop);
        playback.play();
        playback.advance(1.5);
        assert!((playback.time - 0.5).abs() < 1e-5);
        assert!(playback.playing);
    }

    #[test]
    fn ping_pong_reverses_at_the_end() {
        let mut playback = Playback::new(1.0, LoopMode::PingPong);
        playback.play();
        playback.advance(1.5);
        // Overshot by 0.5 past duration 1.0 -> bounced back to 0.5, now reversed.
        assert!((playback.time - 0.5).abs() < 1e-5);
        playback.advance(1.0);
        // Continuing backward from 0.5 by 1.0 overshoots 0.0 by 0.5 -> bounces to 0.5, forward again.
        assert!((playback.time - 0.5).abs() < 1e-5);
    }

    #[test]
    fn seek_clamps_to_duration() {
        let mut playback = Playback::new(2.0, LoopMode::Once);
        playback.seek(5.0);
        assert_eq!(playback.time, 2.0);
        playback.seek(-5.0);
        assert_eq!(playback.time, 0.0);
    }
}
