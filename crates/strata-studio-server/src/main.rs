//! strata-studio: the Strata Studio server.
//!
//! For now it runs a **simulated studio** (switcher, four PTZ, console,
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
//! | `POST /api/demo/say` | simulate the host's words (`{"text": "On retrouve Julie en duplex"}`), as the speech recogniser would give them |
//! | `POST /api/demo/remote/{id}/voice/{on}` | simulate a remote source speaking (`on` = 1) or silent (0) |

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
use strata_studio_devices::sim::{SimAutomation, SimConsole, SimPtz, SimSwitcher};
use strata_studio_devices::{AudioConsole, Automation, Ptz, Switcher};

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
    /// Directing profile: 0 podcast, 1 débat, 2 interview, 3 radio.
    #[arg(long, default_value_t = 0)]
    profile: usize,
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
    switcher: SimSwitcher,
    ptz: BTreeMap<CamId, SimPtz>,
    console: SimConsole,
    automation: SimAutomation,
    script: Script,
    start: Instant,
    last: Perception,
    journal: VecDeque<Entry>,
    listener: Listener,
    heard: Vec<Hint>,
    remote_voice: BTreeMap<RemoteId, bool>,
}

type Shared = Arc<Mutex<App>>;

impl App {
    fn new(profile: Profile) -> Self {
        let setup = StudioSetup::example();
        let first = [(1, 1), (2, 1), (3, 2), (4, 3)];
        let ptz = first
            .into_iter()
            .map(|(cam, preset)| {
                (
                    CamId(cam),
                    SimPtz::new(format!("Cam {cam}"), preset, Duration::from_millis(1200)),
                )
            })
            .collect();
        App {
            listener: Listener::new(&setup),
            heard: Vec::new(),
            remote_voice: BTreeMap::new(),
            director: Director::new(setup, profile),
            control: Control::default(),
            switcher: SimSwitcher::new(),
            ptz,
            console: SimConsole::default(),
            automation: SimAutomation::default(),
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
        self.journal.push_back(Entry {
            t_ms: self.now(),
            text,
            sent,
        });
        while self.journal.len() > JOURNAL {
            self.journal.pop_front();
        }
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
                }
            })
            .collect();
        let program = self
            .switcher
            .program()
            .and_then(|i| setup.camera_on_input(i))
            .map(|c| c.id);
        let mics = self.script.mics(setup.seats.iter().map(|s| s.id), t);
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
        let p = self.perceive();
        let decisions = self.director.tick(&p, &self.control);
        self.last = p;
        for d in decisions {
            self.execute(d);
        }
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
                // The simulated switcher has no layouts: a real one sets its
                // SuperSource (ATEM) or layout input (vMix), then takes it.
                self.log(
                    format!("COMPOSE → {what} [{}]{shadow} — {why}", cams.join(", ")),
                    auto,
                );
            }
            Decision::Preload { program, .. } => {
                let what = self.composite_label(&program);
                self.log(format!("prépare en coulisse : {what}{shadow}"), auto);
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
    app.lock().unwrap().switcher.panel_cut(input);
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

async fn remote_voice(State(app): State<Shared>, Path((id, on)): Path<(u8, u8)>) -> StatusCode {
    let mut app = app.lock().unwrap();
    if app.director.setup().remote(RemoteId(id)).is_none() {
        return StatusCode::NOT_FOUND;
    }
    app.remote_voice.insert(RemoteId(id), on != 0);
    StatusCode::NO_CONTENT
}

fn router(app: Shared) -> Router {
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
        .route("/api/demo/remote/{id}/voice/{on}", post(remote_voice))
        .with_state(app)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let profile = Profile::all()
        .into_iter()
        .nth(args.profile)
        .context("profil inconnu (0 à 3)")?;
    let app: Shared = Arc::new(Mutex::new(App::new(profile)));
    {
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
    tokio::spawn(async move {
        let mut every = tokio::time::interval(TICK);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            every.tick().await;
            ticker.lock().unwrap().tick();
        }
    });

    let listener = tokio::net::TcpListener::bind(&args.listen)
        .await
        .with_context(|| format!("écoute sur {}", args.listen))?;
    tracing::info!("Strata Studio sur http://{}", args.listen);
    axum::serve(listener, router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
