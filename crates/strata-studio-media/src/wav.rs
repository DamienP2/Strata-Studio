//! One microphone, one 24-bit 48 kHz WAV file, written as the sound comes.
//!
//! The header is written first with the sizes left at zero, and set when
//! the file is closed; a file cut short (a crash) keeps its sound and
//! only needs its header mended.

use crate::Result;
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub struct WavWriter {
    path: PathBuf,
    out: BufWriter<File>,
    rate: u32,
    frames: u64,
}

fn header(rate: u32, data_len: u32) -> [u8; 44] {
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&1u16.to_le_bytes()); // mono
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * 3).to_le_bytes());
    h[32..34].copy_from_slice(&3u16.to_le_bytes());
    h[34..36].copy_from_slice(&24u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

impl WavWriter {
    pub fn create(path: impl AsRef<Path>, rate: u32) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut out = BufWriter::new(File::create(&path)?);
        out.write_all(&header(rate, 0))?;
        Ok(WavWriter {
            path,
            out,
            rate,
            frames: 0,
        })
    }

    /// Samples from -1 to 1.
    pub fn write(&mut self, samples: impl IntoIterator<Item = f32>) -> Result<()> {
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * 8_388_607.0) as i32;
            self.out.write_all(&v.to_le_bytes()[0..3])?;
            self.frames += 1;
        }
        Ok(())
    }

    pub fn seconds(&self) -> f64 {
        self.frames as f64 / self.rate as f64
    }

    /// Sets the sizes in the header; the file is complete.
    pub fn finish(mut self) -> Result<PathBuf> {
        let data_len = (self.frames * 3).min(u32::MAX as u64 - 36) as u32;
        self.out.flush()?;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&header(self.rate, data_len))?;
        f.sync_all()?;
        Ok(self.path)
    }
}

/// The microphones of a shoot: one file per seat, fed with the stream's
/// interleaved blocks.
pub struct MicFiles {
    channels: usize,
    /// (channel in the stream, its file)
    files: Vec<(usize, WavWriter)>,
}

impl MicFiles {
    /// `mics`: (file path, channel in the stream).
    pub fn create(mics: &[(PathBuf, usize)], channels: usize, rate: u32) -> Result<Self> {
        let files = mics
            .iter()
            .map(|(p, ch)| Ok((*ch, WavWriter::create(p, rate)?)))
            .collect::<Result<_>>()?;
        Ok(MicFiles { channels, files })
    }

    pub fn feed(&mut self, interleaved: &[f32]) -> Result<()> {
        for (ch, w) in &mut self.files {
            w.write(interleaved.iter().skip(*ch).step_by(self.channels).copied())?;
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<PathBuf>> {
        self.files.into_iter().map(|(_, w)| w.finish()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("strata-media-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_file_per_microphone() {
        let d = tmp("mics");
        let mut m = MicFiles::create(
            &[(d.join("paul.wav"), 0), (d.join("alice.wav"), 2)],
            4,
            48000,
        )
        .unwrap();
        // 1 s of 4 channels: channel 0 at 0.5, channel 2 at -0.25.
        let block: Vec<f32> = (0..48000).flat_map(|_| [0.5, 0.1, -0.25, 0.0]).collect();
        m.feed(&block).unwrap();
        let files = m.finish().unwrap();
        let paul = std::fs::read(&files[0]).unwrap();
        assert_eq!(paul.len(), 44 + 48000 * 3);
        assert_eq!(&paul[0..4], b"RIFF");
        assert_eq!(u16::from_le_bytes([paul[34], paul[35]]), 24);
        assert_eq!(
            u32::from_le_bytes([paul[40], paul[41], paul[42], paul[43]]),
            48000 * 3
        );
        let first = i32::from_le_bytes([0, paul[44], paul[45], paul[46]]) >> 8;
        assert_eq!(first, (0.5 * 8_388_607.0) as i32);
        let alice = std::fs::read(&files[1]).unwrap();
        let first = i32::from_le_bytes([0, alice[44], alice[45], alice[46]]) >> 8;
        assert_eq!(first, (-0.25 * 8_388_607.0) as i32);
        std::fs::remove_dir_all(d).unwrap();
    }
}
