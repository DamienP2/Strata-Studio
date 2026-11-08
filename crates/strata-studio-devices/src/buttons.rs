//! Bitfocus Buttons, over its HTTP API: what Studio does not drive itself
//! (the lights in DMX, the ON AIR light, the other automations).
//!
//! - press a button: `POST /api/location/<page>/<row>/<column>/press` ;
//! - set a variable the buttons read: `POST
//!   /api/custom-variable/<name>/value?value=<text>`.
//!
//! These are the routes of Bitfocus Companion's HTTP API; Buttons comes
//! from the same makers. To be checked against the studio's Buttons.

use crate::{Automation, DeviceError, Result};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// `text` made safe for a URL (percent-encoding of everything but
/// letters, digits and `-_.~`).
pub fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn press_path(page: u16, row: u8, column: u8) -> String {
    format!("/api/location/{page}/{row}/{column}/press")
}

pub fn variable_path(name: &str, value: &str) -> String {
    format!(
        "/api/custom-variable/{}/value?value={}",
        url_encode(name),
        url_encode(value)
    )
}

pub struct Buttons {
    pub addr: SocketAddr,
    pub timeout: Duration,
}

impl Buttons {
    pub fn new(addr: SocketAddr) -> Self {
        Buttons {
            addr,
            timeout: Duration::from_secs(2),
        }
    }

    fn post(&self, path: &str) -> Result<()> {
        let mut s = TcpStream::connect_timeout(&self.addr, self.timeout)?;
        s.set_read_timeout(Some(self.timeout))?;
        // One write: the whole request in one piece.
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.addr
        );
        s.write_all(request.as_bytes())?;
        let mut reply = String::new();
        s.read_to_string(&mut reply)?;
        let status = reply.lines().next().unwrap_or("");
        if status.contains(" 200 ") || status.contains(" 204 ") {
            Ok(())
        } else {
            Err(DeviceError::Refused(self.name(), status.to_string()))
        }
    }
}

impl Automation for Buttons {
    fn name(&self) -> String {
        format!("Bitfocus Buttons {}", self.addr)
    }
    fn press(&mut self, page: u16, row: u8, column: u8) -> Result<()> {
        self.post(&press_path(page, row, column))
    }
    fn set_variable(&mut self, name: &str, value: &str) -> Result<()> {
        self.post(&variable_path(name, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn paths() {
        assert_eq!(press_path(2, 0, 1), "/api/location/2/0/1/press");
        assert_eq!(
            variable_path("studio_speaker", "Léa Moreau"),
            "/api/custom-variable/studio_speaker/value?value=L%C3%A9a%20Moreau"
        );
    }

    #[test]
    fn a_press_reaches_buttons() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 512];
            while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = s.read(&mut buf).unwrap();
                got.extend_from_slice(&buf[..n]);
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                .unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        Buttons::new(addr).press(2, 1, 4).unwrap();
        assert!(fake
            .join()
            .unwrap()
            .starts_with("POST /api/location/2/1/4/press HTTP/1.1"));
    }
}
