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
//!
//! **Spoken cues** ([`crate::speech`]) are intentions, not orders. A weak or
//! early one only prepares (the layout in the switcher, the host's camera);
//! a strong one announced for now is acted on at a natural moment: the end
//! of the host's sentence, or the first words of the person given the
//! floor. With a remote source on air, the picture then follows the
//! conversation: side by side while they talk together, the remote full
//! frame when it speaks long, the host in a box on a very long answer, and
//! back to the set when the host says goodbye and the remote is silent.

use crate::model::{
    CamId, Composite, RemoteId, RemoteKind, SeatId, Shot, ShotId, ShotKind, StudioSetup, Subject,
};
use crate::perception::Perception;
use crate::profile::Profile;
use crate::speech::{Hint, Layout, Target, When};
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
    Speaker {
        seat: SeatId,
    },
    Together {
        seats: Vec<SeatId>,
    },
    Silence,
    HeldTooLong,
    Forced,
    CameraLost,
    MaySpeak {
        seat: SeatId,
    },
    Fallback,
    /// The host announced the remote source.
    GoRemote {
        remote: RemoteId,
    },
    /// The remote source speaks, long enough to be seen alone.
    RemoteSpeaks {
        remote: RemoteId,
    },
    /// A very long answer: the host is shown listening.
    Listening {
        remote: RemoteId,
    },
    /// The remote source started speaking without being announced.
    RemoteUnannounced {
        remote: RemoteId,
    },
    /// The host said goodbye to the remote source.
    BackToSet,
    /// The remote source is gone.
    RemoteLost {
        remote: RemoteId,
    },
    /// The host gave the floor to this guest.
    Addressed {
        seat: SeatId,
    },
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
            Reason::GoRemote { remote } => {
                format!("l'animateur annonce {}", setup.remote_name(*remote))
            }
            Reason::RemoteSpeaks { remote } => {
                format!("{} parle : plein cadre", setup.remote_name(*remote))
            }
            Reason::Listening { remote } => format!(
                "longue réponse de {} : l'animateur écoute en vignette",
                setup.remote_name(*remote)
            ),
            Reason::RemoteUnannounced { remote } => {
                format!("{} prend la parole", setup.remote_name(*remote))
            }
            Reason::BackToSet => "l'animateur revient en plateau".into(),
            Reason::RemoteLost { remote } => {
                format!("{} perdu : retour en plateau", setup.remote_name(*remote))
            }
            Reason::Addressed { seat } => {
                format!("l'animateur donne la parole à {}", setup.seat_name(*seat))
            }
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
    /// Put a composite on the program (a remote alone, side by side, a box);
    /// `cameras` are the cameras showing the seats in it, in order.
    Compose {
        program: Composite,
        cameras: Vec<CamId>,
        reason: Reason,
    },
    /// Set a composite up off air (ATEM SuperSource, vMix layout), so that
    /// it is ready when it is taken.
    Preload { program: Composite, reason: Reason },
    /// A spoken cue was understood (for the journal).
    Understood { hint: Hint },
}

#[derive(Clone, Debug)]
struct Intent {
    hint: Hint,
    until_ms: u64,
}

fn seat_of(s: Subject) -> Option<SeatId> {
    match s {
        Subject::Seat(x) => Some(x),
        Subject::Remote(_) => None,
    }
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
    intents: Vec<Intent>,
    remote_voice_since: BTreeMap<RemoteId, u64>,
    remote_gone_since: BTreeMap<RemoteId, u64>,
    /// The composite on air, since when, and the cameras in it.
    composite: Option<(Composite, u64, Vec<CamId>)>,
    preloaded: Option<Composite>,
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
            intents: Vec::new(),
            remote_voice_since: BTreeMap::new(),
            remote_gone_since: BTreeMap::new(),
            composite: None,
            preloaded: None,
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

    /// The composite on air, if the program is not a single camera shot.
    pub fn composite(&self) -> Option<Composite> {
        self.composite.as_ref().map(|(c, _, _)| *c)
    }

    /// Called at each frame of the program.
    pub fn tick(&mut self, p: &Perception, c: &Control) -> Vec<Decision> {
        let t = p.t_ms;
        let mut out = Vec::new();
        self.listen(p, &mut out);
        self.hear(p, t, &mut out);

        if matches!(c.mode, Mode::Manual { .. }) {
            self.was_manual = true;
            self.expected = None;
            self.composite = None;
            return out;
        }
        // Coming back from a person's hand, or starting while the switcher
        // already shows something: carry on from the shot on air, no cut.
        if self.was_manual
            || (self.on_air.is_none() && self.composite.is_none() && p.program.is_some())
        {
            self.was_manual = false;
            if let Some(cam) = p.program {
                self.adopt(cam, p, t);
            }
        }
        if c.mode == Mode::Auto && self.composite.is_none() {
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
        if !self.direct_remote(p, c, t, &mut out) && !self.act_on_intents(p, c, t, &mut out) {
            self.direct(p, c, t, &mut out);
        }
        self.prepare_cameras(p, c, t, &mut out);
        out
    }

    /// The remote sources' voices and presence, and the cues heard.
    fn hear(&mut self, p: &Perception, t: u64, out: &mut Vec<Decision>) {
        for r in &p.remotes {
            if r.voice && r.online {
                self.remote_voice_since.entry(r.remote).or_insert(t);
            } else {
                self.remote_voice_since.remove(&r.remote);
            }
            if r.online {
                self.remote_gone_since.remove(&r.remote);
            } else {
                self.remote_gone_since.entry(r.remote).or_insert(t);
            }
        }
        for h in &p.hints {
            self.intents.retain(|i| i.hint.target != h.target);
            let life = match h.when {
                When::Now => self.profile.cue_life_ms,
                When::Soon => self.profile.cue_life_ms * 3,
                When::Later => self.profile.cue_life_ms * 18,
            };
            self.intents.push(Intent {
                hint: h.clone(),
                until_ms: t + life,
            });
            out.push(Decision::Understood { hint: h.clone() });
        }
        self.intents.retain(|i| t <= i.until_ms);
    }

    fn host(&self) -> Option<SeatId> {
        self.setup.host_seat()
    }

    /// The host has just finished a sentence.
    fn host_paused(&self, t: u64) -> bool {
        let Some(h) = self.host() else { return false };
        !self.voice_since.contains_key(&h)
            && self.last_voice.get(&h).is_some_and(|l| {
                let d = t.saturating_sub(*l);
                d >= self.profile.phrase_end_ms && d <= 4000
            })
    }

    /// How long the remote source has been speaking, once it is speech.
    fn remote_speaking(&self, r: RemoteId, t: u64) -> Option<u64> {
        self.remote_voice_since
            .get(&r)
            .map(|s| t.saturating_sub(*s))
            .filter(|d| *d >= self.profile.speech_onset_ms)
    }

    /// The person of the set speaking now, and for how long.
    fn seat_speaking(&self, t: u64) -> Option<(SeatId, u64)> {
        let s = match &self.situation {
            Situation::Speaking(s) => *s,
            Situation::Together(xs) => *xs.first()?,
            _ => return None,
        };
        Some((s, t.saturating_sub(*self.voice_since.get(&s)?)))
    }

    fn intent(&self, target: Target) -> Option<&Hint> {
        self.intents
            .iter()
            .find(|i| i.hint.target == target)
            .map(|i| &i.hint)
    }

    fn drop_intent(&mut self, target: Target) {
        self.intents.retain(|i| i.hint.target != target);
    }

    /// How the remote source is shown in the layout asked for.
    fn composite_for(&self, r: RemoteId, layout: Layout, with: Option<SeatId>) -> Composite {
        let seat = with.or(self.host()).unwrap_or(SeatId(1));
        match layout {
            Layout::Full => Composite::Full { remote: r },
            Layout::Split => Composite::Split {
                left: Subject::Seat(seat),
                right: Subject::Remote(r),
            },
            Layout::Pip => Composite::Pip {
                main: Subject::Remote(r),
                inset: Subject::Seat(seat),
            },
        }
    }

    /// The camera showing a seat in a composite: its close shot if one is
    /// framed, else the wide shot.
    fn camera_for(&self, seat: SeatId, p: &Perception, c: &Control) -> Option<CamId> {
        let single = self
            .setup
            .shots
            .iter()
            .find(|s| s.kind == ShotKind::Single(seat) && self.usable(s, p, c));
        let wide = self
            .setup
            .shots
            .iter()
            .find(|s| s.kind == ShotKind::Wide && self.usable(s, p, c));
        single.or(wide).map(|s| s.camera)
    }

    fn compose(
        &mut self,
        program: Composite,
        reason: Reason,
        p: &Perception,
        c: &Control,
        t: u64,
        out: &mut Vec<Decision>,
    ) {
        let seats: Vec<SeatId> = match program {
            Composite::Full { .. } => vec![],
            Composite::Split { left, right } => {
                [left, right].into_iter().filter_map(seat_of).collect()
            }
            Composite::Pip { main, inset } => {
                [main, inset].into_iter().filter_map(seat_of).collect()
            }
        };
        // A seat already shown keeps its camera.
        let kept = self
            .composite
            .as_ref()
            .map(|(_, _, cams)| cams.clone())
            .unwrap_or_default();
        let on_air_cam = self
            .on_air
            .and_then(|o| self.setup.shot(o.shot))
            .map(|s| (s.kind.clone(), s.camera));
        let cameras: Vec<CamId> = seats
            .iter()
            .filter_map(|seat| {
                if let Some((ShotKind::Single(x), cam)) = &on_air_cam {
                    if x == seat {
                        return Some(*cam);
                    }
                }
                kept.iter()
                    .copied()
                    .find(|cam| {
                        p.cam(*cam)
                            .and_then(|s| s.framed)
                            .and_then(|f| self.setup.shot(f))
                            .is_some_and(|s| s.kind == ShotKind::Single(*seat))
                    })
                    .or_else(|| self.camera_for(*seat, p, c))
            })
            .collect();
        self.composite = Some((program, t, cameras.clone()));
        self.on_air = None;
        self.expected = None;
        self.preloaded = None;
        out.push(Decision::Compose {
            program,
            cameras,
            reason,
        });
    }

    /// Back to the set: the person speaking, else the wide shot.
    fn leave_remote(
        &mut self,
        reason: Reason,
        p: &Perception,
        c: &Control,
        t: u64,
        out: &mut Vec<Decision>,
    ) {
        self.composite = None;
        self.preloaded = None;
        let speaking = self.seat_speaking(t).map(|(s, _)| s);
        let pick = |kind: ShotKind| {
            self.setup
                .shots
                .iter()
                .find(|s| s.kind == kind && self.usable(s, p, c))
                .cloned()
        };
        let shot = speaking
            .and_then(|s| pick(ShotKind::Single(s)))
            .or_else(|| pick(ShotKind::Wide))
            .or_else(|| {
                self.setup
                    .shots
                    .iter()
                    .find(|s| self.usable(s, p, c))
                    .cloned()
            });
        if let Some(shot) = shot {
            self.cut(&shot, reason, t, out);
        }
    }

    /// With the set on air: act on the cues heard, at the right moment.
    /// Returns whether a decision was taken.
    fn act_on_intents(
        &mut self,
        p: &Perception,
        c: &Control,
        t: u64,
        out: &mut Vec<Decision>,
    ) -> bool {
        let held = self
            .on_air
            .map_or(u64::MAX, |o| t.saturating_sub(o.since_ms));
        let online = |r: RemoteId| p.remote(r).is_some_and(|s| s.online);

        let best = self
            .intents
            .iter()
            .filter_map(|i| match i.hint.target {
                Target::Remote(r) if online(r) => Some((r, i.hint.clone())),
                _ => None,
            })
            .max_by(|a, b| {
                a.1.confidence
                    .partial_cmp(&b.1.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        if let Some((r, h)) = best {
            let comp = self.composite_for(r, h.layout.unwrap_or(Layout::Split), None);
            if h.when != When::Later && h.confidence >= 0.4 && self.preloaded != Some(comp) {
                self.preloaded = Some(comp);
                out.push(Decision::Preload {
                    program: comp,
                    reason: Reason::GoRemote { remote: r },
                });
            }
            let moment = self.host_paused(t) || self.remote_speaking(r, t).is_some();
            if h.when == When::Now
                && h.confidence >= 0.6
                && moment
                && held >= self.profile.min_shot_ms / 2
            {
                self.drop_intent(Target::Remote(r));
                self.compose(comp, Reason::GoRemote { remote: r }, p, c, t, out);
                return true;
            }
        }

        // A remote source speaking without having been announced.
        if self.seat_speaking(t).is_none() && held >= self.profile.min_shot_ms {
            let unannounced = p
                .remotes
                .iter()
                .filter(|r| r.online)
                .find(|r| {
                    self.remote_speaking(r.remote, t)
                        .is_some_and(|d| d >= 2 * self.profile.speech_onset_ms)
                })
                .map(|r| r.remote);
            if let Some(r) = unannounced {
                let layout = if self
                    .setup
                    .remote(r)
                    .is_some_and(|x| x.kind == RemoteKind::Agency)
                {
                    Layout::Full
                } else {
                    Layout::Split
                };
                let comp = self.composite_for(r, layout, None);
                self.compose(comp, Reason::RemoteUnannounced { remote: r }, p, c, t, out);
                return true;
            }
        }

        // The floor given to a guest: cut to them when the host stops.
        let addressed: Vec<SeatId> = self
            .intents
            .iter()
            .filter(|i| i.hint.when == When::Now && i.hint.confidence >= 0.6)
            .filter_map(|i| match i.hint.target {
                Target::Seat(s) => Some(s),
                _ => None,
            })
            .collect();
        for s in addressed {
            let on_air_kind = self
                .on_air
                .and_then(|o| self.setup.shot(o.shot))
                .map(|x| x.kind.clone());
            if on_air_kind == Some(ShotKind::Single(s)) || self.voice_since.contains_key(&s) {
                self.drop_intent(Target::Seat(s));
                continue;
            }
            if self.host_paused(t) && held >= self.profile.min_shot_ms * 6 / 10 {
                let shot = self
                    .setup
                    .shots
                    .iter()
                    .find(|x| x.kind == ShotKind::Single(s) && self.usable(x, p, c))
                    .cloned();
                if let Some(shot) = shot {
                    self.drop_intent(Target::Seat(s));
                    self.cut(&shot, Reason::Addressed { seat: s }, t, out);
                    return true;
                }
            }
        }
        false
    }

    /// With a remote source on air: follow the conversation. Returns
    /// whether the remote part of the show is on (then the set's own
    /// directing waits).
    fn direct_remote(
        &mut self,
        p: &Perception,
        c: &Control,
        t: u64,
        out: &mut Vec<Decision>,
    ) -> bool {
        let Some((comp, since, _)) = self.composite.clone() else {
            return false;
        };
        let Some(r) = comp.remote() else {
            self.composite = None;
            return false;
        };
        let held = t.saturating_sub(since);
        let kind = self.setup.remote(r).map(|x| x.kind);

        if self
            .remote_gone_since
            .get(&r)
            .is_some_and(|g| t.saturating_sub(*g) >= self.profile.remote_lost_ms)
        {
            self.leave_remote(Reason::RemoteLost { remote: r }, p, c, t, out);
            return true;
        }
        let seat_spk = self.seat_speaking(t);
        let remote_spk = self.remote_speaking(r, t);
        if self
            .intent(Target::Set)
            .is_some_and(|h| h.confidence >= 0.5)
        {
            let quiet = !self.remote_voice_since.contains_key(&r);
            if quiet
                && (self.host_paused(t) || seat_spk.is_some())
                && held >= self.profile.min_shot_ms / 2
            {
                self.drop_intent(Target::Set);
                self.leave_remote(Reason::BackToSet, p, c, t, out);
                return true;
            }
        }
        // Another source announced, or a layout asked for this one.
        let next = self
            .intents
            .iter()
            .filter(|i| i.hint.when == When::Now && i.hint.confidence >= 0.6)
            .find_map(|i| match i.hint.target {
                Target::Remote(x) if p.remote(x).is_some_and(|s| s.online) => {
                    Some((x, i.hint.layout))
                }
                _ => None,
            });
        if let Some((x, layout)) = next {
            if self.host_paused(t) || self.remote_speaking(x, t).is_some() {
                self.drop_intent(Target::Remote(x));
                let wanted = self.composite_for(x, layout.unwrap_or(Layout::Split), None);
                if wanted != comp {
                    self.compose(wanted, Reason::GoRemote { remote: x }, p, c, t, out);
                }
                return true;
            }
        }
        if held < self.profile.min_shot_ms {
            return true;
        }
        let split_with = |s: SeatId| Composite::Split {
            left: Subject::Seat(s),
            right: Subject::Remote(r),
        };
        let agency = kind == Some(RemoteKind::Agency);
        match comp {
            Composite::Split { left, .. } => {
                if let Some(d) = remote_spk {
                    if d >= self.profile.remote_full_ms && seat_spk.is_none() {
                        self.compose(
                            Composite::Full { remote: r },
                            Reason::RemoteSpeaks { remote: r },
                            p,
                            c,
                            t,
                            out,
                        );
                    }
                } else if let Some((s, _)) = seat_spk {
                    if left != Subject::Seat(s) {
                        self.compose(split_with(s), Reason::Speaker { seat: s }, p, c, t, out);
                    }
                }
            }
            Composite::Full { .. } => {
                if let Some((s, d)) = seat_spk {
                    if !agency {
                        self.compose(split_with(s), Reason::Speaker { seat: s }, p, c, t, out);
                    } else if d >= 8000 {
                        let pip = Composite::Pip {
                            main: Subject::Remote(r),
                            inset: Subject::Seat(s),
                        };
                        self.compose(pip, Reason::Speaker { seat: s }, p, c, t, out);
                    }
                } else if remote_spk.is_some() && held >= self.profile.max_shot_ms && !agency {
                    let host = self.host().unwrap_or(SeatId(1));
                    let pip = Composite::Pip {
                        main: Subject::Remote(r),
                        inset: Subject::Seat(host),
                    };
                    self.compose(pip, Reason::Listening { remote: r }, p, c, t, out);
                }
            }
            Composite::Pip { .. } => {
                if let Some((s, _)) = seat_spk {
                    if !agency {
                        self.compose(split_with(s), Reason::Speaker { seat: s }, p, c, t, out);
                    }
                } else if remote_spk.is_some() && held >= self.profile.max_shot_ms {
                    self.compose(
                        Composite::Full { remote: r },
                        Reason::RemoteSpeaks { remote: r },
                        p,
                        c,
                        t,
                        out,
                    );
                }
            }
        }
        true
    }

    /// How much the cues heard make a seat likely to be needed next.
    fn cue_boost(&self, seat: SeatId) -> f32 {
        let mut b: f32 = 0.0;
        for i in &self.intents {
            match i.hint.target {
                Target::Seat(s) if s == seat => b = b.max(i.hint.confidence),
                Target::Remote(_)
                    if Some(seat) == self.host()
                        && i.hint.layout != Some(Layout::Full)
                        && i.hint.when != When::Later =>
                {
                    b = b.max(0.9 * i.hint.confidence)
                }
                _ => {}
            }
        }
        b
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
        self.composite
            .as_ref()
            .is_some_and(|(_, _, cams)| cams.contains(&cam))
            || self
                .on_air
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
            let pa = p.next_probability(*a).max(self.cue_boost(*a));
            let pb = p.next_probability(*b).max(self.cue_boost(*b));
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
                remotes: vec![],
                hints: vec![],
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

    /// Spoken cues and remote sources.
    mod voice {
        use super::*;
        use crate::model::{Composite, RemoteId, Subject};
        use crate::perception::RemoteState;
        use crate::speech::{Heard, Listener};

        struct Scene {
            script: Script,
            /// When the duplex speaks (ms).
            duplex_voice: Vec<(u64, u64)>,
            /// When the duplex is gone (ms).
            duplex_off: Vec<(u64, u64)>,
            /// (ms, seat, text) heard.
            lines: Vec<(u64, u8, &'static str)>,
        }

        fn setup() -> StudioSetup {
            let mut s = StudioSetup::example();
            for (seat, n) in s
                .seats
                .iter_mut()
                .zip(["Paul Martin", "Alice Durand", "Karim Benali"])
            {
                seat.guest = Some(n.into());
            }
            s.remotes[1].guest = Some("Julie Roux".into());
            s
        }

        fn play(scene: &Scene, until_ms: u64) -> Vec<(u64, Decision)> {
            let mut plateau = Plateau::new();
            plateau.setup = setup();
            let mut director = Director::new(plateau.setup.clone(), Profile::podcast());
            let listener = Listener::new(&plateau.setup);
            let within =
                |spans: &Vec<(u64, u64)>, t: u64| spans.iter().any(|(a, b)| (*a..*b).contains(&t));
            let mut in_composite: Vec<CamId> = Vec::new();
            let mut log = Vec::new();
            for t in (0..until_ms).step_by(40) {
                let mut p = plateau.perceive(&scene.script, t);
                let on = !within(&scene.duplex_off, t);
                p.remotes = vec![
                    RemoteState {
                        remote: RemoteId(1),
                        online: true,
                        voice: false,
                    },
                    RemoteState {
                        remote: RemoteId(2),
                        online: on,
                        voice: on && within(&scene.duplex_voice, t),
                    },
                ];
                p.hints = scene
                    .lines
                    .iter()
                    .filter(|(at, _, _)| *at >= t && *at < t + 40)
                    .flat_map(|(at, seat, text)| {
                        listener.hear(&Heard {
                            t_ms: *at,
                            seat: SeatId(*seat),
                            text: (*text).into(),
                            is_final: true,
                        })
                    })
                    .collect();
                for d in director.tick(&p, &Control::default()) {
                    match &d {
                        Decision::Compose { cameras, .. } => {
                            plateau.program = None;
                            in_composite = cameras.clone();
                        }
                        Decision::Cut { .. } => in_composite.clear(),
                        Decision::Prepare { camera, .. } => {
                            assert!(
                                !in_composite.contains(camera),
                                "a camera in the composite was moved at {t}"
                            )
                        }
                        _ => {}
                    }
                    plateau.apply(&d, t);
                    log.push((t, d));
                }
            }
            log
        }

        fn composes(log: &[(u64, Decision)]) -> Vec<(u64, Composite, Reason)> {
            log.iter()
                .filter_map(|(t, d)| match d {
                    Decision::Compose {
                        program, reason, ..
                    } => Some((*t, *program, reason.clone())),
                    _ => None,
                })
                .collect()
        }

        fn the_duplex() -> Scene {
            Scene {
                script: Script::default()
                    .say(1, 2.0, 6.0)
                    .say(1, 20.0, 23.0)
                    .say(1, 30.0, 33.0),
                duplex_voice: vec![(7000, 19000), (23500, 30000)],
                duplex_off: vec![],
                lines: vec![
                    (
                        5500,
                        1,
                        "On retrouve tout de suite Julie en duplex à Lyon, bonjour Julie !",
                    ),
                    (31500, 1, "Merci Julie, on revient en plateau."),
                ],
            }
        }

        const SPLIT: Composite = Composite::Split {
            left: Subject::Seat(SeatId(1)),
            right: Subject::Remote(RemoteId(2)),
        };

        #[test]
        fn the_duplex_is_announced_then_followed() {
            let log = play(&the_duplex(), 34000);
            let c = composes(&log);
            assert!(
                log.iter().any(|(t, d)| *t < 6000
                    && matches!(d, Decision::Preload { program, .. } if *program == SPLIT)),
                "prepared while the host speaks"
            );
            let (t, comp, why) = &c[0];
            assert!(
                (6300..7000).contains(t),
                "side by side at the end of the host's sentence, not at {t}"
            );
            assert_eq!(*comp, SPLIT);
            assert_eq!(
                *why,
                Reason::GoRemote {
                    remote: RemoteId(2)
                }
            );

            let full = c
                .iter()
                .find(|(_, comp, _)| {
                    *comp
                        == Composite::Full {
                            remote: RemoteId(2),
                        }
                })
                .expect("Julie alone");
            assert!(
                (12900..13700).contains(&full.0),
                "full frame after a long answer, not at {}",
                full.0
            );
            let back = c
                .iter()
                .find(|(t, comp, _)| *t > 20000 && *comp == SPLIT)
                .expect("side by side again when the host speaks");
            assert!(back.0 < 21300, "{}", back.0);

            let home = log
                .iter()
                .find_map(|(t, d)| match d {
                    Decision::Cut {
                        shot,
                        reason: Reason::BackToSet,
                        ..
                    } => Some((*t, *shot)),
                    _ => None,
                })
                .expect("back to the set");
            assert!((31400..32200).contains(&home.0), "{}", home.0);
            assert_eq!(
                StudioSetup::example().shot(home.1).unwrap().kind,
                ShotKind::Single(SeatId(1))
            );
        }

        #[test]
        fn later_is_only_noted() {
            let scene = Scene {
                lines: vec![(5500, 1, "Tout à l'heure, on retrouvera Julie en duplex.")],
                duplex_voice: vec![],
                ..the_duplex()
            };
            let log = play(&scene, 15000);
            assert!(log
                .iter()
                .any(|(_, d)| matches!(d, Decision::Understood { .. })));
            assert!(composes(&log).is_empty());
            assert!(!log
                .iter()
                .any(|(_, d)| matches!(d, Decision::Preload { .. })));
        }

        #[test]
        fn a_denied_duplex_does_nothing() {
            let scene = Scene {
                lines: vec![(5500, 1, "Pas de duplex aujourd'hui.")],
                duplex_voice: vec![],
                ..the_duplex()
            };
            let log = play(&scene, 15000);
            assert!(!log
                .iter()
                .any(|(_, d)| matches!(d, Decision::Understood { .. } | Decision::Compose { .. })));
        }

        fn cut_to(log: &[(u64, Decision)], seat: u8) -> Option<(u64, Reason)> {
            log.iter().find_map(|(t, d)| match d {
                Decision::Cut { shot, reason, .. }
                    if StudioSetup::example().shot(*shot).unwrap().kind
                        == ShotKind::Single(SeatId(seat)) =>
                {
                    Some((*t, reason.clone()))
                }
                _ => None,
            })
        }

        #[test]
        fn a_guest_given_the_floor_is_shown_before_answering() {
            let base = Scene {
                script: Script::default().say(1, 2.0, 6.0).say(2, 7.0, 11.0),
                duplex_voice: vec![],
                duplex_off: vec![],
                lines: vec![],
            };
            let without = cut_to(&play(&base, 12000), 2).expect("Alice shown");
            let with = cut_to(
                &play(
                    &Scene {
                        lines: vec![(5500, 1, "Alice, qu'en pensez-vous ?")],
                        ..base
                    },
                    12000,
                ),
                2,
            )
            .expect("Alice shown");
            assert_eq!(with.1, Reason::Addressed { seat: SeatId(2) });
            assert!(
                with.0 < 7000,
                "on Alice before her first word, not at {}",
                with.0
            );
            assert!(
                without.0 >= 7700,
                "without the cue, only once she speaks ({})",
                without.0
            );
        }

        #[test]
        fn a_lost_duplex_goes_back_to_the_set() {
            let scene = Scene {
                duplex_off: vec![(9000, 30000)],
                ..the_duplex()
            };
            let log = play(&scene, 14000);
            let lost = log
                .iter()
                .find(|(_, d)| {
                    matches!(
                        d,
                        Decision::Cut {
                            reason: Reason::RemoteLost { .. },
                            ..
                        }
                    )
                })
                .expect("back to the set");
            assert!((10900..11500).contains(&lost.0), "{}", lost.0);
        }

        #[test]
        fn an_unannounced_remote_is_shown_when_it_speaks() {
            let scene = Scene {
                script: Script::default().say(1, 2.0, 6.0),
                duplex_voice: vec![(8000, 14000)],
                duplex_off: vec![],
                lines: vec![],
            };
            let c = composes(&play(&scene, 12000));
            let (t, comp, why) = &c[0];
            assert_eq!(
                *why,
                Reason::RemoteUnannounced {
                    remote: RemoteId(2)
                }
            );
            assert_eq!(*comp, SPLIT);
            assert!((9300..10000).contains(t), "{t}");
        }
    }
}
