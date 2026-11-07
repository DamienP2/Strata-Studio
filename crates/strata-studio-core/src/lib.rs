//! Strata Studio's contract: the studio as it is set up (seats, cameras,
//! shots), what Studio perceives of it at each instant, and the Director,
//! which turns those perceptions into decisions (cut, prepare a PTZ, who
//! speaks).
//!
//! Nothing here touches a device, a clock or the network: the Director is a
//! pure function of what it is given, so a show recorded as perceptions can
//! be replayed with another profile and give the same decisions every time.

pub mod director;
pub mod model;
pub mod perception;
pub mod profile;
pub mod script;

pub use director::{Control, Decision, Director, Forced, Mode, Reason};
pub use model::{
    CamId, Camera, CameraModel, Mic, Seat, SeatId, Shot, ShotId, ShotKind, StudioSetup,
};
pub use perception::{CamState, MicState, Perception};
pub use profile::Profile;
