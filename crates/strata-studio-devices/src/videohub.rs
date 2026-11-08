//! Blackmagic Videohub (the SDI « nodal »): TCP 9990, a text protocol.
//!
//! On connection the router sends its state in blocks, each a title line
//! (`INPUT LABELS:`, `OUTPUT LABELS:`, `VIDEO OUTPUT ROUTING:`…) then lines,
//! then an empty line. Numbers are 0-based. A route is asked with a block
//! `VIDEO OUTPUT ROUTING:` / `<output> <input>`; the router answers `ACK`
//! and sends the new routing to everyone.
//!
//! Studio speaks of sources and destinations by their **names** (the labels
//! set in the router): « PGM ATEM » → « GO 3 ».

use crate::{DeviceError, Result, Router};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PORT: u16 = 9990;

/// What the router has said: labels and routes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubState {
    pub inputs: BTreeMap<u16, String>,
    pub outputs: BTreeMap<u16, String>,
    /// output → input
    pub routes: BTreeMap<u16, u16>,
}

impl HubState {
    /// Reads one block (title and lines).
    pub fn apply(&mut self, title: &str, lines: &[String]) {
        let pairs = || {
            lines.iter().filter_map(|l| {
                let (a, b) = l.split_once(' ')?;
                Some((a.parse::<u16>().ok()?, b.trim().to_string()))
            })
        };
        match title {
            "INPUT LABELS:" => self.inputs.extend(pairs()),
            "OUTPUT LABELS:" => self.outputs.extend(pairs()),
            "VIDEO OUTPUT ROUTING:" => self
                .routes
                .extend(pairs().filter_map(|(o, i)| Some((o, i.parse().ok()?)))),
            _ => {}
        }
    }

    fn find(map: &BTreeMap<u16, String>, name: &str) -> Option<u16> {
        map.iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(i, _)| *i)
    }
}

/// The block asking for a route.
pub fn route_block(output: u16, input: u16) -> String {
    format!("VIDEO OUTPUT ROUTING:\n{output} {input}\n\n")
}

pub struct Videohub {
    addr: SocketAddr,
    stream: Mutex<TcpStream>,
    state: Arc<Mutex<HubState>>,
}

impl Videohub {
    pub fn connect(addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
        let read = stream.try_clone()?;
        let state = Arc::new(Mutex::new(HubState::default()));
        let st = state.clone();
        std::thread::Builder::new()
            .name(format!("videohub {addr}"))
            .spawn(move || {
                let mut reader = BufReader::new(read);
                let (mut title, mut lines) = (String::new(), Vec::new());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                    let l = line.trim_end().to_string();
                    line.clear();
                    if l.is_empty() {
                        if !title.is_empty() {
                            st.lock().expect("videohub state").apply(&title, &lines);
                        }
                        title.clear();
                        lines.clear();
                    } else if title.is_empty() {
                        title = l;
                    } else {
                        lines.push(l);
                    }
                }
            })?;
        Ok(Videohub {
            addr,
            stream: Mutex::new(stream),
            state,
        })
    }

    pub fn state(&self) -> HubState {
        self.state.lock().expect("videohub state").clone()
    }
}

impl Router for Videohub {
    fn name(&self) -> String {
        format!("Videohub {}", self.addr)
    }
    fn route(&mut self, destination: &str, source: &str) -> Result<()> {
        let st = self.state();
        let out = HubState::find(&st.outputs, destination).ok_or_else(|| {
            DeviceError::Refused(self.name(), format!("pas de sortie « {destination} »"))
        })?;
        let inp = HubState::find(&st.inputs, source).ok_or_else(|| {
            DeviceError::Refused(self.name(), format!("pas d'entrée « {source} »"))
        })?;
        self.stream
            .lock()
            .expect("videohub stream")
            .write_all(route_block(out, inp).as_bytes())?;
        Ok(())
    }
    fn routes(&self) -> Vec<(String, String)> {
        let st = self.state();
        st.routes
            .iter()
            .map(|(o, i)| {
                (
                    st.outputs
                        .get(o)
                        .cloned()
                        .unwrap_or_else(|| format!("sortie {}", o + 1)),
                    st.inputs
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("entrée {}", i + 1)),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn routes_by_name() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            s.write_all(
                b"PROTOCOL PREAMBLE:\nVersion: 2.8\n\nINPUT LABELS:\n0 CAM 1\n1 PGM ATEM\n\nOUTPUT LABELS:\n0 ATEM 1\n2 GO 3\n\nVIDEO OUTPUT ROUTING:\n0 0\n2 0\n\n",
            )
            .unwrap();
            let mut buf = [0u8; 256];
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let n = s.read(&mut buf).unwrap();
            String::from_utf8_lossy(&buf[..n]).to_string()
        });
        let mut hub = Videohub::connect(addr).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(hub.routes().contains(&("GO 3".into(), "CAM 1".into())));
        hub.route("GO 3", "pgm atem").unwrap();
        assert_eq!(fake.join().unwrap(), "VIDEO OUTPUT ROUTING:\n2 1\n\n");
        assert!(hub.route("GO 9", "PGM ATEM").is_err());
    }
}
