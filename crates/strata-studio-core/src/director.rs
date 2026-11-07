//! The Director: from what Studio perceives to what the switcher and the
//! cameras do.
//!
//! Two stages:
//! 1. **Rules** say what is forbidden or required: a shot is held at least
//!    `min_shot_ms` and left after `max_shot_ms`; an interjection is not a
//!    speaker; a moving camera, a camera that is not framed on the shot, a
//!    locked camera and a banned shot are never taken; the camera on air is
//!    never moved.
//! 2. **Scores** rank the shots the rules allow: who speaks, who speaks
//!    together, the silence, the variety. The cut is made when a shot beats
//!    the one on air by the profile's hysteresis. Today the score is a
//!    formula; the learned model (trained on the directors' takeovers)
//!    will take its place behind the same rules.
//!
//! The PTZ cameras that are not on air are framed in advance on the people
//! most likely to speak next ([`Decision::Prepare`]): the signal goes
//! through the switcher, Studio cannot delay it, so the cut is on time only
//! if the shot is ready before the voice.
//!
//! A cut that the switcher shows and Studio did not ask for is a person's
//! hand on the panel: [`Decision::HumanTookOver`].

use crate::model::{CamId, SeatId, Shot, ShotId, ShotKind, StudioSetup};
use crate::perception::Perception;
use crate::profile::Profile;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Who switches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum Mode {
    /// The AI switches.
    Auto,
    /// The AI decides but nothing is sent: it is compared with a director.
    Shadow,
    /// A person has the hand; the AI only listens.
    Manual { by: String },
}

/// A shot imposed by a person until a time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Forced {
    pub shot: ShotId,
    pub until_ms: u64,
}

/// What the people in charge impose on the Director.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub mode: Mode,
    /// Cameras the AI neither takes nor moves.
    #[serde(default)]
    pub locked_cams: BTreeSet<CamId>,
    /// Shots the AI does not take.
    #[serde(default)]
    pub banned_shots: BTreeSet<ShotId>,
    #[serde(default)]
    pub forced: Option<Forced>,
}

impl Default for Control {
    fn default() -> Self {
        Control {
            mode: Mode::Auto,
            locked_cams: BTreeSet::new(),
            banned_shots: BTreeSet::new(),
            forced: None,
        }
    }
}

/// Why a decision was taken: shown to the people, kept in the journal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Reason {
    Opening,
    Speaker { seat: SeatId },
    Together { seats: Vec<SeatId> },
    Silence,
    HeldTooLong,
    Forced,
    CameraLost,
    MaySpeak { seat: SeatId },
    Fallback,
}

impl Reason {
    /// In words, with the names of the people.
    pub fn describe(&self, setup: &StudioSetup) -> String {
        match self {
            Reason::Opening => "ouverture : plan large".into(),
            Reason::Speaker { seat } => format!("{} parle", setup.seat_name(*seat)),
            Reason::Together { seats } => {
                let names: Vec<String> = seats.iter().map(|s| setup.seat_name(*s)).collect();
                format!("{} parlent ensemble", names.join(" et "))
            }
            Reason::Silence => "silence : plan large".into(),
            Reason::HeldTooLong => "plan tenu longtemps : plan de coupe".into(),
            Reason::Forced => "plan imposé".into(),
            Reason::CameraLost => "caméra perdue ou verrouillée".into(),
            Reason::MaySpeak { seat } => format!("{} pourrait parler", setup.seat_name(*seat)),
            Reason::Fallback => "plan de repli prêt".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Decision {
    /// Put this shot on the program.
    Cut {
        shot: ShotId,
        camera: CamId,
        reason: Reason,
    },
    /// Frame a camera that is not on air (recall a PTZ preset).
    Prepare {
        camera: CamId,
        shot: ShotId,
        preset: u8,
        reason: Reason,
    },
    /// The person speaking changed (for the graphics: name, speaking time).
    Speaker { seat: Option<SeatId> },
    /// The switcher shows a camera Studio did not ask for.
    HumanTookOver { program: Option<CamId> },
}

#[derive(Clone, Debug, PartialEq)]
enum Situation {
    /// Nobody has spoken yet.
    Opening,
    Speaking(SeatId),
    Together(Vec<SeatId>),
    /// Nobody speaks, for this long.
    Silence(u64),
}

#[derive(Clone, Copy, Debug)]
struct OnAir {
    shot: ShotId,
    since_ms: u64,
}

/// A PTZ preset recall that is pending: when it was asked.
const PREPARE_RETRY_MS: u64 = 3000;
/// Taking a shot taken recently costs this much.
const VARIETY_PENALTY: f32 = 0.15;

pub struct Director {
    setup: StudioSetup,
    profile: Profile,
    on_air: Option<OnAir>,
    /// The camera last cut to and when: the switcher must show it.
    expected: Option<(CamId, u64)>,
    voice_since: BTreeMap<SeatId, u64>,
    last_voice: BTreeMap<SeatId, u64>,
    last_any_voice: Option<u64>,
    speaker: Option<SeatId>,
    situation: Situation,
    recent: VecDeque<ShotId>,
    prepared: BTreeMap<CamId, (ShotId, u64)>,
    was_manual: bool,
}

impl Director {
    pub fn new(setup: StudioSetup, profile: Profile) -> Self {
        Director {
            setup,
            profile,
            on_air: None,
            expected: None,
            voice_since: BTreeMap::new(),
            last_voice: BTreeMap::new(),
            last_any_voice: None,
            speaker: None,
            situation: Situation::Opening,
            recent: VecDeque::new(),
            prepared: BTreeMap::new(),
            was_manual: false,
        }
    }

    pub fn setup(&self) -> &StudioSetup {
        &self.setup
    }

    /// The guests' names change from one show to the next.
    pub fn setup_mut(&mut self) -> &mut StudioSetup {
        &mut self.setup
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn set_profile(&mut self, profile: Profile) {
        self.profile = profile;
    }

    /// The shot the Director has on air (in shadow mode, the one it would have).
    pub fn on_air(&self) -> Option<ShotId> {
        self.on_air.map(|o| o.shot)
    }

    pub fn speaker(&self) -> Option<SeatId> {
        self.speaker
    }

    /// Called at each frame of the program.
    pub fn tick(&mut self, p: &Perception, c: &Control) -> Vec<Decision> {
        let t = p.t_ms;
        let mut out = Vec::new();
        self.listen(p, &mut out);

        if matches!(c.mode, Mode::Manual { .. }) {
            self.was_manual = true;
            self.expected = None;
            return out;
        }
        // Coming back from a person's hand, or starting while the switcher
        // already shows something: carry on from the shot on air, no cut.
        if self.was_manual || (self.on_air.is_none() && p.program.is_some()) {
            self.was_manual = false;
            if let Some(cam) = p.program {
                self.adopt(cam, p, t);
            }
        }
        if c.mode == Mode::Auto {
            if let (Some((cam, at)), Some(program)) = (self.expected, p.program) {
                if program != cam && t.saturating_sub(at) >= self.profile.human_cut_grace_ms {
                    self.expected = None;
                    self.was_manual = true;
                    out.push(Decision::HumanTookOver {
                        program: Some(program),
                    });
                    return out;
                }
            }
        }

        if let Some(f) = c.forced.as_ref().filter(|f| t < f.until_ms) {
            self.force(f.shot, p, t, &mut out);
            return out;
        }
        self.direct(p, c, t, &mut out);
        self.prepare_cameras(p, c, t, &mut out);
        out
    }

    /// Who has a voice, who speaks, together or not.
    fn listen(&mut self, p: &Perception, out: &mut Vec<Decision>) {
        let t = p.t_ms;
        for m in &p.mics {
            if m.voice {
                self.voice_since.entry(m.seat).or_insert(t);
                self.last_voice.insert(m.seat, t);
                self.last_any_voice = Some(t);
            } else {
                self.voice_since.remove(&m.seat);
            }
        }
        let mut established: Vec<(SeatId, u64)> = self
            .voice_since
            .iter()
            .map(|(s, since)| (*s, t.saturating_sub(*since)))
            .filter(|(_, d)| *d >= self.profile.speech_onset_ms)
            .collect();
        established.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        self.situation = match established.len() {
            0 => match self.last_any_voice {
                None => Situation::Opening,
                Some(l) => Situation::Silence(t.saturating_sub(l)),
            },
            1 => Situation::Speaking(established[0].0),
            _ if established[1].1 >= self.profile.overlap_ms => {
                let mut seats: Vec<SeatId> = established.iter().map(|(s, _)| *s).collect();
                seats.sort();
                Situation::Together(seats)
            }
            _ => match self
                .speaker
                .filter(|s| established.iter().any(|(e, _)| e == s))
            {
                Some(s) => Situation::Speaking(s),
                None => Situation::Speaking(established[0].0),
            },
        };
        if let Situation::Speaking(s) = self.situation {
            if self.speaker != Some(s) {
                self.speaker = Some(s);
                out.push(Decision::Speaker { seat: Some(s) });
            }
        }
    }

    fn adopt(&mut self, cam: CamId, p: &Perception, t: u64) {
        let framed = p
            .cam(cam)
            .and_then(|c| c.framed)
            .filter(|s| self.setup.shot(*s).is_some_and(|s| s.camera == cam));
        let shot = framed.or_else(|| {
            self.setup
                .shots
                .iter()
                .find(|s| s.camera == cam)
                .map(|s| s.id)
        });
        if let Some(shot) = shot {
            self.on_air = Some(OnAir { shot, since_ms: t });
            self.expected = Some((cam, t));
        }
    }

    fn usable(&self, shot: &Shot, p: &Perception, c: &Control) -> bool {
        let Some(cam) = p.cam(shot.camera) else {
            return false;
        };
        cam.online
            && !cam.moving
            && !c.locked_cams.contains(&shot.camera)
            && !c.banned_shots.contains(&shot.id)
            && (shot.preset.is_none() || cam.framed == Some(shot.id))
    }

    fn score(&self, shot: &Shot) -> f32 {
        let base = match (&shot.kind, &self.situation) {
            (ShotKind::Wide, Situation::Opening) => 1.0,
            (_, Situation::Opening) => 0.2,
            (ShotKind::Single(s), Situation::Speaking(x)) => {
                if s == x {
                    1.0
                } else {
                    0.25
                }
            }
            (ShotKind::Single(s), Situation::Together(xs)) => {
                if xs.contains(s) {
                    0.3
                } else {
                    0.1
                }
            }
            (ShotKind::Single(_), Situation::Silence(_)) => 0.2,
            (ShotKind::Group(g), Situation::Speaking(x)) => {
                if g.contains(x) {
                    0.6
                } else {
                    0.2
                }
            }
            (ShotKind::Group(g), Situation::Together(xs)) => {
                let n = xs.iter().filter(|x| g.contains(x)).count();
                if n == xs.len() {
                    1.0
                } else {
                    0.5 * n as f32 / xs.len() as f32
                }
            }
            (ShotKind::Group(_), Situation::Silence(_)) => 0.4,
            (ShotKind::Wide, Situation::Speaking(_)) => 0.4,
            (ShotKind::Wide, Situation::Together(_)) => 0.8,
            (ShotKind::Wide, Situation::Silence(ms)) => {
                if *ms >= self.profile.silence_to_wide_ms {
                    0.9
                } else {
                    0.35
                }
            }
        };
        let current = self.on_air.map(|o| o.shot);
        if Some(shot.id) != current && self.recent.contains(&shot.id) {
            base - VARIETY_PENALTY
        } else {
            base
        }
    }

    fn situation_reason(&self) -> Reason {
        match &self.situation {
            Situation::Opening => Reason::Opening,
            Situation::Speaking(s) => Reason::Speaker { seat: *s },
            Situation::Together(xs) => Reason::Together { seats: xs.clone() },
            Situation::Silence(_) => Reason::Silence,
        }
    }

    fn direct(&mut self, p: &Perception, c: &Control, t: u64, out: &mut Vec<Decision>) {
        let current = self.on_air.and_then(|o| self.setup.shot(o.shot)).cloned();
        let held = self
            .on_air
            .map_or(u64::MAX, |o| t.saturating_sub(o.since_ms));
        let (must_leave, lost) = match &current {
            None => (true, false),
            Some(s) => {
                let online = p.cam(s.camera).is_some_and(|c| c.online);
                let allowed = !c.locked_cams.contains(&s.camera) && !c.banned_shots.contains(&s.id);
                (!online || !allowed, !online || !allowed)
            }
        };
        let too_long = current.is_some() && held >= self.profile.max_shot_ms;
        if !must_leave && held < self.profile.min_shot_ms {
            return;
        }
        if !must_leave && !too_long {
            if let Situation::Silence(ms) = self.situation {
                if ms < self.profile.silence_to_wide_ms {
                    return;
                }
            }
        }

        // A cutaway leaves the person on air: not them again from elsewhere.
        let away_from = match (&current, too_long) {
            (
                Some(Shot {
                    kind: ShotKind::Single(s),
                    ..
                }),
                true,
            ) => Some(*s),
            _ => None,
        };
        let best = self
            .setup
            .shots
            .iter()
            .filter(|s| Some(s.id) != current.as_ref().map(|c| c.id))
            .filter(|s| self.usable(s, p, c))
            .filter(|s| !matches!((&s.kind, away_from), (ShotKind::Single(x), Some(a)) if *x == a))
            .map(|s| (self.score(s), s))
            .fold(None::<(f32, &Shot)>, |acc, (score, s)| match acc {
                Some((b, _)) if b >= score => acc,
                _ => Some((score, s)),
            });
        let Some((best_score, best)) = best else {
            return;
        };
        let best = best.clone();

        let leave = must_leave || too_long;
        let current_score = current
            .as_ref()
            .map_or(f32::NEG_INFINITY, |s| self.score(s));
        if !leave && best_score < current_score + self.profile.hysteresis {
            return;
        }
        // A shot left for being held too long is a cutaway, unless the shot
        // taken is the person who now speaks.
        let to_speaker = matches!((&self.situation, &best.kind), (Situation::Speaking(s), ShotKind::Single(x)) if s == x);
        let reason = if current.is_none() && matches!(self.situation, Situation::Opening) {
            Reason::Opening
        } else if lost {
            Reason::CameraLost
        } else if too_long && !to_speaker {
            Reason::HeldTooLong
        } else {
            self.situation_reason()
        };
        self.cut(&best, reason, t, out);
    }

    fn force(&mut self, shot: ShotId, p: &Perception, t: u64, out: &mut Vec<Decision>) {
        if self.on_air() == Some(shot) {
            return;
        }
        let Some(shot) = self.setup.shot(shot).cloned() else {
            return;
        };
        let Some(cam) = p.cam(shot.camera) else {
            return;
        };
        if !cam.online {
            return;
        }
        if shot.preset.is_none() || (cam.framed == Some(shot.id) && !cam.moving) {
            self.cut(&shot, Reason::Forced, t, out);
        } else if !self.is_on_air_camera(shot.camera) {
            self.prepare(&shot, Reason::Forced, p, t, out);
        }
    }

    fn cut(&mut self, shot: &Shot, reason: Reason, t: u64, out: &mut Vec<Decision>) {
        self.on_air = Some(OnAir {
            shot: shot.id,
            since_ms: t,
        });
        self.expected = Some((shot.camera, t));
        self.recent.push_back(shot.id);
        while self.recent.len() > self.profile.variety_memory {
            self.recent.pop_front();
        }
        out.push(Decision::Cut {
            shot: shot.id,
            camera: shot.camera,
            reason,
        });
    }

    fn is_on_air_camera(&self, cam: CamId) -> bool {
        self.on_air
            .and_then(|o| self.setup.shot(o.shot))
            .is_some_and(|s| s.camera == cam)
    }

    fn prepare(
        &mut self,
        shot: &Shot,
        reason: Reason,
        p: &Perception,
        t: u64,
        out: &mut Vec<Decision>,
    ) {
        let Some(preset) = shot.preset else { return };
        if self.is_on_air_camera(shot.camera) {
            return;
        }
        let state = p.cam(shot.camera);
        if state.is_some_and(|c| c.framed == Some(shot.id) && !c.moving) {
            self.prepared.insert(shot.camera, (shot.id, t));
            return;
        }
        if let Some((s, at)) = self.prepared.get(&shot.camera) {
            let waiting =
                state.is_some_and(|c| c.moving) || t.saturating_sub(*at) < PREPARE_RETRY_MS;
            if *s == shot.id && waiting {
                return;
            }
        }
        self.prepared.insert(shot.camera, (shot.id, t));
        out.push(Decision::Prepare {
            camera: shot.camera,
            shot: shot.id,
            preset,
            reason,
        });
    }

    /// The shot a camera is on, or on its way to.
    fn aimed(&self, cam: CamId, p: &Perception) -> Option<ShotId> {
        self.prepared
            .get(&cam)
            .map(|(s, _)| *s)
            .or_else(|| p.cam(cam).and_then(|c| c.framed))
    }

    /// Frame the free PTZ cameras on the people most likely to speak next;
    /// a camera with nobody to wait for goes back to the shot showing those
    /// speaking together, else to the wide shot.
    fn prepare_cameras(&mut self, p: &Perception, c: &Control, t: u64, out: &mut Vec<Decision>) {
        let on_air_single = match self
            .on_air
            .and_then(|o| self.setup.shot(o.shot))
            .map(|s| &s.kind)
        {
            Some(ShotKind::Single(s)) => Some(*s),
            _ => None,
        };
        let free: Vec<CamId> = self
            .setup
            .cameras
            .iter()
            .filter(|cam| {
                cam.model.is_ptz()
                    && !self.is_on_air_camera(cam.id)
                    && !c.locked_cams.contains(&cam.id)
            })
            .filter(|cam| p.cam(cam.id).is_some_and(|s| s.online))
            .map(|cam| cam.id)
            .collect();
        if free.is_empty() {
            return;
        }
        let single_on = |cam: CamId, seat: SeatId| {
            self.setup
                .shots
                .iter()
                .find(|s| {
                    s.camera == cam
                        && s.kind == ShotKind::Single(seat)
                        && !c.banned_shots.contains(&s.id)
                })
                .cloned()
        };

        let mut ranked: Vec<SeatId> = self
            .setup
            .seats
            .iter()
            .map(|s| s.id)
            .filter(|s| Some(*s) != on_air_single)
            .collect();
        ranked.sort_by(|a, b| {
            let pa = p.next_probability(*a);
            let pb = p.next_probability(*b);
            pb.partial_cmp(&pa)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| self.last_voice.get(b).cmp(&self.last_voice.get(a)))
                .then_with(|| a.cmp(b))
        });
        let singles_cams = free
            .iter()
            .filter(|cam| {
                self.setup
                    .seats
                    .iter()
                    .any(|s| single_on(**cam, s.id).is_some())
            })
            .count();
        let needed: Vec<SeatId> = ranked.into_iter().take(singles_cams).collect();

        let mut assigned: BTreeMap<CamId, SeatId> = BTreeMap::new();
        for cam in &free {
            let kind = self
                .aimed(*cam, p)
                .and_then(|s| self.setup.shot(s))
                .map(|s| s.kind.clone());
            if let Some(ShotKind::Single(seat)) = kind {
                if needed.contains(&seat) && !assigned.values().any(|s| *s == seat) {
                    assigned.insert(*cam, seat);
                }
            }
        }
        let mut moves = Vec::new();
        for seat in &needed {
            if assigned.values().any(|s| s == seat) {
                continue;
            }
            let cam = free
                .iter()
                .find(|cam| !assigned.contains_key(cam) && single_on(**cam, *seat).is_some());
            if let Some(cam) = cam {
                assigned.insert(*cam, *seat);
                if let Some(shot) = single_on(*cam, *seat) {
                    moves.push((shot, Reason::MaySpeak { seat: *seat }));
                }
            }
        }
        for cam in free.iter().filter(|cam| !assigned.contains_key(cam)) {
            let wide = self
                .setup
                .shots
                .iter()
                .find(|s| s.camera == *cam && s.kind == ShotKind::Wide);
            // The wide shot is the fallback: a camera does not leave it
            // while no other camera has it.
            let wide_elsewhere = self.setup.shots.iter().any(|s| {
                s.kind == ShotKind::Wide
                    && s.camera != *cam
                    && (self.is_on_air_camera(s.camera) || self.aimed(s.camera, p) == Some(s.id))
            });
            let together = match &self.situation {
                Situation::Together(xs) => self.setup.shots.iter().find(|s| {
                    s.camera == *cam
                        && matches!(&s.kind, ShotKind::Group(g) if xs.iter().all(|x| g.contains(x)))
                }),
                _ => None,
            };
            let fallback = match wide {
                Some(w) if !wide_elsewhere => Some(w),
                _ => together.or(wide),
            };
            if let Some(shot) = fallback.filter(|s| !c.banned_shots.contains(&s.id)) {
                let reason = match &self.situation {
                    Situation::Together(xs) if shot.kind != ShotKind::Wide => {
                        Reason::Together { seats: xs.clone() }
                    }
                    _ => Reason::Fallback,
                };
                moves.push((shot.clone(), reason));
            }
        }
        for (shot, reason) in moves {
            self.prepare(&shot, reason, p, t, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::CamState;
    use crate::script::Script;

    /// A studio that obeys: the switcher shows what is cut to, a PTZ moves
    /// for a second then is framed on the preset recalled.
    struct Plateau {
        setup: StudioSetup,
        program: Option<CamId>,
        framed: BTreeMap<CamId, ShotId>,
        moving_until: BTreeMap<CamId, (u64, ShotId)>,
        online: BTreeMap<CamId, bool>,
    }

    impl Plateau {
        fn new() -> Self {
            let setup = StudioSetup::example();
            let framed = [(1, 100), (2, 201), (3, 302), (4, 403)]
                .into_iter()
                .map(|(c, s)| (CamId(c), ShotId(s)))
                .collect();
            let online = setup.cameras.iter().map(|c| (c.id, true)).collect();
            Plateau {
                setup,
                program: None,
                framed,
                moving_until: BTreeMap::new(),
                online,
            }
        }

        fn perceive(&mut self, script: &Script, t: u64) -> Perception {
            self.moving_until.retain(|cam, (until, shot)| {
                if t >= *until {
                    self.framed.insert(*cam, *shot);
                    false
                } else {
                    true
                }
            });
            Perception {
                t_ms: t,
                mics: script.mics(self.setup.seats.iter().map(|s| s.id), t),
                cams: self
                    .setup
                    .cameras
                    .iter()
                    .map(|c| CamState {
                        cam: c.id,
                        online: self.online[&c.id],
                        moving: self.moving_until.contains_key(&c.id),
                        framed: if self.moving_until.contains_key(&c.id) {
                            None
                        } else {
                            self.framed.get(&c.id).copied()
                        },
                    })
                    .collect(),
                program: self.program,
                next_speaker: vec![],
            }
        }

        fn apply(&mut self, d: &Decision, t: u64) {
            match d {
                Decision::Cut { camera, .. } => self.program = Some(*camera),
                Decision::Prepare { camera, shot, .. } => {
                    assert_ne!(Some(*camera), self.program, "a camera on air was moved");
                    self.moving_until.insert(*camera, (t + 1000, *shot));
                }
                _ => {}
            }
        }
    }

    /// Runs the script; returns (t, decision) for every decision.
    fn run(
        script: &Script,
        until_s: u64,
        control: impl Fn(u64) -> Control,
    ) -> Vec<(u64, Decision)> {
        let mut plateau = Plateau::new();
        let mut director = Director::new(plateau.setup.clone(), Profile::podcast());
        let mut log = Vec::new();
        for t in (0..until_s * 1000).step_by(40) {
            let p = plateau.perceive(script, t);
            let c = control(t);
            for d in director.tick(&p, &c) {
                if c.mode == Mode::Auto {
                    plateau.apply(&d, t);
                }
                log.push((t, d));
            }
        }
        log
    }

    fn cuts(log: &[(u64, Decision)]) -> Vec<(u64, ShotId, Reason)> {
        log.iter()
            .filter_map(|(t, d)| match d {
                Decision::Cut { shot, reason, .. } => Some((*t, *shot, reason.clone())),
                _ => None,
            })
            .collect()
    }

    fn auto(_: u64) -> Control {
        Control::default()
    }

    fn kind(shot: ShotId) -> ShotKind {
        StudioSetup::example().shot(shot).unwrap().kind.clone()
    }

    #[test]
    fn opens_on_the_wide_shot() {
        let log = run(&Script::demo(), 1, auto);
        let c = cuts(&log);
        assert_eq!(c[0].0, 0);
        assert_eq!(kind(c[0].1), ShotKind::Wide);
        assert_eq!(c[0].2, Reason::Opening);
    }

    #[test]
    fn cuts_to_the_speaker_once_their_speech_is_established() {
        let c = cuts(&run(&Script::demo(), 30, auto));
        let first = c
            .iter()
            .find(|(_, s, _)| kind(*s) == ShotKind::Single(SeatId(3)))
            .expect("seat 3 shown");
        let onset = Profile::podcast().speech_onset_ms;
        assert!(
            first.0 >= 10_000 + onset && first.0 <= 10_000 + onset + 1500,
            "cut at {}",
            first.0
        );
        assert_eq!(first.2, Reason::Speaker { seat: SeatId(3) });
    }

    #[test]
    fn an_interjection_is_not_a_speaker() {
        let c = cuts(&run(&Script::demo(), 24, auto));
        assert!(!c.iter().any(
            |(t, s, _)| (15_000..17_000).contains(t) && kind(*s) == ShotKind::Single(SeatId(1))
        ));
    }

    #[test]
    fn shots_are_held_their_minimum() {
        let c = cuts(&run(&Script::demo(), 125, auto));
        assert!(c.len() > 8, "{c:?}");
        for w in c.windows(2) {
            assert!(
                w[1].0 - w[0].0 >= Profile::podcast().min_shot_ms,
                "{:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn two_speaking_together_get_a_shot_showing_both() {
        let c = cuts(&run(&Script::demo(), 46, auto));
        let together = c
            .iter()
            .find(|(t, _, r)| (39_000..43_000).contains(t) && matches!(r, Reason::Together { .. }));
        let (_, shot, _) = together.expect("a cut for seats 4 and 5 together");
        let k = kind(*shot);
        assert!(k.shows(SeatId(4)) && k.shows(SeatId(5)), "{k:?}");
    }

    #[test]
    fn a_long_silence_goes_wide() {
        let c = cuts(&run(&Script::demo(), 63, auto));
        let wide = c
            .iter()
            .find(|(t, _, r)| (60_000..63_000).contains(t) && *r == Reason::Silence);
        assert_eq!(kind(wide.expect("wide on silence").1), ShotKind::Wide);
    }

    #[test]
    fn a_long_answer_gets_a_cutaway_then_the_speaker_again() {
        let c = cuts(&run(&Script::demo(), 90, auto));
        let away = c
            .iter()
            .position(|(t, _, r)| *t > 63_000 && *r == Reason::HeldTooLong)
            .expect("a cutaway");
        assert_ne!(kind(c[away].1), ShotKind::Single(SeatId(5)));
        let back = &c[away + 1];
        assert!(back.0 < 90_000);
        assert_eq!(kind(back.1), ShotKind::Single(SeatId(5)));
    }

    #[test]
    fn a_cut_to_the_person_now_speaking_says_so_even_after_a_long_shot() {
        // Seat 3 is on air from about 10.7 s; seat 1 speaks at 25 s, when
        // that shot has been held its maximum.
        let c = cuts(&run(&Script::demo(), 28, auto));
        let to_host = c
            .iter()
            .find(|(t, s, _)| *t > 25_000 && kind(*s) == ShotKind::Single(SeatId(1)))
            .expect("seat 1 shown");
        assert_eq!(to_host.2, Reason::Speaker { seat: SeatId(1) });
    }

    #[test]
    fn the_only_wide_camera_keeps_the_wide_shot() {
        let log = run(&Script::demo(), 125, auto);
        assert!(!log.iter().any(|(_, d)| matches!(d, Decision::Prepare { camera: CamId(1), shot, .. } if *shot != ShotId(100))));
    }

    #[test]
    fn a_locked_camera_is_neither_taken_nor_moved() {
        let lock = |_| Control {
            locked_cams: [CamId(4)].into(),
            ..Control::default()
        };
        let log = run(&Script::demo(), 125, lock);
        for (_, d) in &log {
            match d {
                Decision::Cut { camera, .. } | Decision::Prepare { camera, .. } => {
                    assert_ne!(*camera, CamId(4))
                }
                _ => {}
            }
        }
    }

    #[test]
    fn the_speaker_is_announced() {
        let log = run(&Script::demo(), 32, auto);
        let speakers: Vec<SeatId> = log
            .iter()
            .filter_map(|(_, d)| match d {
                Decision::Speaker { seat: Some(s) } => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(speakers, vec![SeatId(1), SeatId(3), SeatId(1), SeatId(4)]);
    }

    #[test]
    fn a_cut_made_on_the_panel_is_a_takeover() {
        let mut plateau = Plateau::new();
        let mut director = Director::new(plateau.setup.clone(), Profile::podcast());
        let script = Script::demo();
        let c = Control::default();
        let mut took_over = None;
        for t in (0..20_000).step_by(40) {
            if t == 12_000 {
                plateau.program = Some(CamId(2)); // a person cuts on the panel
            }
            let p = plateau.perceive(&script, t);
            for d in director.tick(&p, &c) {
                if let Decision::HumanTookOver { program } = d {
                    took_over.get_or_insert((t, program));
                }
                plateau.apply(&d, t);
            }
        }
        let (t, program) = took_over.expect("the takeover is seen");
        assert!((12_000..12_000 + 600).contains(&t), "seen at {t}");
        assert_eq!(program, Some(CamId(2)));
    }

    #[test]
    fn handing_back_carries_on_from_the_shot_on_air() {
        let manual = |t: u64| Control {
            mode: if t < 20_000 {
                Mode::Manual { by: "Paul".into() }
            } else {
                Mode::Auto
            },
            ..Control::default()
        };
        let mut plateau = Plateau::new();
        plateau.program = Some(CamId(2));
        let mut director = Director::new(plateau.setup.clone(), Profile::podcast());
        let script = Script::demo();
        let mut first_cut = None;
        for t in (0..30_000).step_by(40) {
            let p = plateau.perceive(&script, t);
            let c = manual(t);
            for d in director.tick(&p, &c) {
                if t < 20_000 {
                    assert!(
                        matches!(d, Decision::Speaker { .. }),
                        "the AI acted by hand's time: {d:?}"
                    );
                }
                if matches!(d, Decision::Cut { .. }) {
                    first_cut.get_or_insert(t);
                }
                if c.mode == Mode::Auto {
                    plateau.apply(&d, t);
                }
            }
        }
        assert!(
            first_cut.is_some_and(|t| t >= 20_000 + Profile::podcast().min_shot_ms),
            "{first_cut:?}"
        );
    }

    #[test]
    fn the_same_show_gives_the_same_decisions() {
        assert_eq!(
            run(&Script::demo(), 125, auto),
            run(&Script::demo(), 125, auto)
        );
    }
}
