//! The devices Studio drives, behind one interface per kind of device, and
//! a simulator for each: Studio is tested and shown without hardware.
//!
//! | Interface | Devices (planned) |
//! | --- | --- |
//! | [`Switcher`] | Blackmagic ATEM, all models (UDP 9910, [`atem`]), vMix (TCP 8099, [`vmix`]) |
//! | [`Router`] | Videohub (TCP 9990), NMOS IS-05 without a registry (ST 2110), NDI routing done by Studio |
//! | [`Ptz`] | Sony FR7, BRC-AM7 (VISCA over IP, [`visca`]) |
//! | [`AudioConsole`] | Midas M32 (OSC, [`m32`]), Yamaha DM5 (RCP) |
//! | [`Automation`] | Bitfocus Buttons: lights (DMX), ON AIR, the rest |

pub mod atem;
pub mod m32;
pub mod sim;
pub mod visca;
pub mod vmix;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("{0} : pas de connexion")]
    NotConnected(String),
    #[error("{0} : refusé ({1})")]
    Refused(String, String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DeviceError>;

/// A production switcher.
pub trait Switcher: Send {
    fn name(&self) -> String;
    fn connected(&self) -> bool;
    /// Cut the program to an input.
    fn cut(&mut self, input: u16) -> Result<()>;
    /// The input on the program, as the switcher says (a person on the
    /// panel changes it too). A layout on air is not a camera input.
    fn program(&self) -> Option<u16>;
    /// Put a picture on the program: an input, or a layout built by the
    /// switcher (ATEM SuperSource, a vMix layout input).
    fn show(&mut self, shown: Shown) -> Result<()>;
    /// Set a layout up without taking it, so that it is ready.
    fn preload(&mut self, _shown: Shown) -> Result<()> {
        Ok(())
    }
    /// Simulation only: a person cuts on the panel. False on a real switcher.
    fn demo_panel_cut(&mut self, _input: u16) -> bool {
        false
    }
}

/// What the switcher puts on the program, by switcher input numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    Input(u16),
    /// Side by side, half and half.
    Split {
        left: u16,
        right: u16,
    },
    /// `main` full frame, `inset` in a small box.
    Pip {
        main: u16,
        inset: u16,
    },
}

/// A video router: the plant's « nodal » (Videohub in SDI, IS-05 in ST 2110,
/// Studio's own NDI routing).
pub trait Router: Send {
    fn name(&self) -> String;
    fn route(&mut self, destination: &str, source: &str) -> Result<()>;
    /// (destination, source), as the router says.
    fn routes(&self) -> Vec<(String, String)>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PtzStatus {
    pub online: bool,
    pub moving: bool,
    /// The preset last reached.
    pub preset: Option<u8>,
}

/// A PTZ camera.
pub trait Ptz: Send {
    fn name(&self) -> String;
    fn recall(&mut self, preset: u8) -> Result<()>;
    fn status(&mut self) -> PtzStatus;
    /// Zoom at a speed from -1 (wide) to 1 (tele); 0 stops.
    fn zoom(&mut self, speed: f32) -> Result<()>;
    /// Pan and tilt at speeds from -1 to 1 (right, up); 0 and 0 stop.
    fn drive(&mut self, pan: f32, tilt: f32) -> Result<()>;
}

/// The audio console the microphones go through.
pub trait AudioConsole: Send {
    fn name(&self) -> String;
    /// Open (`true`) or mute a channel (1-based).
    fn set_on(&mut self, channel: u8, on: bool) -> Result<()>;
    /// The name on the channel's strip (the guest's).
    fn set_name(&mut self, channel: u8, name: &str) -> Result<()>;
}

/// Bitfocus Buttons: what Studio does not drive itself (the
/// lights through DMX, the ON AIR light, other automations).
pub trait Automation: Send {
    fn name(&self) -> String;
    /// Press a button (page, row, column).
    fn press(&mut self, page: u16, row: u8, column: u8) -> Result<()>;
    /// Set a variable the buttons read (who speaks, AI or manual…).
    fn set_variable(&mut self, name: &str, value: &str) -> Result<()>;
}
