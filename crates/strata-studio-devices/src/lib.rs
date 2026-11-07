//! The devices Studio drives, behind one interface per kind of device, and
//! a simulator for each: Studio is tested and shown without hardware.
//!
//! | Interface | Devices (planned) |
//! | --- | --- |
//! | [`Switcher`] | Blackmagic ATEM (UDP 9910), vMix (TCP 8099) |
//! | [`Router`] | Videohub (TCP 9990), SW-P-08, NMOS IS-05 (ST 2110), NDI routing |
//! | [`Ptz`] | Sony FR7, BRC-AM7 (VISCA over IP, CGI) |
//! | [`AudioConsole`] | Midas M32 (OSC, [`m32`]), Yamaha (RCP) |
//! | [`Automation`] | Bitfocus Companion / Buttons: lights (DMX), ON AIR, the rest |

pub mod m32;
pub mod sim;

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
    /// panel changes it too).
    fn program(&self) -> Option<u16>;
}

/// A video router: the plant's « nodal », SDI, ST 2110 or NDI.
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
}

/// The audio console the microphones go through.
pub trait AudioConsole: Send {
    fn name(&self) -> String;
    /// Open (`true`) or mute a channel (1-based).
    fn set_on(&mut self, channel: u8, on: bool) -> Result<()>;
    /// The name on the channel's strip (the guest's).
    fn set_name(&mut self, channel: u8, name: &str) -> Result<()>;
}

/// Bitfocus Companion / Buttons: what Studio does not drive itself (the
/// lights through DMX, the ON AIR light, other automations).
pub trait Automation: Send {
    fn name(&self) -> String;
    /// Press a button (page, row, column).
    fn press(&mut self, page: u16, row: u8, column: u8) -> Result<()>;
    /// Set a variable the buttons read (who speaks, AI or manual…).
    fn set_variable(&mut self, name: &str, value: &str) -> Result<()>;
}
