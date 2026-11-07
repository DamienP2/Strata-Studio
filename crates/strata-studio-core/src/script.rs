//! A scripted conversation: who speaks when. Used by the tests and by the
//! server's demonstration studio, in place of microphones.

use crate::model::SeatId;
use crate::perception::MicState;

#[derive(Clone, Debug)]
pub struct Utterance {
    pub seat: SeatId,
    pub from_ms: u64,
    pub to_ms: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Script {
    pub utterances: Vec<Utterance>,
    /// The script starts again after this long (0: it does not loop).
    pub loop_ms: u64,
}

impl Script {
    pub fn say(mut self, seat: u8, from_s: f64, to_s: f64) -> Self {
        self.utterances.push(Utterance {
            seat: SeatId(seat),
            from_ms: (from_s * 1000.0) as u64,
            to_ms: (to_s * 1000.0) as u64,
        });
        self
    }

    /// The microphones of `seats` at `t_ms`.
    pub fn mics(&self, seats: impl IntoIterator<Item = SeatId>, t_ms: u64) -> Vec<MicState> {
        let t = if self.loop_ms > 0 {
            t_ms % self.loop_ms
        } else {
            t_ms
        };
        seats
            .into_iter()
            .map(|seat| {
                let voice = self
                    .utterances
                    .iter()
                    .any(|u| u.seat == seat && (u.from_ms..u.to_ms).contains(&t));
                MicState {
                    seat,
                    voice,
                    level_db: if voice { -18.0 } else { -60.0 },
                }
            })
            .collect()
    }

    /// A conversation between five people, about two minutes long: a host
    /// (seat 1) asks, guests answer, an interjection, a moment where two
    /// speak together, a silence, a long answer.
    pub fn demo() -> Self {
        Script {
            loop_ms: 125_000,
            ..Default::default()
        }
        .say(1, 1.0, 9.0)
        .say(3, 10.0, 24.0)
        .say(1, 15.0, 15.4) // « hum »
        .say(1, 25.0, 29.0)
        .say(4, 30.0, 41.0)
        .say(5, 38.0, 44.0) // speaks over seat 4
        .say(2, 46.0, 52.0)
        .say(1, 52.5, 57.0)
        // silence 57 → 63
        .say(5, 63.0, 90.0) // a long answer
        .say(1, 91.0, 95.0)
        .say(2, 96.0, 108.0)
        .say(3, 108.5, 120.0)
    }
}
