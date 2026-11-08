//! What the devices do along the journey: the studio woken up and
//! checked, the guests invited, each person tested, the shoot launched
//! (recording, GO, YouTube, TikTok, AI) and ended (files, 9:16, copy,
//! studio put to sleep), and what the shoot leaves in the history.
//!
//! The slow requests (encoders, Quicklink, a scenario's waits) never run
//! under the studio's lock: they are [`Job`]s, carried out by threads.

use crate::deck::{self, Act, DeckView};
use crate::history::{self, Note, PerceptionLog, Said, ShootRecord, Speech};
use crate::record::{self, AfterState, RecordRequest};
use crate::scenario::{self, Context, Event, What};
use crate::session::{Check, DraftPatch, Ended, GuestView, Test};
use crate::{App, Shared};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use strata_studio_core::{Listener, Mode, Remote, RemoteId, RemoteKind, SeatId, ShotKind};
use strata_studio_devices::quicklink::{Calls, GuestState};
use strata_studio_devices::webencoder::{GoEncoder, GoState};
use strata_studio_media::encoder::Resolution;
use strata_studio_media::export::Segment;
use strata_studio_media::live::{Live, LiveState};

/// The devices behind slow network requests, kept apart from the studio's
/// lock.
pub struct Ext {
    pub gos: Vec<Box<dyn GoEncoder>>,
    pub calls: Box<dyn Calls>,
}

pub type SharedExt = Arc<Mutex<Ext>>;

/// What those devices last said.
#[derive(Clone, Debug, Default)]
pub struct ExtView {
    pub gos: Vec<GoState>,
}

/// Work for a thread.
#[derive(Clone, Debug)]
pub enum Job {
    Checks,
    CreateGuests {
        from: usize,
        to: usize,
        start: String,
        end: String,
    },
    GoStart(usize),
    GoStop(usize),
    Run(Vec<What>),
}

/// The shoot being recorded into the history.
pub struct Capture {
    pub record: ShootRecord,
    pub t0: u64,
    pub log: Option<PerceptionLog>,
    pub last_source: Option<String>,
    pub speaker: Option<(String, u64)>,
    pub last_human_program: Option<u16>,
    pub ai_cuts: Vec<u64>,
    pub alerted: bool,
}

/// A live stream: pushed by ffmpeg, or simulated (the simulated studio
/// without a platform's key).
pub enum LiveRun {
    Real(Live),
    Sim(Instant),
}

impl LiveRun {
    pub fn state(&mut self) -> LiveState {
        match self {
            LiveRun::Real(l) => l.state(),
            LiveRun::Sim(t) => LiveState::Live {
                seconds: t.elapsed().as_secs(),
            },
        }
    }
    pub fn stop(self) {
        if let LiveRun::Real(l) = self {
            l.stop();
        }
    }
}

/// Cached disk space.
#[derive(Default)]
pub struct Disks {
    pub at: Option<Instant>,
    pub rec_free: Option<u64>,
    pub storages: Vec<Option<u64>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageView {
    label: String,
    path: String,
    free_bytes: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoView {
    label: String,
    #[serde(flatten)]
    state: GoState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    seat: u8,
    name: String,
    level_db: f32,
    voice: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShootingView {
    id: String,
    seconds: u64,
}

/// Everything a screen needs, sent four times a second.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    studio: String,
    simulated: bool,
    t_ms: u64,
    step: u8,
    draft: crate::config::Programme,
    from_programme: Option<String>,
    blocked: Option<String>,
    boot_ms: Option<u64>,
    checks: Vec<Check>,
    checks_ms: Option<u64>,
    guests: Vec<GuestView>,
    tests: Vec<Test>,
    test_index: usize,
    test_ms: Option<u64>,
    hearing: crate::session::Hearing,
    screen: String,
    recap_ms: Option<u64>,
    recap_checks: Vec<Check>,
    shooting: Option<ShootingView>,
    ended: Option<Ended>,
    end_ms: Option<u64>,
    after: AfterState,
    alerts: Vec<String>,
    programmes: Vec<crate::config::Programme>,
    profiles: Vec<String>,
    storages: Vec<StorageView>,
    gos: Vec<GoView>,
    hours_left: Option<f64>,
    rec_free_bytes: Option<u64>,
    mode: Mode,
    on_air: Option<String>,
    speaker: Option<String>,
    program: Option<u16>,
    levels: Vec<Level>,
    lives: BTreeMap<String, LiveState>,
    deck: DeckView,
    camera_for_seat: Vec<u8>,
    journal: Vec<crate::Entry>,
}

/// The camera that films a seat alone: the cameras with single shots,
/// shared out along the table (seat 1 to the first, seat 5 to the last).
pub fn camera_for_seat(setup: &strata_studio_core::StudioSetup, seat: u8) -> Option<u8> {
    let mut cams: Vec<u8> = setup
        .shots
        .iter()
        .filter(|s| matches!(s.kind, ShotKind::Single(_)))
        .map(|s| s.camera.0)
        .collect();
    cams.sort();
    cams.dedup();
    if cams.is_empty() {
        return setup.cameras.first().map(|c| c.id.0);
    }
    let n = setup.seats.len().max(1);
    let idx = (seat.saturating_sub(1) as usize * cams.len()) / n;
    cams.get(idx.min(cams.len() - 1)).copied()
}

/// « 3,2 To », « 640 Go ».
pub fn bytes(b: u64) -> String {
    let (v, unit) = if b >= 1_000_000_000_000 {
        (b as f64 / 1e12, "To")
    } else {
        (b as f64 / 1e9, "Go")
    };
    format!("{v:.1} {unit}").replace('.', ",")
}

fn local_time(offset_s: i64) -> String {
    let t = (history::unix_ms() / 1000) as i64 + offset_s;
    let days = t.div_euclid(86400);
    let secs = t.rem_euclid(86400);
    let date = record::date_of(days);
    format!(
        "{date} {:02}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

impl App {
    pub(crate) fn gos_free(&self) -> Vec<bool> {
        (0..self.settings.gos.len())
            .map(|i| self.ext.gos.get(i).is_none_or(|g| g.is_free()))
            .collect()
    }

    fn context(&self) -> Context {
        let d = &self.session.draft;
        Context {
            go: (d.go > 0)
                .then(|| {
                    self.settings
                        .gos
                        .get(d.go as usize - 1)
                        .map(|g| g.destination.clone())
                })
                .flatten(),
            live: d.go > 0 || d.youtube || d.tiktok,
            vertical: d.vertical,
            seats: self.session.placed().iter().map(|(s, _, _)| *s).collect(),
        }
    }

    pub(crate) fn scenario(&mut self, event: Event) {
        let acts = scenario::plan(&self.settings.scenarios, event, &self.context());
        if !acts.is_empty() {
            self.jobs.push(Job::Run(acts));
        }
    }

    // ---- the journey ----

    pub(crate) fn act(&mut self, act: Act) -> Result<(), String> {
        match act {
            Act::Patch(p) => self.patch(p),
            Act::Programme(i) => self.take_programme(i),
            Act::Start => self.start_shoot(),
            Act::Next => self.next(),
            Act::Prev => self.prev(),
            Act::TestOk => self.test_ok(),
            Act::TestRetry => {
                self.session.test_retry();
                Ok(())
            }
            Act::Launch => self.launch(),
            Act::End => self.end(),
            Act::Restart => self.restart_session(),
            Act::Nothing => Ok(()),
        }
    }

    pub(crate) fn patch(&mut self, p: DraftPatch) -> Result<(), String> {
        if self.session.shooting.is_some() {
            return Err("tournage en cours".into());
        }
        let free = self.gos_free();
        let calls_before = self.session.draft.calls;
        self.session.apply(p, &free);
        if self.session.step == 10 && self.session.draft.calls > calls_before {
            self.invite();
        }
        Ok(())
    }

    pub(crate) fn take_programme(&mut self, i: usize) -> Result<(), String> {
        let p = self
            .settings
            .programmes
            .get(i)
            .cloned()
            .ok_or("programme inconnu")?;
        self.session.take_programme(&p);
        Ok(())
    }

    pub(crate) fn start_shoot(&mut self) -> Result<(), String> {
        if self.session.step != 1 || self.session.boot_at.is_some() {
            return Ok(());
        }
        self.session.boot_at = Some(Instant::now());
        self.log("le studio s'allume".into(), true);
        self.scenario(Event::Start);
        Ok(())
    }

    pub(crate) fn restart_session(&mut self) -> Result<(), String> {
        if self.session.shooting.is_some() {
            return Err("tournage en cours".into());
        }
        self.session = crate::session::Session::default();
        Ok(())
    }

    pub(crate) fn next(&mut self) -> Result<(), String> {
        if let Some(why) = self.session.blocked() {
            return Err(why.into());
        }
        let s = self.session.step;
        if !(2..=12).contains(&s) {
            return Ok(());
        }
        self.goto(s + 1)
    }

    pub(crate) fn prev(&mut self) -> Result<(), String> {
        let s = self.session.step;
        if (3..=13).contains(&s) && self.session.shooting.is_none() {
            self.session.step = s - 1;
        }
        Ok(())
    }

    /// Goes to a step: backwards freely, forwards only to the next one, or
    /// straight to the tests with a programme taken back.
    pub(crate) fn goto(&mut self, step: u8) -> Result<(), String> {
        let s = self.session.step;
        if self.session.shooting.is_some() || !(2..=13).contains(&step) {
            return Err("impossible pendant le tournage".into());
        }
        let shortcut = step == 12 && s == 3 && self.session.from_programme.is_some();
        if step > s + 1 && !shortcut {
            return Err("répondez d'abord aux étapes précédentes".into());
        }
        self.session.step = step;
        match step {
            10 if self.session.draft.calls as usize > self.session.guests.len() => self.invite(),
            12 => self.prepare_tests(),
            13 => self.session.recap_at = Some(Instant::now()),
            _ => {}
        }
        Ok(())
    }

    /// Creates the guests missing for the calls chosen.
    fn invite(&mut self) {
        let want = self.session.draft.calls as usize;
        let have = self.session.guests.len();
        if want <= have {
            return;
        }
        for k in have..want {
            self.session.guests.push(GuestView {
                label: format!("Invité {}", k + 1),
                uid: String::new(),
                link: String::new(),
                state: "création".into(),
            });
        }
        self.jobs.push(Job::CreateGuests {
            from: have,
            to: want,
            start: local_time(-3600),
            end: local_time(6 * 3600),
        });
    }

    /// The people's names on the seats, the remote sources of the day,
    /// and the list of tests.
    fn prepare_tests(&mut self) {
        let placed = self.session.placed();
        let d = self.session.draft.clone();
        {
            let setup = self.director.setup_mut();
            for seat in &mut setup.seats {
                seat.guest = placed
                    .iter()
                    .find(|(s, _, _)| *s == seat.id.0)
                    .map(|(_, _, n)| n.clone());
            }
            // The host is the first person.
            setup.host = placed
                .iter()
                .find(|(_, p, _)| *p == 0)
                .map(|(s, _, _)| SeatId(*s));
            let mut remotes = Vec::new();
            for k in 0..d.calls as usize {
                remotes.push(Remote {
                    id: RemoteId(k as u8 + 1),
                    kind: RemoteKind::Call,
                    label: format!("Call {}", k + 1),
                    guest: None,
                    switcher_input: self
                        .settings
                        .calls
                        .inputs
                        .get(k)
                        .copied()
                        .unwrap_or(5 + k as u16),
                });
            }
            if d.agency {
                remotes.push(Remote {
                    id: RemoteId(10),
                    kind: RemoteKind::Agency,
                    label: "Agence".into(),
                    guest: None,
                    switcher_input: self.settings.agency_input,
                });
            }
            if d.duplex {
                remotes.push(Remote {
                    id: RemoteId(11),
                    kind: RemoteKind::Duplex,
                    label: "Duplex".into(),
                    guest: None,
                    switcher_input: self.settings.duplex_input,
                });
            }
            setup.remotes = remotes;
        }
        self.listener = Listener::new(self.director.setup());
        let strips: Vec<(u8, String)> = self
            .director
            .setup()
            .seats
            .iter()
            .map(|s| (s.mic.console_channel, s.guest.clone().unwrap_or_default()))
            .collect();
        for (ch, name) in strips {
            let _ = self.console.set_name(ch, &name);
        }
        let setup = self.director.setup().clone();
        let mut tests: Vec<Test> = placed
            .iter()
            .map(|(seat, _, name)| {
                let cam = camera_for_seat(&setup, *seat);
                Test {
                    label: name.clone(),
                    seat: Some(*seat),
                    camera: cam,
                    input: cam
                        .and_then(|c| setup.cameras.iter().find(|k| k.id.0 == c))
                        .map_or(0, |c| c.switcher_input),
                    source: cam.map_or("PGM".into(), |c| format!("CAM{c}")),
                    done: false,
                }
            })
            .collect();
        for r in &setup.remotes {
            tests.push(Test {
                label: match r.kind {
                    RemoteKind::Call => r.label.clone(),
                    RemoteKind::Agency => "Agence de presse".into(),
                    RemoteKind::Duplex => "Duplex Aviwest".into(),
                },
                seat: None,
                camera: None,
                input: r.switcher_input,
                source: "PGM".into(),
                done: false,
            });
        }
        self.session.tests = tests;
        self.session.test_index = 0;
        self.session.hearing = Default::default();
        self.session.test_at = Some(Instant::now());
        self.show_test();
    }

    /// Frames and shows the person being tested.
    fn show_test(&mut self) {
        let Some(t) = self.session.tests.get(self.session.test_index).cloned() else {
            return;
        };
        if let (Some(cam), Some(seat)) = (t.camera, t.seat) {
            let preset = self
                .director
                .setup()
                .shots
                .iter()
                .find(|s| s.camera.0 == cam && s.kind == ShotKind::Single(SeatId(seat)))
                .and_then(|s| s.preset);
            if let (Some(p), Some(ptz)) =
                (preset, self.ptz.get_mut(&strata_studio_core::CamId(cam)))
            {
                let _ = ptz.recall(p);
            }
        }
        if t.input > 0 {
            let _ = self.switcher.cut(t.input);
        }
    }

    pub(crate) fn test_ok(&mut self) -> Result<(), String> {
        let Some(t) = self.session.tests.get(self.session.test_index) else {
            return Ok(());
        };
        let elapsed = self
            .session
            .test_at
            .map_or(0, |a| a.elapsed().as_millis() as u64);
        let ready = if t.seat.is_some() {
            self.session.hearing.heard && !self.session.hearing.low
        } else {
            elapsed > 1200
        };
        if !ready {
            return Err("pas encore entendu".into());
        }
        self.session.test_done();
        self.show_test();
        Ok(())
    }

    /// The level heard on the tested seat's microphone. Without real
    /// microphones, a voice is made up: the third person speaks too low
    /// until the test is done again.
    fn test_level(&self) -> Option<f32> {
        let t = self.session.tests.get(self.session.test_index)?;
        let seat = t.seat?;
        if self.voices.is_some() {
            return self
                .last
                .mics
                .iter()
                .find(|m| m.seat.0 == seat)
                .map(|m| m.level_db);
        }
        let since = self.session.test_at.map_or(0, |a| a.elapsed().as_millis());
        let wobble = ((self.now() as f32) / 150.0).sin().abs() * 6.0;
        Some(if since < 1400 {
            -70.0
        } else if self.session.test_index == 2 && self.session.hearing.tries == 0 {
            -52.0 + wobble
        } else {
            -26.0 + wobble
        })
    }

    // ---- launch and end ----

    pub(crate) fn launch(&mut self) -> Result<(), String> {
        if self.session.shooting.is_some() {
            return Ok(());
        }
        if self.session.step != 13 {
            return Err("passez d'abord par le récapitulatif".into());
        }
        let d = self.session.draft.clone();
        let profile = self
            .settings
            .profiles
            .get(d.profile)
            .cloned()
            .unwrap_or_default();
        self.director.set_profile(profile.clone());
        self.control.vertical = d.vertical;
        self.control.mode = Mode::Auto;
        self.control.forced = None;
        let storage = self.settings.storages.get(d.storage).cloned();
        let req = RecordRequest {
            programme: d.name.trim().to_string(),
            pgm: d.pgm,
            clean: d.clean,
            iso: d.iso,
            mics: d.mics,
            resolution: if d.resolution == "2160p50" {
                Resolution::Uhd
            } else {
                Resolution::Hd
            },
            vertical: d.vertical,
            destination: storage.as_ref().map(|s| s.path.clone()),
        };
        let mics: Vec<(String, u16)> = self
            .director
            .setup()
            .seats
            .iter()
            .filter(|s| s.guest.is_some())
            .map(|s| (s.display_name().to_string(), s.mic.dante_channel))
            .collect();
        if let Some(desk) = self.desk.as_mut() {
            if d.pgm || d.clean || d.iso || d.mics {
                desk.start(req.clone(), &mics)?;
            }
        }
        // The chosen GO: the programme routed to it, then streamed.
        if d.go > 0 {
            let i = d.go as usize - 1;
            if let Some(g) = self.settings.gos.get(i) {
                if !g.destination.is_empty() {
                    let src = self.settings.router.pgm_source.clone();
                    if let Err(e) = self.router.route(&g.destination, &src) {
                        self.session.alerts.push(format!("{} : {e}", g.label));
                    }
                }
            }
            self.jobs.push(Job::GoStart(i));
        }
        let input = if self.settings.live.input.is_empty() {
            self.settings
                .recording
                .channels
                .iter()
                .find(|c| c.name == "PGM")
                .map(|c| c.input.clone())
                .unwrap_or_default()
        } else {
            self.settings.live.input.clone()
        };
        for (name, on, target) in [
            ("youtube", d.youtube, self.settings.live.youtube.clone()),
            ("tiktok", d.tiktok, self.settings.live.tiktok.clone()),
        ] {
            if !on {
                continue;
            }
            if self.simulated && target.key.is_empty() {
                self.lives.insert(name.into(), LiveRun::Sim(Instant::now()));
                continue;
            }
            match Live::start(&self.ffmpeg, &input, self.settings.live.accel, &target) {
                Ok(l) => {
                    self.lives.insert(name.into(), LiveRun::Real(l));
                }
                Err(e) => self.session.alerts.push(format!("{name} : {e}")),
            }
        }
        self.scenario(Event::Launch);
        // The history.
        let started = history::unix_ms();
        let date = record::today();
        let id = history::new_id(&date, started, &req.programme);
        let (w, h) = req.resolution.size();
        let log = self
            .history
            .perception_path(&id)
            .and_then(|p| PerceptionLog::create(&p).ok());
        self.capture = Some(Capture {
            record: ShootRecord {
                id: id.clone(),
                programme: req.programme.clone(),
                date,
                started_unix_ms: started,
                status: "en cours".into(),
                answers: d.clone(),
                profile,
                setup: Some(self.director.setup().clone()),
                storage: storage
                    .as_ref()
                    .map(|s| s.label.clone())
                    .unwrap_or_default(),
                storage_path: storage
                    .as_ref()
                    .map(|s| s.path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                width: w,
                height: h,
                ..Default::default()
            },
            t0: self.now(),
            log,
            last_source: None,
            speaker: None,
            last_human_program: self.switcher.program(),
            ai_cuts: vec![],
            alerted: false,
        });
        if let Some(c) = &self.capture {
            let _ = self.history.save(&c.record);
        }
        self.session.shooting = Some((id, Instant::now()));
        self.log(format!("TOURNAGE lancé : {}", req.programme), true);
        Ok(())
    }

    pub(crate) fn end(&mut self) -> Result<(), String> {
        let Some((id, since)) = self.session.shooting.take() else {
            return Ok(());
        };
        let d = self.session.draft.clone();
        if let Some(desk) = self.desk.as_mut() {
            let _ = desk.stop();
        }
        let files: Vec<String> = self
            .desk
            .as_ref()
            .map(|k| {
                k.last_files
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        if d.go > 0 {
            self.jobs.push(Job::GoStop(d.go as usize - 1));
        }
        for (_, l) in std::mem::take(&mut self.lives) {
            l.stop();
        }
        self.scenario(Event::End);
        let now = self.now();
        if let Some(mut c) = self.capture.take() {
            if let Some((name, from)) = c.speaker.take() {
                c.record.speakers.push(Speech {
                    start_ms: from,
                    end_ms: now - c.t0,
                    name,
                });
            }
            if let Some(log) = c.log.as_mut() {
                log.flush();
            }
            c.record.duration_ms = now - c.t0;
            c.record.status = "terminé".into();
            c.record.files = files.clone();
            let _ = self.history.save(&c.record);
        }
        let n_files = files.len() * if d.vertical { 2 } else { 1 };
        self.session.ended = Some(Ended {
            id,
            seconds: since.elapsed().as_secs(),
            files: n_files,
        });
        self.session.end_at = Some(Instant::now());
        self.session.step = 14;
        self.log("TOURNAGE terminé".into(), true);
        Ok(())
    }

    // ---- every frame ----

    /// The journey's share of the frame: the studio waking up, the tests'
    /// ears, the shoot's history, the Stream Deck.
    pub(crate) fn tick_session(&mut self) {
        if self.session.step == 1 {
            if let Some(at) = self.session.boot_at {
                if at.elapsed() > Duration::from_millis(2600) {
                    self.session.step = 2;
                    self.session.boot_at = None;
                    self.session.checks.clear();
                    self.jobs.push(Job::Checks);
                }
            }
        }
        if self.session.step == 12 {
            if let Some(level) = self.test_level() {
                self.session
                    .hearing
                    .feed(level, crate::TICK.as_millis() as u64);
            }
        }
        // Live streams that fall.
        let mut fallen = Vec::new();
        for (name, l) in self.lives.iter_mut() {
            if let LiveState::Failed { error } = l.state() {
                fallen.push(format!("{name} a décroché : {error}"));
            }
        }
        if !fallen.is_empty() {
            for f in &fallen {
                if !self.session.alerts.contains(f) {
                    self.session.alerts.push(f.clone());
                    self.log(f.clone(), true);
                }
            }
            if self.capture.as_ref().is_some_and(|c| !c.alerted) {
                if let Some(c) = self.capture.as_mut() {
                    c.alerted = true;
                }
                self.scenario(Event::Alert);
            }
        }
        self.capture_frame();
        self.push_deck();
    }

    fn capture_frame(&mut self) {
        let now = self.now();
        let program = self.switcher.program();
        let on_air_label = self.director.on_air().map(|s| self.shot_label(s));
        let setup = self.director.setup().clone();
        let mode = self.control.mode.clone();
        let layout_label = self
            .director
            .composite()
            .map_or_else(|| "programme".to_string(), |k| self.composite_label(&k));
        let Some(c) = self.capture.as_mut() else {
            return;
        };
        let t = now - c.t0;
        if let Some(log) = c.log.as_mut() {
            log.write(&self.last);
        }
        // What is on the programme: a camera, or the PGM (a layout, a
        // remote source).
        let source = match program.and_then(|i| setup.camera_on_input(i)) {
            Some(cam) => format!("CAM{}", cam.id.0),
            None => "PGM".to_string(),
        };
        if c.last_source.as_deref() != Some(&source) {
            let label = if source == "PGM" {
                layout_label
            } else {
                on_air_label.clone().unwrap_or_else(|| source.clone())
            };
            c.record.segments.push(Segment {
                start_ms: t,
                source: source.clone(),
                label,
            });
            c.last_source = Some(source);
        }
        // A person cutting at the panel: counted against the AI's cuts.
        if program != c.last_human_program {
            if let (Mode::Shadow, Some(_)) = (&mode, program) {
                c.record.agreement.human_cuts += 1;
                if c.ai_cuts.iter().any(|a| t.abs_diff(*a) <= 1000) {
                    c.record.agreement.matched += 1;
                }
            }
            c.last_human_program = program;
        }
        if matches!(mode, Mode::Manual { .. }) {
            c.record.manual_ms += crate::TICK.as_millis() as u64;
        }
        // Saved every 5 s, so that a crash loses little.
        if t % 5000 < crate::TICK.as_millis() as u64 {
            if let Some(log) = c.log.as_mut() {
                log.flush();
            }
            let _ = self.history.save(&c.record);
        }
    }

    /// Called by `log`: the journal of a shoot under way.
    pub(crate) fn capture_note(&mut self, text: &str, sent: bool) {
        let now = self.now();
        if let Some(c) = self.capture.as_mut() {
            c.record.notes.push(Note {
                t_ms: now - c.t0,
                text: text.into(),
                sent,
            });
            if text.starts_with("COUPE") && matches!(self.control.mode, Mode::Shadow) {
                c.ai_cuts.push(now - c.t0);
                c.record.agreement.ai_cuts += 1;
            }
        }
    }

    pub(crate) fn capture_speaker(&mut self, name: &str) {
        let now = self.now();
        if let Some(c) = self.capture.as_mut() {
            let t = now - c.t0;
            if let Some((prev, from)) = c.speaker.take() {
                c.record.speakers.push(Speech {
                    start_ms: from,
                    end_ms: t,
                    name: prev,
                });
            }
            if !name.is_empty() {
                c.speaker = Some((name.to_string(), t));
            }
        }
    }

    pub(crate) fn capture_heard(&mut self, text: &str) {
        let now = self.now();
        if let Some(c) = self.capture.as_mut() {
            c.record.heard.push(Said {
                t_ms: now - c.t0,
                text: text.into(),
            });
        }
    }

    fn deck_armed(&self) -> bool {
        self.deck_armed_until.is_some_and(|u| Instant::now() < u)
    }

    pub(crate) fn deck_view(&self) -> DeckView {
        deck::view(
            &self.session,
            &self.settings,
            &self.gos_free(),
            self.deck_armed(),
        )
    }

    pub(crate) fn deck_key(&mut self, n: usize) -> Result<(), String> {
        let armed = self.deck_armed();
        let act = deck::press(&self.session, &self.settings, n, armed);
        if self.session.step == 13 && n == 8 && !armed {
            self.deck_armed_until = Some(Instant::now() + Duration::from_secs(3));
            return Ok(());
        }
        self.deck_armed_until = None;
        self.act(act)
    }

    fn push_deck(&mut self) {
        let Some(tx) = &self.deck_tx else { return };
        let v = self.deck_view();
        if v == self.deck_last {
            return;
        }
        let old = deck::variables(&self.deck_last);
        let changed: Vec<(String, String)> = deck::variables(&v)
            .into_iter()
            .filter(|kv| !old.contains(kv))
            .collect();
        let _ = tx.send(changed);
        self.deck_last = v;
    }

    // ---- what the screens get ----

    fn disks(&mut self) {
        if self
            .disks
            .at
            .is_some_and(|a| a.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        let root = self.desk.as_ref().map(|d| d.root.clone());
        self.disks.rec_free = root.and_then(|r| record::free_bytes(&r));
        self.disks.storages = self
            .settings
            .storages
            .iter()
            .map(|s| record::free_bytes(&s.path))
            .collect();
        self.disks.at = Some(Instant::now());
    }

    pub(crate) fn snapshot(&mut self) -> Snapshot {
        self.disks();
        let now = self.now();
        let s = &self.session;
        let d = &s.draft;
        let channels = d.pgm as u32 + d.clean as u32 + if d.iso { 4 } else { 0 };
        let mbps = if d.resolution == "2160p50" {
            150.0
        } else {
            50.0
        };
        let bits = channels as f64 * mbps * 1e6 + if d.mics { 8.0 * 48000.0 * 24.0 } else { 0.0 };
        let hours_left = self
            .disks
            .rec_free
            .filter(|_| bits > 0.0)
            .map(|b| b as f64 * 8.0 / bits / 3600.0);
        let ms = |t: Option<Instant>| t.map(|a| a.elapsed().as_millis() as u64);
        let mut recap_checks = vec![Check::ok(
            "Place sur le serveur",
            hours_left.map_or("—".into(), |h| {
                format!(
                    "{} h {:02} possibles",
                    h as u64,
                    ((h.fract()) * 60.0) as u64
                )
            }),
        )];
        if d.go > 0 {
            let label = self
                .settings
                .gos
                .get(d.go as usize - 1)
                .map_or(String::new(), |g| g.label.clone());
            let free = self
                .ext
                .gos
                .get(d.go as usize - 1)
                .is_none_or(|g| g.is_free());
            recap_checks.push(if free {
                Check::ok(&format!("{label} prête à diffuser"), "")
            } else {
                Check::fail(&format!("{label} déjà en service"), "")
            });
        }
        if d.youtube {
            let ok = !self.settings.live.youtube.key.is_empty() || self.simulated;
            recap_checks.push(if ok {
                Check::ok("YouTube prêt", "")
            } else {
                Check::warn("YouTube", "clé de diffusion à régler dans l'administration")
            });
        }
        if d.tiktok {
            let ok = !self.settings.live.tiktok.url.is_empty() || self.simulated;
            recap_checks.push(if ok {
                Check::ok("TikTok prêt", "")
            } else {
                Check::warn(
                    "TikTok",
                    "adresse de diffusion à régler dans l'administration",
                )
            });
        }
        if let Some(st) = self.settings.storages.get(d.storage) {
            let reachable = self
                .disks
                .storages
                .get(d.storage)
                .copied()
                .flatten()
                .is_some();
            recap_checks.push(if reachable {
                Check::ok(&format!("{} joignable", st.label), "")
            } else {
                Check::warn(
                    &st.label,
                    "partage non monté : la copie se fera à son retour",
                )
            });
        }
        recap_checks.push(Check::ok(
            "IA prête à réaliser",
            self.director.profile().name.clone(),
        ));
        let setup = self.director.setup();
        let levels = self
            .last
            .mics
            .iter()
            .filter_map(|m| {
                let seat = setup.seat(m.seat)?;
                seat.guest.as_ref()?;
                Some(Level {
                    seat: m.seat.0,
                    name: seat.display_name().to_string(),
                    level_db: m.level_db,
                    voice: m.voice,
                })
            })
            .collect();
        let after = self
            .desk
            .as_mut()
            .map(|k| k.status().after)
            .unwrap_or_default();
        let camera_for_seat = (1..=setup.seats.len() as u8)
            .map(|n| camera_for_seat(setup, n).unwrap_or(0))
            .collect();
        let lives = self
            .lives
            .iter_mut()
            .map(|(n, l)| (n.clone(), l.state()))
            .collect();
        let deck = self.deck_view();
        let s = &self.session;
        Snapshot {
            studio: self.settings.studio.clone(),
            simulated: self.simulated,
            t_ms: now,
            step: s.step,
            draft: s.draft.clone(),
            from_programme: s.from_programme.clone(),
            blocked: s.blocked().map(String::from),
            boot_ms: ms(s.boot_at),
            checks: s.checks.clone(),
            checks_ms: ms(s.checks_at),
            guests: s.guests.clone(),
            tests: s.tests.clone(),
            test_index: s.test_index,
            test_ms: ms(s.test_at),
            hearing: s.hearing.clone(),
            screen: s.screen.clone(),
            recap_ms: ms(s.recap_at),
            recap_checks,
            shooting: s.shooting.as_ref().map(|(id, t)| ShootingView {
                id: id.clone(),
                seconds: t.elapsed().as_secs(),
            }),
            ended: s.ended.clone(),
            end_ms: ms(s.end_at),
            after,
            alerts: s.alerts.clone(),
            programmes: self.settings.programmes.clone(),
            profiles: self
                .settings
                .profiles
                .iter()
                .map(|p| p.name.clone())
                .collect(),
            storages: self
                .settings
                .storages
                .iter()
                .enumerate()
                .map(|(i, st)| StorageView {
                    label: st.label.clone(),
                    path: st.path.to_string_lossy().into_owned(),
                    free_bytes: self.disks.storages.get(i).copied().flatten(),
                })
                .collect(),
            gos: self
                .settings
                .gos
                .iter()
                .enumerate()
                .map(|(i, g)| GoView {
                    label: g.label.clone(),
                    state: self.ext.gos.get(i).cloned().unwrap_or(GoState::Idle),
                })
                .collect(),
            hours_left,
            rec_free_bytes: self.disks.rec_free,
            mode: self.control.mode.clone(),
            on_air: self
                .director
                .composite()
                .map(|k| self.composite_label(&k))
                .or_else(|| self.director.on_air().map(|sh| self.shot_label(sh))),
            speaker: self
                .director
                .speaker()
                .map(|sp| self.director.setup().seat_name(sp)),
            program: self.switcher.program(),
            levels,
            lives,
            deck,
            camera_for_seat,
            journal: self.journal.iter().rev().take(30).cloned().collect(),
        }
    }
}

/// The checks of step 2, from the devices' own answers.
fn run_checks(app: &Shared, ext: &SharedExt) {
    let gos: Vec<(String, GoState)> = {
        let mut e = ext.lock().unwrap();
        let labels: Vec<String> = app
            .lock()
            .unwrap()
            .settings
            .gos
            .iter()
            .map(|g| g.label.clone())
            .collect();
        e.gos
            .iter_mut()
            .enumerate()
            .map(|(i, g)| (labels.get(i).cloned().unwrap_or_default(), g.state()))
            .collect()
    };
    let mut a = app.lock().unwrap();
    a.ext.gos = gos.iter().map(|(_, s)| s.clone()).collect();
    let mut checks = Vec::new();
    // Buttons drives the lights: it must answer.
    checks.push(
        match a.automation.set_variable("studio_state", "vérification") {
            Ok(()) => Check::ok("Lumière", "scène par défaut"),
            Err(e) => Check::warn("Lumière", format!("Buttons ne répond pas : {e}")),
        },
    );
    let cams: Vec<bool> = a.ptz.values_mut().map(|p| p.status().online).collect();
    let on = cams.iter().filter(|o| **o).count();
    checks.push(if on == cams.len() {
        Check::ok("Caméras", format!("{on} sur {} allumées", cams.len()))
    } else {
        Check::warn("Caméras", format!("{on} sur {} répondent", cams.len()))
    });
    checks.push(if a.switcher.connected() {
        Check::ok("Mélangeur", a.switcher.name())
    } else {
        Check::fail("Mélangeur", format!("{} ne répond pas", a.switcher.name()))
    });
    checks.push(Check::ok("Console", a.console.name()));
    checks.push(match &a.voices {
        Some(_) => Check::ok("Micros", "Dante · flux AES67 reçu"),
        None => Check::ok("Micros", "conversation simulée"),
    });
    let routes = a.router.routes().len();
    checks.push(Check::ok(
        "Nodal",
        format!("{} · {routes} routes", a.router.name()),
    ));
    let busy: Vec<String> = gos
        .iter()
        .filter(|(_, s)| matches!(s, GoState::Live { .. }))
        .map(|(l, _)| l.clone())
        .collect();
    let offline: Vec<String> = gos
        .iter()
        .filter(|(_, s)| matches!(s, GoState::Offline { .. }))
        .map(|(l, _)| l.clone())
        .collect();
    checks.push(if !offline.is_empty() {
        Check::warn("Diffusion", format!("{} ne répond pas", offline.join(", ")))
    } else if busy.is_empty() {
        Check::ok("Diffusion", "les GO sont libres")
    } else {
        Check::ok("Diffusion", format!("{} déjà en service", busy.join(", ")))
    });
    let root = a.desk.as_ref().map(|d| d.root.clone());
    checks.push(match root.and_then(|r| record::free_bytes(&r)) {
        Some(b) => Check::ok("Stockage", format!("{} libres sur le serveur", bytes(b))),
        None => Check::warn("Stockage", "place sur le serveur inconnue"),
    });
    // A device simulated because it did not answer at start.
    for (label, error) in a.device_errors.clone() {
        // Without a switcher there is no shoot; the rest is shot without.
        let detail = format!("ne répond pas : {error}");
        let c = if label == "Mélangeur" {
            Check::fail(&label, detail)
        } else {
            Check::warn(&label, detail)
        };
        match checks.iter_mut().find(|k| k.label == label) {
            Some(k) => *k = c,
            None => checks.push(c),
        }
    }
    a.session.checks = checks;
    a.session.checks_at = Some(Instant::now());
}

/// Carries out a job, outside the studio's lock (except to read and write
/// the studio).
pub fn run_job(app: Shared, ext: SharedExt, job: Job) {
    match job {
        Job::Checks => run_checks(&app, &ext),
        Job::CreateGuests {
            from,
            to,
            start,
            end,
        } => {
            for k in from..to {
                let name = format!("Call {}", k + 1);
                let res = ext.lock().unwrap().calls.create_guest(&name, &start, &end);
                let mut a = app.lock().unwrap();
                let position = a.settings.calls.positions.get(k).copied();
                match res {
                    Ok(g) => {
                        if let Some(p) = position {
                            let uid = g.uid.clone();
                            drop(a);
                            if let Err(e) = ext.lock().unwrap().calls.place(&uid, p) {
                                tracing::warn!("placement de l'invité : {e}");
                            }
                            a = app.lock().unwrap();
                        }
                        if let Some(v) = a.session.guests.get_mut(k) {
                            v.uid = g.uid;
                            v.link = g.link;
                            v.state = "waiting".into();
                        }
                    }
                    Err(e) => {
                        if let Some(v) = a.session.guests.get_mut(k) {
                            v.state = "unknown".into();
                        }
                        a.session.alerts.push(format!("invité non créé : {e}"));
                    }
                }
            }
        }
        Job::GoStart(i) => {
            let res = ext.lock().unwrap().gos.get_mut(i).map(|g| g.start());
            if let Some(Err(e)) = res {
                let mut a = app.lock().unwrap();
                a.session.alerts.push(format!("GO{} : {e}", i + 1));
                a.log(format!("GO{} : {e}", i + 1), false);
            }
        }
        Job::GoStop(i) => {
            if let Some(Err(e)) = ext.lock().unwrap().gos.get_mut(i).map(|g| g.stop()) {
                tracing::warn!("GO{} : {e}", i + 1);
            }
        }
        Job::Run(acts) => {
            for act in acts {
                if let What::Wait { ms } = act {
                    std::thread::sleep(Duration::from_millis(ms.min(60_000)));
                    continue;
                }
                let mut a = app.lock().unwrap();
                let res = a.perform(&act);
                if let Err(e) = res {
                    a.log(format!("automation refusée : {e}"), false);
                }
            }
        }
    }
}

impl App {
    /// One action of a scenario, on the devices.
    pub(crate) fn perform(&mut self, act: &What) -> Result<(), String> {
        match act {
            What::Press {
                page, row, column, ..
            } => self
                .automation
                .press(*page, *row, *column)
                .map_err(|e| e.to_string()),
            What::Variable { name, value } => self
                .automation
                .set_variable(name, value)
                .map_err(|e| e.to_string()),
            What::Route {
                destination,
                source,
            } => {
                let src = if source == "PGM" {
                    self.settings.router.pgm_source.clone()
                } else {
                    source.clone()
                };
                self.router
                    .route(destination, &src)
                    .map_err(|e| e.to_string())
            }
            What::CamerasPower { on } => {
                for p in self.ptz.values_mut() {
                    p.power(*on).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            What::CameraPreset { camera, preset } => self
                .ptz
                .get_mut(&strata_studio_core::CamId(*camera))
                .ok_or("caméra inconnue")?
                .recall(*preset)
                .map_err(|e| e.to_string()),
            What::Mics { on } => {
                let seats: Vec<(u8, bool)> = self
                    .director
                    .setup()
                    .seats
                    .iter()
                    .map(|s| (s.mic.console_channel, *on && s.guest.is_some()))
                    .collect();
                for (ch, open) in seats {
                    self.console.set_on(ch, open).map_err(|e| e.to_string())?;
                }
                Ok(())
            }
            What::Wait { .. } | What::Trigger { .. } => Ok(()),
            cond if cond.is_condition() => Ok(()),
            _ => Ok(()),
        }
    }
}

/// Every 2 s: the GO encoders' states and the guests' (slow requests).
pub fn poll(app: Shared, ext: SharedExt) {
    loop {
        let uids: Vec<String> = app
            .lock()
            .unwrap()
            .session
            .guests
            .iter()
            .filter(|g| !g.uid.is_empty())
            .map(|g| g.uid.clone())
            .collect();
        let (gos, guests) = {
            let mut e = ext.lock().unwrap();
            let gos: Vec<GoState> = e.gos.iter_mut().map(|g| g.state()).collect();
            let guests: BTreeMap<String, GuestState> =
                uids.iter().map(|u| (u.clone(), e.calls.guest(u))).collect();
            (gos, guests)
        };
        {
            let mut a = app.lock().unwrap();
            for g in a.session.guests.iter_mut() {
                if let Some(st) = guests.get(&g.uid) {
                    g.state = match st {
                        GuestState::Waiting => "waiting",
                        GuestState::Connected { unstable: true, .. } => "unstable",
                        GuestState::Connected { .. } => "connected",
                        GuestState::Unknown { .. } => "unknown",
                    }
                    .into();
                }
            }
            a.ext = ExtView { gos };
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Writes the Stream Deck's keys into Buttons, away from the studio.
pub fn deck_pusher(
    mut buttons: Box<dyn strata_studio_devices::Automation>,
) -> std::sync::mpsc::Sender<Vec<(String, String)>> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<(String, String)>>();
    std::thread::Builder::new()
        .name("stream deck".into())
        .spawn(move || {
            for batch in rx {
                for (k, v) in batch {
                    if let Err(e) = buttons.set_variable(&k, &v) {
                        tracing::warn!("Stream Deck : {e}");
                        break;
                    }
                }
            }
        })
        .expect("thread stream deck");
    tx
}
