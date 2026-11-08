//! What Studio records, and what it does with the files afterwards.
//!
//! - [`encoder`]: the H.264 / H.265 4:2:2 encoders (processor, NVIDIA,
//!   Intel), shared with the encoding bench.
//! - [`recorder`]: up to 6 video channels (4 cameras in ISO, PGM, CLEAN),
//!   each encoded by its own ffmpeg, in MP4 or MOV.
//! - [`wav`]: one 24-bit WAV file per microphone, written as the sound
//!   comes.
//! - [`after`]: after the shoot, the 9:16 versions cut from the 16:9
//!   files, and the copy to the final storage, with its progress.
//! - [`live`]: YouTube and TikTok (9:16) streamed by the server, and the
//!   small previews of the sources for the browser.
//! - [`export`]: the edit of a shoot for the editing software (EDL,
//!   FCPXML, Premiere XML, OpenTimelineIO).

pub mod after;
pub mod encoder;
pub mod export;
pub mod live;
pub mod recorder;
pub mod wav;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("{0}")]
    Settings(String),
    #[error("ffmpeg : {0}")]
    Ffmpeg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, MediaError>;
