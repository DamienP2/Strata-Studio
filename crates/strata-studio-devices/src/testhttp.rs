//! A fake HTTP device for the tests: it answers a script of requests in
//! order and keeps what it received.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

pub struct FakeHttp {
    addr: String,
    got: Arc<Mutex<Vec<(String, String)>>>,
}

impl FakeHttp {
    /// `script`: (« METHOD /path » expected, status, body) in order. A
    /// request that does not match gets a 404.
    pub fn start(script: Vec<(&'static str, u16, &'static str)>) -> Self {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap().to_string();
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        std::thread::spawn(move || {
            let mut script = script.into_iter();
            for stream in server.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head, body) = loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break (String::from_utf8_lossy(&buf).to_string(), String::new());
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..end]).to_string();
                        let len = head
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        while buf.len() < end + 4 + len {
                            let n = s.read(&mut chunk).unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&chunk[..n]);
                        }
                        let body = String::from_utf8_lossy(&buf[end + 4..]).to_string();
                        break (head, body);
                    }
                };
                let line = head.lines().next().unwrap_or("").to_string();
                let request = line
                    .rsplit_once(' ')
                    .map_or(line.clone(), |(a, _)| a.to_string());
                g.lock().unwrap().push((request.clone(), body));
                let (status, reply) = match script.next() {
                    Some((want, status, reply)) if request.starts_with(want) => (status, reply),
                    _ => (404, "not found"),
                };
                let text = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = s.write_all(text.as_bytes());
            }
        });
        FakeHttp { addr, got }
    }

    pub fn addr(&self) -> String {
        self.addr.clone()
    }

    /// (« METHOD /path?query », body) received.
    pub fn requests(&self) -> Vec<(String, String)> {
        self.got.lock().unwrap().clone()
    }
}
