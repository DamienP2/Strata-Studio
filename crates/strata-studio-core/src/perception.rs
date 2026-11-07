//! What Studio perceives of the studio at one instant: who has a voice on
//! their microphone, what each camera frames, what the switcher shows.
//!
//! The perception layer (strata-studio-sense) has already done the hard
//! part: `voice` is the seat's *own* voice, its neighbours' bleed removed,
//! smoothed over the short gaps between words.

use crate::model::{CamId, SeatId, ShotId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Perception {
    /// Milliseconds on the show's clock.
    pub t_ms: u64,
    pub mics: Vec<MicState>,
    pub cams: Vec<CamState>,
    /// The camera on the switcher's program, as the switcher says.
    #[serde(default)]
    pub program: Option<CamId>,
    /// How likely each seat is to speak next (the anticipation model);
    /// empty when there is none.
    #[serde(default)]
    pub next_speaker: Vec<(SeatId, f32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicState {
    pub seat: SeatId,
    /// The person in that seat is speaking.
    pub voice: bool,
    /// Level, dBFS.
    pub level_db: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CamState {
    pub cam: CamId,
    /// Answering, with a picture.
    pub online: bool,
    /// Its head or its zoom is moving.
    pub moving: bool,
    /// The shot it is framed on (the preset it last reached); a camera
    /// that does not move is always on its shot.
    pub framed: Option<ShotId>,
}

impl Perception {
    pub fn cam(&self, id: CamId) -> Option<&CamState> {
        self.cams.iter().find(|c| c.cam == id)
    }

    pub fn next_probability(&self, seat: SeatId) -> f32 {
        self.next_speaker
            .iter()
            .find(|(s, _)| *s == seat)
            .map_or(0.0, |(_, p)| *p)
    }
}
