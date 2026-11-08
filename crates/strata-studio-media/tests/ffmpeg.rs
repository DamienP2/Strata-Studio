//! A real recording with the machine's ffmpeg (skipped when there is none).

use std::path::Path;
use std::process::Command;
use std::time::Duration;
use strata_studio_media::after::{copy_all, make_vertical};
use strata_studio_media::encoder::{Accel, Codec, Encoder, Resolution};
use strata_studio_media::recorder::{shoot_dir, Channel, ChannelState, Container, Recording};

fn probe(path: &Path, entries: &str) -> String {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            entries,
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn two_channels_recorded_then_cut_to_9x16_and_copied() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("pas de ffmpeg : essai sauté");
        return;
    }
    let root = std::env::temp_dir().join(format!("strata-rec-{}", std::process::id()));
    let dir = shoot_dir(&root, "Le Grand Débrief", "2026-11-08");
    let generated = |hue: &str| Channel {
        name: hue.into(),
        input: ["-re", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=50"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let channels = [generated("PGM"), generated("CAM1")];
    let enc = Encoder::new(Codec::H264, Accel::Cpu, Resolution::Hd);
    let mut rec = Recording::start(
        "ffmpeg",
        &dir,
        "Le Grand Débrief",
        "2026-11-08",
        &channels,
        &enc,
        Container::Mov,
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let status = rec.status();
    assert!(
        status
            .iter()
            .all(|(_, s)| matches!(s, ChannelState::Recording { bytes } if *bytes > 0)),
        "{status:?}"
    );
    let files = rec.stop(Duration::from_secs(10));
    assert!(
        files
            .iter()
            .all(|(_, _, s)| matches!(s, ChannelState::Done { .. })),
        "{files:?}"
    );

    let pgm = &files[0].1;
    assert!(
        pgm.ends_with("Le-Grand-Debrief/2026-11-08/Le-Grand-Debrief_2026-11-08_PGM.mov"),
        "{pgm:?}"
    );
    assert_eq!(probe(pgm, "stream=codec_name"), "h264");
    assert_eq!(probe(pgm, "stream=profile"), "High 4:2:2");
    assert_eq!(probe(pgm, "stream=pix_fmt"), "yuv422p10le");
    assert_eq!(probe(pgm, "stream=width,height"), "1920\n1080");

    let vertical = make_vertical("ffmpeg", pgm, &enc).unwrap();
    assert_eq!(probe(&vertical, "stream=width,height"), "1080\n1920");

    let mut last = (0, 0);
    let copied = copy_all(&[pgm.clone(), vertical], &root.join("partage"), |d, t| {
        last = (d, t)
    })
    .unwrap();
    assert_eq!(copied.len(), 2);
    assert_eq!(last.0, last.1);
    std::fs::remove_dir_all(root).unwrap();
}
