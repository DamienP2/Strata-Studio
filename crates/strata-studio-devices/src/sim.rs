//! Simulated devices: they obey at once (a PTZ takes its travel time) and
//! remember what they were told.

use crate::{AudioConsole, Automation, Ptz, PtzStatus, Result, Router, Shown, Switcher};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// The input number the simulated switcher gives its layouts.
pub const SIM_LAYOUT: u16 = 6000;

#[derive(Debug, Default)]
pub struct SimSwitcher {
    program: Option<u16>,
    /// What was last shown, and set up.
    pub shown: Option<Shown>,
    pub preloaded: Option<Shown>,
}

impl SimSwitcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// A person cuts on the panel.
    pub fn panel_cut(&mut self, input: u16) {
        self.program = Some(input);
    }
}

impl Switcher for SimSwitcher {
    fn name(&self) -> String {
        "mélangeur simulé".into()
    }
    fn connected(&self) -> bool {
        true
    }
    fn cut(&mut self, input: u16) -> Result<()> {
        self.program = Some(input);
        Ok(())
    }
    fn program(&self) -> Option<u16> {
        self.program
    }
    fn show(&mut self, shown: Shown) -> Result<()> {
        self.program = Some(match shown {
            Shown::Input(i) => i,
            _ => SIM_LAYOUT,
        });
        self.shown = Some(shown);
        Ok(())
    }
    fn preload(&mut self, shown: Shown) -> Result<()> {
        self.preloaded = Some(shown);
        Ok(())
    }
    fn demo_panel_cut(&mut self, input: u16) -> bool {
        self.program = Some(input);
        true
    }
}

#[derive(Debug)]
pub struct SimPtz {
    name: String,
    preset: Option<u8>,
    travel: Duration,
    moving: Option<(Instant, u8)>,
    /// The last zoom and pan/tilt speeds asked for.
    pub zoom_speed: f32,
    pub drive_speed: (f32, f32),
}

impl SimPtz {
    pub fn new(name: impl Into<String>, preset: u8, travel: Duration) -> Self {
        SimPtz {
            name: name.into(),
            preset: Some(preset),
            travel,
            moving: None,
            zoom_speed: 0.0,
            drive_speed: (0.0, 0.0),
        }
    }
}

impl Ptz for SimPtz {
    fn name(&self) -> String {
        self.name.clone()
    }
    fn recall(&mut self, preset: u8) -> Result<()> {
        self.moving = Some((Instant::now() + self.travel, preset));
        self.zoom_speed = 0.0;
        self.drive_speed = (0.0, 0.0);
        Ok(())
    }
    fn zoom(&mut self, speed: f32) -> Result<()> {
        self.zoom_speed = speed;
        Ok(())
    }
    fn drive(&mut self, pan: f32, tilt: f32) -> Result<()> {
        self.drive_speed = (pan, tilt);
        Ok(())
    }
    fn status(&mut self) -> PtzStatus {
        if let Some((until, preset)) = self.moving {
            if Instant::now() >= until {
                self.preset = Some(preset);
                self.moving = None;
            }
        }
        PtzStatus {
            online: true,
            moving: self.moving.is_some(),
            preset: if self.moving.is_some() {
                None
            } else {
                self.preset
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct SimConsole {
    /// channel → (on, name)
    pub channels: BTreeMap<u8, (bool, String)>,
}

impl AudioConsole for SimConsole {
    fn name(&self) -> String {
        "console simulée".into()
    }
    fn set_on(&mut self, channel: u8, on: bool) -> Result<()> {
        self.channels.entry(channel).or_default().0 = on;
        Ok(())
    }
    fn set_name(&mut self, channel: u8, name: &str) -> Result<()> {
        self.channels.entry(channel).or_default().1 = name.into();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct SimRouter {
    routes: BTreeMap<String, String>,
}

impl Router for SimRouter {
    fn name(&self) -> String {
        "nodal simulé".into()
    }
    fn route(&mut self, destination: &str, source: &str) -> Result<()> {
        self.routes.insert(destination.into(), source.into());
        Ok(())
    }
    fn routes(&self) -> Vec<(String, String)> {
        self.routes
            .iter()
            .map(|(d, s)| (d.clone(), s.clone()))
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct SimAutomation {
    pub pressed: Vec<(u16, u8, u8)>,
    pub variables: BTreeMap<String, String>,
}

impl Automation for SimAutomation {
    fn name(&self) -> String {
        "Bitfocus simulé".into()
    }
    fn press(&mut self, page: u16, row: u8, column: u8) -> Result<()> {
        self.pressed.push((page, row, column));
        Ok(())
    }
    fn set_variable(&mut self, name: &str, value: &str) -> Result<()> {
        self.variables.insert(name.into(), value.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ptz_is_moving_then_framed() {
        let mut ptz = SimPtz::new("Cam 2", 1, Duration::from_millis(30));
        ptz.recall(4).unwrap();
        assert_eq!(
            ptz.status(),
            PtzStatus {
                online: true,
                moving: true,
                preset: None
            }
        );
        std::thread::sleep(Duration::from_millis(40));
        assert_eq!(
            ptz.status(),
            PtzStatus {
                online: true,
                moving: false,
                preset: Some(4)
            }
        );
    }
}
