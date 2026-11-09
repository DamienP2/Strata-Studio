//! Yamaha consoles (DM5, and DM3, DM7, CL, QL, TF): the Remote Control
//! Protocol, TCP 49280, one text line per command.
//!
//! - `set MIXER:Current/InCh/Fader/On <ch> 0 <0|1>` opens or mutes an
//!   input channel (0-based) ;
//! - `set MIXER:Current/InCh/Label/Name <ch> 0 "<name>"` names it ;
//! - `ssrecall_ex MIXER:Lib/Scene <n>` recalls a scene.
//!
//! The console answers each line with `OK …` or `ERROR …`. The parameter
//! names are those of the DM7 and CL / QL generation; to be checked on the
//! studio's DM5.

use crate::{AudioConsole, DeviceError, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub const PORT: u16 = 49280;

pub fn on_line(channel: u8, on: bool) -> String {
    format!(
        "set MIXER:Current/InCh/Fader/On {} 0 {}\n",
        channel.saturating_sub(1),
        on as u8
    )
}

/// A name as the console's label takes it: no quotes, 8 characters at most.
pub fn name_line(channel: u8, name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| *c != '"' && !c.is_control())
        .take(8)
        .collect();
    format!(
        "set MIXER:Current/InCh/Label/Name {} 0 \"{clean}\"\n",
        channel.saturating_sub(1)
    )
}

pub fn scene_line(scene: u16) -> String {
    format!("ssrecall_ex MIXER:Lib/Scene {scene}\n")
}

pub struct Yamaha {
    addr: SocketAddr,
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Yamaha {
    pub fn connect(addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Yamaha {
            addr,
            stream,
            reader,
        })
    }

    /// Sends a line and reads the console's answer.
    fn ask(&mut self, line: &str) -> Result<()> {
        self.stream.write_all(line.as_bytes())?;
        let mut answer = String::new();
        self.reader.read_line(&mut answer)?;
        if answer.starts_with("OK") {
            Ok(())
        } else {
            Err(DeviceError::Refused(self.name(), answer.trim().to_string()))
        }
    }

    pub fn recall_scene(&mut self, scene: u16) -> Result<()> {
        self.ask(&scene_line(scene))
    }

    /// `devinfo productname` → `OK devinfo productname "DM5"`.
    pub fn product(&mut self) -> Result<String> {
        self.stream.write_all(b"devinfo productname\n")?;
        let mut answer = String::new();
        self.reader.read_line(&mut answer)?;
        if !answer.starts_with("OK") {
            return Err(DeviceError::Refused(self.name(), answer.trim().to_string()));
        }
        Ok(answer.split('"').nth(1).unwrap_or("Yamaha").to_string())
    }
}

impl AudioConsole for Yamaha {
    fn check(&mut self) -> Result<String> {
        self.product()
    }
    fn name(&self) -> String {
        format!("Yamaha {}", self.addr)
    }
    fn set_on(&mut self, channel: u8, on: bool) -> Result<()> {
        self.ask(&on_line(channel, on))
    }
    fn set_name(&mut self, channel: u8, name: &str) -> Result<()> {
        self.ask(&name_line(channel, name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn lines() {
        assert_eq!(on_line(1, false), "set MIXER:Current/InCh/Fader/On 0 0 0\n");
        assert_eq!(
            name_line(3, "Alice \"Al\" Durand"),
            "set MIXER:Current/InCh/Label/Name 2 0 \"Alice Al\"\n"
        );
        assert_eq!(scene_line(12), "ssrecall_ex MIXER:Lib/Scene 12\n");
    }

    #[test]
    fn the_console_answers() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let (s, _) = server.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut w = s;
            let mut got = Vec::new();
            for _ in 0..2 {
                let mut l = String::new();
                r.read_line(&mut l).unwrap();
                let answer = if l.contains("Fader/On") {
                    format!("OK {}", &l[4..])
                } else {
                    "ERROR set InvalidArgument\n".into()
                };
                w.write_all(answer.as_bytes()).unwrap();
                got.push(l);
            }
            got
        });
        let mut y = Yamaha::connect(addr).unwrap();
        y.set_on(2, true).unwrap();
        assert!(y.set_name(2, "Alice").is_err(), "an ERROR is a refusal");
        let got = fake.join().unwrap();
        assert_eq!(got[0], "set MIXER:Current/InCh/Fader/On 1 0 1\n");
    }

    #[test]
    fn the_console_says_its_product() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let (s, _) = server.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut l = String::new();
            r.read_line(&mut l).unwrap();
            assert_eq!(l, "devinfo productname\n");
            (&s).write_all(b"OK devinfo productname \"DM5\"\n").unwrap();
        });
        let mut y = Yamaha::connect(addr).unwrap();
        assert_eq!(y.check().unwrap(), "DM5");
        fake.join().unwrap();
    }
}
