//! Blackmagic ATEM switchers, all models: UDP 9910, the protocol of their
//! control panels.
//!
//! A packet: a 12-byte header (5 bits of flags and the length, the
//! session, the packet acknowledged, the packet's own number), then
//! commands (their length, 2 bytes, 2 spare bytes, 4 letters, their data).
//! The client says hello, the switcher answers with the session and sends
//! its whole state (ending with `InCm`), then each change; every packet
//! asking for it is acknowledged, or the switcher drops the client after a
//! few seconds. The session handling comes from Strata Multiview's tally.
//!
//! Studio sends:
//! - `CPgI` the program input of an M/E (a hard cut), `CPvI` its preview ;
//! - `DCut` a cut, `MAct` a macro run ;
//! - `CSBP` a SuperSource box (source, position, size): the side by side
//!   and the picture in picture are SuperSource layouts, put on air by
//!   taking the SuperSource input (6000).
//!
//! It reads `PrgI`, the program input: a change Studio did not ask for is
//! a person's hand on the panel.
//!
//! The SuperSource box layout (`CSBP`) is the one of protocol 8 and later;
//! the geometry is a setting ([`AtemLayout`]). Both are to be checked on
//! the studio's ATEM.

use crate::{DeviceError, Result, Shown, Switcher};
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PORT: u16 = 9910;
/// The SuperSource input.
pub const SUPERSOURCE: u16 = 6000;

const ACK_REQUEST: u16 = 0x01;
const NEW_SESSION: u16 = 0x02;
const ACK_REPLY: u16 = 0x10;

fn header(flags: u16, len: usize, session: u16, ack: u16) -> Vec<u8> {
    let mut h = vec![0u8; 12];
    h[0..2].copy_from_slice(&((flags << 11) | (len as u16 & 0x07ff)).to_be_bytes());
    h[2..4].copy_from_slice(&session.to_be_bytes());
    h[4..6].copy_from_slice(&ack.to_be_bytes());
    h
}

/// One command: its length, two spare bytes, its name, its data.
pub fn command(name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(8 + data.len());
    c.extend(((8 + data.len()) as u16).to_be_bytes());
    c.extend([0, 0]);
    c.extend(name);
    c.extend(data);
    c
}

/// The program input of an M/E: a hard cut to `source`.
pub fn program_input(me: u8, source: u16) -> Vec<u8> {
    let s = source.to_be_bytes();
    command(b"CPgI", &[me, 0, s[0], s[1]])
}

pub fn preview_input(me: u8, source: u16) -> Vec<u8> {
    let s = source.to_be_bytes();
    command(b"CPvI", &[me, 0, s[0], s[1]])
}

pub fn cut(me: u8) -> Vec<u8> {
    command(b"DCut", &[me, 0, 0, 0])
}

/// Run a macro (0-based index).
pub fn run_macro(index: u16) -> Vec<u8> {
    let i = index.to_be_bytes();
    command(b"MAct", &[i[0], i[1], 0, 0])
}

/// Where a SuperSource box is: centre (`x`, `y` in hundredths, -4800 to
/// 4800 and -2700 to 2700) and size (thousandths, 70 to 1000).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxGeom {
    pub x: i16,
    pub y: i16,
    pub size: u16,
}

/// A SuperSource box: on or off, its source, where it is.
pub fn supersource_box(index: u8, enabled: bool, source: u16, g: BoxGeom) -> Vec<u8> {
    // Changed: enabled, source, x, y, size.
    let mask: u16 = 0b1_1111;
    let mut d = vec![0u8; 24];
    d[0..2].copy_from_slice(&mask.to_be_bytes());
    d[2] = 0; // SuperSource 1
    d[3] = index;
    d[4] = enabled as u8;
    d[6..8].copy_from_slice(&source.to_be_bytes());
    d[8..10].copy_from_slice(&g.x.to_be_bytes());
    d[10..12].copy_from_slice(&g.y.to_be_bytes());
    d[12..14].copy_from_slice(&g.size.to_be_bytes());
    command(b"CSBP", &d)
}

/// The geometry of Studio's layouts in the SuperSource.
#[derive(Clone, Debug, PartialEq)]
pub struct AtemLayout {
    /// The M/E whose program Studio drives.
    pub me: u8,
    pub supersource: u16,
    /// Left, right.
    pub split: [BoxGeom; 2],
    /// Main, inset.
    pub pip: [BoxGeom; 2],
}

impl Default for AtemLayout {
    fn default() -> Self {
        AtemLayout {
            me: 0,
            supersource: SUPERSOURCE,
            split: [
                BoxGeom {
                    x: -800,
                    y: 0,
                    size: 500,
                },
                BoxGeom {
                    x: 800,
                    y: 0,
                    size: 500,
                },
            ],
            pip: [
                BoxGeom {
                    x: 0,
                    y: 0,
                    size: 1000,
                },
                BoxGeom {
                    x: 1150,
                    y: -620,
                    size: 300,
                },
            ],
        }
    }
}

impl AtemLayout {
    /// The commands setting the SuperSource boxes for a layout.
    pub fn boxes(&self, shown: Shown) -> Vec<Vec<u8>> {
        let (geom, sources) = match shown {
            Shown::Input(_) => return vec![],
            Shown::Split { left, right } => (self.split, [left, right]),
            Shown::Pip { main, inset } => (self.pip, [main, inset]),
        };
        let off = BoxGeom {
            x: 0,
            y: 0,
            size: 500,
        };
        vec![
            supersource_box(0, true, sources[0], geom[0]),
            supersource_box(1, true, sources[1], geom[1]),
            supersource_box(2, false, 0, off),
            supersource_box(3, false, 0, off),
        ]
    }
}

/// What the switcher said.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// The program input of an M/E.
    Program { me: u8, source: u16 },
    /// The whole state has been sent: commands can go.
    Ready,
}

/// One connection to a switcher.
pub struct Session {
    id: u16,
    /// The last packet of the switcher handled (repeats are only acknowledged).
    last: Option<u16>,
    /// Our own packets' numbers.
    number: u16,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            id: 0x53ab,
            last: None,
            number: 0,
        }
    }
}

impl Session {
    /// The first packet: asks the switcher for a session.
    pub fn hello(&mut self) -> Vec<u8> {
        self.last = None;
        self.number = 0;
        let mut p = header(NEW_SESSION, 20, self.id, 0);
        p[9] = 0x3a;
        p.extend([0x01, 0, 0, 0, 0, 0, 0, 0]);
        p
    }

    /// Commands in one packet the switcher must acknowledge.
    pub fn wrap(&mut self, cmds: &[Vec<u8>]) -> Vec<u8> {
        self.number = self.number.wrapping_add(1);
        let body: Vec<u8> = cmds.concat();
        let mut p = header(ACK_REQUEST, 12 + body.len(), self.id, 0);
        p[10..12].copy_from_slice(&self.number.to_be_bytes());
        p.extend(body);
        p
    }

    /// Handles a packet of the switcher: what to send back, what it said.
    pub fn on_packet(&mut self, p: &[u8]) -> (Vec<Vec<u8>>, Vec<Event>) {
        let (mut replies, mut events) = (Vec::new(), Vec::new());
        if p.len() < 12 {
            return (replies, events);
        }
        let word = u16::from_be_bytes([p[0], p[1]]);
        let flags = word >> 11;
        let len = ((word & 0x07ff) as usize).min(p.len());
        self.id = u16::from_be_bytes([p[2], p[3]]);
        let number = u16::from_be_bytes([p[10], p[11]]);
        if flags & NEW_SESSION != 0 {
            replies.push(header(ACK_REPLY, 12, self.id, number));
            return (replies, events);
        }
        if flags & ACK_REQUEST != 0 {
            replies.push(header(ACK_REPLY, 12, self.id, number));
            if self.last == Some(number) {
                return (replies, events);
            }
            self.last = Some(number);
            commands(&p[12..len], &mut events);
        }
        (replies, events)
    }
}

fn commands(mut d: &[u8], events: &mut Vec<Event>) {
    while d.len() >= 8 {
        let n = u16::from_be_bytes([d[0], d[1]]) as usize;
        if n < 8 || n > d.len() {
            break;
        }
        let (name, data) = (&d[4..8], &d[8..n]);
        match name {
            b"PrgI" if data.len() >= 4 => events.push(Event::Program {
                me: data[0],
                source: u16::from_be_bytes([data[2], data[3]]),
            }),
            b"InCm" => events.push(Event::Ready),
            _ => {}
        }
        d = &d[n..];
    }
}

#[derive(Default)]
struct State {
    connected: bool,
    program: Option<u16>,
}

/// A connection to an ATEM, kept alive by its own thread.
pub struct AtemSwitcher {
    addr: SocketAddr,
    layout: AtemLayout,
    state: Arc<Mutex<State>>,
    tx: Sender<Vec<Vec<u8>>>,
}

impl AtemSwitcher {
    pub fn connect(addr: SocketAddr, layout: AtemLayout) -> Result<Self> {
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        socket.set_read_timeout(Some(Duration::from_millis(20)))?;
        let state = Arc::new(Mutex::new(State::default()));
        let (tx, rx) = mpsc::channel();
        let me = layout.me;
        let st = state.clone();
        std::thread::Builder::new()
            .name(format!("atem {addr}"))
            .spawn(move || run(socket, addr, me, st, rx))?;
        Ok(AtemSwitcher {
            addr,
            layout,
            state,
            tx,
        })
    }

    fn send(&self, cmds: Vec<Vec<u8>>) -> Result<()> {
        if cmds.is_empty() {
            return Ok(());
        }
        self.tx
            .send(cmds)
            .map_err(|_| DeviceError::NotConnected(self.name()))
    }
}

/// The connection's life: hello, acknowledgements, commands once ready,
/// a new hello when the switcher has gone quiet.
fn run(
    socket: UdpSocket,
    addr: SocketAddr,
    me: u8,
    state: Arc<Mutex<State>>,
    rx: Receiver<Vec<Vec<u8>>>,
) {
    let mut session = Session::default();
    let mut ready = false;
    let mut heard = Instant::now();
    let mut waiting: Vec<Vec<Vec<u8>>> = Vec::new();
    let _ = socket.send_to(&session.hello(), addr);
    let mut buf = [0u8; 2048];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, from)) if from == addr => {
                heard = Instant::now();
                let (replies, events) = session.on_packet(&buf[..n]);
                for r in replies {
                    let _ = socket.send_to(&r, addr);
                }
                let mut s = state.lock().expect("atem state");
                s.connected = true;
                for e in events {
                    match e {
                        Event::Program { me: m, source } if m == me => s.program = Some(source),
                        Event::Ready => ready = true,
                        _ => {}
                    }
                }
            }
            Ok(_) => {}
            Err(_) => {}
        }
        loop {
            match rx.try_recv() {
                Ok(cmds) => waiting.push(cmds),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        if ready {
            for cmds in waiting.drain(..) {
                let _ = socket.send_to(&session.wrap(&cmds), addr);
            }
        }
        if heard.elapsed() > Duration::from_secs(3) {
            state.lock().expect("atem state").connected = false;
            ready = false;
            session = Session::default();
            let _ = socket.send_to(&session.hello(), addr);
            heard = Instant::now();
        }
    }
}

impl Switcher for AtemSwitcher {
    fn name(&self) -> String {
        format!("ATEM {}", self.addr)
    }
    fn connected(&self) -> bool {
        self.state.lock().expect("atem state").connected
    }
    fn cut(&mut self, input: u16) -> Result<()> {
        self.send(vec![program_input(self.layout.me, input)])
    }
    fn program(&self) -> Option<u16> {
        self.state.lock().expect("atem state").program
    }
    fn show(&mut self, shown: Shown) -> Result<()> {
        let mut cmds = self.layout.boxes(shown);
        cmds.push(program_input(
            self.layout.me,
            match shown {
                Shown::Input(i) => i,
                _ => self.layout.supersource,
            },
        ));
        self.send(cmds)
    }
    fn preload(&mut self, shown: Shown) -> Result<()> {
        self.send(self.layout.boxes(shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commands_bytes() {
        assert_eq!(
            program_input(0, 6000),
            vec![0, 12, 0, 0, b'C', b'P', b'g', b'I', 0, 0, 0x17, 0x70]
        );
        assert_eq!(run_macro(3)[8..], [0, 3, 0, 0]);
        let b = supersource_box(
            1,
            true,
            6,
            BoxGeom {
                x: 800,
                y: -270,
                size: 500,
            },
        );
        assert_eq!(b.len(), 32);
        assert_eq!(&b[4..8], b"CSBP");
        assert_eq!(&b[8..16], &[0, 0x1f, 0, 1, 1, 0, 0, 6]);
        assert_eq!(&b[16..22], &[0x03, 0x20, 0xfe, 0xf2, 0x01, 0xf4]);
    }

    #[test]
    fn a_split_sets_two_boxes_and_turns_the_others_off() {
        let cmds = AtemLayout::default().boxes(Shown::Split { left: 2, right: 6 });
        assert_eq!(cmds.len(), 4);
        assert_eq!(u16::from_be_bytes([cmds[0][14], cmds[0][15]]), 2);
        assert_eq!(u16::from_be_bytes([cmds[1][14], cmds[1][15]]), 6);
        assert_eq!(cmds[2][12], 0, "box 3 off");
        assert!(AtemLayout::default().boxes(Shown::Input(3)).is_empty());
    }

    /// A switcher on a local socket, as far as Studio sees it.
    #[test]
    fn it_connects_reads_the_program_and_cuts() {
        let atem = UdpSocket::bind("127.0.0.1:0").unwrap();
        atem.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut sw =
            AtemSwitcher::connect(atem.local_addr().unwrap(), AtemLayout::default()).unwrap();
        let mut buf = [0u8; 2048];

        let (n, studio) = atem.recv_from(&mut buf).unwrap();
        assert_eq!(n, 20, "hello");
        let mut answer = header(NEW_SESSION, 20, 0x0042, 0);
        answer.extend([0u8; 8]);
        atem.send_to(&answer, studio).unwrap();
        atem.recv_from(&mut buf).unwrap(); // its acknowledgement

        // The state: camera 2 on the program, then the end of the state.
        let mut s = Session {
            id: 0x8042,
            ..Session::default()
        };
        let state = s.wrap(&[
            command(b"PrgI", &[0, 0, 0, 2]),
            command(b"InCm", &[1, 0, 0, 0]),
        ]);
        atem.send_to(&state, studio).unwrap();
        let (n, _) = atem.recv_from(&mut buf).unwrap();
        assert_eq!(n, 12, "acknowledged");
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(sw.program(), Some(2));
        assert!(sw.connected());

        sw.cut(5).unwrap();
        let (n, _) = atem.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[12..n], program_input(0, 5).as_slice());
        assert_eq!(
            u16::from_be_bytes([buf[2], buf[3]]),
            0x8042,
            "in the switcher's session"
        );

        sw.show(Shown::Split { left: 2, right: 6 }).unwrap();
        let (n, _) = atem.recv_from(&mut buf).unwrap();
        let body = &buf[12..n];
        assert_eq!(
            body.len(),
            4 * 32 + 12,
            "four boxes, then the SuperSource on air"
        );
        assert_eq!(&body[4 * 32..], program_input(0, SUPERSOURCE).as_slice());
    }
}
