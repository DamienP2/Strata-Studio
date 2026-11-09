//! A device the studio does not have yet (its address is not set): it
//! does nothing, says so when asked, and the studio check shows it as
//! « non réglé ». Studio never pretends a device is there.

use crate::quicklink::{Calls, Guest, GuestState};
use crate::webencoder::{GoEncoder, GoState};
use crate::{AudioConsole, Automation, DeviceError, Result, Router, Shown, Switcher};

pub const UNSET: &str = "non réglé";

/// An absent device, by what it would be (« mélangeur », « console »…).
#[derive(Clone, Debug)]
pub struct Absent(pub String);

impl Absent {
    pub fn new(what: &str) -> Self {
        Absent(what.into())
    }
    fn refuse<T>(&self) -> Result<T> {
        Err(DeviceError::Refused(self.0.clone(), UNSET.into()))
    }
}

impl Switcher for Absent {
    fn name(&self) -> String {
        format!("{} {UNSET}", self.0)
    }
    fn connected(&self) -> bool {
        false
    }
    fn cut(&mut self, _input: u16) -> Result<()> {
        self.refuse()
    }
    fn program(&self) -> Option<u16> {
        None
    }
    fn show(&mut self, _shown: Shown) -> Result<()> {
        self.refuse()
    }
}

impl AudioConsole for Absent {
    fn name(&self) -> String {
        format!("{} {UNSET}", self.0)
    }
    fn set_on(&mut self, _channel: u8, _on: bool) -> Result<()> {
        self.refuse()
    }
    fn set_name(&mut self, _channel: u8, _name: &str) -> Result<()> {
        self.refuse()
    }
    fn check(&mut self) -> Result<String> {
        self.refuse()
    }
}

impl Automation for Absent {
    fn name(&self) -> String {
        format!("{} {UNSET}", self.0)
    }
    fn press(&mut self, _page: u16, _row: u8, _column: u8) -> Result<()> {
        self.refuse()
    }
    fn set_variable(&mut self, _name: &str, _value: &str) -> Result<()> {
        self.refuse()
    }
}

impl Router for Absent {
    fn name(&self) -> String {
        format!("{} {UNSET}", self.0)
    }
    fn route(&mut self, _destination: &str, _source: &str) -> Result<()> {
        self.refuse()
    }
    fn routes(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn connected(&self) -> bool {
        false
    }
}

impl GoEncoder for Absent {
    fn name(&self) -> String {
        format!("{} {UNSET}", self.0)
    }
    fn state(&mut self) -> GoState {
        GoState::Offline {
            error: UNSET.into(),
        }
    }
    fn start(&mut self) -> Result<()> {
        self.refuse()
    }
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}

impl Calls for Absent {
    fn name(&self) -> String {
        format!("{} {UNSET}", self.0)
    }
    fn create_guest(&mut self, _name: &str, _start: &str, _end: &str) -> Result<Guest> {
        Err(DeviceError::Refused(
            self.0.clone(),
            "Quicklink non réglé (Administration › Calls)".into(),
        ))
    }
    fn guest(&mut self, _uid: &str) -> GuestState {
        GuestState::Unknown {
            error: UNSET.into(),
        }
    }
    fn place(&mut self, _uid: &str, _position: u16) -> Result<()> {
        self.refuse()
    }
    fn unplace(&mut self, _uid: &str, _position: u16) -> Result<()> {
        self.refuse()
    }
    fn mute(&mut self, _uid: &str, _audio: bool, _video: bool) -> Result<()> {
        self.refuse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_device_refuses_and_says_why() {
        let mut sw = Absent::new("Mélangeur");
        assert!(!Switcher::connected(&sw));
        assert!(sw.cut(1).unwrap_err().to_string().contains(UNSET));
        let mut go = Absent::new("GO1");
        assert_eq!(
            go.state(),
            GoState::Offline {
                error: UNSET.into()
            }
        );
    }
}
