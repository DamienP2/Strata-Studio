//! vMix, its TCP API (port 8099): one line per command.
//!
//! - `FUNCTION CutDirect Input=n` puts an input on the program at once ;
//! - `SUBSCRIBE TALLY` makes vMix send `TALLY OK 0120…` at each change: one
//!   digit per input, 1 on the program, 2 on the preview ;
//! - the side by side and the picture in picture are **layout inputs**
//!   prepared in vMix (two layers each), set in Studio's settings; their
//!   layers are filled with `FUNCTION SetMultiViewOverlay
//!   Input=layout&Value=layer,input`, then the layout input is cut to.

use crate::{DeviceError, Result, Shown, Switcher};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PORT: u16 = 8099;

/// The layout inputs prepared in vMix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmixLayout {
    /// Two layers: left, right.
    pub split_input: u16,
    /// Two layers: main, inset.
    pub pip_input: u16,
}

impl VmixLayout {
    /// The commands showing (or only preparing) a picture.
    pub fn commands(&self, shown: Shown, take: bool) -> Vec<String> {
        let (layout, a, b) = match shown {
            Shown::Input(i) => {
                return if take {
                    vec![format!("FUNCTION CutDirect Input={i}")]
                } else {
                    vec![]
                }
            }
            Shown::Split { left, right } => (self.split_input, left, right),
            Shown::Pip { main, inset } => (self.pip_input, main, inset),
        };
        let mut v = vec![
            format!("FUNCTION SetMultiViewOverlay Input={layout}&Value=1,{a}"),
            format!("FUNCTION SetMultiViewOverlay Input={layout}&Value=2,{b}"),
        ];
        if take {
            v.push(format!("FUNCTION CutDirect Input={layout}"));
        }
        v
    }
}

/// The input on the program in a `TALLY OK` line (1-based).
pub fn program_in_tally(line: &str) -> Option<u16> {
    let digits = line.trim().strip_prefix("TALLY OK ")?;
    digits.chars().position(|c| c == '1').map(|i| i as u16 + 1)
}

#[derive(Default)]
struct State {
    connected: bool,
    program: Option<u16>,
}

/// A connection to vMix, kept by its own thread (and made again if lost).
pub struct VmixSwitcher {
    addr: SocketAddr,
    layout: VmixLayout,
    state: Arc<Mutex<State>>,
    tx: Sender<String>,
}

impl VmixSwitcher {
    pub fn connect(addr: SocketAddr, layout: VmixLayout) -> Result<Self> {
        let state = Arc::new(Mutex::new(State::default()));
        let (tx, rx) = mpsc::channel();
        let st = state.clone();
        std::thread::Builder::new()
            .name(format!("vmix {addr}"))
            .spawn(move || run(addr, st, rx))?;
        Ok(VmixSwitcher {
            addr,
            layout,
            state,
            tx,
        })
    }

    fn send(&self, lines: Vec<String>) -> Result<()> {
        for l in lines {
            self.tx
                .send(l)
                .map_err(|_| DeviceError::NotConnected(self.name()))?;
        }
        Ok(())
    }
}

fn run(addr: SocketAddr, state: Arc<Mutex<State>>, rx: Receiver<String>) {
    loop {
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else {
            state.lock().expect("vmix state").connected = false;
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
        if stream.write_all(b"SUBSCRIBE TALLY\r\n").is_err() {
            continue;
        }
        state.lock().expect("vmix state").connected = true;
        let Ok(read) = stream.try_clone() else {
            continue;
        };
        let mut reader = BufReader::new(read);
        let mut line = String::new();
        'session: loop {
            match reader.read_line(&mut line) {
                Ok(0) => break 'session,
                Ok(_) => {
                    if let Some(p) = program_in_tally(&line) {
                        state.lock().expect("vmix state").program = Some(p);
                    }
                    line.clear();
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break 'session,
            }
            loop {
                match rx.try_recv() {
                    Ok(cmd) => {
                        if stream.write_all(format!("{cmd}\r\n").as_bytes()).is_err() {
                            break 'session;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return,
                }
            }
        }
        state.lock().expect("vmix state").connected = false;
    }
}

impl Switcher for VmixSwitcher {
    fn name(&self) -> String {
        format!("vMix {}", self.addr)
    }
    fn connected(&self) -> bool {
        self.state.lock().expect("vmix state").connected
    }
    fn cut(&mut self, input: u16) -> Result<()> {
        self.send(vec![format!("FUNCTION CutDirect Input={input}")])
    }
    fn program(&self) -> Option<u16> {
        self.state.lock().expect("vmix state").program
    }
    fn show(&mut self, shown: Shown) -> Result<()> {
        self.send(self.layout.commands(shown, true))
    }
    fn preload(&mut self, shown: Shown) -> Result<()> {
        self.send(self.layout.commands(shown, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn tally_lines() {
        assert_eq!(program_in_tally("TALLY OK 0201\r\n"), Some(4));
        assert_eq!(program_in_tally("TALLY OK 1000"), Some(1));
        assert_eq!(program_in_tally("TALLY OK 0200"), None);
        assert_eq!(program_in_tally("FUNCTION OK Completed"), None);
    }

    #[test]
    fn a_split_fills_the_layout_then_takes_it() {
        let l = VmixLayout {
            split_input: 20,
            pip_input: 21,
        };
        assert_eq!(
            l.commands(Shown::Split { left: 2, right: 6 }, true),
            vec![
                "FUNCTION SetMultiViewOverlay Input=20&Value=1,2",
                "FUNCTION SetMultiViewOverlay Input=20&Value=2,6",
                "FUNCTION CutDirect Input=20",
            ]
        );
        assert_eq!(
            l.commands(Shown::Pip { main: 6, inset: 2 }, false).len(),
            2,
            "prepared, not taken"
        );
    }

    #[test]
    fn it_subscribes_reads_the_program_and_cuts() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut sw = VmixSwitcher::connect(
            server.local_addr().unwrap(),
            VmixLayout {
                split_input: 20,
                pip_input: 21,
            },
        )
        .unwrap();
        let (mut s, _) = server.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut buf = [0u8; 256];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"SUBSCRIBE TALLY\r\n");
        s.write_all(b"SUBSCRIBE OK TALLY\r\nTALLY OK 0102\r\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(sw.program(), Some(2));
        assert!(sw.connected());
        sw.cut(3).unwrap();
        let n = s.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"FUNCTION CutDirect Input=3\r\n");
    }
}
