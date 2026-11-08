//! Who speaks, from the microphones: one voice detector per channel, and
//! the bleed taken out.
//!
//! Every microphone also hears its neighbours, quieter. A channel has a
//! **voice** when it is loud enough (above the threshold) **and** close to
//! the loudest channel speaking (within `bleed_db`): the neighbour's
//! microphone, 10 dB under, is bleed. Short gaps between words are kept
//! (`hangover_ms`), so that a sentence is one stretch of voice.
//!
//! The loudness detector stands in for a voice model (Silero VAD) until it
//! is in; the comparison between channels stays.

use strata_studio_core::{MicState, SeatId};

#[derive(Clone, Debug)]
pub struct Settings {
    /// A channel louder than this (dBFS, over 20 ms) may be a voice.
    pub threshold_db: f32,
    /// A channel this far under the loudest is the loudest's bleed.
    pub bleed_db: f32,
    /// A voice stays this long after the last loud frame.
    pub hangover_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            threshold_db: -45.0,
            bleed_db: 9.0,
            hangover_ms: 250,
        }
    }
}

/// The voices of the channels of a stream.
pub struct Voices {
    channels: usize,
    rate: u32,
    settings: Settings,
    /// Samples of the frame being filled, per channel (sum of squares).
    acc: Vec<f32>,
    filled: usize,
    /// Per channel: last level (dBFS), ms since the last voice frame.
    level: Vec<f32>,
    quiet_ms: Vec<u64>,
}

impl Voices {
    pub fn new(channels: usize, rate: u32, settings: Settings) -> Self {
        Voices {
            channels,
            rate,
            settings,
            acc: vec![0.0; channels],
            filled: 0,
            level: vec![-120.0; channels],
            quiet_ms: vec![u64::MAX / 2; channels],
        }
    }

    /// Feed interleaved samples.
    pub fn feed(&mut self, interleaved: &[f32]) {
        let frame = (self.rate / 50) as usize;
        for chunk in interleaved.chunks_exact(self.channels) {
            for (a, s) in self.acc.iter_mut().zip(chunk) {
                *a += s * s;
            }
            self.filled += 1;
            if self.filled == frame {
                self.frame(frame);
            }
        }
    }

    fn frame(&mut self, n: usize) {
        for (l, a) in self.level.iter_mut().zip(&self.acc) {
            *l = 10.0 * (a / n as f32).max(1e-12).log10();
        }
        let loudest = self.level.iter().cloned().fold(f32::MIN, f32::max);
        for (ch, l) in self.level.iter().enumerate() {
            let voice = *l >= self.settings.threshold_db && *l >= loudest - self.settings.bleed_db;
            self.quiet_ms[ch] = if voice {
                0
            } else {
                self.quiet_ms[ch].saturating_add(20)
            };
        }
        self.acc.iter_mut().for_each(|a| *a = 0.0);
        self.filled = 0;
    }

    /// Whether a channel (0-based) has a voice now.
    pub fn voice(&self, channel: usize) -> bool {
        self.quiet_ms
            .get(channel)
            .is_some_and(|q| *q <= self.settings.hangover_ms)
    }

    pub fn level_db(&self, channel: usize) -> f32 {
        self.level.get(channel).copied().unwrap_or(-120.0)
    }

    /// The microphones of the seats, by their channel in the stream.
    pub fn mics(&self, seats: impl IntoIterator<Item = (SeatId, u16)>) -> Vec<MicState> {
        seats
            .into_iter()
            .map(|(seat, ch)| MicState {
                seat,
                voice: self.voice(ch as usize),
                level_db: self.level_db(ch as usize),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secs` of 2 channels at 48 kHz: a voice-like tone at `a` and `b` (amplitudes).
    fn two(secs: f32, a: f32, b: f32) -> Vec<f32> {
        (0..(secs * 48000.0) as usize)
            .flat_map(|i| {
                let s = (i as f32 * 0.06).sin();
                [a * s, b * s]
            })
            .collect()
    }

    #[test]
    fn the_neighbours_bleed_is_not_a_voice() {
        let mut v = Voices::new(2, 48000, Settings::default());
        // Channel 0 speaks; channel 1 hears it 12 dB under.
        v.feed(&two(0.5, 0.2, 0.05));
        assert!(v.voice(0));
        assert!(!v.voice(1));
        let mics = v.mics([(SeatId(1), 0), (SeatId(2), 1)]);
        assert!(mics[0].voice && !mics[1].voice);
        assert!(mics[0].level_db > mics[1].level_db + 10.0);
    }

    #[test]
    fn two_speaking_together_both_have_a_voice() {
        let mut v = Voices::new(2, 48000, Settings::default());
        v.feed(&two(0.5, 0.2, 0.15));
        assert!(v.voice(0) && v.voice(1));
    }

    #[test]
    fn a_short_gap_is_kept_a_long_one_ends_the_voice() {
        let mut v = Voices::new(2, 48000, Settings::default());
        v.feed(&two(0.5, 0.2, 0.0));
        v.feed(&two(0.2, 0.0, 0.0));
        assert!(v.voice(0), "200 ms between two words");
        v.feed(&two(0.2, 0.0, 0.0));
        assert!(!v.voice(0), "400 ms: the sentence is over");
    }

    #[test]
    fn silence_is_no_voice() {
        let mut v = Voices::new(2, 48000, Settings::default());
        v.feed(&two(0.5, 0.0001, 0.0));
        assert!(!v.voice(0) && !v.voice(1));
    }
}
