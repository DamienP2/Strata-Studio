//! Midas M32 (and Behringer X32): OSC over UDP, port 10023.
//!
//! A message is its address, its type tags and its arguments, each padded
//! with zeros to a multiple of 4 bytes, numbers big-endian. The channels
//! are `/ch/01` to `/ch/32`: `/mix/on` (1 open, 0 muted), `/config/name`
//! (12 characters at most). The console sends its changes to a client that
//! says `/xremote` at least every 10 s.

use crate::{AudioConsole, DeviceError, Result};
use std::net::{SocketAddr, UdpSocket};

pub const PORT: u16 = 10023;

/// An OSC argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg<'a> {
    Int(i32),
    Float(f32),
    Str(&'a str),
}

fn pad(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(s);
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// One OSC message.
pub fn message(address: &str, args: &[Arg]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32);
    pad(&mut out, address.as_bytes());
    let mut tags = String::from(",");
    for a in args {
        tags.push(match a {
            Arg::Int(_) => 'i',
            Arg::Float(_) => 'f',
            Arg::Str(_) => 's',
        });
    }
    pad(&mut out, tags.as_bytes());
    for a in args {
        match a {
            Arg::Int(i) => out.extend_from_slice(&i.to_be_bytes()),
            Arg::Float(f) => out.extend_from_slice(&f.to_be_bytes()),
            Arg::Str(s) => pad(&mut out, s.as_bytes()),
        }
    }
    out
}

fn channel(ch: u8) -> Result<String> {
    if (1..=32).contains(&ch) {
        Ok(format!("/ch/{ch:02}"))
    } else {
        Err(DeviceError::Refused(
            "M32".into(),
            format!("pas de canal {ch}"),
        ))
    }
}

/// The name as the strip takes it: 12 characters at most.
fn strip_name(name: &str) -> String {
    name.chars().take(12).collect()
}

pub struct M32 {
    socket: UdpSocket,
    console: SocketAddr,
}

impl M32 {
    pub fn connect(console: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        let m = M32 { socket, console };
        m.send(&message("/xremote", &[]))?;
        Ok(m)
    }

    fn send(&self, msg: &[u8]) -> Result<()> {
        self.socket.send_to(msg, self.console)?;
        Ok(())
    }

    /// To be called every few seconds, to keep receiving the changes.
    pub fn keep_alive(&self) -> Result<()> {
        self.send(&message("/xremote", &[]))
    }
}

impl AudioConsole for M32 {
    fn name(&self) -> String {
        format!("Midas M32 {}", self.console)
    }
    fn set_on(&mut self, ch: u8, on: bool) -> Result<()> {
        self.send(&message(
            &format!("{}/mix/on", channel(ch)?),
            &[Arg::Int(i32::from(on))],
        ))
    }
    fn set_name(&mut self, ch: u8, name: &str) -> Result<()> {
        self.send(&message(
            &format!("{}/config/name", channel(ch)?),
            &[Arg::Str(&strip_name(name))],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mute_message() {
        assert_eq!(
            message("/ch/01/mix/on", &[Arg::Int(0)]),
            b"/ch/01/mix/on\0\0\0,i\0\0\0\0\0\0".to_vec()
        );
    }

    #[test]
    fn a_name_message() {
        assert_eq!(
            message("/ch/03/config/name", &[Arg::Str("Alice")]),
            b"/ch/03/config/name\0\0,s\0\0Alice\0\0\0".to_vec()
        );
    }

    #[test]
    fn channels_and_names_are_kept_in_range() {
        assert!(channel(0).is_err());
        assert!(channel(33).is_err());
        assert_eq!(channel(7).unwrap(), "/ch/07");
        assert_eq!(strip_name("Jean-Baptiste Durand"), "Jean-Baptist");
    }

    #[test]
    fn the_console_receives_it() {
        let console = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut m32 = M32::connect(console.local_addr().unwrap()).unwrap();
        m32.set_on(2, true).unwrap();
        let mut buf = [0u8; 64];
        let n = console.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], message("/xremote", &[]).as_slice());
        let n = console.recv(&mut buf).unwrap();
        assert_eq!(
            &buf[..n],
            message("/ch/02/mix/on", &[Arg::Int(1)]).as_slice()
        );
    }
}
