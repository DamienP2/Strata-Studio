//! The host's ear: the host's microphone, cut into sentences, transcribed,
//! and handed to the Director's listener as what was heard.
//!
//! Whisper sometimes "hears" text in silence or noise: credits it learned
//! from subtitles ("Sous-titres réalisés par…"), "Merci d'avoir regardé",
//! a lone "Merci.", bracketed sounds. Those are dropped: a cue must come
//! from the host's words.

use crate::segment::{Segment, Segmenter, Settings, RATE};
use crate::wav;
use crate::whisper::Recognizer;
use strata_studio_core::{Heard, SeatId};

/// Texts Whisper is known to invent on silence or noise (lower case, no
/// punctuation).
const INVENTED: &[&str] = &[
    "sous titres realises par la communaute d'amara org",
    "sous titres realises para la communaute d'amara org",
    "sous titrage st' 501",
    "sous titrage societe radio canada",
    "merci d'avoir regarde",
    "merci d'avoir regarde cette video",
    "merci",
    "merci beaucoup",
    "...",
];

fn plain(s: &str) -> String {
    let lower: String = s
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'â' => 'a',
            'é' | 'è' | 'ê' => 'e',
            'î' | 'ï' => 'i',
            'ô' => 'o',
            'ù' | 'û' => 'u',
            'ç' => 'c',
            '’' => '\'',
            c if c.is_alphanumeric() || c == '\'' => c,
            _ => ' ',
        })
        .collect();
    lower.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a transcript is the host's words, not something Whisper invented.
pub fn is_speech(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty()
        || (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || t.starts_with('*')
    {
        return false;
    }
    let p = plain(t);
    !p.is_empty() && !INVENTED.contains(&p.as_str())
}

pub struct HostEar<R: Recognizer> {
    seat: SeatId,
    segmenter: Segmenter,
    recognizer: R,
    /// Where the sound fed starts, on the show's clock.
    origin_ms: u64,
}

impl<R: Recognizer> HostEar<R> {
    pub fn new(seat: SeatId, rate_in: u32, recognizer: R) -> Self {
        HostEar {
            seat,
            segmenter: Segmenter::new(rate_in, Settings::default()),
            recognizer,
            origin_ms: 0,
        }
    }

    /// The show's clock time at which the sound fed starts.
    pub fn starting_at(mut self, origin_ms: u64) -> Self {
        self.origin_ms = origin_ms;
        self
    }

    /// Feed the host's sound; returns what was heard (sentences finished,
    /// or pieces of long ones). A recogniser that fails loses that
    /// sentence, not the ear.
    pub fn hear(&mut self, input: &[f32]) -> Vec<Heard> {
        let segs = self.segmenter.feed(input);
        self.transcribe(segs)
    }

    /// The end of the sound.
    pub fn finish(&mut self) -> Vec<Heard> {
        let segs = self.segmenter.flush();
        self.transcribe(segs)
    }

    fn transcribe(&mut self, segs: Vec<Segment>) -> Vec<Heard> {
        segs.into_iter()
            .filter_map(|s| {
                let text = self
                    .recognizer
                    .transcribe(&wav::encode(&s.samples, RATE))
                    .ok()?;
                is_speech(&text).then(|| Heard {
                    t_ms: self.origin_ms + s.end_ms,
                    seat: self.seat,
                    text,
                    is_final: s.is_final,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Result;

    struct Script(Vec<&'static str>);

    impl Recognizer for Script {
        fn transcribe(&mut self, _wav: &[u8]) -> Result<String> {
            Ok(if self.0.is_empty() {
                String::new()
            } else {
                self.0.remove(0).to_string()
            })
        }
    }

    fn tone(secs: f32, loud: bool) -> Vec<f32> {
        (0..(secs * 16000.0) as usize)
            .map(|i| {
                if loud {
                    0.2 * (i as f32 * 0.09).sin()
                } else {
                    0.0
                }
            })
            .collect()
    }

    #[test]
    fn what_whisper_invents_is_dropped() {
        assert!(!is_speech(
            " Sous-titres réalisés par la communauté d'Amara.org"
        ));
        assert!(!is_speech("[Musique]"));
        assert!(!is_speech("Merci."));
        assert!(is_speech("Merci Julie, on revient en plateau."));
    }

    #[test]
    fn sentences_become_what_was_heard() {
        let rec = Script(vec![
            "On retrouve Julie en duplex.",
            "Sous-titres réalisés par la communauté d'Amara.org",
        ]);
        let mut ear = HostEar::new(SeatId(1), 16000, rec).starting_at(60_000);
        let mut sound = tone(0.5, false);
        sound.extend(tone(1.5, true));
        sound.extend(tone(1.0, false));
        sound.extend(tone(0.5, true));
        sound.extend(tone(1.0, false));
        let heard = ear.hear(&sound);
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].text, "On retrouve Julie en duplex.");
        assert_eq!(heard[0].seat, SeatId(1));
        assert_eq!(
            heard[0].t_ms, 62_000,
            "when the sentence ends, on the show's clock"
        );
        assert!(heard[0].is_final);
    }
}
