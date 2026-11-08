//! strata-studio: the Strata Studio server.
//!
//! For now it runs a **simulated studio** (switcher unless `--atem` or
//! `--vmix` is given, four PTZ, console,
//! Bitfocus, a scripted conversation) directed live by the Director, and
//! serves its state and the takeover commands:
//!
//! | Route | |
//! | --- | --- |
//! | `GET /api/health` | alive |
//! | `GET /api/state` | mode, shot on air, speaker, cameras, journal of decisions |
//! | `GET /api/setup` · `PUT /api/setup/guests` | the studio; the guests' names (`{"1": "Alice"}`) |
//! | `POST /api/control/auto` · `/shadow` · `/take?by=Paul` | AI on, AI in shadow, a person takes the hand |
//! | `POST /api/control/force/{shot}?seconds=10` | impose a shot |
//! | `POST /api/control/lock/{cam}` · `/unlock/{cam}` | the AI leaves a camera alone |
//! | `POST /api/control/profile/{n}` | profile: 0 podcast, 1 débat, 2 interview, 3 radio |
//! | `POST /api/demo/panel-cut/{input}` | simulate a person cutting on the switcher's panel |
//! | `POST /api/record/estimate` · `/start` · `/stop` · `GET /api/record/status` | the recording: time left, start with the tablet's answers, stop (then 9:16 versions and copy in the background), state of each channel |
//! | `POST /api/demo/say` | simulate the host's words (`{"text": "On retrouve Julie en duplex"}`), as the speech recogniser would give them |
//! | `POST /api/demo/remote/{id}/voice/{on}` | simulate a remote source speaking (`on` = 1) or silent (0) |

mod api;
mod config;
mod deck;
mod history;
mod record;
mod scenario;
mod session;
mod shoot;
mod web;

use anyhow::Context;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use strata_studio_core::script::Script;
use strata_studio_core::{
    CamId, CamState, Composite, Control, Decision, Director, Forced, Heard, Hint, Listener, Mode,
    Perception, Profile, RemoteId, RemoteState, SeatId, ShotId, ShotKind, StudioSetup, Subject,
};
use strata_studio_devices::atem::{AtemLayout, AtemSwitcher};
use strata_studio_devices::buttons::Buttons;
use strata_studio_devices::m32::M32;
use strata_studio_devices::quicklink::{Calls, Quicklink, SimCalls};
use strata_studio_devices::sim::{SimAutomation, SimConsole, SimPtz, SimRouter, SimSwitcher};
use strata_studio_devices::visca::ViscaPtz;
use strata_studio_devices::vmix::{VmixLayout, VmixSwitcher};
use strata_studio_devices::webencoder::{BlackmagicEncoder, GoEncoder, SimEncoder};
use strata_studio_devices::yamaha::Yamaha;
use strata_studio_devices::{AudioConsole, Automation, Ptz, Router as Nodal, Shown, Switcher};
use strata_studio_sense::aes67;
use strata_studio_sense::ear::HostEar;
use strata_studio_sense::voices::{Settings as VoiceSettings, Voices};
use strata_studio_sense::whisper::WhisperServer;

#[derive(Parser)]
#[command(
    name = "strata-studio",
    version,
    about = "Strata Studio : le studio autonome"
)]
struct Args {
    /// Address to listen on.
    #[arg(long, env = "STRATA_STUDIO_LISTEN", default_value = "0.0.0.0:8095")]
    listen: String,
    /// Studio's data folder: the settings (`reglages.json`) and the shoots.
    #[arg(long, env = "STRATA_STUDIO_DATA", default_value = ".")]
    data: String,
    /// Directing profile: 0 podcast, 1 débat, 2 interview, 3 radio.
    #[arg(long, default_value_t = 0)]
    profile: usize,
    /// Drive a real ATEM at this address (else a simulated switcher).
    #[arg(long)]
    atem: Option<String>,
    /// Drive a real vMix at this address (else a simulated switcher).
    #[arg(long)]
    vmix: Option<String>,
    /// vMix: the layout input with two layers for the side by side.
    #[arg(long)]
    vmix_split: Option<u16>,
    /// vMix: the layout input with two layers for the picture in picture.
    #[arg(long)]
    vmix_pip: Option<u16>,
    /// The microphones: the SDP file of the AES67 stream (Dante) carrying
    /// them (else the scripted conversation). Each seat's channel is its
    /// `danteChannel`.
    #[arg(long)]
    aes67: Option<String>,
    /// The network interface to receive the AES67 stream on.
    #[arg(long)]
    aes67_if: Option<String>,
    /// The whisper.cpp server transcribing the host's microphone.
    #[arg(long)]
    whisper: Option<String>,
    /// Where the recordings are written on the server.
    #[arg(long, env = "STRATA_STUDIO_REC_DIR", default_value = "enregistrements")]
    rec_dir: String,
    /// The recording settings (codec, acceleration, container, each
    /// channel's ffmpeg input), as JSON; else the simulated studio's.
    #[arg(long)]
    rec_settings: Option<String>,
    /// The ffmpeg to record with.
    #[arg(long, default_value = "ffmpeg")]
    ffmpeg: String,
    /// The audio console: `m32:<address>` or `dm5:<address>` (else simulated).
    #[arg(long)]
    console: Option<String>,
    /// Bitfocus Buttons' HTTP API: `<address>:<port>` (else simulated).
    #[arg(long)]
    buttons: Option<String>,
}

fn with_port(a: &str, port: u16) -> anyhow::Result<std::net::SocketAddr> {
    let a = if a.contains(':') {
        a.to_string()
    } else {
        format!("{a}:{port}")
    };
    use std::net::ToSocketAddrs;
    a.to_socket_addrs()
        .with_context(|| format!("adresse : {a}"))?
        .next()
        .with_context(|| format!("adresse : {a}"))
}

/// The command line's devices over the settings' (to try a device
/// quickly, or from a script).
fn apply_args(s: &mut config::Settings, args: &Args) -> anyhow::Result<()> {
    if let Some(a) = &args.atem {
        s.switcher.kind = "atem".into();
        s.switcher.address = a.clone();
    } else if let Some(v) = &args.vmix {
        s.switcher.kind = "vmix".into();
        s.switcher.address = v.clone();
    }
    if let Some(x) = args.vmix_split {
        s.switcher.vmix_split = x;
    }
    if let Some(x) = args.vmix_pip {
        s.switcher.vmix_pip = x;
    }
    if let Some(c) = &args.console {
        let (k, a) = c.split_once(':').unwrap_or((c, ""));
        s.console.kind = k.into();
        s.console.address = a.into();
    }
    if let Some(b) = &args.buttons {
        s.buttons.address = b.clone();
    }
    if let Some(a) = &args.aes67 {
        s.audio.sdp = a.clone();
    }
    if let Some(i) = &args.aes67_if {
        s.audio.interface = i.clone();
    }
    if let Some(w) = &args.whisper {
        s.audio.whisper = w.clone();
    }
    if let Some(path) = &args.rec_settings {
        s.recording = serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("lecture de {path}"))?,
        )
        .with_context(|| format!("réglages d'enregistrement : {path}"))?;
    }
    Ok(())
}

fn console(s: &config::Settings) -> anyhow::Result<Box<dyn AudioConsole>> {
    let a = &s.console.address;
    Ok(match s.console.kind.as_str() {
        "m32" => Box::new(M32::connect(with_port(
            a,
            strata_studio_devices::m32::PORT,
        )?)?),
        "dm5" | "yamaha" => Box::new(Yamaha::connect(with_port(
            a,
            strata_studio_devices::yamaha::PORT,
        )?)?),
        "sim" | "" => Box::new(SimConsole::default()),
        k => anyhow::bail!("console inconnue : {k} (m32 ou dm5)"),
    })
}

fn automation(s: &config::Settings) -> anyhow::Result<Box<dyn Automation>> {
    Ok(if s.buttons.address.is_empty() {
        Box::new(SimAutomation::default())
    } else {
        Box::new(Buttons::new(with_port(&s.buttons.address, 8000)?))
    })
}

fn cameras(s: &config::Settings) -> anyhow::Result<BTreeMap<CamId, Box<dyn Ptz>>> {
    let travel = Duration::from_millis(s.cameras.travel_ms);
    s.setup
        .cameras
        .iter()
        .map(|c| -> anyhow::Result<(CamId, Box<dyn Ptz>)> {
            let first = if c.id.0 == 1 { 1 } else { c.id.0 - 1 };
            let ptz: Box<dyn Ptz> = match (s.cameras.control.as_str(), &c.address) {
                ("visca", Some(a)) if !a.is_empty() => Box::new(ViscaPtz::connect(
                    c.label.clone(),
                    with_port(a, strata_studio_devices::visca::PORT)?,
                    travel,
                )?),
                _ => Box::new(SimPtz::new(c.label.clone(), first, travel)),
            };
            Ok((c.id, ptz))
        })
        .collect()
}

fn nodal(s: &config::Settings) -> anyhow::Result<Box<dyn Nodal>> {
    let r = &s.router;
    Ok(match r.kind.as_str() {
        "videohub" => Box::new(strata_studio_devices::videohub::Videohub::connect(
            with_port(&r.address, strata_studio_devices::videohub::PORT)?,
        )?),
        "nmos" => Box::new(strata_studio_devices::nmos::NmosRouter::new(
            r.senders.clone(),
            r.receivers.clone(),
        )),
        "ndi" => Box::new(strata_studio_devices::ndi::NdiRouter::load(&r.ndi_prefix)?),
        "sim" | "" => Box::new(SimRouter::default()),
        k => anyhow::bail!("nodal inconnu : {k} (videohub, nmos ou ndi)"),
    })
}

fn externals(s: &config::Settings) -> shoot::Ext {
    let gos = s
        .gos
        .iter()
        .map(|g| -> Box<dyn GoEncoder> {
            if g.address.is_empty() {
                Box::new(SimEncoder::new(g.label.clone(), g.sim_busy))
            } else {
                Box::new(BlackmagicEncoder::new(g.label.clone(), &g.address))
            }
        })
        .collect();
    let c = &s.calls;
    let calls: Box<dyn Calls> = if c.api.is_empty() {
        Box::new(SimCalls::default())
    } else {
        Box::new(Quicklink::new(&c.api, &c.username, &c.password, &c.room))
    };
    shoot::Ext { gos, calls }
}

/// What the audio start gives: the voices, the ear's output, the
/// microphones' stream (channels, rate).
type Audio = (Option<SharedVoices>, EarOut, Option<(usize, u32)>);

/// The real microphones' voices, shared with the thread receiving them.
type SharedVoices = Arc<Mutex<Voices>>;
/// What the host's ear heard, shared with the thread transcribing.
type EarOut = Arc<Mutex<Vec<Heard>>>;

/// The microphones from the AES67 stream, and the host's ear on its channel.
fn start_audio(
    audio: &config::AudioSettings,
    host: SeatId,
    host_channel: u16,
    mic_sink: record::MicSink,
) -> anyhow::Result<Audio> {
    let heard = Arc::new(Mutex::new(Vec::new()));
    if audio.sdp.is_empty() {
        return Ok((None, heard, None));
    }
    let path = &audio.sdp;
    let sdp = std::fs::read_to_string(path).with_context(|| format!("lecture de {path}"))?;
    let stream = aes67::parse_sdp(&sdp)?;
    let iface: std::net::Ipv4Addr = audio.interface.parse().context("interface AES67")?;
    let rx = aes67::receive(&stream, iface)?;
    tracing::info!(
        "micros : « {} », {} canaux, {}:{}",
        stream.name,
        stream.channels,
        stream.group,
        stream.port
    );
    let voices = Arc::new(Mutex::new(Voices::new(
        stream.channels as usize,
        stream.rate,
        VoiceSettings::default(),
    )));

    let ear_tx = if audio.whisper.is_empty() {
        None
    } else {
        let addr = with_port(&audio.whisper, 8178).context("serveur whisper")?;
        let (tx, ear_rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let out = heard.clone();
        let rate = stream.rate;
        std::thread::Builder::new()
            .name("oreille".into())
            .spawn(move || {
                let mut ear = HostEar::new(host, rate, WhisperServer::new(addr));
                for block in ear_rx {
                    let got = ear.hear(&block);
                    if !got.is_empty() {
                        out.lock().unwrap().extend(got);
                    }
                }
            })?;
        tracing::info!("oreille de l'animateur : whisper sur {addr}");
        Some(tx)
    };
    let v = voices.clone();
    let channels = stream.channels as usize;
    std::thread::Builder::new()
        .name("micros".into())
        .spawn(move || {
            for block in rx {
                v.lock().unwrap().feed(&block);
                if let Some(m) = mic_sink.lock().unwrap().as_mut() {
                    if let Err(e) = m.feed(&block) {
                        tracing::warn!("micros : {e}");
                    }
                }
                if let Some(tx) = &ear_tx {
                    let host: Vec<f32> = block
                        .iter()
                        .skip(host_channel as usize)
                        .step_by(channels)
                        .copied()
                        .collect();
                    if tx.send(host).is_err() {
                        break;
                    }
                }
            }
        })?;
    let layout = (stream.channels as usize, stream.rate);
    Ok((Some(voices), heard, Some(layout)))
}

/// The switcher: a real one when its address is given, else the simulated one.
fn switcher(s: &config::Settings) -> anyhow::Result<Box<dyn Switcher>> {
    let sw = &s.switcher;
    Ok(match sw.kind.as_str() {
        "atem" => Box::new(AtemSwitcher::connect(
            with_port(&sw.address, strata_studio_devices::atem::PORT)?,
            AtemLayout::default(),
        )?),
        "vmix" => Box::new(VmixSwitcher::connect(
            with_port(&sw.address, strata_studio_devices::vmix::PORT)?,
            VmixLayout {
                split_input: sw.vmix_split,
                pip_input: sw.vmix_pip,
            },
        )?),
        "sim" | "" => Box::new(SimSwitcher::new()),
        k => anyhow::bail!("mélangeur inconnu : {k} (atem ou vmix)"),
    })
}

/// The program's frame period.
const TICK: Duration = Duration::from_millis(40);
const JOURNAL: usize = 200;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    t_ms: u64,
    text: String,
    /// Sent to the devices (false: shadow mode, or a person had the hand).
    sent: bool,
}

struct App {
    director: Director,
    control: Control,
    switcher: Box<dyn Switcher>,
    ptz: BTreeMap<CamId, Box<dyn Ptz>>,
    console: Box<dyn AudioConsole>,
    automation: Box<dyn Automation>,
    script: Script,
    start: Instant,
    last: Perception,
    journal: VecDeque<Entry>,
    listener: Listener,
    heard: Vec<Hint>,
    remote_voice: BTreeMap<RemoteId, bool>,
    /// Slow movements under way: when to stop them.
    zoom_until: BTreeMap<CamId, u64>,
    drive_until: BTreeMap<CamId, u64>,
    /// The real microphones, when an AES67 stream is received.
    voices: Option<Arc<Mutex<Voices>>>,
    /// What the host's ear heard, not yet listened to.
    ear_out: Arc<Mutex<Vec<Heard>>>,
    /// The recording desk.
    desk: Option<record::Desk>,
    settings: config::Settings,
    settings_path: std::path::PathBuf,
    data: std::path::PathBuf,
    ffmpeg: String,
    /// Every device simulated (a studio to try Studio).
    simulated: bool,
    router: Box<dyn Nodal>,
    session: session::Session,
    history: history::History,
    capture: Option<shoot::Capture>,
    lives: BTreeMap<String, shoot::LiveRun>,
    ext: shoot::ExtView,
    jobs: Vec<shoot::Job>,
    deck_armed_until: Option<Instant>,
    deck_tx: Option<std::sync::mpsc::Sender<Vec<(String, String)>>>,
    deck_last: deck::DeckView,
    disks: shoot::Disks,
}

type Shared = Arc<Mutex<App>>;

/// The devices an [`App`] is built with.
struct Devices {
    switcher: Box<dyn Switcher>,
    console: Box<dyn AudioConsole>,
    automation: Box<dyn Automation>,
    ptz: BTreeMap<CamId, Box<dyn Ptz>>,
    router: Box<dyn Nodal>,
}

impl App {
    fn new(
        profile: Profile,
        settings: config::Settings,
        data: std::path::PathBuf,
        devices: Devices,
    ) -> Self {
        let setup: StudioSetup = settings.setup.clone();
        let Devices {
            switcher,
            console,
            automation,
            ptz,
            router,
        } = devices;
        App {
            simulated: settings.simulated(),
            settings_path: data.join("reglages.json"),
            history: history::History::new(&data),
            data,
            settings,
            ffmpeg: "ffmpeg".into(),
            router,
            session: session::Session::default(),
            capture: None,
            lives: BTreeMap::new(),
            ext: shoot::ExtView::default(),
            jobs: Vec::new(),
            deck_armed_until: None,
            deck_tx: None,
            deck_last: deck::DeckView::default(),
            disks: shoot::Disks::default(),
            listener: Listener::new(&setup),
            heard: Vec::new(),
            remote_voice: BTreeMap::new(),
            zoom_until: BTreeMap::new(),
            drive_until: BTreeMap::new(),
            voices: None,
            ear_out: Arc::new(Mutex::new(Vec::new())),
            desk: None,
            director: Director::new(setup, profile),
            control: Control::default(),
            switcher,
            ptz,
            console,
            automation,
            script: Script::demo(),
            start: Instant::now(),
            last: Perception::default(),
            journal: VecDeque::new(),
        }
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    fn log(&mut self, text: String, sent: bool) {
        tracing::info!("{text}");
        self.capture_note(&text, sent);
        self.journal.push_back(Entry {
            t_ms: self.now(),
            text,
            sent,
        });
        while self.journal.len() > JOURNAL {
            self.journal.pop_front();
        }
    }

    /// The camera framed on a seat's close shot, if any (else the wide one).
    fn camera_on(&self, seat: SeatId) -> Option<CamId> {
        let setup = self.director.setup();
        let framed = |kind: ShotKind| {
            setup
                .shots
                .iter()
                .find(|s| {
                    s.kind == kind
                        && self
                            .last
                            .cam(s.camera)
                            .is_some_and(|c| c.framed == Some(s.id))
                })
                .map(|s| s.camera)
        };
        framed(ShotKind::Single(seat)).or_else(|| framed(ShotKind::Wide))
    }

    /// A composite as switcher inputs: seats by the cameras given (in
    /// order), remote sources by their own input.
    fn shown(&self, c: &Composite, cameras: &[CamId]) -> Option<Shown> {
        let setup = self.director.setup();
        let mut cams = cameras.iter();
        let mut input = |s: &Subject| -> Option<u16> {
            match s {
                Subject::Seat(_) => cams
                    .next()
                    .and_then(|c| setup.camera(*c))
                    .map(|c| c.switcher_input),
                Subject::Remote(r) => setup.remote(*r).map(|r| r.switcher_input),
            }
        };
        Some(match c {
            Composite::Full { remote } => Shown::Input(setup.remote(*remote)?.switcher_input),
            Composite::Split { left, right } => Shown::Split {
                left: input(left)?,
                right: input(right)?,
            },
            Composite::Pip { main, inset } => Shown::Pip {
                main: input(main)?,
                inset: input(inset)?,
            },
        })
    }

    fn composite_label(&self, c: &Composite) -> String {
        let setup = self.director.setup();
        let name = |s: &Subject| setup.subject_name(*s);
        match c {
            Composite::Full { remote } => format!("{} plein cadre", setup.remote_name(*remote)),
            Composite::Split { left, right } => format!("{} | {} (50/50)", name(left), name(right)),
            Composite::Pip { main, inset } => {
                format!("{} avec {} en vignette", name(main), name(inset))
            }
        }
    }

    fn shot_label(&self, shot: ShotId) -> String {
        let setup = self.director.setup();
        let Some(s) = setup.shot(shot) else {
            return format!("plan {}", shot.0);
        };
        let cam = setup
            .camera(s.camera)
            .map_or_else(|| format!("Cam {}", s.camera.0), |c| c.label.clone());
        let what = match &s.kind {
            ShotKind::Wide => "large".to_string(),
            ShotKind::Single(seat) => setup.seat_name(*seat),
            ShotKind::Group(g) => g
                .iter()
                .map(|s| setup.seat_name(*s))
                .collect::<Vec<_>>()
                .join(", "),
        };
        format!("{cam} · {what}")
    }

    fn perceive(&mut self) -> Perception {
        let t = self.now();
        let setup = self.director.setup();
        let cams = setup
            .cameras
            .iter()
            .map(|c| {
                let st = self
                    .ptz
                    .get_mut(&c.id)
                    .map(|p| p.status())
                    .unwrap_or_default();
                let framed = st
                    .preset
                    .and_then(|preset| {
                        setup
                            .shots
                            .iter()
                            .find(|s| s.camera == c.id && s.preset == Some(preset))
                    })
                    .map(|s| s.id);
                CamState {
                    cam: c.id,
                    online: st.online,
                    moving: st.moving,
                    framed,
                    subject: None,
                }
            })
            .collect();
        let program = self
            .switcher
            .program()
            .and_then(|i| setup.camera_on_input(i))
            .map(|c| c.id);
        let mics = match &self.voices {
            Some(v) => v
                .lock()
                .unwrap()
                .mics(setup.seats.iter().map(|s| (s.id, s.mic.dante_channel))),
            None => {
                // The simulated conversation: only the seats taken speak.
                let mut m = self.script.mics(setup.seats.iter().map(|s| s.id), t);
                for x in &mut m {
                    if setup.seat(x.seat).is_none_or(|s| s.guest.is_none()) {
                        x.voice = false;
                        x.level_db = -60.0;
                    }
                }
                m
            }
        };
        let remotes = setup
            .remotes
            .iter()
            .map(|r| RemoteState {
                remote: r.id,
                online: true,
                voice: self.remote_voice.get(&r.id).copied().unwrap_or(false),
            })
            .collect();
        Perception {
            t_ms: t,
            mics,
            cams,
            program,
            remotes,
            hints: std::mem::take(&mut self.heard),
            ..Default::default()
        }
    }

    fn tick(&mut self) {
        let now = self.now();
        let heard: Vec<Heard> = std::mem::take(&mut *self.ear_out.lock().unwrap());
        for h in heard {
            if h.is_final {
                self.capture_heard(&h.text);
            }
            let hints = self.listener.hear(&h);
            if hints.is_empty() {
                self.log(format!("entendu : « {} »", h.text), true);
            }
            self.heard.extend(hints);
        }
        for (cam, until) in self.zoom_until.clone() {
            if now >= until {
                self.zoom_until.remove(&cam);
                if let Some(ptz) = self.ptz.get_mut(&cam) {
                    let _ = ptz.zoom(0.0);
                }
            }
        }
        for (cam, until) in self.drive_until.clone() {
            if now >= until {
                self.drive_until.remove(&cam);
                if let Some(ptz) = self.ptz.get_mut(&cam) {
                    let _ = ptz.drive(0.0, 0.0);
                }
            }
        }
        let p = self.perceive();
        let decisions = self.director.tick(&p, &self.control);
        self.last = p;
        for d in decisions {
            self.execute(d);
        }
        self.tick_session();
    }

    fn execute(&mut self, d: Decision) {
        let auto = self.control.mode == Mode::Auto;
        let shadow = if self.control.mode == Mode::Shadow {
            " (ombre)"
        } else {
            ""
        };
        match d {
            Decision::Cut {
                shot,
                camera,
                reason,
            } => {
                let why = reason.describe(self.director.setup());
                if auto {
                    let input = self
                        .director
                        .setup()
                        .camera(camera)
                        .map(|c| c.switcher_input);
                    if let Some(Err(e)) = input.map(|i| self.switcher.cut(i)) {
                        self.log(format!("coupe refusée : {e}"), false);
                    }
                }
                self.log(
                    format!("COUPE → {}{shadow} — {why}", self.shot_label(shot)),
                    auto,
                );
            }
            Decision::Prepare {
                camera,
                shot,
                preset,
                reason,
            } => {
                let why = reason.describe(self.director.setup());
                if auto {
                    if let Some(Err(e)) = self.ptz.get_mut(&camera).map(|p| p.recall(preset)) {
                        self.log(format!("preset refusé : {e}"), false);
                    }
                }
                self.log(
                    format!("prépare {}{shadow} — {why}", self.shot_label(shot)),
                    auto,
                );
            }
            Decision::Speaker { seat } => {
                let name = seat
                    .map(|s| self.director.setup().seat_name(s))
                    .unwrap_or_default();
                let _ = self.automation.set_variable("studio_speaker", &name);
                self.capture_speaker(&name);
                self.log(format!("parole : {name}"), true);
            }
            Decision::Compose {
                program,
                cameras,
                reason,
            } => {
                let why = reason.describe(self.director.setup());
                let what = self.composite_label(&program);
                let cams: Vec<String> = cameras.iter().map(|c| format!("Cam {}", c.0)).collect();
                if auto {
                    match self
                        .shown(&program, &cameras)
                        .map(|s| self.switcher.show(s))
                    {
                        Some(Err(e)) => self.log(format!("mise en page refusée : {e}"), false),
                        None => {
                            self.log("mise en page impossible : une entrée manque".into(), false)
                        }
                        _ => {}
                    }
                }
                self.log(
                    format!("COMPOSE → {what} [{}]{shadow} — {why}", cams.join(", ")),
                    auto,
                );
            }
            Decision::Preload { program, .. } => {
                let what = self.composite_label(&program);
                if auto {
                    // Which camera shows the host is only known when it is taken.
                    let host_cam = self
                        .director
                        .setup()
                        .host_seat()
                        .and_then(|h| self.camera_on(h));
                    if let Some(s) = self.shown(&program, &host_cam.into_iter().collect::<Vec<_>>())
                    {
                        let _ = self.switcher.preload(s);
                    }
                }
                self.log(format!("prépare en coulisse : {what}{shadow}"), auto);
            }
            Decision::Drift {
                camera,
                zoom,
                for_ms,
                reason,
            } => {
                if auto {
                    let until = self.now() + for_ms;
                    if let Some(ptz) = self.ptz.get_mut(&camera) {
                        let _ = ptz.zoom(zoom);
                        self.zoom_until.insert(camera, until);
                    }
                }
                let why = reason.describe(self.director.setup());
                self.log(format!("Cam {} : {why}{shadow}", camera.0), auto);
            }
            Decision::Follow {
                camera,
                pan,
                tilt,
                reason,
            } => {
                if auto {
                    let until = self.now() + 400;
                    if let Some(ptz) = self.ptz.get_mut(&camera) {
                        let _ = ptz.drive(pan, tilt);
                        self.drive_until.insert(camera, until);
                    }
                }
                let why = reason.describe(self.director.setup());
                self.log(
                    format!("Cam {} : {why} ({pan:+.2}, {tilt:+.2}){shadow}", camera.0),
                    auto,
                );
            }
            Decision::Understood { hint } => {
                self.log(
                    format!(
                        "compris : « {} » ({:?}, {:?}, {:.0} %)",
                        hint.heard,
                        hint.target,
                        hint.when,
                        hint.confidence * 100.0
                    ),
                    true,
                );
            }
            Decision::HumanTookOver { program } => {
                self.control.mode = Mode::Manual {
                    by: "pupitre".into(),
                };
                let cam = program.map_or_else(String::new, |c| format!(" (Cam {})", c.0));
                let _ = self.automation.set_variable("studio_mode", "MANUEL");
                self.log(
                    format!("REPRISE EN MAIN au pupitre{cam} : l'IA se suspend"),
                    true,
                );
                self.scenario(scenario::Event::Manual);
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateView {
    t_ms: u64,
    mode: Mode,
    profile: String,
    on_air: Option<String>,
    program: Option<CamId>,
    speaker: Option<String>,
    control: Control,
    perception: Perception,
    journal: Vec<Entry>,
}

async fn state(State(app): State<Shared>) -> Json<StateView> {
    let app = app.lock().unwrap();
    Json(StateView {
        t_ms: app.now(),
        mode: app.control.mode.clone(),
        profile: app.director.profile().name.clone(),
        on_air: app.director.on_air().map(|s| app.shot_label(s)),
        program: app.last.program,
        speaker: app
            .director
            .speaker()
            .map(|s| app.director.setup().seat_name(s)),
        control: app.control.clone(),
        perception: app.last.clone(),
        journal: app.journal.iter().rev().take(50).cloned().collect(),
    })
}

async fn setup(State(app): State<Shared>) -> Json<StudioSetup> {
    Json(app.lock().unwrap().director.setup().clone())
}

async fn guests(State(app): State<Shared>, Json(names): Json<HashMap<u8, String>>) -> StatusCode {
    let mut app = app.lock().unwrap();
    let mut strips = Vec::new();
    for seat in &mut app.director.setup_mut().seats {
        if let Some(name) = names.get(&seat.id.0) {
            seat.guest = (!name.trim().is_empty()).then(|| name.trim().to_string());
            strips.push((seat.mic.console_channel, seat.display_name().to_string()));
        }
    }
    app.listener = Listener::new(app.director.setup());
    for (ch, name) in strips {
        let _ = app.console.set_name(ch, &name);
    }
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct By {
    by: Option<String>,
}

#[derive(Deserialize)]
struct Seconds {
    seconds: Option<u64>,
}

async fn set_mode(app: &Shared, mode: Mode) -> StatusCode {
    let mut app = app.lock().unwrap();
    let label = match &mode {
        Mode::Auto => "l'IA a la main".to_string(),
        Mode::Shadow => "l'IA en ombre : elle décide, rien n'est envoyé".to_string(),
        Mode::Manual { by } => format!("{by} prend la main"),
    };
    let _ = app.automation.set_variable(
        "studio_mode",
        if mode == Mode::Auto { "IA" } else { "MANUEL" },
    );
    app.control.mode = mode;
    app.control.forced = None;
    app.log(label, true);
    StatusCode::NO_CONTENT
}

async fn auto(State(app): State<Shared>) -> StatusCode {
    set_mode(&app, Mode::Auto).await
}

async fn shadow(State(app): State<Shared>) -> StatusCode {
    set_mode(&app, Mode::Shadow).await
}

async fn take(State(app): State<Shared>, Query(q): Query<By>) -> StatusCode {
    set_mode(
        &app,
        Mode::Manual {
            by: q.by.unwrap_or_else(|| "le réalisateur".into()),
        },
    )
    .await
}

async fn force(
    State(app): State<Shared>,
    Path(shot): Path<u16>,
    Query(q): Query<Seconds>,
) -> StatusCode {
    let mut app = app.lock().unwrap();
    if app.director.setup().shot(ShotId(shot)).is_none() {
        return StatusCode::NOT_FOUND;
    }
    let until_ms = app.now() + q.seconds.unwrap_or(10) * 1000;
    app.control.forced = Some(Forced {
        shot: ShotId(shot),
        until_ms,
    });
    let label = app.shot_label(ShotId(shot));
    app.log(format!("plan imposé : {label}"), true);
    StatusCode::NO_CONTENT
}

async fn lock(State(app): State<Shared>, Path(cam): Path<u8>) -> StatusCode {
    let mut app = app.lock().unwrap();
    app.control.locked_cams.insert(CamId(cam));
    app.log(
        format!("Cam {cam} verrouillée : l'IA ne la prend plus et ne la bouge plus"),
        true,
    );
    StatusCode::NO_CONTENT
}

async fn unlock(State(app): State<Shared>, Path(cam): Path<u8>) -> StatusCode {
    let mut app = app.lock().unwrap();
    app.control.locked_cams.remove(&CamId(cam));
    app.log(format!("Cam {cam} rendue à l'IA"), true);
    StatusCode::NO_CONTENT
}

async fn profile(State(app): State<Shared>, Path(n): Path<usize>) -> StatusCode {
    let Some(p) = Profile::all().into_iter().nth(n) else {
        return StatusCode::NOT_FOUND;
    };
    let mut app = app.lock().unwrap();
    let name = p.name.clone();
    app.director.set_profile(p);
    app.log(format!("profil : {name}"), true);
    StatusCode::NO_CONTENT
}

async fn panel_cut(State(app): State<Shared>, Path(input): Path<u16>) -> StatusCode {
    if !app.lock().unwrap().switcher.demo_panel_cut(input) {
        return StatusCode::CONFLICT;
    }
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct Say {
    text: String,
    #[serde(default)]
    seat: Option<u8>,
}

async fn say(State(app): State<Shared>, Json(said): Json<Say>) -> StatusCode {
    let mut app = app.lock().unwrap();
    let seat = said
        .seat
        .map(SeatId)
        .or_else(|| app.director.setup().host_seat())
        .unwrap_or(SeatId(1));
    let heard = Heard {
        t_ms: app.now(),
        seat,
        text: said.text,
        is_final: true,
    };
    let hints = app.listener.hear(&heard);
    if hints.is_empty() {
        app.log(format!("entendu, rien à faire : « {} »", heard.text), true);
    }
    app.heard.extend(hints);
    StatusCode::NO_CONTENT
}

async fn record_estimate(
    State(app): State<Shared>,
    Json(req): Json<record::RecordRequest>,
) -> Json<serde_json::Value> {
    let app = app.lock().unwrap();
    let hours = app.desk.as_ref().and_then(|d| d.estimate(&req));
    Json(serde_json::json!({ "hours": hours }))
}

async fn record_start(
    State(app): State<Shared>,
    Json(req): Json<record::RecordRequest>,
) -> (StatusCode, String) {
    let mut app = app.lock().unwrap();
    let setup = app.director.setup().clone();
    let mics: Vec<(String, u16)> = setup
        .seats
        .iter()
        .filter(|s| s.guest.is_some())
        .map(|s| (s.display_name().to_string(), s.mic.dante_channel))
        .collect();
    let programme = req.programme.clone();
    let Some(desk) = app.desk.as_mut() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "pas de pupitre d'enregistrement".into(),
        );
    };
    match desk.start(req, &mics) {
        Ok(()) => {
            app.log(format!("ENREGISTREMENT lancé : {programme}"), true);
            (StatusCode::NO_CONTENT, String::new())
        }
        Err(e) => (StatusCode::CONFLICT, e),
    }
}

async fn record_stop(State(app): State<Shared>) -> (StatusCode, Json<serde_json::Value>) {
    let mut app = app.lock().unwrap();
    let Some(desk) = app.desk.as_mut() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "pas de pupitre" })),
        );
    };
    match desk.stop() {
        Ok(channels) => {
            app.log("ENREGISTREMENT arrêté".into(), true);
            (
                StatusCode::OK,
                Json(serde_json::json!({ "channels": channels })),
            )
        }
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": e })),
        ),
    }
}

async fn record_status(State(app): State<Shared>) -> Json<serde_json::Value> {
    let mut app = app.lock().unwrap();
    match app.desk.as_mut() {
        Some(d) => Json(serde_json::to_value(d.status()).unwrap_or_default()),
        None => Json(serde_json::json!({ "recording": false })),
    }
}

async fn remote_voice(State(app): State<Shared>, Path((id, on)): Path<(u8, u8)>) -> StatusCode {
    let mut app = app.lock().unwrap();
    if app.director.setup().remote(RemoteId(id)).is_none() {
        return StatusCode::NOT_FOUND;
    }
    app.remote_voice.insert(RemoteId(id), on != 0);
    StatusCode::NO_CONTENT
}

fn router(ctx: api::Ctx) -> Router {
    Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/state", get(state))
        .route("/api/setup", get(setup))
        .route("/api/setup/guests", put(guests))
        .route("/api/control/auto", post(auto))
        .route("/api/control/shadow", post(shadow))
        .route("/api/control/take", post(take))
        .route("/api/control/force/{shot}", post(force))
        .route("/api/control/lock/{cam}", post(lock))
        .route("/api/control/unlock/{cam}", post(unlock))
        .route("/api/control/profile/{n}", post(profile))
        .route("/api/demo/panel-cut/{input}", post(panel_cut))
        .route("/api/demo/say", post(say))
        .route("/api/record/estimate", post(record_estimate))
        .route("/api/record/start", post(record_start))
        .route("/api/record/stop", post(record_stop))
        .route("/api/record/status", get(record_status))
        .route("/api/demo/remote/{id}/voice/{on}", post(remote_voice))
        .merge(api::routes())
        .merge(web::routes())
        .with_state(ctx)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let data = std::path::PathBuf::from(&args.data);
    std::fs::create_dir_all(&data).with_context(|| format!("dossier {}", data.display()))?;
    let settings_path = data.join("reglages.json");
    let mut settings = config::Settings::load(&settings_path)?;
    apply_args(&mut settings, &args)?;
    tracing::info!("réglages : {}", settings_path.display());
    let profile = settings
        .profiles
        .get(args.profile)
        .cloned()
        .or_else(|| Profile::all().into_iter().nth(args.profile))
        .context("profil inconnu")?;
    let sw = switcher(&settings)?;
    tracing::info!("mélangeur : {}", sw.name());
    let console = console(&settings)?;
    let buttons = automation(&settings)?;
    let router_dev = nodal(&settings)?;
    tracing::info!(
        "console : {} · automations : {} · nodal : {}",
        console.name(),
        buttons.name(),
        router_dev.name()
    );
    let devices = Devices {
        switcher: sw,
        console,
        automation: buttons,
        ptz: cameras(&settings)?,
        router: router_dev,
    };
    let ext: shoot::SharedExt = Arc::new(Mutex::new(externals(&settings)));
    let deck_tx = if settings.buttons.deck && !settings.buttons.address.is_empty() {
        Some(shoot::deck_pusher(automation(&settings)?))
    } else {
        None
    };
    let simulated = settings.simulated();
    let app: Shared = Arc::new(Mutex::new(App::new(profile, settings, data, devices)));
    {
        let mut a = app.lock().unwrap();
        a.deck_tx = deck_tx;
        a.ffmpeg = args.ffmpeg.clone();
        let setup = a.director.setup().clone();
        let host = setup.host_seat().unwrap_or(SeatId(1));
        let host_channel = setup.seat(host).map_or(0, |s| s.mic.dante_channel);
        let mic_sink: record::MicSink = Arc::new(Mutex::new(None));
        let (voices, heard, stream) =
            start_audio(&a.settings.audio, host, host_channel, mic_sink.clone())?;
        a.voices = voices;
        a.ear_out = heard;
        let rec_settings = a.settings.recording.clone();
        a.desk = Some(record::Desk::new(
            args.rec_dir.clone().into(),
            args.ffmpeg.clone(),
            rec_settings,
            mic_sink,
            stream,
        ));
    }
    if simulated {
        let mut a = app.lock().unwrap();
        let names = [
            (1u8, "Paul"),
            (2, "Alice"),
            (3, "Karim"),
            (4, "Sophie"),
            (5, "Léa"),
        ];
        for seat in &mut a.director.setup_mut().seats {
            seat.guest = names
                .iter()
                .find(|(n, _)| SeatId(*n) == seat.id)
                .map(|(_, g)| g.to_string());
        }
        if let Some(duplex) = a
            .director
            .setup_mut()
            .remotes
            .iter_mut()
            .find(|r| r.id == RemoteId(2))
        {
            duplex.guest = Some("Julie Roux".into());
        }
        a.listener = Listener::new(a.director.setup());
        a.log(
            "studio simulé : 5 places, 4 PTZ, conversation de démonstration".into(),
            true,
        );
    }

    let ticker = app.clone();
    let ticker_ext = ext.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(TICK);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            every.tick().await;
            let jobs = {
                let mut a = ticker.lock().unwrap();
                a.tick();
                std::mem::take(&mut a.jobs)
            };
            for job in jobs {
                let (app, ext) = (ticker.clone(), ticker_ext.clone());
                std::thread::spawn(move || shoot::run_job(app, ext, job));
            }
        }
    });
    {
        let (app, ext) = (app.clone(), ext.clone());
        std::thread::Builder::new()
            .name("encodeurs et invités".into())
            .spawn(move || shoot::poll(app, ext))?;
    }

    let listener = tokio::net::TcpListener::bind(&args.listen)
        .await
        .with_context(|| format!("écoute sur {}", args.listen))?;
    tracing::info!("Strata Studio sur http://{}", args.listen);
    let at_end = app.clone();
    let ctx = api::Ctx { app };
    axum::serve(listener, router(ctx))
        .with_graceful_shutdown(stop_signal())
        .await?;
    // Stopped by systemd or Ctrl-C during a shoot: close the files properly.
    let mut app = at_end.lock().unwrap();
    if app.session.shooting.is_some() {
        let _ = app.end();
        tracing::info!("tournage terminé à l'arrêt du serveur");
    } else if let Some(desk) = app.desk.as_mut() {
        if desk.stop().is_ok() {
            tracing::info!("enregistrement arrêté à l'arrêt du serveur");
        }
    }
    Ok(())
}

/// Ctrl-C, or SIGTERM (what systemd sends to stop the service).
async fn stop_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal SIGTERM");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
