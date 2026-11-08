//! WAV files, 16-bit PCM: what the speech recogniser takes, and what a
//! recording of a microphone usually is.

use crate::{Result, SenseError};

/// A mono 16-bit PCM WAV file.
pub fn encode(samples: &[i16], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A 16-bit PCM WAV file: its first channel, from -1 to 1, and its rate.
pub fn decode(bytes: &[u8]) -> Result<(Vec<f32>, u32)> {
    let bad = |m: &str| SenseError::Audio(m.to_string());
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("pas un fichier WAV"));
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut pos = 12;
    let (mut channels, mut rate, mut bits) = (0u16, 0u32, 0u16);
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32_at(pos + 4) as usize;
        let body = pos + 8;
        if body + len > bytes.len() {
            return Err(bad("fichier WAV tronqué"));
        }
        if id == b"fmt " {
            if u16_at(body) != 1 {
                return Err(bad("seul le PCM est lu"));
            }
            channels = u16_at(body + 2);
            rate = u32_at(body + 4);
            bits = u16_at(body + 14);
        } else if id == b"data" {
            if bits != 16 || channels == 0 {
                return Err(bad("seul le PCM 16 bits est lu"));
            }
            let frame = 2 * channels as usize;
            let samples = bytes[body..body + len]
                .chunks_exact(frame)
                .map(|f| i16::from_le_bytes([f[0], f[1]]) as f32 / 32768.0)
                .collect();
            return Ok((samples, rate));
        }
        pos = body + len + (len & 1);
    }
    Err(bad("pas de données audio"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_round_trip() {
        let samples: Vec<i16> = (0..1600)
            .map(|i| ((i as f32 * 0.05).sin() * 12000.0) as i16)
            .collect();
        let (back, rate) = decode(&encode(&samples, 16000)).unwrap();
        assert_eq!(rate, 16000);
        assert_eq!(back.len(), samples.len());
        assert!((back[100] - samples[100] as f32 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn not_a_wav() {
        assert!(decode(b"hello").is_err());
    }
}
