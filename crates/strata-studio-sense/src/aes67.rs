//! The microphones, received from the Dante network in AES67 (ST 2110-30).
//!
//! - A stream is described by its **SDP** (multicast group, port, payload
//!   type, `L24/48000/n` channels), announced by **SAP** on
//!   239.255.255.255:9875 (or given by hand).
//! - Each **RTP** packet (usually 1 ms) carries interleaved 24-bit samples,
//!   big-endian.
//!
//! Studio only listens: the console keeps the sound; this copy feeds the
//! voice detection and the host's ear.

use crate::{Result, SenseError};
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// SAP announcements.
pub const SAP_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 255);
pub const SAP_PORT: u16 = 9875;

/// An AES67 stream, as its SDP describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Stream {
    pub name: String,
    /// Multicast group (or unicast address) and port.
    pub group: Ipv4Addr,
    pub port: u16,
    pub payload_type: u8,
    pub rate: u32,
    pub channels: u16,
}

/// The stream an SDP describes.
pub fn parse_sdp(sdp: &str) -> Result<Stream> {
    let bad = |m: &str| SenseError::Audio(format!("SDP : {m}"));
    let (mut name, mut group, mut port, mut pt, mut rate, mut channels) =
        (String::new(), None, None, None, None, None);
    for line in sdp.lines().map(str::trim) {
        if let Some(v) = line.strip_prefix("s=") {
            name = v.to_string();
        } else if let Some(v) = line.strip_prefix("c=IN IP4 ") {
            group = v.split('/').next().and_then(|a| a.parse::<Ipv4Addr>().ok());
        } else if let Some(v) = line.strip_prefix("m=audio ") {
            let mut it = v.split_whitespace();
            port = it.next().and_then(|p| p.parse::<u16>().ok());
            pt = it.nth(1).and_then(|p| p.parse::<u8>().ok());
        } else if let Some(v) = line.strip_prefix("a=rtpmap:") {
            // 96 L24/48000/8
            let mut it = v.split_whitespace();
            let _pt = it.next();
            let mut f = it.next().unwrap_or("").split('/');
            if f.next() != Some("L24") {
                return Err(bad("seul le L24 est lu"));
            }
            rate = f.next().and_then(|r| r.parse::<u32>().ok());
            channels = Some(f.next().and_then(|c| c.parse::<u16>().ok()).unwrap_or(1));
        }
    }
    Ok(Stream {
        name,
        group: group.ok_or_else(|| bad("pas d'adresse (c=)"))?,
        port: port.ok_or_else(|| bad("pas de port (m=audio)"))?,
        payload_type: pt.ok_or_else(|| bad("pas de type (m=audio)"))?,
        rate: rate.ok_or_else(|| bad("pas de rtpmap L24"))?,
        channels: channels.unwrap_or(1),
    })
}

/// The SDP inside a SAP announcement (RFC 2974), if it is one.
pub fn sap_sdp(packet: &[u8]) -> Option<String> {
    let first = *packet.first()?;
    if first >> 5 != 1 || first & 0x10 != 0 {
        return None; // not version 1, or IPv6 origin (not used here)
    }
    let auth = *packet.get(1)? as usize * 4;
    let mut at = 8 + auth;
    let rest = packet.get(at..)?;
    if rest.starts_with(b"v=0") {
        return Some(String::from_utf8_lossy(rest).into_owned());
    }
    let end = rest.iter().position(|&b| b == 0)?;
    if &rest[..end] != b"application/sdp" {
        return None;
    }
    at += end + 1;
    Some(String::from_utf8_lossy(packet.get(at..)?).into_owned())
}

/// The samples of an RTP packet: interleaved, from -1 to 1.
pub fn rtp_l24(packet: &[u8], payload_type: u8) -> Option<Vec<f32>> {
    if packet.len() < 12 || packet[0] >> 6 != 2 || packet[1] & 0x7f != payload_type {
        return None;
    }
    let csrc = (packet[0] & 0x0f) as usize;
    let mut at = 12 + 4 * csrc;
    if packet[0] & 0x10 != 0 {
        let len = u16::from_be_bytes([*packet.get(at + 2)?, *packet.get(at + 3)?]) as usize;
        at += 4 + 4 * len;
    }
    let mut end = packet.len();
    if packet[0] & 0x20 != 0 {
        end = end.checked_sub(*packet.last()? as usize)?;
    }
    let payload = packet.get(at..end)?;
    Some(
        payload
            .chunks_exact(3)
            .map(|b| {
                (((b[0] as i32) << 24 | (b[1] as i32) << 16 | (b[2] as i32) << 8) >> 8) as f32
                    / 8_388_608.0
            })
            .collect(),
    )
}

/// Receives a stream; blocks of interleaved samples come out of the channel.
pub fn receive(stream: &Stream, interface: Ipv4Addr) -> Result<Receiver<Vec<f32>>> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, stream.port))?;
    if stream.group.is_multicast() {
        socket.join_multicast_v4(&stream.group, &interface)?;
    }
    socket.set_read_timeout(Some(Duration::from_millis(500)))?;
    let (tx, rx) = mpsc::channel();
    let pt = stream.payload_type;
    std::thread::Builder::new()
        .name(format!("aes67 {}", stream.name))
        .spawn(move || {
            let mut buf = [0u8; 1500];
            loop {
                match socket.recv(&mut buf) {
                    Ok(n) => {
                        if let Some(s) = rtp_l24(&buf[..n], pt) {
                            if tx.send(s).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => return,
                }
            }
        })?;
    Ok(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDP: &str = "v=0\r\no=- 1 1 IN IP4 192.168.20.31\r\ns=M32 Dante\r\nc=IN IP4 239.69.12.1/32\r\nt=0 0\r\nm=audio 5004 RTP/AVP 97\r\na=rtpmap:97 L24/48000/8\r\na=ptime:1\r\n";

    #[test]
    fn an_sdp() {
        let s = parse_sdp(SDP).unwrap();
        assert_eq!(
            s,
            Stream {
                name: "M32 Dante".into(),
                group: Ipv4Addr::new(239, 69, 12, 1),
                port: 5004,
                payload_type: 97,
                rate: 48000,
                channels: 8
            }
        );
        assert!(parse_sdp(&SDP.replace("L24", "L16")).is_err());
    }

    #[test]
    fn a_sap_announcement() {
        let mut p = vec![0x20, 0, 0x12, 0x34, 192, 168, 20, 31];
        p.extend(b"application/sdp\0");
        p.extend(SDP.as_bytes());
        assert_eq!(sap_sdp(&p).unwrap(), SDP);
        let mut bare = vec![0x20, 0, 0x12, 0x34, 192, 168, 20, 31];
        bare.extend(SDP.as_bytes());
        assert_eq!(sap_sdp(&bare).unwrap(), SDP);
    }

    pub(crate) fn rtp(pt: u8, seq: u16, samples: &[f32]) -> Vec<u8> {
        let mut p = vec![0x80, pt];
        p.extend(seq.to_be_bytes());
        p.extend([0, 0, 0, 0, 0xde, 0xad, 0xbe, 0xef]);
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
            p.extend(&v.to_be_bytes()[1..4]);
        }
        p
    }

    #[test]
    fn rtp_samples() {
        let s = rtp_l24(&rtp(97, 1, &[0.5, -0.25, 0.0, -1.0]), 97).unwrap();
        assert!(
            (s[0] - 0.5).abs() < 1e-6
                && (s[1] + 0.25).abs() < 1e-6
                && s[2] == 0.0
                && (s[3] + 1.0).abs() < 1e-6
        );
        assert!(
            rtp_l24(&rtp(96, 1, &[0.5]), 97).is_none(),
            "another payload type"
        );
    }

    #[test]
    fn a_stream_received_on_the_loopback() {
        let stream = Stream {
            name: "test".into(),
            group: Ipv4Addr::LOCALHOST,
            port: 0,
            payload_type: 97,
            rate: 48000,
            channels: 2,
        };
        // Port 0 cannot be sent to: take a free one.
        let port = UdpSocket::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let rx = receive(&Stream { port, ..stream }, Ipv4Addr::UNSPECIFIED).unwrap();
        let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
        tx.send_to(&rtp(97, 7, &[0.25, -0.5, 0.125, 0.0]), ("127.0.0.1", port))
            .unwrap();
        let got = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(got.len(), 4);
        assert!((got[1] + 0.5).abs() < 1e-6);
    }
}
