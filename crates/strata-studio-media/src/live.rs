//! YouTube and TikTok, encoded by the server: the PGM (the switcher's
//! program arriving on the server) pushed in RTMP by ffmpeg. TikTok gets
//! the 9:16 cut out of the middle of the 16:9, live.

use crate::encoder::Accel;
use crate::{MediaError, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Where to stream: an RTMP(S) server and its key (set in the administration).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTarget {
    /// `rtmp://a.rtmp.youtube.com/live2`, `rtmps://…tiktokcdn…/…`.
    pub url: String,
    #[serde(default)]
    pub key: String,
    /// The 9:16 cut (TikTok).
    #[serde(default)]
    pub vertical: bool,
    /// Video bit rate, in kb/s.
    #[serde(default = "default_kbps")]
    pub kbps: u32,
}

fn default_kbps() -> u32 {
    6000
}

impl LiveTarget {
    pub fn destination(&self) -> String {
        if self.key.is_empty() {
            self.url.clone()
        } else {
            format!("{}/{}", self.url.trim_end_matches('/'), self.key)
        }
    }
}

/// The ffmpeg arguments: `input` (the PGM's ffmpeg input options), the
/// encoding for a live platform (H.264 4:2:0, AAC, a key frame every 2 s),
/// the 9:16 cut when asked, FLV to the destination.
pub fn args(input: &[String], accel: Accel, target: &LiveTarget) -> Vec<String> {
    // ffmpeg reads « q » on its input to stop cleanly.
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(input.iter().cloned());
    a.extend(
        ["-map", "0:v:0", "-map", "0:a:0?"]
            .iter()
            .map(|s| s.to_string()),
    );
    let vf = if target.vertical {
        "crop=trunc(ih*9/16/2)*2:ih,scale=1080:1920,format=yuv420p"
    } else {
        "scale=1920:1080,format=yuv420p"
    };
    a.extend(["-vf".to_string(), vf.to_string()]);
    let codec: &[&str] = match accel {
        Accel::Cpu => &[
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-tune",
            "zerolatency",
        ],
        Accel::Nvenc => &["-c:v", "h264_nvenc", "-preset", "p4"],
        Accel::Qsv => &["-c:v", "h264_qsv", "-preset", "faster"],
    };
    a.extend(codec.iter().map(|s| s.to_string()));
    let k = target.kbps;
    a.extend([
        "-b:v".to_string(),
        format!("{k}k"),
        "-maxrate".into(),
        format!("{k}k"),
        "-bufsize".into(),
        format!("{}k", k * 2),
        "-g".into(),
        "100".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "160k".into(),
        "-ar".into(),
        "48000".into(),
        "-f".into(),
        "flv".into(),
        target.destination(),
    ]);
    a
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum LiveState {
    Live { seconds: u64 },
    Stopped,
    Failed { error: String },
}

/// A live stream under way.
pub struct Live {
    child: Child,
    since: Instant,
    errors: Arc<Mutex<String>>,
}

impl Live {
    pub fn start(
        ffmpeg: &str,
        input: &[String],
        accel: Accel,
        target: &LiveTarget,
    ) -> Result<Self> {
        if target.url.is_empty() {
            return Err(MediaError::Settings("pas d'adresse de diffusion".into()));
        }
        let mut child = Command::new(ffmpeg)
            .args(args(input, accel, target))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let errors = Arc::new(Mutex::new(String::new()));
        if let Some(mut err) = child.stderr.take() {
            let e = errors.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 1024];
                while let Ok(n) = err.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let mut e = e.lock().unwrap();
                    e.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if e.len() > 4000 {
                        let mut cut = e.len() - 2000;
                        while !e.is_char_boundary(cut) {
                            cut += 1;
                        }
                        *e = e[cut..].to_string();
                    }
                }
            });
        }
        Ok(Live {
            child,
            since: Instant::now(),
            errors,
        })
    }

    pub fn state(&mut self) -> LiveState {
        match self.child.try_wait() {
            Ok(None) => LiveState::Live {
                seconds: self.since.elapsed().as_secs(),
            },
            Ok(Some(_)) | Err(_) => {
                let e = self.errors.lock().unwrap();
                let last = e
                    .lines()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("ffmpeg arrêté");
                LiveState::Failed {
                    error: last.to_string(),
                }
            }
        }
    }

    /// Stops cleanly (« q »), or by force after 3 s.
    pub fn stop(mut self) {
        use std::io::Write;
        if let Some(mut stdin) = self.child.stdin.take() {
            let _ = stdin.write_all(b"q");
        }
        let deadline = Instant::now() + std::time::Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The ffmpeg arguments for a preview of a source in the browser: a
/// small MJPEG stream (multipart, boundary `ffmpeg`) on the standard output.
pub fn preview_args(input: &[String], fps: u32, width: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.extend(input.iter().cloned());
    a.extend([
        "-an".to_string(),
        "-vf".into(),
        format!("fps={fps},scale={width}:-2"),
        "-q:v".into(),
        "7".into(),
        "-f".into(),
        "mpjpeg".into(),
        "pipe:1".into(),
    ]);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Vec<String> {
        ["-f", "decklink", "-i", "DeckLink 8K Pro (1)"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn youtube_in_16_9() {
        let t = LiveTarget {
            url: "rtmp://a.rtmp.youtube.com/live2/".into(),
            key: "abcd-1234".into(),
            vertical: false,
            kbps: 6000,
        };
        let a = args(&input(), Accel::Cpu, &t);
        assert_eq!(
            a.last().unwrap(),
            "rtmp://a.rtmp.youtube.com/live2/abcd-1234"
        );
        assert!(a.windows(2).any(|w| w == ["-c:v", "libx264"]));
        assert!(a.windows(2).any(|w| w == ["-f", "flv"]));
        assert!(a.iter().any(|x| x.starts_with("scale=1920:1080")));
    }

    #[test]
    fn tiktok_in_9_16() {
        let t = LiveTarget {
            url: "rtmps://push.tiktok/live".into(),
            key: "k".into(),
            vertical: true,
            kbps: 4000,
        };
        let a = args(&input(), Accel::Nvenc, &t);
        assert!(a
            .iter()
            .any(|x| x.starts_with("crop=trunc(ih*9/16/2)*2:ih,scale=1080:1920")));
        assert!(a.windows(2).any(|w| w == ["-c:v", "h264_nvenc"]));
        assert!(a.windows(2).any(|w| w == ["-b:v", "4000k"]));
    }

    #[test]
    fn a_real_stream_to_a_file_runs_and_stops() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("strata-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("live.flv");
        let t = LiveTarget {
            url: out.to_string_lossy().into(),
            key: String::new(),
            vertical: true,
            kbps: 800,
        };
        let input: Vec<String> = ["-re", "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut live = Live::start("ffmpeg", &input, Accel::Cpu, &t).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            matches!(live.state(), LiveState::Live { .. }),
            "{:?}",
            live.state()
        );
        live.stop();
        assert!(std::fs::metadata(&out)
            .map(|m| m.len() > 0)
            .unwrap_or(false));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
