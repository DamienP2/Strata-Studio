//! The 4:2:2 encoders: which ffmpeg options give H.264 or H.265 4:2:2 on
//! the processor, an NVIDIA card or Intel Quick Sync.
//!
//! - processor: libx264 (High 4:2:2) and libx265, 10 bits ;
//! - NVIDIA: 4:2:2 needs a Blackwell card and an FFmpeg recent enough to
//!   offer it (FFmpeg 6.1 does not) ;
//! - Intel Quick Sync: H.265 4:2:2 10 bits only.

use crate::{MediaError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    H265,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accel {
    Cpu,
    Nvenc,
    Qsv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resolution {
    #[serde(rename = "1080p50")]
    Hd,
    #[serde(rename = "2160p50")]
    Uhd,
}

impl Resolution {
    pub fn size(self) -> (u32, u32) {
        match self {
            Resolution::Hd => (1920, 1080),
            Resolution::Uhd => (3840, 2160),
        }
    }

    /// The bit rate of one channel, in Mb/s.
    pub fn mbps(self) -> u32 {
        match self {
            Resolution::Hd => 50,
            Resolution::Uhd => 150,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoder {
    pub codec: Codec,
    pub accel: Accel,
    pub resolution: Resolution,
    /// Overrides the pixel format chosen.
    #[serde(default)]
    pub pix_fmt: Option<String>,
    /// More ffmpeg options, after the encoder's.
    #[serde(default)]
    pub extra: Vec<String>,
}

impl Encoder {
    pub fn new(codec: Codec, accel: Accel, resolution: Resolution) -> Self {
        Encoder {
            codec,
            accel,
            resolution,
            pix_fmt: None,
            extra: vec![],
        }
    }

    /// The ffmpeg output options (video), and the pixel format.
    pub fn args(&self) -> Result<(Vec<String>, String)> {
        let (args, pix): (&[&str], &str) = match (self.accel, self.codec) {
            (Accel::Cpu, Codec::H264) => (
                &[
                    "-c:v",
                    "libx264",
                    "-preset",
                    "veryfast",
                    "-profile:v",
                    "high422",
                ],
                "yuv422p10le",
            ),
            (Accel::Cpu, Codec::H265) => (
                &[
                    "-c:v",
                    "libx265",
                    "-preset",
                    "ultrafast",
                    "-x265-params",
                    "log-level=error",
                ],
                "yuv422p10le",
            ),
            (Accel::Nvenc, Codec::H264) => (
                &[
                    "-c:v",
                    "h264_nvenc",
                    "-preset",
                    "p4",
                    "-profile:v",
                    "high422",
                ],
                "nv16",
            ),
            (Accel::Nvenc, Codec::H265) => (
                &["-c:v", "hevc_nvenc", "-preset", "p4", "-profile:v", "rext"],
                "p210le",
            ),
            (Accel::Qsv, Codec::H265) => (&["-c:v", "hevc_qsv", "-preset", "veryfast"], "y210le"),
            (Accel::Qsv, Codec::H264) => {
                return Err(MediaError::Settings(
                    "Quick Sync n'encode pas le H.264 en 4:2:2 : choisissez le H.265".into(),
                ))
            }
        };
        let mut v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        v.extend(["-b:v".into(), format!("{}M", self.resolution.mbps())]);
        v.extend(self.extra.iter().cloned());
        Ok((v, self.pix_fmt.clone().unwrap_or_else(|| pix.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let (a, pix) = Encoder::new(Codec::H264, Accel::Cpu, Resolution::Hd)
            .args()
            .unwrap();
        assert!(a.windows(2).any(|w| w == ["-profile:v", "high422"]));
        assert!(a.windows(2).any(|w| w == ["-b:v", "50M"]));
        assert_eq!(pix, "yuv422p10le");
        let (_, pix) = Encoder::new(Codec::H265, Accel::Qsv, Resolution::Uhd)
            .args()
            .unwrap();
        assert_eq!(pix, "y210le");
        assert!(Encoder::new(Codec::H264, Accel::Qsv, Resolution::Hd)
            .args()
            .is_err());
    }
}
