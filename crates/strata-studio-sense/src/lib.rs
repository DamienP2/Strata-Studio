//! Perception: what Studio hears (and later sees) of the studio.
//!
//! For now, the host's ear: the host's microphone is cut into sentences
//! ([`segment`]), each sentence is transcribed by a speech recogniser
//! running on the server ([`whisper`]: a whisper.cpp server, local, no
//! cloud), and the text becomes [`strata_studio_core::Heard`] for the
//! [`strata_studio_core::Listener`] ([`ear`]).

pub mod ear;
pub mod segment;
pub mod wav;
pub mod whisper;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SenseError {
    #[error("audio : {0}")]
    Audio(String),
    #[error("reconnaissance vocale : {0}")]
    Recognizer(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SenseError>;
