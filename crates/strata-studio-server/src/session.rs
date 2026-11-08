//! The shoot, step by step (the tablet's journey), held by the server so
//! that the tablet, a phone, the Stream Deck and the studio screen all
//! show the same thing:
//!
//! 1 Accueil · 2 Vérification · 3 Programme · 4 Enregistrement · 5 Stockage
//! · 6 Résolution · 7 Format · 8 Diffusion · 9 Personnes · 10 Flux
//! extérieurs · 11 Placement · 12 Tests · 13 Récapitulatif, then the shoot
//! · 14 Fin.
//!
//! This module keeps the answers and checks them; what the devices do at
//! each step is in `shoot.rs`.

use crate::config::Programme;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// A line of the studio check (step 2) or of the recap (13).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub label: String,
    pub detail: String,
    /// `ok`, `warn` (works, with a remark) or `fail`.
    pub state: String,
}

impl Check {
    pub fn ok(label: &str, detail: impl Into<String>) -> Self {
        Check {
            label: label.into(),
            detail: detail.into(),
            state: "ok".into(),
        }
    }
    pub fn warn(label: &str, detail: impl Into<String>) -> Self {
        Check {
            label: label.into(),
            detail: detail.into(),
            state: "warn".into(),
        }
    }
    pub fn fail(label: &str, detail: impl Into<String>) -> Self {
        Check {
            label: label.into(),
            detail: detail.into(),
            state: "fail".into(),
        }
    }
}

/// Someone or something to test at step 12.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Test {
    pub label: String,
    /// The seat (a person) or none (a remote source).
    pub seat: Option<u8>,
    pub camera: Option<u8>,
    /// The switcher input shown.
    pub input: u16,
    /// The preview source (« CAM2 », « PGM »).
    pub source: String,
    pub done: bool,
}

/// How the current test goes.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hearing {
    /// Time with a voice at a good level, in ms.
    pub good_ms: u64,
    /// Time with a voice too low, in ms.
    pub low_ms: u64,
    /// The level now, 0 to 1 (for the meter).
    pub level: f32,
    pub heard: bool,
    pub low: bool,
    /// Tries of this test.
    pub tries: u32,
}

impl Hearing {
    /// A voice above −40 dBFS for a second is heard; one between −58 and
    /// −40 for 1.5 s, never good enough, is too low.
    pub fn feed(&mut self, level_db: f32, dt_ms: u64) {
        self.level = ((level_db + 60.0) / 60.0).clamp(0.0, 1.0);
        if self.heard {
            return;
        }
        if level_db > -40.0 {
            self.good_ms += dt_ms;
            if self.good_ms >= 1000 {
                self.heard = true;
                self.low = false;
            }
        } else if level_db > -58.0 {
            self.low_ms += dt_ms;
            if self.low_ms >= 1500 {
                self.low = true;
            }
        }
    }
}

/// A call's guest, with the link to send.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestView {
    pub label: String,
    pub uid: String,
    pub link: String,
    /// `waiting`, `connected`, `unstable`, `unknown`.
    pub state: String,
}

/// After « Terminer ».
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ended {
    pub id: String,
    pub seconds: u64,
    pub files: usize,
}

pub struct Session {
    pub step: u8,
    pub draft: Programme,
    /// The programme the answers were taken from.
    pub from_programme: Option<String>,
    pub boot_at: Option<Instant>,
    pub checks: Vec<Check>,
    pub checks_at: Option<Instant>,
    pub guests: Vec<GuestView>,
    pub tests: Vec<Test>,
    pub test_index: usize,
    pub hearing: Hearing,
    pub test_at: Option<Instant>,
    pub recap_at: Option<Instant>,
    /// The shoot under way: its id and start.
    pub shooting: Option<(String, Instant)>,
    pub ended: Option<Ended>,
    pub end_at: Option<Instant>,
    /// What the studio screen shows during the tests: `zone` (the 9:16 box
    /// drawn) or `only` (the 9:16 alone).
    pub screen: String,
    /// Messages to the journalist (a live stream dropped…).
    pub alerts: Vec<String>,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            step: 1,
            draft: Programme {
                names: vec![String::new(); 5],
                ..Programme::default()
            },
            from_programme: None,
            boot_at: None,
            checks: vec![],
            checks_at: None,
            guests: vec![],
            tests: vec![],
            test_index: 0,
            hearing: Hearing::default(),
            test_at: None,
            recap_at: None,
            shooting: None,
            ended: None,
            end_at: None,
            screen: "zone".into(),
            alerts: vec![],
        }
    }
}

/// The answers a client may change, all optional.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftPatch {
    pub name: Option<String>,
    pub profile: Option<usize>,
    pub pgm: Option<bool>,
    pub clean: Option<bool>,
    pub iso: Option<bool>,
    pub mics: Option<bool>,
    pub storage: Option<usize>,
    pub resolution: Option<String>,
    pub vertical: Option<bool>,
    pub go: Option<u8>,
    pub youtube: Option<bool>,
    pub tiktok: Option<bool>,
    pub people: Option<u8>,
    pub names: Option<Vec<String>>,
    pub calls: Option<u8>,
    pub agency: Option<bool>,
    pub duplex: Option<bool>,
    pub seats: Option<Vec<Option<u8>>>,
    pub screen: Option<String>,
}

/// The default seats for a number of people: spread around the table
/// (seats 1 to 5), as on the tablet's plan.
pub fn default_seats(people: u8) -> Vec<Option<u8>> {
    let slots: &[usize] = match people {
        1 => &[2],
        2 => &[1, 3],
        3 => &[0, 2, 4],
        4 => &[0, 1, 3, 4],
        _ => &[0, 1, 2, 3, 4],
    };
    let mut seats = vec![None; 5];
    for (i, s) in slots.iter().enumerate() {
        seats[*s] = Some(i as u8);
    }
    seats
}

impl Session {
    pub fn apply(&mut self, p: DraftPatch, gos_free: &[bool]) {
        let d = &mut self.draft;
        if let Some(v) = p.name {
            if self.from_programme.as_deref() != Some(v.trim()) {
                self.from_programme = None;
            }
            d.name = v;
        }
        if let Some(v) = p.profile {
            d.profile = v;
        }
        if let Some(v) = p.pgm {
            d.pgm = v;
        }
        if let Some(v) = p.clean {
            d.clean = v;
        }
        if let Some(v) = p.iso {
            d.iso = v;
        }
        if let Some(v) = p.mics {
            d.mics = v;
        }
        if let Some(v) = p.storage {
            d.storage = v;
        }
        if let Some(v) = p.resolution {
            if v == "1080p50" || v == "2160p50" {
                d.resolution = v;
            }
        }
        if let Some(v) = p.vertical {
            d.vertical = v;
            if !v {
                d.tiktok = false;
            }
        }
        if let Some(v) = p.go {
            // A GO taken by another studio cannot be chosen.
            let free = v == 0 || gos_free.get(v as usize - 1).copied().unwrap_or(false);
            if free {
                d.go = v;
            }
        }
        if let Some(v) = p.youtube {
            d.youtube = v;
        }
        if let Some(v) = p.tiktok {
            d.tiktok = v;
            if v {
                d.vertical = true;
            }
        }
        if let Some(v) = p.people {
            let v = v.clamp(1, 5);
            if v != d.people {
                d.people = v;
                d.seats = default_seats(v);
            }
        }
        if let Some(mut v) = p.names {
            v.resize(5, String::new());
            d.names = v;
        }
        if let Some(v) = p.calls {
            d.calls = v.min(4);
        }
        if let Some(v) = p.agency {
            d.agency = v;
        }
        if let Some(v) = p.duplex {
            d.duplex = v;
        }
        if let Some(mut v) = p.seats {
            v.resize(5, None);
            d.seats = v;
        }
        if let Some(v) = p.screen {
            if v == "zone" || v == "only" {
                self.screen = v;
            }
        }
        if d.seats.len() != 5 {
            d.seats = default_seats(d.people);
        }
    }

    /// Takes a programme's answers back.
    pub fn take_programme(&mut self, p: &Programme) {
        let mut d = p.clone();
        d.names.resize(5, String::new());
        if d.seats.len() != 5 {
            d.seats = default_seats(d.people);
        }
        self.from_programme = Some(d.name.clone());
        self.draft = d;
    }

    /// What blocks going on from the current step, if anything.
    pub fn blocked(&self) -> Option<&'static str> {
        let d = &self.draft;
        match self.step {
            2 if self.checks.is_empty() => Some("vérification en cours"),
            2 if self.checks.iter().any(|c| c.state == "fail") => Some("un appareil ne répond pas"),
            3 if d.name.trim().is_empty() => Some("donnez un nom au programme"),
            4 if !(d.pgm || d.clean || d.iso || d.mics) => Some("choisissez au moins une image"),
            12 if self.tests.iter().any(|t| !t.done) => Some("tous les tests ne sont pas faits"),
            _ => None,
        }
    }

    /// The people placed: (seat 1 to 5, person's index, name).
    pub fn placed(&self) -> Vec<(u8, u8, String)> {
        let d = &self.draft;
        d.seats
            .iter()
            .enumerate()
            .filter_map(|(slot, p)| {
                let p = (*p)?;
                (p < d.people).then(|| {
                    let name = d
                        .names
                        .get(p as usize)
                        .map(|n| n.trim().to_string())
                        .unwrap_or_default();
                    let name = if name.is_empty() {
                        if p == 0 {
                            "Animateur".to_string()
                        } else {
                            format!("Invité {p}")
                        }
                    } else {
                        name
                    };
                    (slot as u8 + 1, p, name)
                })
            })
            .collect()
    }

    pub fn test_done(&mut self) {
        if let Some(t) = self.tests.get_mut(self.test_index) {
            t.done = true;
        }
        self.test_index = (self.test_index + 1).min(self.tests.len());
        self.hearing = Hearing::default();
        self.test_at = Some(Instant::now());
    }

    pub fn test_retry(&mut self) {
        let tries = self.hearing.tries + 1;
        self.hearing = Hearing {
            tries,
            ..Hearing::default()
        };
        self.test_at = Some(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_are_spread_around_the_table() {
        assert_eq!(
            default_seats(3),
            vec![Some(0), None, Some(1), None, Some(2)]
        );
        assert_eq!(default_seats(1), vec![None, None, Some(0), None, None]);
    }

    #[test]
    fn a_busy_go_cannot_be_chosen_and_tiktok_brings_the_9_16() {
        let mut s = Session::default();
        s.apply(
            DraftPatch {
                go: Some(2),
                ..Default::default()
            },
            &[true, false, true, true],
        );
        assert_eq!(s.draft.go, 0);
        s.apply(
            DraftPatch {
                go: Some(3),
                tiktok: Some(true),
                ..Default::default()
            },
            &[true, false, true, true],
        );
        assert_eq!(s.draft.go, 3);
        assert!(s.draft.vertical);
        s.apply(
            DraftPatch {
                vertical: Some(false),
                ..Default::default()
            },
            &[],
        );
        assert!(!s.draft.tiktok);
    }

    #[test]
    fn the_name_and_the_tests_block() {
        let mut s = Session {
            step: 3,
            ..Default::default()
        };
        assert!(s.blocked().is_some());
        s.apply(
            DraftPatch {
                name: Some("Tech Hebdo".into()),
                ..Default::default()
            },
            &[],
        );
        assert!(s.blocked().is_none());
        s.step = 12;
        s.tests = vec![Test {
            label: "Paul".into(),
            seat: Some(1),
            camera: Some(2),
            input: 2,
            source: "CAM2".into(),
            done: false,
        }];
        assert!(s.blocked().is_some());
        s.test_done();
        assert!(s.blocked().is_none());
    }

    #[test]
    fn a_voice_is_heard_or_too_low() {
        let mut h = Hearing::default();
        for _ in 0..30 {
            h.feed(-50.0, 50);
        }
        assert!(h.low && !h.heard);
        for _ in 0..25 {
            h.feed(-25.0, 50);
        }
        assert!(h.heard && !h.low);
    }

    #[test]
    fn placed_people_have_names() {
        let mut s = Session::default();
        s.apply(
            DraftPatch {
                people: Some(2),
                names: Some(vec!["Paul Martin".into(), "".into()]),
                ..Default::default()
            },
            &[],
        );
        assert_eq!(
            s.placed(),
            vec![(2, 0, "Paul Martin".into()), (4, 1, "Invité 1".into())]
        );
    }
}
