//! Perception: what Studio hears (and later sees) of the studio.
//!
//! - The microphones, received in AES67 from the Dante network
//!   ([`aes67`]), and who speaks on them, bleed taken out ([`voices`]).
//! - The host's ear: the host's microphone is cut into sentences
//!   ([`segment`]), each sentence is transcribed by a speech recogniser
//!   running on the server ([`whisper`]: a whisper.cpp server, local, no
//!   cloud), and the text becomes [`strata_studio_core::Heard`] for the
//!   [`strata_studio_core::Listener`] ([`ear`]).

pub mod aes67;
pub mod ear;
pub mod segment;
pub mod voices;
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
