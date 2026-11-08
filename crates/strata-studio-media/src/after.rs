//! After the shoot: the 9:16 versions, and the copy to the final storage.
//!
//! - **9:16**: everything is recorded in 16:9, framed for a vertical cut
//!   when 9:16 was chosen; the vertical version is the centre of the
//!   picture (`crop=ih*9/16:ih`), scaled to 1080 × 1920. Made after the
//!   shoot, it does not weigh on the encoders while recording.
//! - **Copy**: to the storage chosen (a mounted SMB share), file by file,
//!   with the progress in bytes; a file is copied under a temporary name
//!   and renamed once whole, so that nobody picks up half a file.

use crate::encoder::Encoder;
use crate::{MediaError, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The 9:16 version's name: `…_CAM1.mov` → `…_CAM1_9x16.mov`.
pub fn vertical_name(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mov".into());
    path.with_file_name(format!("{stem}_9x16.{ext}"))
}

/// The ffmpeg command making the 9:16 version of a 16:9 file.
pub fn vertical_command(ffmpeg: &str, src: &Path, enc: &Encoder) -> Result<Command> {
    let (video, pix) = enc.args()?;
    let mut c = Command::new(ffmpeg);
    c.args(["-hide_banner", "-nostats", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args([
            "-vf",
            "crop=trunc(ih*9/16/2)*2:ih,scale=1080:1920:flags=lanczos",
        ])
        .args(&video)
        .args(["-pix_fmt", &pix, "-c:a", "copy"])
        .arg(vertical_name(src))
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    Ok(c)
}

/// Makes the 9:16 version of a file; returns its path.
pub fn make_vertical(ffmpeg: &str, src: &Path, enc: &Encoder) -> Result<PathBuf> {
    let out = vertical_command(ffmpeg, src, enc)?
        .output()
        .map_err(|e| MediaError::Ffmpeg(e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(MediaError::Ffmpeg(
            err.lines().last().unwrap_or("échec").to_string(),
        ));
    }
    Ok(vertical_name(src))
}

/// Copies files into `dest` (kept in a folder of the same name as their
/// own), calling `progress(done, total)` in bytes as it goes.
pub fn copy_all(
    files: &[PathBuf],
    dest: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dest)?;
    let total: u64 = files
        .iter()
        .map(|f| std::fs::metadata(f).map(|m| m.len()).unwrap_or(0))
        .sum();
    let mut done = 0u64;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 4 << 20];
    for f in files {
        let name = f
            .file_name()
            .ok_or_else(|| MediaError::Settings(format!("{} : pas un fichier", f.display())))?;
        let target = dest.join(name);
        let partial = dest.join(format!(".{}.partiel", name.to_string_lossy()));
        let mut from = File::open(f)?;
        let mut to = File::create(&partial)?;
        loop {
            let n = from.read(&mut buf)?;
            if n == 0 {
                break;
            }
            to.write_all(&buf[..n])?;
            done += n as u64;
            progress(done, total);
        }
        to.sync_all()?;
        std::fs::rename(&partial, &target)?;
        out.push(target);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vertical_name_and_command() {
        assert_eq!(
            vertical_name(Path::new("/r/P_2026-11-08_PGM.mov")),
            PathBuf::from("/r/P_2026-11-08_PGM_9x16.mov")
        );
        let enc = Encoder::new(
            crate::encoder::Codec::H264,
            crate::encoder::Accel::Cpu,
            crate::encoder::Resolution::Uhd,
        );
        let c = vertical_command("ffmpeg", Path::new("/r/a.mov"), &enc).unwrap();
        let args: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a.starts_with("crop=")));
        assert_eq!(args.last().unwrap(), "/r/a_9x16.mov");
    }

    #[test]
    fn a_copy_with_its_progress() {
        let d = std::env::temp_dir().join(format!("strata-copy-{}", std::process::id()));
        let src = d.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let a = src.join("a.mov");
        let b = src.join("b.wav");
        std::fs::write(&a, vec![1u8; 5_000_000]).unwrap();
        std::fs::write(&b, vec![2u8; 1000]).unwrap();
        let mut seen = Vec::new();
        let out = copy_all(&[a, b], &d.join("dest"), |done, total| {
            seen.push((done, total))
        })
        .unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(std::fs::read(&out[1]).unwrap(), vec![2u8; 1000]);
        assert_eq!(*seen.last().unwrap(), (5_001_000, 5_001_000));
        assert!(seen.len() >= 2, "progress as it goes");
        assert!(!d.join("dest").join(".a.mov.partiel").exists());
        std::fs::remove_dir_all(d).unwrap();
    }
}
