//! The routes of the journey, the history, the administration, the
//! Stream Deck and the previews (the older control routes are in
//! `main.rs`).
//!
//! | Route | |
//! | --- | --- |
//! | `GET /api/live` | the snapshot every 250 ms (Server-Sent Events) |
//! | `GET /api/snapshot` | the same, once |
//! | `POST /api/session/start` · `/next` · `/prev` · `/goto/{step}` · `/reset` | the journey |
//! | `POST /api/session/draft` | change answers (`{"people": 3}`) |
//! | `POST /api/session/programme/{i}` | take a programme's answers back |
//! | `POST /api/session/test/ok` · `/test/retry` | the tests |
//! | `POST /api/session/launch` · `/end` | launch, end the shoot |
//! | `POST /api/deck/key/{n}` · `/api/deck/dial/{n}/{delta}` · `GET /api/deck` | the Stream Deck through Buttons |
//! | `GET /api/tournages` · `/api/tournages/{id}` · `DELETE …` | the shoots |
//! | `GET /api/tournages/{id}/export/{edl,fcpxml,xml,otio}` | the edit |
//! | `POST /api/tournages/{id}/replay` | the AI replayed with a profile |
//! | `GET /api/settings` · `PUT` | the settings (devices change at restart) |
//! | `PUT /api/programmes` · `/api/profiles` · `/api/scenarios` | parts of the settings, applied at once |
//! | `POST /api/scenarios/plan/{event}` · `/run/{event}` | what an event sets off; set it off |
//! | `GET /api/router` · `POST /api/router/route` | the nodal |
//! | `GET /api/system` · `POST /api/system/restart` | the server |
//! | `GET /api/preview/{source}` | a source's picture (MJPEG) |

use crate::config::{Programme, Settings};
use crate::deck;
use crate::history::{self, Stats};
use crate::scenario::{self, Event, Scenario};
use crate::session::DraftPatch;
use crate::Shared;
use axum::body::Body;
use axum::extract::{FromRef, Path, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;
use std::convert::Infallible;
use std::time::Duration;
use strata_studio_core::Profile;

#[derive(Clone)]
pub struct Ctx {
    pub(crate) app: Shared,
}

impl FromRef<Ctx> for Shared {
    fn from_ref(c: &Ctx) -> Shared {
        c.app.clone()
    }
}

type Reply = (StatusCode, Json<serde_json::Value>);

fn done(r: Result<(), String>) -> Reply {
    match r {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": e })),
        ),
    }
}

async fn snapshot(State(app): State<Shared>) -> Json<serde_json::Value> {
    let s = app.lock().unwrap().snapshot();
    Json(serde_json::to_value(s).unwrap_or_default())
}

async fn live(
    State(app): State<Shared>,
) -> Sse<impl futures_core::Stream<Item = Result<SseEvent, Infallible>>> {
    let stream = async_stream(app);
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn async_stream(app: Shared) -> impl futures_core::Stream<Item = Result<SseEvent, Infallible>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<SseEvent, Infallible>>(4);
    tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_millis(250));
        loop {
            every.tick().await;
            let text = {
                let s = app.lock().unwrap().snapshot();
                serde_json::to_string(&s).unwrap_or_default()
            };
            if tx.send(Ok(SseEvent::default().data(text))).await.is_err() {
                break;
            }
        }
    });
    tokio_stream::wrappers::ReceiverStream::new(rx)
}

async fn s_start(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().start_shoot())
}
async fn s_next(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().next())
}
async fn s_prev(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().prev())
}
async fn s_goto(State(app): State<Shared>, Path(step): Path<u8>) -> Reply {
    done(app.lock().unwrap().goto(step))
}
async fn s_reset(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().restart_session())
}
async fn s_draft(State(app): State<Shared>, Json(p): Json<DraftPatch>) -> Reply {
    done(app.lock().unwrap().patch(p))
}
async fn s_programme(State(app): State<Shared>, Path(i): Path<usize>) -> Reply {
    done(app.lock().unwrap().take_programme(i))
}
async fn s_test_ok(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().test_ok())
}
async fn s_test_retry(State(app): State<Shared>) -> Reply {
    app.lock().unwrap().session.test_retry();
    done(Ok(()))
}
async fn s_launch(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().launch())
}
async fn s_end(State(app): State<Shared>) -> Reply {
    done(app.lock().unwrap().end())
}
async fn s_dismiss(State(app): State<Shared>) -> Reply {
    app.lock().unwrap().session.alerts.clear();
    done(Ok(()))
}

async fn deck_get(State(app): State<Shared>) -> Json<deck::DeckView> {
    Json(app.lock().unwrap().deck_view())
}
async fn deck_key(State(app): State<Shared>, Path(n): Path<usize>) -> Reply {
    done(app.lock().unwrap().deck_key(n))
}
async fn deck_dial(State(app): State<Shared>, Path((n, delta)): Path<(usize, i32)>) -> Reply {
    let mut a = app.lock().unwrap();
    let act = deck::dial(&a.session, n, delta.signum());
    done(a.act(act))
}

// ---- the shoots ----

async fn shoots(State(app): State<Shared>) -> Json<serde_json::Value> {
    let list = app.lock().unwrap().history.list();
    Json(serde_json::to_value(list).unwrap_or_default())
}

async fn shoot(State(app): State<Shared>, Path(id): Path<String>) -> Response {
    match app.lock().unwrap().history.get(&id) {
        Some(r) => Json(r).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn shoot_delete(State(app): State<Shared>, Path(id): Path<String>) -> StatusCode {
    let a = app.lock().unwrap();
    if a.session.shooting.as_ref().is_some_and(|(s, _)| *s == id) {
        return StatusCode::CONFLICT;
    }
    if a.history.delete(&id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn export(State(app): State<Shared>, Path((id, fmt)): Path<(String, String)>) -> Response {
    let Some(r) = app.lock().unwrap().history.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let e = history::edit(&r);
    let (text, ext, mime) = match fmt.as_str() {
        "edl" => (e.edl(), "edl", "text/plain; charset=utf-8"),
        "fcpxml" => (e.fcpxml(), "fcpxml", "application/xml"),
        "xml" => (e.premiere_xml(), "xml", "application/xml"),
        "otio" => (e.otio(), "otio", "application/json"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let name = format!("{}.{ext}", r.id);
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        text,
    )
        .into_response()
}

#[derive(Deserialize)]
struct ReplayAsk {
    profile: Profile,
}

async fn replay(
    State(app): State<Shared>,
    Path(id): Path<String>,
    Json(ask): Json<ReplayAsk>,
) -> Response {
    let (record, perceptions) = {
        let a = app.lock().unwrap();
        let Some(r) = a.history.get(&id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let p = a.history.perceptions(&id);
        (r, p)
    };
    let Some(setup) = record.setup.clone() else {
        return (
            StatusCode::CONFLICT,
            "pas de studio enregistré pour ce tournage",
        )
            .into_response();
    };
    if perceptions.is_empty() {
        return (
            StatusCode::CONFLICT,
            "pas de perception enregistrée pour ce tournage",
        )
            .into_response();
    }
    let out = tokio::task::spawn_blocking(move || {
        let duration =
            perceptions.last().map_or(0, |p| p.t_ms) - perceptions.first().map_or(0, |p| p.t_ms);
        let now = history::replay(&setup, record.profile.clone(), &perceptions);
        let new = history::replay(&setup, ask.profile, &perceptions);
        let st = |s: &[strata_studio_media::export::Segment]| -> Stats {
            history::stats(s, duration, &history::is_wide)
        };
        serde_json::json!({
            "durationMs": duration,
            "current": { "segments": now, "stats": st(&now) },
            "proposed": { "segments": new, "stats": st(&new) },
            "agreement": record.agreement,
        })
    })
    .await
    .unwrap_or_default();
    Json(out).into_response()
}

// ---- the settings ----

async fn settings_get(State(app): State<Shared>) -> Json<Settings> {
    Json(app.lock().unwrap().settings.clone())
}

async fn settings_put(State(app): State<Shared>, Json(s): Json<Settings>) -> Reply {
    if let Err(e) = s.setup.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        );
    }
    let mut a = app.lock().unwrap();
    let devices_changed = {
        let mut x = a.settings.clone();
        x.programmes = s.programmes.clone();
        x.profiles = s.profiles.clone();
        x.scenarios = s.scenarios.clone();
        x.storages = s.storages.clone();
        x.live = s.live.clone();
        x.studio = s.studio.clone();
        x != s
    };
    if let Err(e) = s.save(&a.settings_path) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        );
    }
    a.settings = s;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "ok": true, "restart": devices_changed })),
    )
}

async fn save_part(app: &Shared, f: impl FnOnce(&mut Settings)) -> Reply {
    let mut a = app.lock().unwrap();
    let mut s = a.settings.clone();
    f(&mut s);
    if let Err(e) = s.save(&a.settings_path) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        );
    }
    a.settings = s;
    done(Ok(()))
}

async fn programmes_put(State(app): State<Shared>, Json(p): Json<Vec<Programme>>) -> Reply {
    save_part(&app, |s| s.programmes = p).await
}
async fn profiles_put(State(app): State<Shared>, Json(p): Json<Vec<Profile>>) -> Reply {
    if p.is_empty() {
        return done(Err("au moins un profil".into()));
    }
    save_part(&app, |s| s.profiles = p).await
}
async fn scenarios_put(State(app): State<Shared>, Json(p): Json<Vec<Scenario>>) -> Reply {
    save_part(&app, |s| s.scenarios = p).await
}

fn event(name: &str) -> Option<Event> {
    Some(match name {
        "start" => Event::Start,
        "launch" => Event::Launch,
        "end" => Event::End,
        "alert" => Event::Alert,
        "manual" => Event::Manual,
        _ => return None,
    })
}

async fn scenario_plan(State(app): State<Shared>, Path(ev): Path<String>) -> Response {
    let Some(e) = event(&ev) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let a = app.lock().unwrap();
    let ctx = scenario::Context {
        go: Some("GO1".into()),
        live: true,
        vertical: a.session.draft.vertical,
        seats: a.session.placed().iter().map(|(s, _, _)| *s).collect(),
    };
    Json(scenario::plan(&a.settings.scenarios, e, &ctx)).into_response()
}

async fn scenario_run(State(app): State<Shared>, Path(ev): Path<String>) -> Reply {
    let Some(e) = event(&ev) else {
        return done(Err("événement inconnu".into()));
    };
    app.lock().unwrap().scenario(e);
    done(Ok(()))
}

// ---- the nodal, the system ----

async fn router_get(State(app): State<Shared>) -> Json<serde_json::Value> {
    let a = app.lock().unwrap();
    Json(serde_json::json!({ "name": a.router.name(), "routes": a.router.routes() }))
}

#[derive(Deserialize)]
struct RouteAsk {
    destination: String,
    source: String,
}

async fn router_route(State(app): State<Shared>, Json(r): Json<RouteAsk>) -> Reply {
    let mut a = app.lock().unwrap();
    let res = a
        .router
        .route(&r.destination, &r.source)
        .map_err(|e| e.to_string());
    done(res)
}

async fn system(State(app): State<Shared>) -> Json<serde_json::Value> {
    let a = app.lock().unwrap();
    let ffmpeg = std::process::Command::new(&a.ffmpeg)
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .map(String::from)
        });
    Json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptimeS": a.start.elapsed().as_secs(),
        "data": a.data.to_string_lossy(),
        "settingsFile": a.settings_path.to_string_lossy(),
        "ffmpeg": ffmpeg,
        "devices": {
            "switcher": a.switcher.name(),
            "switcherConnected": a.switcher.connected(),
            "console": a.console.name(),
            "automation": a.automation.name(),
            "router": a.router.name(),
            "cameras": a.ptz.values().map(|p| p.name()).collect::<Vec<_>>(),
            "microphones": if a.voices.is_some() { "AES67" } else { "simulés" },
        },
        "ndi": strata_studio_devices::ndi::LIBRARIES.iter().any(|l| unsafe { libloading::Library::new(l) }.is_ok()),
        "simulated": a.simulated,
    }))
}

async fn restart(State(app): State<Shared>) -> Reply {
    if app.lock().unwrap().session.shooting.is_some() {
        return done(Err("tournage en cours".into()));
    }
    tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(400)).await;
        tracing::info!("redémarrage demandé depuis l'administration");
        std::process::exit(0);
    });
    done(Ok(()))
}

// ---- the previews ----

async fn preview(State(app): State<Shared>, Path(source): Path<String>) -> Response {
    let (ffmpeg, input) = {
        let a = app.lock().unwrap();
        let ch = a
            .settings
            .recording
            .channels
            .iter()
            .find(|c| {
                c.name
                    .eq_ignore_ascii_case(source.trim_end_matches(".mjpg"))
            })
            .map(|c| c.input.clone());
        (a.ffmpeg.clone(), ch)
    };
    let Some(input) = input else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let args = strata_studio_media::live::preview_args(&input, 12, 640);
    let child = tokio::process::Command::new(ffmpeg)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let Ok(mut child) = child else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(out) = child.stdout.take() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    // The child is dropped (and killed) with the stream, when the page goes.
    let stream = tokio_util::io::ReaderStream::new(out);
    let guarded = KeepChild {
        stream,
        _child: child,
    };
    (
        [
            (
                header::CONTENT_TYPE,
                "multipart/x-mixed-replace;boundary=ffmpeg",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(guarded),
    )
        .into_response()
}

/// A stream that keeps its ffmpeg alive as long as it is read.
struct KeepChild<S> {
    stream: S,
    _child: tokio::process::Child,
}

impl<S: futures_core::Stream + Unpin> futures_core::Stream for KeepChild<S> {
    type Item = S::Item;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.stream).poll_next(cx)
    }
}

pub fn routes() -> Router<Ctx> {
    Router::new()
        .route("/api/live", get(live))
        .route("/api/snapshot", get(snapshot))
        .route("/api/session/start", post(s_start))
        .route("/api/session/next", post(s_next))
        .route("/api/session/prev", post(s_prev))
        .route("/api/session/goto/{step}", post(s_goto))
        .route("/api/session/reset", post(s_reset))
        .route("/api/session/draft", post(s_draft))
        .route("/api/session/programme/{i}", post(s_programme))
        .route("/api/session/test/ok", post(s_test_ok))
        .route("/api/session/test/retry", post(s_test_retry))
        .route("/api/session/launch", post(s_launch))
        .route("/api/session/end", post(s_end))
        .route("/api/session/alerts/clear", post(s_dismiss))
        .route("/api/deck", get(deck_get))
        .route("/api/deck/key/{n}", post(deck_key))
        .route("/api/deck/dial/{n}/{delta}", post(deck_dial))
        .route("/api/tournages", get(shoots))
        .route("/api/tournages/{id}", get(shoot).delete(shoot_delete))
        .route("/api/tournages/{id}/export/{fmt}", get(export))
        .route("/api/tournages/{id}/replay", post(replay))
        .route("/api/settings", get(settings_get).put(settings_put))
        .route("/api/programmes", put(programmes_put))
        .route("/api/profiles", put(profiles_put))
        .route("/api/scenarios", put(scenarios_put))
        .route("/api/scenarios/plan/{event}", post(scenario_plan))
        .route("/api/scenarios/run/{event}", post(scenario_run))
        .route("/api/router", get(router_get))
        .route("/api/router/route", post(router_route))
        .route("/api/system", get(system))
        .route("/api/system/restart", post(restart))
        .route("/api/preview/{source}", get(preview))
}
