//! The edit of a shoot for the editing software: what the AI (or the
//! person at the panel) cut, as a cut list over the camera files (ISO).
//! Every file starts at the same moment, so a source time is the same as
//! the programme time.
//!
//! - **EDL** CMX 3600: Avid, Premiere, DaVinci Resolve;
//! - **FCPXML** 1.9: Final Cut Pro, DaVinci Resolve;
//! - **XML** (Final Cut Pro 7, `xmeml`): Premiere Pro;
//! - **OTIO** (OpenTimelineIO JSON): the others.
//!
//! A moment shown by a layout (50/50, inset) or a remote source is taken
//! from the PGM file.

use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// One shot of the programme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    /// When it starts, from the start of the recording.
    pub start_ms: u64,
    /// The channel it comes from: « CAM1 »… or « PGM ».
    pub source: String,
    /// What it shows (« Cam 2 · Alice »).
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub title: String,
    pub fps: u32,
    pub duration_ms: u64,
    /// In order of time.
    pub segments: Vec<Segment>,
    /// Each source's file (« CAM2 » → `/…/Le_Grand_Debrief_CAM2.mov`).
    pub files: Vec<(String, String)>,
    pub width: u32,
    pub height: u32,
}

/// A segment with its end, in frames.
struct Span<'a> {
    seg: &'a Segment,
    from: u64,
    to: u64,
}

impl Edit {
    fn frames(&self, ms: u64) -> u64 {
        (ms * self.fps as u64 + 500) / 1000
    }

    fn spans(&self) -> Vec<Span<'_>> {
        let end = self.frames(self.duration_ms);
        let mut v: Vec<Span> = Vec::new();
        for (i, seg) in self.segments.iter().enumerate() {
            let from = self.frames(seg.start_ms).min(end);
            let to = self
                .segments
                .get(i + 1)
                .map_or(end, |n| self.frames(n.start_ms).min(end));
            if to > from {
                v.push(Span { seg, from, to });
            }
        }
        v
    }

    fn file(&self, source: &str) -> String {
        self.files
            .iter()
            .find(|(s, _)| s == source)
            .map(|(_, f)| f.clone())
            .unwrap_or_else(|| format!("{source}.mov"))
    }

    fn file_name(&self, source: &str) -> String {
        let f = self.file(source);
        f.rsplit(['/', '\\']).next().unwrap_or(&f).to_string()
    }

    fn tc(&self, frames: u64) -> String {
        let fps = self.fps as u64;
        let (h, m, s, f) = (
            frames / (3600 * fps),
            frames / (60 * fps) % 60,
            frames / fps % 60,
            frames % fps,
        );
        format!("{h:02}:{m:02}:{s:02}:{f:02}")
    }

    /// CMX 3600.
    pub fn edl(&self) -> String {
        let mut out = format!("TITLE: {}\nFCM: NON-DROP FRAME\n\n", self.title);
        for (i, sp) in self.spans().iter().enumerate() {
            let reel: String = sp
                .seg
                .source
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(8)
                .collect();
            let _ = writeln!(
                out,
                "{:03}  {:<8} V     C        {} {} {} {}",
                i + 1,
                reel,
                self.tc(sp.from),
                self.tc(sp.to),
                self.tc(sp.from),
                self.tc(sp.to)
            );
            let _ = writeln!(out, "* FROM CLIP NAME: {}", self.file_name(&sp.seg.source));
            if !sp.seg.label.is_empty() {
                let _ = writeln!(out, "* COMMENT: {}", sp.seg.label);
            }
            out.push('\n');
        }
        out
    }

    fn sources(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        for s in &self.segments {
            if !v.contains(&s.source) {
                v.push(s.source.clone());
            }
        }
        v
    }

    fn url(&self, source: &str) -> String {
        let f = self.file(source).replace('\\', "/");
        let path: String = f
            .split('/')
            .map(|p| {
                p.bytes()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                            (b as char).to_string()
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("/");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    }

    /// FCPXML 1.9 (Final Cut Pro, DaVinci Resolve).
    pub fn fcpxml(&self) -> String {
        let fps = self.fps;
        let r = |frames: u64| format!("{frames}/{fps}s");
        let total = self.frames(self.duration_ms);
        let mut out = String::new();
        let _ = write!(
            out,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n<fcpxml version=\"1.9\">\n  <resources>\n    <format id=\"r0\" frameDuration=\"1/{fps}s\" width=\"{}\" height=\"{}\"/>\n",
            self.width, self.height
        );
        let sources = self.sources();
        for (i, s) in sources.iter().enumerate() {
            let _ = writeln!(
                out,
                "    <asset id=\"a{}\" name=\"{}\" start=\"0s\" duration=\"{}\" hasVideo=\"1\" hasAudio=\"1\" format=\"r0\">\n      <media-rep kind=\"original-media\" src=\"{}\"/>\n    </asset>",
                i + 1,
                xml(&self.file_name(s)),
                r(total),
                xml(&self.url(s))
            );
        }
        let _ = write!(
            out,
            "  </resources>\n  <library>\n    <event name=\"{t}\">\n      <project name=\"{t}\">\n        <sequence format=\"r0\" duration=\"{}\" tcStart=\"0s\" tcFormat=\"NDF\">\n          <spine>\n",
            r(total),
            t = xml(&self.title)
        );
        for sp in self.spans() {
            let a = sources
                .iter()
                .position(|s| *s == sp.seg.source)
                .unwrap_or(0)
                + 1;
            let _ = writeln!(
                out,
                "            <asset-clip ref=\"a{a}\" name=\"{}\" offset=\"{}\" start=\"{}\" duration=\"{}\" format=\"r0\"/>",
                xml(if sp.seg.label.is_empty() { &sp.seg.source } else { &sp.seg.label }),
                r(sp.from),
                r(sp.from),
                r(sp.to - sp.from)
            );
        }
        out.push_str("          </spine>\n        </sequence>\n      </project>\n    </event>\n  </library>\n</fcpxml>\n");
        out
    }

    /// Final Cut Pro 7 XML (`xmeml` 5), which Premiere Pro imports.
    pub fn premiere_xml(&self) -> String {
        let fps = self.fps;
        let total = self.frames(self.duration_ms);
        let rate = format!("<rate><timebase>{fps}</timebase><ntsc>FALSE</ntsc></rate>");
        let mut out = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n<xmeml version=\"5\">\n<sequence id=\"sequence-1\">\n<name>{}</name>\n<duration>{total}</duration>\n{rate}\n<media>\n<video>\n<format><samplecharacteristics><width>{}</width><height>{}</height>{rate}</samplecharacteristics></format>\n<track>\n",
            xml(&self.title),
            self.width,
            self.height
        );
        let mut declared: Vec<String> = Vec::new();
        for (i, sp) in self.spans().iter().enumerate() {
            let src = &sp.seg.source;
            let file = if declared.contains(src) {
                format!("<file id=\"file-{src}\"/>")
            } else {
                declared.push(src.clone());
                format!(
                    "<file id=\"file-{src}\"><name>{}</name><pathurl>{}</pathurl>{rate}<duration>{total}</duration><media><video/><audio/></media></file>",
                    xml(&self.file_name(src)),
                    xml(&self.url(src))
                )
            };
            let _ = writeln!(
                out,
                "<clipitem id=\"clipitem-{}\"><name>{}</name><duration>{total}</duration>{rate}<start>{}</start><end>{}</end><in>{}</in><out>{}</out>{file}</clipitem>",
                i + 1,
                xml(if sp.seg.label.is_empty() { src } else { &sp.seg.label }),
                sp.from,
                sp.to,
                sp.from,
                sp.to
            );
        }
        out.push_str("</track>\n</video>\n</media>\n</sequence>\n</xmeml>\n");
        out
    }

    /// OpenTimelineIO (JSON).
    pub fn otio(&self) -> String {
        let fps = self.fps as f64;
        let rt = |v: u64| serde_json::json!({ "OTIO_SCHEMA": "RationalTime.1", "value": v as f64, "rate": fps });
        let clips: Vec<serde_json::Value> = self
            .spans()
            .iter()
            .map(|sp| {
                serde_json::json!({
                    "OTIO_SCHEMA": "Clip.2",
                    "name": if sp.seg.label.is_empty() { sp.seg.source.clone() } else { sp.seg.label.clone() },
                    "source_range": { "OTIO_SCHEMA": "TimeRange.1", "start_time": rt(sp.from), "duration": rt(sp.to - sp.from) },
                    "media_references": { "DEFAULT_MEDIA": {
                        "OTIO_SCHEMA": "ExternalReference.1",
                        "target_url": self.url(&sp.seg.source),
                        "available_range": null,
                        "metadata": {}
                    }},
                    "active_media_reference_key": "DEFAULT_MEDIA",
                    "metadata": { "strata": { "source": sp.seg.source } },
                    "effects": [], "markers": [], "enabled": true
                })
            })
            .collect();
        let tl = serde_json::json!({
            "OTIO_SCHEMA": "Timeline.1",
            "name": self.title,
            "global_start_time": null,
            "metadata": {},
            "tracks": {
                "OTIO_SCHEMA": "Stack.1", "name": "tracks", "metadata": {}, "effects": [], "markers": [], "enabled": true, "source_range": null,
                "children": [{
                    "OTIO_SCHEMA": "Track.1", "name": "V1", "kind": "Video", "metadata": {}, "effects": [], "markers": [], "enabled": true, "source_range": null,
                    "children": clips
                }]
            }
        });
        serde_json::to_string_pretty(&tl).unwrap_or_default()
    }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit() -> Edit {
        let seg = |ms, s: &str, l: &str| Segment {
            start_ms: ms,
            source: s.into(),
            label: l.into(),
        };
        Edit {
            title: "Le Grand Débrief".into(),
            fps: 50,
            duration_ms: 12_000,
            segments: vec![
                seg(0, "CAM1", "Cam 1 · large"),
                seg(4_000, "CAM2", "Cam 2 · Paul"),
                seg(9_500, "PGM", "Paul | Julie (50/50)"),
            ],
            files: vec![
                (
                    "CAM1".into(),
                    "/mnt/redaction/Le Grand Débrief/CAM1.mov".into(),
                ),
                (
                    "CAM2".into(),
                    "/mnt/redaction/Le Grand Débrief/CAM2.mov".into(),
                ),
            ],
            width: 1920,
            height: 1080,
        }
    }

    #[test]
    fn edl_cuts_on_frames() {
        let e = edit().edl();
        assert!(e.starts_with("TITLE: Le Grand Débrief\nFCM: NON-DROP FRAME"));
        assert!(e.contains(
            "001  CAM1     V     C        00:00:00:00 00:00:04:00 00:00:00:00 00:00:04:00"
        ));
        assert!(e.contains("002  CAM2     V     C        00:00:04:00 00:00:09:25"));
        assert!(e.contains("003  PGM      V     C        00:00:09:25 00:00:12:00"));
        assert!(e.contains("* FROM CLIP NAME: CAM2.mov"));
    }

    #[test]
    fn fcpxml_and_premiere_reference_the_files() {
        let f = edit().fcpxml();
        assert!(f.contains("<fcpxml version=\"1.9\">"));
        assert!(f.contains("src=\"file:///mnt/redaction/Le%20Grand%20D%C3%A9brief/CAM1.mov\""));
        assert!(f.contains("offset=\"200/50s\" start=\"200/50s\" duration=\"275/50s\""));
        let p = edit().premiere_xml();
        assert!(p.contains("<xmeml version=\"5\">"));
        assert_eq!(p.matches("<pathurl>").count(), 3);
        assert!(p.contains("<start>200</start><end>475</end>"));
    }

    #[test]
    fn otio_is_json_with_three_clips() {
        let v: serde_json::Value = serde_json::from_str(&edit().otio()).unwrap();
        let clips = &v["tracks"]["children"][0]["children"];
        assert_eq!(clips.as_array().unwrap().len(), 3);
        assert_eq!(clips[1]["source_range"]["duration"]["value"], 275.0);
    }
}
