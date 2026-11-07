//! A directing profile: the rules the Director keeps and how it weighs the
//! shots it may take.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    /// Speech shorter than this is an interjection ("oui", "hum"): no cut.
    pub speech_onset_ms: u64,
    /// A shot is held at least this long.
    pub min_shot_ms: u64,
    /// A shot held this long is left for a cutaway.
    pub max_shot_ms: u64,
    /// Two people speaking together this long: a shot showing both.
    pub overlap_ms: u64,
    /// Nobody speaking this long: the wide shot.
    pub silence_to_wide_ms: u64,
    /// A shot must score this much more than the one on air to replace it.
    pub hysteresis: f32,
    /// Shots taken recently score less (variety), over this many cuts.
    pub variety_memory: usize,
    /// After a cut, the switcher has this long to show it before a
    /// different program is taken as a person's hand.
    pub human_cut_grace_ms: u64,
}

impl Profile {
    /// Calm conversation: long shots, few cutaways.
    pub fn podcast() -> Self {
        Profile {
            name: "Podcast posé".into(),
            speech_onset_ms: 700,
            min_shot_ms: 3000,
            max_shot_ms: 15000,
            overlap_ms: 1200,
            silence_to_wide_ms: 4000,
            hysteresis: 0.15,
            variety_memory: 3,
            human_cut_grace_ms: 500,
        }
    }

    /// Lively exchanges: quicker cuts, wide shot when they speak together.
    pub fn debate() -> Self {
        Profile {
            name: "Débat".into(),
            speech_onset_ms: 500,
            min_shot_ms: 2000,
            max_shot_ms: 10000,
            overlap_ms: 800,
            silence_to_wide_ms: 3000,
            ..Self::podcast()
        }
    }

    /// One host, one guest.
    pub fn interview() -> Self {
        Profile {
            name: "Interview".into(),
            max_shot_ms: 18000,
            ..Self::podcast()
        }
    }

    /// Filmed radio: wider, slower.
    pub fn radio() -> Self {
        Profile {
            name: "Radio filmée".into(),
            speech_onset_ms: 1000,
            min_shot_ms: 5000,
            max_shot_ms: 20000,
            silence_to_wide_ms: 2500,
            ..Self::podcast()
        }
    }

    pub fn all() -> Vec<Profile> {
        vec![
            Self::podcast(),
            Self::debate(),
            Self::interview(),
            Self::radio(),
        ]
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self::podcast()
    }
}
