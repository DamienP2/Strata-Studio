//! The video channels of a shoot, each recorded by its own ffmpeg.
//!
//! A channel is a name (`CAM1`, `PGM`, `CLEAN`) and the ffmpeg input
//! options that bring its picture: a DeckLink input (`-f decklink -i
//! "DeckLink Quad (1)"`), a stream (SRT, RTSP), or a generated picture for
//! the simulated studio. Its sound comes with it when it has some.
//!
//! The files are named after the programme and the day:
//! `<programme>/<date>/<programme>_<date>_<CHANNEL>.<mov|mp4>`.
//!
//! Each recording is watched: a process that stops, or a file that stops
//! growing, is a channel in trouble, shown on the tablet.

use crate::encoder::Encoder;
use crate::{MediaError, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mov,
    Mp4,
}

impl Container {
    pub fn ext(self) -> &'static str {
        match self {
            Container::Mov => "mov",
            Container::Mp4 => "mp4",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    /// `CAM1` … `CAM4`, `PGM`, `CLEAN`.
    pub name: String,
    /// The ffmpeg input options bringing its picture.
    pub input: Vec<String>,
}

/// A name usable in a file name: letters, digits, `-`; spaces become `-`.
pub fn file_safe(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let c = match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'À' | 'Â' => 'A',
            'É' | 'È' | 'Ê' => 'E',
            'Ç' => 'C',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        if c != '-' || !out.ends_with('-') {
            out.push(c);
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "programme".into()
    } else {
        out
    }
}

/// Where the files of a shoot go.
pub fn shoot_dir(root: &Path, programme: &str, date: &str) -> PathBuf {
    root.join(file_safe(programme)).join(date)
}

pub fn file_name(programme: &str, date: &str, channel: &str, ext: &str) -> String {
    format!(
        "{}_{date}_{}.{ext}",
        file_safe(programme),
        file_safe(channel)
    )
}

/// The ffmpeg command recording one channel.
pub fn command(
    ffmpeg: &str,
    channel: &Channel,
    enc: &Encoder,
    container: Container,
    out: &Path,
) -> Result<Command> {
    let (video, pix) = enc.args()?;
    let (w, h) = enc.frame_size();
    let mut c = Command::new(ffmpeg);
    c.args(["-hide_banner", "-nostats", "-loglevel", "error", "-y"])
        .args(&channel.input)
        .args(["-map", "0:v:0", "-map", "0:a?"])
        .args(["-vf", &format!("scale={w}:{h}:flags=bicubic,fps=50")])
        .args(&video)
        .args(["-pix_fmt", &pix, "-c:a", "pcm_s24le"]);
    if container == Container::Mp4 {
        // MP4 has no PCM: AAC for the channel's own sound.
        c.args(["-c:a", "aac", "-b:a", "256k", "-movflags", "+faststart"]);
    }
    c.arg(out)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    Ok(c)
}

/// How a channel is doing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ChannelState {
    Recording {
        bytes: u64,
    },
    /// Its file has not grown for a while.
    Stalled {
        bytes: u64,
    },
    /// Its ffmpeg has stopped.
    Failed {
        why: String,
    },
    Done {
        bytes: u64,
    },
}

struct Running {
    channel: Channel,
    path: PathBuf,
    child: Option<Child>,
    last_size: u64,
    grew_at: Instant,
    failed: Option<String>,
}

/// The recording of a shoot.
pub struct Recording {
    started: Instant,
    channels: Vec<Running>,
}

impl Recording {
    /// Starts every channel; `dir` is created.
    pub fn start(
        ffmpeg: &str,
        dir: &Path,
        programme: &str,
        date: &str,
        channels: &[Channel],
        enc: &Encoder,
        container: Container,
    ) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut running = Vec::new();
        for ch in channels {
            let path = dir.join(file_name(programme, date, &ch.name, container.ext()));
            let child = command(ffmpeg, ch, enc, container, &path)?
                .spawn()
                .map_err(|e| MediaError::Ffmpeg(format!("{ffmpeg} : {e}")))?;
            running.push(Running {
                channel: ch.clone(),
                path,
                child: Some(child),
                last_size: 0,
                grew_at: Instant::now(),
                failed: None,
            });
        }
        Ok(Recording {
            started: Instant::now(),
            channels: running,
        })
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// The state of each channel (name, state).
    pub fn status(&mut self) -> Vec<(String, ChannelState)> {
        self.channels
            .iter_mut()
            .map(|r| {
                let bytes = std::fs::metadata(&r.path).map(|m| m.len()).unwrap_or(0);
                if bytes > r.last_size {
                    r.last_size = bytes;
                    r.grew_at = Instant::now();
                }
                if r.failed.is_none() {
                    if let Some(child) = r.child.as_mut() {
                        if let Ok(Some(status)) = child.try_wait() {
                            let mut err = String::new();
                            if let Some(mut e) = child.stderr.take() {
                                use std::io::Read;
                                let _ = e.read_to_string(&mut err);
                            }
                            let last = err
                                .lines()
                                .rev()
                                .find(|l| !l.trim().is_empty())
                                .unwrap_or("arrêté")
                                .to_string();
                            r.failed = Some(format!("{status} : {last}"));
                        }
                    }
                }
                let state = match &r.failed {
                    Some(why) => ChannelState::Failed { why: why.clone() },
                    None if r.grew_at.elapsed() > Duration::from_secs(5) => {
                        ChannelState::Stalled { bytes }
                    }
                    None => ChannelState::Recording { bytes },
                };
                (r.channel.name.clone(), state)
            })
            .collect()
    }

    /// Stops every channel cleanly (ffmpeg finishes its file), within
    /// `wait`; returns the files written.
    pub fn stop(mut self, wait: Duration) -> Vec<(String, PathBuf, ChannelState)> {
        for r in &mut self.channels {
            if let Some(child) = r.child.as_mut() {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(b"q");
                }
                drop(child.stdin.take());
            }
        }
        let deadline = Instant::now() + wait;
        self.channels
            .into_iter()
            .map(|mut r| {
                let mut ok = false;
                if let Some(mut child) = r.child.take() {
                    loop {
                        match child.try_wait() {
                            Ok(Some(s)) => {
                                ok = s.success();
                                break;
                            }
                            Ok(None) if Instant::now() < deadline => {
                                std::thread::sleep(Duration::from_millis(20))
                            }
                            _ => {
                                let _ = child.kill();
                                let _ = child.wait();
                                break;
                            }
                        }
                    }
                }
                let bytes = std::fs::metadata(&r.path).map(|m| m.len()).unwrap_or(0);
                let state = if ok && bytes > 0 {
                    ChannelState::Done { bytes }
                } else {
                    ChannelState::Failed {
                        why: r.failed.unwrap_or_else(|| "fichier incomplet".into()),
                    }
                };
                (r.channel.name, r.path, state)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoder::{Accel, Codec, Resolution};

    #[test]
    fn names() {
        assert_eq!(file_safe("Le Grand Débrief !"), "Le-Grand-Debrief");
        assert_eq!(file_safe("  "), "programme");
        assert_eq!(
            file_name("L'Invité de 13h", "2026-11-08", "CAM1", "mov"),
            "L-Invite-de-13h_2026-11-08_CAM1.mov"
        );
        assert_eq!(
            shoot_dir(Path::new("/srv/rec"), "Tech Hebdo", "2026-11-08"),
            PathBuf::from("/srv/rec/Tech-Hebdo/2026-11-08")
        );
    }

    #[test]
    fn the_command_maps_picture_and_sound() {
        let ch = Channel {
            name: "CAM1".into(),
            input: vec![
                "-f".into(),
                "decklink".into(),
                "-i".into(),
                "DeckLink Quad (1)".into(),
            ],
        };
        let enc = Encoder::new(Codec::H265, Accel::Nvenc, Resolution::Uhd);
        let c = command("ffmpeg", &ch, &enc, Container::Mov, Path::new("/tmp/a.mov")).unwrap();
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let joined = args.join(" ");
        assert!(joined.contains("-f decklink -i DeckLink Quad (1)"));
        assert!(joined.contains("scale=3840:2160"));
        assert!(joined.contains("-c:v hevc_nvenc"));
        assert!(joined.contains("-pix_fmt p210le"));
        assert!(joined.ends_with("/tmp/a.mov"));
    }
}
