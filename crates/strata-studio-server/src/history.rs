//! The shoots, kept in Studio's data folder (`tournages/<id>.json`): the
//! answers, the cuts (what was on the programme and why), who spoke, what
//! the host said, the files. Each shoot also keeps what Studio perceived
//! (`tournages/<id>.perception.jsonl`, one line per frame), so that the AI
//! can be **replayed** on it with other settings.

use crate::config::Programme;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use strata_studio_core::{
    CamId, Control, Decision, Director, Perception, Profile, Reason, ShotId, ShotKind, StudioSetup,
};
use strata_studio_media::export::{Edit, Segment};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Speech {
    pub start_ms: u64,
    pub end_ms: u64,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Said {
    pub t_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub t_ms: u64,
    pub text: String,
    /// Sent to the devices (false: in shadow, or a person had the hand).
    pub sent: bool,
}

/// The AI compared with a person at the panel (shadow mode).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agreement {
    pub ai_cuts: u32,
    pub human_cuts: u32,
    /// Human cuts with an AI cut within a second.
    pub matched: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShootRecord {
    pub id: String,
    pub programme: String,
    pub date: String,
    pub started_unix_ms: u64,
    pub duration_ms: u64,
    /// `en cours`, `terminé`.
    pub status: String,
    pub answers: Programme,
    pub profile: Profile,
    pub setup: Option<StudioSetup>,
    pub segments: Vec<Segment>,
    pub speakers: Vec<Speech>,
    pub notes: Vec<Note>,
    pub heard: Vec<Said>,
    /// The recorded files (on the server), then where they were copied.
    pub files: Vec<String>,
    pub storage: String,
    pub storage_path: String,
    /// Time a person had the hand, in ms.
    pub manual_ms: u64,
    pub agreement: Agreement,
    pub width: u32,
    pub height: u32,
}

pub struct History {
    pub dir: PathBuf,
}

impl History {
    pub fn new(data: &Path) -> Self {
        History {
            dir: data.join("tournages"),
        }
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        let safe = !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c));
        safe.then(|| self.dir.join(format!("{id}.json")))
    }

    pub fn perception_path(&self, id: &str) -> Option<PathBuf> {
        self.path(id).map(|p| p.with_extension("perception.jsonl"))
    }

    pub fn save(&self, r: &ShootRecord) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let p = self
            .path(&r.id)
            .ok_or_else(|| anyhow::anyhow!("identifiant : {}", r.id))?;
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(r)?)?;
        std::fs::rename(tmp, p)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<ShootRecord> {
        let text = std::fs::read_to_string(self.path(id)?).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The shoots, newest first, without their details.
    pub fn list(&self) -> Vec<ShootRecord> {
        let mut v: Vec<ShootRecord> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
            .filter_map(|e| {
                serde_json::from_str::<ShootRecord>(&std::fs::read_to_string(e.path()).ok()?).ok()
            })
            .map(|mut r| {
                r.notes.clear();
                r.heard.clear();
                r.setup = None;
                r
            })
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.started_unix_ms));
        v
    }

    pub fn delete(&self, id: &str) -> bool {
        let Some(p) = self.path(id) else { return false };
        let _ = std::fs::remove_file(p.with_extension("perception.jsonl"));
        std::fs::remove_file(p).is_ok()
    }

    pub fn perceptions(&self, id: &str) -> Vec<Perception> {
        let Some(p) = self.perception_path(id) else {
            return vec![];
        };
        let Ok(f) = std::fs::File::open(p) else {
            return vec![];
        };
        std::io::BufReader::new(f)
            .lines()
            .map_while(Result::ok)
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect()
    }
}

/// A shoot's id: date, time, programme (`2026-11-08_1432_le-grand-debrief`).
pub fn new_id(date: &str, unix_ms: u64, programme: &str) -> String {
    let secs = unix_ms / 1000 % 86400;
    let slug: String = strata_studio_media::recorder::file_safe(programme)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!(
        "{date}_{:02}{:02}{:02}_{slug}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The edit for the editing software.
pub fn edit(r: &ShootRecord) -> Edit {
    let files = r
        .files
        .iter()
        .filter_map(|f| {
            let name = Path::new(f).file_stem()?.to_string_lossy().to_string();
            let ch = ["PGM", "CLEAN", "CAM1", "CAM2", "CAM3", "CAM4"]
                .into_iter()
                .find(|c| name.ends_with(&format!("_{c}")))?;
            let shown = if r.storage_path.is_empty() {
                f.clone()
            } else {
                let file = Path::new(f).file_name()?.to_string_lossy().to_string();
                Path::new(&r.storage_path)
                    .join(strata_studio_media::recorder::file_safe(&r.programme))
                    .join(&r.date)
                    .join(file)
                    .to_string_lossy()
                    .into()
            };
            Some((ch.to_string(), shown))
        })
        .collect::<Vec<_>>();
    // Without the camera files, everything comes from the programme.
    let has_iso = files.iter().any(|(c, _)| c.starts_with("CAM"));
    let segments = if has_iso {
        r.segments.clone()
    } else {
        vec![Segment {
            start_ms: 0,
            source: "PGM".into(),
            label: r.programme.clone(),
        }]
    };
    Edit {
        title: r.programme.clone(),
        fps: 50,
        duration_ms: r.duration_ms,
        segments,
        files,
        width: if r.width > 0 { r.width } else { 1920 },
        height: if r.height > 0 { r.height } else { 1080 },
    }
}

/// How a cut list reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub cuts_per_minute: f32,
    pub mean_shot_s: f32,
    /// Share of the time on a wide shot, in %.
    pub wide_percent: f32,
    pub listening_shots: u32,
}

pub fn stats(segments: &[Segment], duration_ms: u64, wide: &dyn Fn(&Segment) -> bool) -> Stats {
    if segments.is_empty() || duration_ms == 0 {
        return Stats::default();
    }
    let mut wide_ms = 0;
    for (i, s) in segments.iter().enumerate() {
        let end = segments.get(i + 1).map_or(duration_ms, |n| n.start_ms);
        if wide(s) {
            wide_ms += end.saturating_sub(s.start_ms);
        }
    }
    let minutes = duration_ms as f32 / 60000.0;
    Stats {
        cuts_per_minute: (segments.len().saturating_sub(1)) as f32 / minutes.max(1.0 / 60.0),
        mean_shot_s: duration_ms as f32 / 1000.0 / segments.len() as f32,
        wide_percent: wide_ms as f32 * 100.0 / duration_ms as f32,
        listening_shots: segments
            .iter()
            .filter(|s| s.label.contains("écoute"))
            .count() as u32,
    }
}

/// The AI replayed on a shoot's perception with another profile: the cuts
/// it would have made. The cameras are taken to reach the presets the
/// replayed AI asks for, and the programme to be what it cut to.
pub fn replay(setup: &StudioSetup, profile: Profile, perceptions: &[Perception]) -> Vec<Segment> {
    let mut d = Director::new(setup.clone(), profile);
    let control = Control::default();
    let mut program: Option<CamId> = None;
    let mut framed: BTreeMap<CamId, (Option<ShotId>, u64)> = BTreeMap::new();
    let mut segments = Vec::new();
    let start = perceptions.first().map_or(0, |p| p.t_ms);
    for p in perceptions {
        let mut p = p.clone();
        for c in &mut p.cams {
            if let Some((shot, until)) = framed.get(&c.cam) {
                c.moving = p.t_ms < *until;
                c.framed = if c.moving { None } else { *shot };
            }
        }
        p.program = program;
        for dec in d.tick(&p, &control) {
            let t = p.t_ms - start;
            match dec {
                Decision::Cut {
                    shot,
                    camera,
                    reason,
                } => {
                    program = Some(camera);
                    let listening =
                        matches!(reason, Reason::Reaction { .. } | Reason::Listening { .. });
                    let what = setup.shot(shot).map_or(String::new(), |s| match &s.kind {
                        ShotKind::Wide => "large".into(),
                        ShotKind::Single(seat) => setup.seat_name(*seat),
                        ShotKind::Group(_) => "groupe".into(),
                    });
                    segments.push(Segment {
                        start_ms: t,
                        source: format!("CAM{}", camera.0),
                        label: format!(
                            "Cam {} · {what}{}",
                            camera.0,
                            if listening { " · écoute" } else { "" }
                        ),
                    });
                }
                Decision::Compose { .. } => {
                    program = None;
                    segments.push(Segment {
                        start_ms: t,
                        source: "PGM".into(),
                        label: "mise en page".into(),
                    });
                }
                Decision::Prepare { camera, shot, .. } => {
                    framed.insert(camera, (Some(shot), p.t_ms + 1200));
                }
                _ => {}
            }
        }
    }
    segments
}

/// Whether a segment is a wide shot (by its label).
pub fn is_wide(s: &Segment) -> bool {
    s.label.contains("large")
}

/// Appends perceptions to a shoot's log.
pub struct PerceptionLog {
    out: std::io::BufWriter<std::fs::File>,
}

impl PerceptionLog {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        Ok(PerceptionLog {
            out: std::io::BufWriter::new(std::fs::File::create(path)?),
        })
    }

    pub fn write(&mut self, p: &Perception) {
        if let Ok(line) = serde_json::to_string(p) {
            let _ = writeln!(self.out, "{line}");
        }
    }

    pub fn flush(&mut self) {
        let _ = self.out.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_studio_core::script::Script;
    use strata_studio_core::{CamState, SeatId};

    #[test]
    fn ids_are_safe() {
        assert_eq!(
            new_id("2026-11-08", 52_330_000, "L'Invité de 13h"),
            "2026-11-08_143210_l-invite-de-13h"
        );
        let h = History::new(Path::new("/tmp"));
        assert!(h.path("../etc/passwd").is_none());
    }

    #[test]
    fn saves_lists_and_deletes() {
        let dir = std::env::temp_dir().join(format!("strata-history-{}", std::process::id()));
        let h = History::new(&dir);
        let r = ShootRecord {
            id: "2026-11-08_100000_essai".into(),
            programme: "Essai".into(),
            started_unix_ms: 5,
            notes: vec![Note {
                t_ms: 1,
                text: "COUPE".into(),
                sent: true,
            }],
            ..Default::default()
        };
        h.save(&r).unwrap();
        assert_eq!(h.get(&r.id).unwrap().notes.len(), 1);
        let list = h.list();
        assert_eq!(list.len(), 1);
        assert!(list[0].notes.is_empty());
        assert!(h.delete(&r.id));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ai_replays_a_conversation_with_another_profile() {
        let setup = StudioSetup::example();
        let script = Script::demo();
        let perceptions: Vec<Perception> = (0..1500u64)
            .map(|i| {
                let t = i * 40;
                Perception {
                    t_ms: t,
                    mics: script.mics(setup.seats.iter().map(|s| s.id), t),
                    cams: setup
                        .cameras
                        .iter()
                        .map(|c| CamState {
                            cam: c.id,
                            online: true,
                            moving: false,
                            framed: None,
                            subject: None,
                        })
                        .collect(),
                    ..Default::default()
                }
            })
            .collect();
        let calm = replay(&setup, Profile::radio(), &perceptions);
        let lively = replay(&setup, Profile::debate(), &perceptions);
        assert!(!calm.is_empty() && !lively.is_empty());
        assert!(
            lively.len() >= calm.len(),
            "{} {}",
            lively.len(),
            calm.len()
        );
        let st = stats(&lively, 60_000, &is_wide);
        assert!(st.cuts_per_minute > 0.0);
        let _ = SeatId(1);
    }

    #[test]
    fn the_edit_uses_the_camera_files_or_the_programme() {
        let r = ShootRecord {
            programme: "Essai".into(),
            date: "2026-11-08".into(),
            duration_ms: 10_000,
            segments: vec![Segment {
                start_ms: 0,
                source: "CAM2".into(),
                label: "Cam 2".into(),
            }],
            files: vec!["/srv/Essai/2026-11-08/Essai_2026-11-08_PGM.mov".into()],
            ..Default::default()
        };
        assert_eq!(edit(&r).segments[0].source, "PGM");
        let mut r2 = r.clone();
        r2.files
            .push("/srv/Essai/2026-11-08/Essai_2026-11-08_CAM2.mov".into());
        r2.storage_path = "/mnt/redaction".into();
        let e = edit(&r2);
        assert_eq!(e.segments[0].source, "CAM2");
        assert!(e.files.iter().any(|(c, f)| c == "CAM2"
            && f == "/mnt/redaction/Essai/2026-11-08/Essai_2026-11-08_CAM2.mov"));
    }
}
