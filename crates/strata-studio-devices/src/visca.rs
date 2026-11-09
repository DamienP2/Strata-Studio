//! Sony VISCA over IP (FR7, BRC-AM7 and the other Sony PTZ): UDP, port
//! 52381.
//!
//! Each datagram is an 8-byte header, then a VISCA message:
//! - the payload type, 2 bytes (`01 00` a command, `01 10` an inquiry),
//! - the payload length, 2 bytes,
//! - a sequence number, 4 bytes, that the camera echoes in its replies.
//!
//! All big-endian. The VISCA message itself starts with `81` (camera 1)
//! and ends with `FF`.
//!
//! The commands used:
//! - preset recall `81 01 04 3F 02 pp FF` ;
//! - zoom `81 01 04 07 2p FF` (tele) / `3p` (wide) / `00` (stop), speed p
//!   from 0 (slowest) to 7 ;
//! - pan/tilt drive `81 01 06 01 VV WW 0X 0Y FF`, speeds VV (pan, 01 to
//!   18h) and WW (tilt, 01 to 17h), X 01 left / 02 right / 03 stop, Y 01
//!   up / 02 down / 03 stop.
//!
//! The very slow movements Studio asks for (a zoom that gives life, a
//! camera following a person) use the lowest speeds.

use crate::{DeviceError, Ptz, PtzStatus, Result};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

pub const PORT: u16 = 52381;

/// The VISCA over IP datagram carrying `msg` as a command.
pub fn datagram(seq: u32, msg: &[u8]) -> Vec<u8> {
    framed(0x0100, seq, msg)
}

/// The datagram carrying `msg` as an inquiry.
pub fn inquiry_datagram(seq: u32, msg: &[u8]) -> Vec<u8> {
    framed(0x0110, seq, msg)
}

fn framed(kind: u16, seq: u32, msg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + msg.len());
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&(msg.len() as u16).to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(msg);
    out
}

/// Recall preset `preset` (0-based on the wire; Studio's presets are 1-based).
/// `CAM_PowerInq`: the reply is `90 50 02 FF` (on) or `90 50 03 FF`
/// (standby).
pub const POWER_INQUIRY: [u8; 5] = [0x81, 0x09, 0x04, 0x00, 0xFF];

/// The power state in an inquiry's reply (the VISCA message, without the
/// 8-byte header), if it is one.
pub fn power_reply(msg: &[u8]) -> Option<bool> {
    match msg {
        [0x90, 0x50, 0x02, 0xFF, ..] => Some(true),
        [0x90, 0x50, 0x03, 0xFF, ..] => Some(false),
        _ => None,
    }
}

/// `CAM_Memory Set`: keep the current framing in a preset (1-based in
/// Studio).
pub fn preset_store(preset: u8) -> Vec<u8> {
    vec![0x81, 0x01, 0x04, 0x3F, 0x01, preset.saturating_sub(1), 0xFF]
}

/// `CAM_Power`: on, or standby.
pub fn power(on: bool) -> Vec<u8> {
    vec![0x81, 0x01, 0x04, 0x00, if on { 0x02 } else { 0x03 }, 0xFF]
}

pub fn preset_recall(preset: u8) -> Vec<u8> {
    vec![0x81, 0x01, 0x04, 0x3F, 0x02, preset.saturating_sub(1), 0xFF]
}

/// Zoom at `speed` from -1 (wide) to 1 (tele); 0 stops.
pub fn zoom(speed: f32) -> Vec<u8> {
    let p = |s: f32| ((s.abs() * 7.0).round() as u8).min(7);
    let arg = if speed > 0.0 {
        0x20 | p(speed)
    } else if speed < 0.0 {
        0x30 | p(speed)
    } else {
        0x00
    };
    vec![0x81, 0x01, 0x04, 0x07, arg, 0xFF]
}

/// Pan and tilt at speeds from -1 to 1 (right, up); 0 stops that axis.
pub fn drive(pan: f32, tilt: f32) -> Vec<u8> {
    let speed = |s: f32, max: f32| ((s.abs() * max).round() as u8).clamp(1, max as u8);
    let vv = speed(pan, 24.0);
    let ww = speed(tilt, 23.0);
    let x = if pan > 0.0 {
        0x02
    } else if pan < 0.0 {
        0x01
    } else {
        0x03
    };
    let y = if tilt > 0.0 {
        0x01
    } else if tilt < 0.0 {
        0x02
    } else {
        0x03
    };
    vec![0x81, 0x01, 0x06, 0x01, vv, ww, x, y, 0xFF]
}

/// A Sony PTZ over VISCA over IP.
///
/// Its status is what Studio last asked for: a recalled preset is taken as
/// reached after `travel` (the inquiries come with the real driver loop).
pub struct ViscaPtz {
    name: String,
    socket: UdpSocket,
    camera: SocketAddr,
    seq: u32,
    preset: Option<u8>,
    moving_until: Option<Instant>,
    travel: Duration,
}

impl ViscaPtz {
    pub fn connect(name: impl Into<String>, camera: SocketAddr, travel: Duration) -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        Ok(ViscaPtz {
            name: name.into(),
            socket,
            camera,
            seq: 0,
            preset: None,
            moving_until: None,
            travel,
        })
    }

    fn send(&mut self, msg: &[u8]) -> Result<()> {
        self.seq = self.seq.wrapping_add(1);
        self.socket
            .send_to(&datagram(self.seq, msg), self.camera)
            .map_err(|e| DeviceError::Refused(self.name.clone(), e.to_string()))?;
        Ok(())
    }
}

impl Ptz for ViscaPtz {
    fn name(&self) -> String {
        format!("{} ({})", self.name, self.camera)
    }
    fn recall(&mut self, preset: u8) -> Result<()> {
        self.send(&preset_recall(preset))?;
        self.preset = Some(preset);
        self.moving_until = Some(Instant::now() + self.travel);
        Ok(())
    }
    fn status(&mut self) -> PtzStatus {
        let moving = self.moving_until.is_some_and(|u| Instant::now() < u);
        PtzStatus {
            online: true,
            moving,
            preset: if moving { None } else { self.preset },
        }
    }
    fn zoom(&mut self, speed: f32) -> Result<()> {
        self.send(&zoom(speed))
    }
    fn drive(&mut self, pan: f32, tilt: f32) -> Result<()> {
        self.send(&drive(pan, tilt))
    }
    fn power(&mut self, on: bool) -> Result<()> {
        self.send(&power(on))
    }
    fn check(&mut self) -> Result<bool> {
        self.seq = self.seq.wrapping_add(1);
        self.socket
            .send_to(&inquiry_datagram(self.seq, &POWER_INQUIRY), self.camera)
            .map_err(|e| DeviceError::Refused(self.name.clone(), e.to_string()))?;
        let deadline = Instant::now() + Duration::from_millis(800);
        let mut buf = [0u8; 64];
        while Instant::now() < deadline {
            self.socket.set_read_timeout(Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(10)),
            ))?;
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) if from.ip() == self.camera.ip() && n > 8 => {
                    if let Some(on) = power_reply(&buf[8..n]) {
                        return Ok(on);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Err(DeviceError::NotConnected(self.name.clone()))
    }
    fn store(&mut self, preset: u8) -> Result<()> {
        self.send(&preset_store(preset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preset_recall_datagram() {
        assert_eq!(
            datagram(7, &preset_recall(3)),
            vec![
                0x01, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x07, 0x81, 0x01, 0x04, 0x3F, 0x02, 0x02,
                0xFF
            ]
        );
    }

    #[test]
    fn the_slowest_zoom_in_and_the_stop() {
        assert_eq!(zoom(0.05), vec![0x81, 0x01, 0x04, 0x07, 0x20, 0xFF]);
        assert_eq!(zoom(-1.0), vec![0x81, 0x01, 0x04, 0x07, 0x37, 0xFF]);
        assert_eq!(zoom(0.0), vec![0x81, 0x01, 0x04, 0x07, 0x00, 0xFF]);
    }

    #[test]
    fn a_gentle_pan_right_without_tilt() {
        assert_eq!(
            drive(0.1, 0.0),
            vec![0x81, 0x01, 0x06, 0x01, 0x02, 0x01, 0x02, 0x03, 0xFF]
        );
        assert_eq!(drive(-1.0, 1.0)[4..8], [0x18, 0x17, 0x01, 0x01]);
        assert_eq!(drive(0.0, 0.0)[6..8], [0x03, 0x03], "stop");
    }

    #[test]
    fn the_camera_receives_it_in_order() {
        let cam = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut ptz = ViscaPtz::connect(
            "Cam 2",
            cam.local_addr().unwrap(),
            Duration::from_millis(10),
        )
        .unwrap();
        ptz.recall(1).unwrap();
        ptz.zoom(0.05).unwrap();
        let mut buf = [0u8; 64];
        let n = cam.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], datagram(1, &preset_recall(1)).as_slice());
        let n = cam.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], datagram(2, &zoom(0.05)).as_slice());
        assert!(ptz.status().moving);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            ptz.status(),
            PtzStatus {
                online: true,
                moving: false,
                preset: Some(1)
            }
        );
    }

    #[test]
    fn the_camera_says_whether_it_is_on() {
        let cam = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = cam.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let mut buf = [0u8; 64];
            let (n, from) = cam.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..2], &[0x01, 0x10]);
            assert_eq!(&buf[8..n], &POWER_INQUIRY);
            let mut reply = vec![0x01, 0x11, 0x00, 0x04];
            reply.extend_from_slice(&buf[4..8]);
            reply.extend_from_slice(&[0x90, 0x50, 0x03, 0xFF]);
            cam.send_to(&reply, from).unwrap();
        });
        let mut ptz = ViscaPtz::connect("Cam 2", addr, Duration::from_millis(10)).unwrap();
        assert!(!ptz.check().unwrap(), "en veille");
        fake.join().unwrap();
        assert_eq!(
            preset_store(3),
            vec![0x81, 0x01, 0x04, 0x3F, 0x01, 0x02, 0xFF]
        );
    }

    #[test]
    fn a_camera_that_does_not_answer_is_said() {
        let cam = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut ptz = ViscaPtz::connect(
            "Cam 3",
            cam.local_addr().unwrap(),
            Duration::from_millis(10),
        )
        .unwrap();
        assert!(ptz.check().is_err());
    }
}
