//! The speech recogniser: a whisper.cpp server running on the Studio
//! machine (`whisper-server`, its HTTP example), with a multilingual model
//! loaded. Nothing leaves the machine.
//!
//! One sentence is one request: `POST /inference`, multipart, with the WAV
//! (`file`), the language (`language=fr`), no randomness
//! (`temperature=0.0`) and a JSON answer (`response_format=json`), which
//! carries the text in `text`.

use crate::{Result, SenseError};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// Turns a sentence (a 16 kHz WAV) into text.
pub trait Recognizer: Send {
    fn transcribe(&mut self, wav: &[u8]) -> Result<String>;
}

pub struct WhisperServer {
    pub addr: SocketAddr,
    pub language: String,
    pub timeout: Duration,
}

impl WhisperServer {
    pub fn new(addr: SocketAddr) -> Self {
        WhisperServer {
            addr,
            language: "fr".into(),
            timeout: Duration::from_secs(10),
        }
    }

    /// The multipart body of a request, and its boundary.
    pub fn body(&self, wav: &[u8]) -> (Vec<u8>, String) {
        let boundary = "strata-studio-boundary-7f3a".to_string();
        let mut b = Vec::with_capacity(wav.len() + 512);
        let field = |b: &mut Vec<u8>, name: &str, value: &str| {
            b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
        };
        field(&mut b, "language", &self.language);
        field(&mut b, "temperature", "0.0");
        field(&mut b, "response_format", "json");
        b.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"phrase.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes(),
        );
        b.extend_from_slice(wav);
        b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        (b, boundary)
    }
}

impl Recognizer for WhisperServer {
    fn transcribe(&mut self, wav: &[u8]) -> Result<String> {
        let err = |m: String| SenseError::Recognizer(m);
        let (body, boundary) = self.body(wav);
        let mut stream = TcpStream::connect_timeout(&self.addr, self.timeout)
            .map_err(|e| err(format!("{} : {e}", self.addr)))?;
        stream.set_read_timeout(Some(self.timeout))?;
        let head = format!(
            "POST /inference HTTP/1.1\r\nHost: {}\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.addr,
            body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&body)?;
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply)?;
        let reply = String::from_utf8_lossy(&reply);
        let (status, rest) = reply
            .split_once("\r\n")
            .ok_or_else(|| err("réponse vide".into()))?;
        if !status.contains(" 200 ") {
            return Err(err(status.to_string()));
        }
        let body = rest.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        let json: serde_json::Value = serde_json::from_str(body.trim())
            .map_err(|e| err(format!("réponse illisible : {e}")))?;
        Ok(json["text"].as_str().unwrap_or("").trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn a_sentence_sent_and_its_text_read() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let fake = std::thread::spawn(move || {
            let (mut s, _) = server.accept().unwrap();
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            // Read until the end of the multipart body.
            while !String::from_utf8_lossy(&got).contains("--strata-studio-boundary-7f3a--") {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
            }
            let body = "{\"text\":\" On retrouve tout de suite Julie en duplex.\\n\"}";
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            String::from_utf8_lossy(&got).to_string()
        });
        let mut w = WhisperServer::new(addr);
        let wav = crate::wav::encode(&[0i16; 160], 16000);
        assert_eq!(
            w.transcribe(&wav).unwrap(),
            "On retrouve tout de suite Julie en duplex."
        );
        let request = fake.join().unwrap();
        assert!(request.starts_with("POST /inference HTTP/1.1"));
        assert!(request.contains("name=\"language\"\r\n\r\nfr"));
        assert!(request.contains("filename=\"phrase.wav\""));
        assert!(request.contains("RIFF"));
    }

    #[test]
    fn no_server_is_an_error_not_a_hang() {
        let mut w = WhisperServer {
            timeout: Duration::from_millis(300),
            ..WhisperServer::new("127.0.0.1:9".parse().unwrap())
        };
        assert!(w.transcribe(&[]).is_err());
    }
}
