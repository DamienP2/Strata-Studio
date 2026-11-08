//! The host's microphone cut into sentences for the speech recogniser.
//!
//! The sound is brought to 16 kHz (what Whisper takes) and read in 20 ms
//! frames. A frame louder than the threshold is speech; a sentence ends
//! after `phrase_end_ms` of quiet. A long sentence is also sent every
//! `partial_every_ms` as it goes (marked not final), so that the Director
//! can prepare before the host has finished; and cut at `max_ms`.
//!
//! The loudness threshold stands in for a voice detector (Silero VAD)
//! until it is in: the console's own channel is clean enough for it.

/// The rate the recogniser takes.
pub const RATE: u32 = 16000;
const FRAME: usize = (RATE / 50) as usize;

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// From the start of the sound, in ms.
    pub start_ms: u64,
    pub end_ms: u64,
    /// 16 kHz, 16-bit.
    pub samples: Vec<i16>,
    /// The end of a sentence (else a piece of one still going).
    pub is_final: bool,
}

#[derive(Clone, Debug)]
pub struct Settings {
    /// A frame louder than this (dBFS, RMS) is speech.
    pub threshold_db: f32,
    pub phrase_end_ms: u64,
    /// Shorter than this, a sound is a noise, not a sentence.
    pub min_speech_ms: u64,
    pub partial_every_ms: u64,
    pub max_ms: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            threshold_db: -42.0,
            phrase_end_ms: 350,
            min_speech_ms: 300,
            partial_every_ms: 2000,
            max_ms: 15000,
        }
    }
}

pub struct Segmenter {
    rate_in: u32,
    settings: Settings,
    /// Input not yet made into a frame.
    pending: Vec<f32>,
    /// Frames read so far.
    frames: u64,
    /// The sentence being heard: its first frame, its samples, its
    /// speech frames, its last speech frame, when it was last sent.
    current: Option<Current>,
}

struct Current {
    start_frame: u64,
    samples: Vec<i16>,
    speech_frames: u64,
    last_speech_frame: u64,
    sent_frames: u64,
}

impl Segmenter {
    pub fn new(rate_in: u32, settings: Settings) -> Self {
        Segmenter {
            rate_in,
            settings,
            pending: Vec::new(),
            frames: 0,
            current: None,
        }
    }

    fn ms(frames: u64) -> u64 {
        frames * 20
    }

    /// Feed sound (mono, from -1 to 1, at the input rate); returns the
    /// sentences, or pieces of sentences, ready.
    pub fn feed(&mut self, input: &[f32]) -> Vec<Segment> {
        self.pending.extend_from_slice(input);
        let in_frame = (self.rate_in / 50) as usize;
        let mut out = Vec::new();
        while self.pending.len() >= in_frame {
            let chunk: Vec<f32> = self.pending.drain(..in_frame).collect();
            let frame = resample(&chunk, FRAME);
            self.frame(&frame, &mut out);
        }
        out
    }

    /// The end of the sound: what is still being heard is sent.
    pub fn flush(&mut self) -> Vec<Segment> {
        let mut out = Vec::new();
        if let Some(c) = self.current.take() {
            if Self::ms(c.speech_frames) >= self.settings.min_speech_ms {
                out.push(self.segment(&c, true));
            }
        }
        out
    }

    fn segment(&self, c: &Current, is_final: bool) -> Segment {
        let end = if is_final {
            c.last_speech_frame + 1
        } else {
            self.frames
        };
        let keep = ((end - c.start_frame) as usize * FRAME).min(c.samples.len());
        Segment {
            start_ms: Self::ms(c.start_frame),
            end_ms: Self::ms(end),
            samples: c.samples[..keep].to_vec(),
            is_final,
        }
    }

    fn frame(&mut self, frame: &[f32], out: &mut Vec<Segment>) {
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        let db = 20.0 * rms.max(1e-9).log10();
        let speech = db >= self.settings.threshold_db;
        let n = self.frames;
        self.frames += 1;
        let pcm = frame.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16);

        match &mut self.current {
            None if speech => {
                self.current = Some(Current {
                    start_frame: n,
                    samples: pcm.collect(),
                    speech_frames: 1,
                    last_speech_frame: n,
                    sent_frames: n,
                });
            }
            None => {}
            Some(c) => {
                c.samples.extend(pcm);
                if speech {
                    c.speech_frames += 1;
                    c.last_speech_frame = n;
                }
            }
        }
        let Some(c) = self.current.as_ref() else {
            return;
        };
        let quiet = Self::ms(n - c.last_speech_frame);
        let long = Self::ms(n + 1 - c.start_frame);
        if quiet >= self.settings.phrase_end_ms || long >= self.settings.max_ms {
            let c = self.current.take().expect("a sentence");
            if Self::ms(c.speech_frames) >= self.settings.min_speech_ms {
                out.push(self.segment(&c, true));
            }
        } else if Self::ms(n + 1 - c.sent_frames) >= self.settings.partial_every_ms
            && Self::ms(c.speech_frames) >= self.settings.min_speech_ms
        {
            let seg = self.segment(c, false);
            if let Some(c) = self.current.as_mut() {
                c.sent_frames = n + 1;
            }
            out.push(seg);
        }
    }
}

/// `input` stretched or shrunk to `len` samples (linear).
fn resample(input: &[f32], len: usize) -> Vec<f32> {
    if input.len() == len {
        return input.to_vec();
    }
    let ratio = input.len() as f32 / len as f32;
    (0..len)
        .map(|i| {
            let x = i as f32 * ratio;
            let a = x.floor() as usize;
            let b = (a + 1).min(input.len() - 1);
            let f = x - a as f32;
            input[a] * (1.0 - f) + input[b] * f
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `secs` of a 220 Hz tone (loud, speech-like level) or silence, at 48 kHz.
    fn sound(parts: &[(f32, bool)]) -> Vec<f32> {
        let mut out = Vec::new();
        for (secs, loud) in parts {
            let n = (secs * 48000.0) as usize;
            out.extend((0..n).map(|i| {
                if *loud {
                    0.2 * (i as f32 * 220.0 * std::f32::consts::TAU / 48000.0).sin()
                } else {
                    0.0
                }
            }));
        }
        out
    }

    #[test]
    fn a_sentence_between_two_silences() {
        let mut s = Segmenter::new(48000, Settings::default());
        let segs = s.feed(&sound(&[(1.0, false), (1.5, true), (1.0, false)]));
        let finals: Vec<_> = segs.iter().filter(|x| x.is_final).collect();
        assert_eq!(finals.len(), 1, "{segs:?}");
        let f = finals[0];
        assert_eq!(f.start_ms, 1000);
        assert_eq!(f.end_ms, 2500);
        assert_eq!(f.samples.len(), 24000, "1.5 s at 16 kHz");
    }

    #[test]
    fn a_long_sentence_is_sent_as_it_goes() {
        let mut s = Segmenter::new(48000, Settings::default());
        let mut segs = s.feed(&sound(&[(0.2, false), (5.0, true)]));
        segs.extend(s.flush());
        let partial: Vec<u64> = segs
            .iter()
            .filter(|x| !x.is_final)
            .map(|x| x.end_ms)
            .collect();
        assert_eq!(partial, vec![2200, 4200]);
        assert!(segs.last().unwrap().is_final);
        assert_eq!(segs.last().unwrap().end_ms, 5200);
    }

    #[test]
    fn a_click_is_not_a_sentence() {
        let mut s = Segmenter::new(16000, Settings::default());
        let mut click = vec![0.0; 16000];
        click.extend(std::iter::repeat_n(0.3, 1600)); // 100 ms
        click.extend(vec![0.0; 16000]);
        assert!(s.feed(&click).is_empty());
    }

    #[test]
    fn quiet_breaths_inside_a_sentence_do_not_cut_it() {
        let mut s = Segmenter::new(48000, Settings::default());
        let segs = s.feed(&sound(&[
            (0.5, false),
            (1.0, true),
            (0.2, false),
            (1.0, true),
            (1.0, false),
        ]));
        let finals: Vec<_> = segs.iter().filter(|x| x.is_final).collect();
        assert_eq!(finals.len(), 1);
        assert_eq!((finals[0].start_ms, finals[0].end_ms), (500, 2700));
    }
}
