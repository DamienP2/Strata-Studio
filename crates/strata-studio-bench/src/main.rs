//! banc-encodage: how many 4:2:2 channels this machine encodes in real time.
//!
//! Studio records up to 6 channels (4 cameras, PGM, CLEAN) in H.264 or
//! H.265 4:2:2, at 1080p50 or 2160p50, and streams on top. This runs 1,
//! 2, 3… ffmpeg encodes at once on a generated picture and measures the
//! speed of each: a number of channels is held when every encode runs at
//! least 10 % faster than real time.
//!
//! ```text
//! banc-encodage --res 2160 --codec h265 --accel nvenc --max 8 --seconds 6
//! ```
//!
//! Accelerations: `cpu` (libx264, libx265), `nvenc` (NVIDIA; 4:2:2 needs a
//! Blackwell card and a recent FFmpeg), `qsv` (Intel; H.265 4:2:2 10-bit).
//! The pixel format and the extra ffmpeg options can be overridden
//! (`--pix-fmt`, `--opts`) to try what the machine's FFmpeg offers.
//!
//! The generated picture costs processor time too: the figures are a floor.

use std::process::{Command, Stdio};
use std::time::Instant;

const RATE: u32 = 50;
const MARGIN: f64 = 1.10;

struct Args {
    res: u32,
    codec: String,
    accel: String,
    max: u32,
    seconds: u32,
    pix_fmt: Option<String>,
    opts: Option<String>,
    ffmpeg: String,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        res: 1080,
        codec: "h264".into(),
        accel: "cpu".into(),
        max: 6,
        seconds: 5,
        pix_fmt: None,
        opts: None,
        ffmpeg: "ffmpeg".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or_else(|| format!("{k} : valeur manquante"));
        match k.as_str() {
            "--res" => a.res = v()?.parse().map_err(|_| "--res : 1080 ou 2160")?,
            "--codec" => a.codec = v()?,
            "--accel" => a.accel = v()?,
            "--max" => a.max = v()?.parse().map_err(|_| "--max : un nombre")?,
            "--seconds" => a.seconds = v()?.parse().map_err(|_| "--seconds : un nombre")?,
            "--pix-fmt" => a.pix_fmt = Some(v()?),
            "--opts" => a.opts = Some(v()?),
            "--ffmpeg" => a.ffmpeg = v()?,
            "-h" | "--help" => {
                return Err("banc-encodage [--res 1080|2160] [--codec h264|h265] [--accel cpu|nvenc|qsv] [--max N] [--seconds S] [--pix-fmt F] [--opts \"...\"] [--ffmpeg chemin]".into())
            }
            _ => return Err(format!("option inconnue : {k}")),
        }
    }
    if a.res != 1080 && a.res != 2160 {
        return Err("--res : 1080 ou 2160".into());
    }
    Ok(a)
}

/// The encoder options for a codec and an acceleration, 4:2:2.
fn encoder(a: &Args) -> Result<(Vec<String>, String), String> {
    let bitrate = if a.res == 2160 { "150M" } else { "50M" };
    let (args, pix): (&[&str], &str) = match (a.accel.as_str(), a.codec.as_str()) {
        ("cpu", "h264") => (
            &[
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-profile:v",
                "high422",
            ],
            "yuv422p10le",
        ),
        ("cpu", "h265") => (
            &[
                "-c:v",
                "libx265",
                "-preset",
                "ultrafast",
                "-x265-params",
                "log-level=error",
            ],
            "yuv422p10le",
        ),
        ("nvenc", "h264") => (
            &[
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p4",
                "-profile:v",
                "high422",
            ],
            "nv16",
        ),
        ("nvenc", "h265") => (
            &["-c:v", "hevc_nvenc", "-preset", "p4", "-profile:v", "rext"],
            "p210le",
        ),
        ("qsv", "h265") => (&["-c:v", "hevc_qsv", "-preset", "veryfast"], "y210le"),
        ("qsv", "h264") => {
            return Err("Quick Sync n'encode pas le H.264 en 4:2:2 : essayez --codec h265".into())
        }
        _ => return Err(format!("combinaison inconnue : {} / {}", a.accel, a.codec)),
    };
    let mut v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    v.extend(["-b:v".to_string(), bitrate.to_string()]);
    if let Some(o) = &a.opts {
        v.extend(o.split_whitespace().map(String::from));
    }
    Ok((v, a.pix_fmt.clone().unwrap_or_else(|| pix.to_string())))
}

fn command(a: &Args, enc: &[String], pix: &str) -> Command {
    let (w, h) = if a.res == 2160 {
        (3840, 2160)
    } else {
        (1920, 1080)
    };
    let frames = a.seconds * RATE;
    let mut c = Command::new(&a.ffmpeg);
    c.args([
        "-hide_banner",
        "-nostats",
        "-loglevel",
        "error",
        "-f",
        "lavfi",
        "-i",
    ])
    .arg(format!("testsrc2=size={w}x{h}:rate={RATE},format={pix}"))
    .args(["-frames:v", &frames.to_string()])
    .args(enc)
    .args(["-pix_fmt", pix, "-f", "null", "-"])
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    c
}

/// Runs `n` encodes at once; the speed of the slowest, in frames per second.
fn run(a: &Args, enc: &[String], pix: &str, n: u32) -> Result<f64, String> {
    let frames = (a.seconds * RATE) as f64;
    let start = Instant::now();
    let children: Vec<_> = (0..n)
        .map(|_| {
            command(a, enc, pix)
                .spawn()
                .map_err(|e| format!("ffmpeg introuvable ({}) : {e}", a.ffmpeg))
        })
        .collect::<Result<_, _>>()?;
    let mut slowest = f64::INFINITY;
    for child in children {
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let wall = start.elapsed().as_secs_f64();
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            // The first line saying why, rather than ffmpeg's last words.
            let telling = [
                "cannot",
                "not supported",
                "unsupported",
                "no device",
                "no capable",
                "failed",
                "error",
                "invalid",
            ];
            let why = err
                .lines()
                .map(str::trim)
                .find(|l| {
                    telling.iter().any(|w| l.to_lowercase().contains(w))
                        && !l.contains("Nothing was written")
                })
                .or_else(|| err.lines().rev().map(str::trim).find(|l| !l.is_empty()))
                .unwrap_or("échec")
                .to_string();
            return Err(format!("l'encodeur a échoué : {why}"));
        }
        slowest = slowest.min(frames / wall);
    }
    Ok(slowest)
}

fn main() {
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let (enc, pix) = match encoder(&a) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    println!(
        "Banc d'encodage · {} · {}p{RATE} · 4:2:2 ({pix}) · {} · {} s par essai",
        a.codec.to_uppercase(),
        a.res,
        a.accel,
        a.seconds
    );
    println!(
        "{:>8}  {:>18}  {:>10}",
        "canaux", "i/s par canal", "temps réel"
    );
    let mut held = 0;
    for n in 1..=a.max {
        match run(&a, &enc, &pix, n) {
            Ok(fps) => {
                let ok = fps >= RATE as f64 * MARGIN;
                println!(
                    "{n:>8}  {fps:>18.1}  {:>9.2}× {}",
                    fps / RATE as f64,
                    if ok { "tient" } else { "ne tient pas" }
                );
                if !ok {
                    break;
                }
                held = n;
            }
            Err(e) => {
                println!("{n:>8}  {e}");
                break;
            }
        }
    }
    println!();
    if held == 0 {
        println!("Verdict : aucun canal en temps réel avec ces réglages.");
    } else if held == a.max {
        println!("Verdict : au moins {held} canaux en temps réel (limite de l'essai).");
    } else {
        println!("Verdict : {held} canal(aux) en temps réel ; Studio en demande jusqu'à 6, plus la diffusion.");
    }
}
