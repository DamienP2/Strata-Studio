//! The recording desk: what the tablet's « Enregistrement », « Résolution »,
//! « Format » and « Stockage » answers become.
//!
//! - `start`: the video channels chosen (PGM, CLEAN, the 4 cameras) each
//!   recorded by ffmpeg, and one WAV per microphone when the microphones
//!   come in AES67 ;
//! - `status`: each channel's state, the time recorded ;
//! - `stop`: the files closed; then, in the background, the 9:16
//!   versions (when 9:16 was chosen) and the copy to the storage, with
//!   their progress ;
//! - `estimate`: how long can still be recorded on the server's disk
//!   with these answers.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use strata_studio_media::after::{copy_all, make_vertical};
use strata_studio_media::encoder::{Accel, Codec, Encoder, Resolution};
use strata_studio_media::recorder::{
    file_name, file_safe, shoot_dir, Channel, ChannelState, Container, Recording,
};
use strata_studio_media::wav::MicFiles;

/// Where the AES67 receiving thread writes the microphones while a
/// recording is on.
pub type MicSink = Arc<Mutex<Option<MicFiles>>>;

/// The tablet's answers about the recording.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordRequest {
    pub programme: String,
    #[serde(default = "yes")]
    pub pgm: bool,
    #[serde(default)]
    pub clean: bool,
    /// The 4 cameras, each in its file.
    #[serde(default)]
    pub iso: bool,
    /// One WAV per microphone.
    #[serde(default = "yes")]
    pub mics: bool,
    #[serde(default = "hd")]
    pub resolution: Resolution,
    /// Also make the 9:16 versions after the shoot.
    #[serde(default)]
    pub vertical: bool,
    /// The storage to copy to (a mounted share), if any.
    #[serde(default)]
    pub destination: Option<PathBuf>,
}

fn yes() -> bool {
    true
}
fn hd() -> Resolution {
    Resolution::Hd
}

impl RecordRequest {
    /// The names of the video channels asked for.
    pub fn channel_names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.pgm {
            v.push("PGM");
        }
        if self.clean {
            v.push("CLEAN");
        }
        if self.iso {
            v.extend(["CAM1", "CAM2", "CAM3", "CAM4"]);
        }
        v
    }
}

/// The studio's settings for recording (set in the administration).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSettings {
    pub codec: Codec,
    pub accel: Accel,
    pub container: Container,
    /// Each channel's ffmpeg input (DeckLink, NDI through a gateway, SRT…).
    pub channels: Vec<Channel>,
}

impl RecordSettings {
    /// A real studio before its inputs are set: H.264 on the processor,
    /// MOV, every channel's input still empty.
    pub fn unset() -> Self {
        RecordSettings {
            codec: Codec::H264,
            accel: Accel::Cpu,
            container: Container::Mov,
            channels: ["PGM", "CLEAN", "CAM1", "CAM2", "CAM3", "CAM4"]
                .into_iter()
                .map(|n| Channel {
                    name: n.into(),
                    input: vec![],
                })
                .collect(),
        }
    }
}

/// What happens after the shoot.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AfterState {
    /// « versions 9:16 », « copie », « terminé ».
    pub step: String,
    pub done: u64,
    pub total: u64,
    pub files: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub recording: bool,
    pub programme: Option<String>,
    pub seconds: f64,
    pub channels: Vec<(String, ChannelState)>,
    pub mics: usize,
    pub after: AfterState,
}

struct Current {
    rec: Recording,
    request: RecordRequest,
    date: String,
    mic_count: usize,
}

pub struct Desk {
    pub root: PathBuf,
    pub ffmpeg: String,
    pub settings: RecordSettings,
    pub mic_sink: MicSink,
    /// The microphones' stream: (channels, rate), when there is one.
    pub mic_stream: Option<(usize, u32)>,
    current: Option<Current>,
    after: Arc<Mutex<AfterState>>,
    /// The files of the last recording (on the server).
    pub last_files: Vec<PathBuf>,
}

/// Today's date, `YYYY-MM-DD` (UTC).
pub fn today() -> String {
    let days = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        / 86400) as i64;
    date_of(days)
}

/// The calendar date of a day counted from 1970-01-01 (H. Hinnant's
/// algorithm).
pub fn date_of(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Free space where `path` is, in bytes (`df`, Linux and macOS).
pub fn free_bytes(path: &Path) -> Option<u64> {
    let probe = path.ancestors().find(|p| p.exists())?;
    let out = std::process::Command::new("df")
        .arg("-Pk")
        .arg(probe)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    line.split_whitespace()
        .nth(3)?
        .parse::<u64>()
        .ok()
        .map(|kb| kb * 1024)
}

impl Desk {
    pub fn new(
        root: PathBuf,
        ffmpeg: String,
        settings: RecordSettings,
        mic_sink: MicSink,
        mic_stream: Option<(usize, u32)>,
    ) -> Self {
        Desk {
            root,
            ffmpeg,
            settings,
            mic_sink,
            mic_stream,
            current: None,
            after: Arc::new(Mutex::new(AfterState::default())),
            last_files: vec![],
        }
    }

    fn encoder(&self, resolution: Resolution) -> Encoder {
        Encoder::new(self.settings.codec, self.settings.accel, resolution)
    }

    /// Hours that can still be recorded with these answers.
    pub fn estimate(&self, req: &RecordRequest) -> Option<f64> {
        let video = req.channel_names().len() as f64 * req.resolution.mbps() as f64 * 1e6;
        let audio = if req.mics { 8.0 * 48000.0 * 24.0 } else { 0.0 };
        let bits = video + audio;
        if bits == 0.0 {
            return None;
        }
        free_bytes(&self.root).map(|b| b as f64 * 8.0 / bits / 3600.0)
    }

    /// Starts the recording. `mics`: (seat's name, channel in the stream).
    pub fn start(&mut self, req: RecordRequest, mics: &[(String, u16)]) -> Result<(), String> {
        if self.current.is_some() {
            return Err("un enregistrement est déjà en cours".into());
        }
        let names = req.channel_names();
        if names.is_empty() && !req.mics {
            return Err("rien à enregistrer".into());
        }
        let channels: Vec<Channel> = names
            .iter()
            .map(|n| {
                self.settings
                    .channels
                    .iter()
                    .find(|c| c.name == *n)
                    .filter(|c| !c.input.is_empty())
                    .cloned()
                    .ok_or_else(|| {
                        format!("l'entrée vidéo de {n} n'est pas réglée (Administration › Images)")
                    })
            })
            .collect::<Result<_, _>>()?;
        let date = today();
        let dir = shoot_dir(&self.root, &req.programme, &date);
        let enc = self.encoder(req.resolution);
        let rec = Recording::start(
            &self.ffmpeg,
            &dir,
            &req.programme,
            &date,
            &channels,
            &enc,
            self.settings.container,
        )
        .map_err(|e| e.to_string())?;
        let mut mic_count = 0;
        if req.mics {
            if let Some((n, rate)) = self.mic_stream {
                let files: Vec<(PathBuf, usize)> = mics
                    .iter()
                    .enumerate()
                    .map(|(i, (name, ch))| {
                        (
                            dir.join(file_name(
                                &req.programme,
                                &date,
                                &format!("MIC{}-{}", i + 1, file_safe(name)),
                                "wav",
                            )),
                            *ch as usize,
                        )
                    })
                    .collect();
                mic_count = files.len();
                let m = MicFiles::create(&files, n, rate).map_err(|e| e.to_string())?;
                *self.mic_sink.lock().unwrap() = Some(m);
            }
        }
        *self.after.lock().unwrap() = AfterState::default();
        self.current = Some(Current {
            rec,
            request: req,
            date,
            mic_count,
        });
        Ok(())
    }

    pub fn status(&mut self) -> Status {
        let after = self.after.lock().unwrap().clone();
        match &mut self.current {
            Some(c) => Status {
                recording: true,
                programme: Some(c.request.programme.clone()),
                seconds: c.rec.elapsed().as_secs_f64(),
                channels: c.rec.status(),
                mics: c.mic_count,
                after,
            },
            None => Status {
                recording: false,
                programme: None,
                seconds: 0.0,
                channels: vec![],
                mics: 0,
                after,
            },
        }
    }

    /// Stops; the 9:16 versions and the copy go on in the background.
    pub fn stop(&mut self) -> Result<Vec<(String, ChannelState)>, String> {
        let c = self.current.take().ok_or("aucun enregistrement en cours")?;
        let mics = self.mic_sink.lock().unwrap().take();
        let videos = c.rec.stop(Duration::from_secs(15));
        let mut files: Vec<PathBuf> = videos
            .iter()
            .filter(|(_, _, s)| matches!(s, ChannelState::Done { .. }))
            .map(|(_, p, _)| p.clone())
            .collect();
        if let Some(m) = mics {
            files.extend(m.finish().map_err(|e| e.to_string())?);
        }
        self.last_files = files.clone();
        let summary = videos
            .iter()
            .map(|(n, _, s)| (n.clone(), s.clone()))
            .collect();

        let after = self.after.clone();
        let ffmpeg = self.ffmpeg.clone();
        let enc = self.encoder(c.request.resolution);
        let req = c.request;
        let date = c.date;
        std::thread::Builder::new()
            .name("après tournage".into())
            .spawn(move || {
                let set = |f: &dyn Fn(&mut AfterState)| f(&mut after.lock().unwrap());
                let mut all = files.clone();
                if req.vertical {
                    let videos: Vec<PathBuf> = files
                        .iter()
                        .filter(|f| f.extension().is_some_and(|e| e != "wav"))
                        .cloned()
                        .collect();
                    for (i, v) in videos.iter().enumerate() {
                        set(&|a| {
                            a.step = "versions 9:16".into();
                            a.done = i as u64;
                            a.total = videos.len() as u64;
                        });
                        match make_vertical(&ffmpeg, v, &enc) {
                            Ok(p) => all.push(p),
                            Err(e) => {
                                set(&|a| a.error = Some(format!("9:16 de {} : {e}", v.display())))
                            }
                        }
                    }
                }
                if let Some(dest) = &req.destination {
                    let target = dest.join(file_safe(&req.programme)).join(&date);
                    set(&|a| a.step = "copie".into());
                    let res = copy_all(&all, &target, |d, t| {
                        let mut a = after.lock().unwrap();
                        a.done = d;
                        a.total = t;
                    });
                    if let Err(e) = res {
                        set(&|a| a.error = Some(format!("copie : {e}")));
                    }
                }
                let names: Vec<String> = all
                    .iter()
                    .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .collect();
                set(&|a| {
                    a.step = "terminé".into();
                    a.files = names.clone();
                });
            })
            .map_err(|e| e.to_string())?;
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_date_is_a_calendar_date() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }

    #[test]
    fn channels_from_the_answers() {
        let r: RecordRequest = serde_json::from_str(r#"{"programme":"P","iso":true}"#).unwrap();
        assert_eq!(
            r.channel_names(),
            vec!["PGM", "CAM1", "CAM2", "CAM3", "CAM4"]
        );
        assert!(r.mics);
        assert_eq!(r.resolution, Resolution::Hd);
    }

    #[test]
    fn free_space_is_read() {
        assert!(free_bytes(&std::env::temp_dir()).is_some_and(|b| b > 0));
    }
}
