//! The studio's settings, kept in `reglages.json` in Studio's data folder
//! and edited in the administration (or by the installer): the devices
//! and their addresses, the encoders, the platforms, the storages, the
//! programmes, the directing profiles and the automations.
//!
//! An address left empty means the device is simulated: Studio runs and
//! shows itself without hardware.

use crate::record::RecordSettings;
use crate::scenario::Scenario;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use strata_studio_core::{Profile, StudioSetup};
use strata_studio_devices::nmos::Endpoint;
use strata_studio_media::encoder::Accel;
use strata_studio_media::live::LiveTarget;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// As shown everywhere (« Studio A »).
    pub studio: String,
    /// Seats, cameras and their shots, remote sources.
    pub setup: StudioSetup,
    pub switcher: SwitcherSettings,
    pub cameras: CameraSettings,
    pub console: ConsoleSettings,
    pub buttons: ButtonsSettings,
    pub router: RouterSettings,
    pub audio: AudioSettings,
    pub recording: RecordSettings,
    /// Where the files go after the shoot (mounted shares).
    pub storages: Vec<Storage>,
    /// GO1 to GO4.
    pub gos: Vec<GoSettings>,
    pub live: LiveSettings,
    pub calls: CallSettings,
    /// The other remote sources: their switcher inputs.
    pub agency_input: u16,
    pub duplex_input: u16,
    /// The programmes (the shows and their answers, kept for next time).
    pub programmes: Vec<Programme>,
    /// The directing profiles (rhythm, breathing, listening shots…).
    pub profiles: Vec<Profile>,
    /// The automations, as node graphs.
    pub scenarios: Vec<Scenario>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SwitcherSettings {
    /// `sim`, `atem` or `vmix`.
    pub kind: String,
    pub address: String,
    /// vMix: the layout inputs (two layers) for the 50/50 and the inset.
    pub vmix_split: u16,
    pub vmix_pip: u16,
}

impl Default for SwitcherSettings {
    fn default() -> Self {
        SwitcherSettings {
            kind: "sim".into(),
            address: String::new(),
            vmix_split: 20,
            vmix_pip: 21,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraSettings {
    /// `sim`, or `visca` (each camera's address is in the setup).
    pub control: String,
    /// The time a preset takes, in ms.
    pub travel_ms: u64,
}

impl Default for CameraSettings {
    fn default() -> Self {
        CameraSettings {
            control: "sim".into(),
            travel_ms: 1200,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConsoleSettings {
    /// `sim`, `m32` or `dm5`.
    pub kind: String,
    pub address: String,
}

impl Default for ConsoleSettings {
    fn default() -> Self {
        ConsoleSettings {
            kind: "sim".into(),
            address: String::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ButtonsSettings {
    /// `address:port` of Bitfocus Buttons' HTTP API (empty: simulated).
    pub address: String,
    /// Studio writes the Stream Deck's keys into Buttons' variables.
    pub deck: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RouterSettings {
    /// `sim`, `videohub`, `nmos` or `ndi`.
    pub kind: String,
    /// Videohub's address.
    pub address: String,
    /// NMOS: the senders and receivers, by name, with their node.
    pub senders: Vec<Endpoint>,
    pub receivers: Vec<Endpoint>,
    /// NDI: the name Studio's routing sources start with.
    pub ndi_prefix: String,
    /// The source carrying the switcher's programme (Videohub input name,
    /// NMOS sender, NDI source).
    pub pgm_source: String,
}

impl Default for RouterSettings {
    fn default() -> Self {
        RouterSettings {
            kind: "sim".into(),
            address: String::new(),
            senders: vec![],
            receivers: vec![],
            ndi_prefix: "STUDIO".into(),
            pgm_source: "PGM".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioSettings {
    /// The SDP file of the AES67 stream carrying the microphones (empty:
    /// simulated conversation).
    pub sdp: String,
    /// The interface to receive it on.
    pub interface: String,
    /// The whisper.cpp server (`127.0.0.1:8178`), empty: no listening.
    pub whisper: String,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            sdp: String::new(),
            interface: "0.0.0.0".into(),
            whisper: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    pub label: String,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GoSettings {
    pub label: String,
    /// The Streaming Encoder's address (empty: simulated).
    pub address: String,
    /// The nodal's destination feeding it.
    pub destination: String,
    /// Simulation only: taken by another studio.
    pub sim_busy: bool,
}

impl Default for GoSettings {
    fn default() -> Self {
        GoSettings {
            label: "GO".into(),
            address: String::new(),
            destination: String::new(),
            sim_busy: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiveSettings {
    /// The programme as an ffmpeg input (empty: the recording's PGM input).
    pub input: Vec<String>,
    pub accel: Accel,
    pub youtube: LiveTarget,
    pub tiktok: LiveTarget,
}

impl Default for LiveSettings {
    fn default() -> Self {
        LiveSettings {
            input: vec![],
            accel: Accel::Cpu,
            youtube: LiveTarget {
                url: "rtmp://a.rtmp.youtube.com/live2".into(),
                ..Default::default()
            },
            tiktok: LiveTarget {
                vertical: true,
                kbps: 4000,
                ..Default::default()
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CallSettings {
    /// The Quicklink API's address (empty: simulated).
    pub api: String,
    pub username: String,
    pub password: String,
    /// The StudioEdge distributor room whose positions are the outputs.
    pub room: String,
    /// For call 1, 2…: the room's position, and the switcher input that
    /// StudioEdge output arrives on.
    pub positions: Vec<u16>,
    pub inputs: Vec<u16>,
}

impl Default for CallSettings {
    fn default() -> Self {
        CallSettings {
            api: String::new(),
            username: String::new(),
            password: String::new(),
            room: String::new(),
            positions: vec![1, 2, 3, 4],
            inputs: vec![5, 7, 8, 9],
        }
    }
}

/// A programme: a show's answers, taken back the next time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Programme {
    pub name: String,
    /// The directing profile's index.
    pub profile: usize,
    pub pgm: bool,
    pub clean: bool,
    pub iso: bool,
    pub mics: bool,
    /// The storage's index.
    pub storage: usize,
    /// `1080p50` or `2160p50`.
    pub resolution: String,
    pub vertical: bool,
    /// GO1 to GO4 (1 to 4), 0: none.
    pub go: u8,
    pub youtube: bool,
    pub tiktok: bool,
    pub people: u8,
    pub names: Vec<String>,
    pub calls: u8,
    pub agency: bool,
    pub duplex: bool,
    /// For each seat (1 to 5), the person's index in `names`, if any.
    pub seats: Vec<Option<u8>>,
}

impl Default for Programme {
    fn default() -> Self {
        Programme {
            name: String::new(),
            profile: 0,
            pgm: true,
            clean: false,
            iso: true,
            mics: true,
            storage: 0,
            resolution: "1080p50".into(),
            vertical: false,
            go: 0,
            youtube: false,
            tiktok: false,
            people: 3,
            names: vec![String::new(); 5],
            calls: 0,
            agency: false,
            duplex: false,
            seats: vec![],
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let p = |name: &str,
                 people: u8,
                 go: u8,
                 yt: bool,
                 tt: bool,
                 vertical: bool,
                 res: &str,
                 calls: u8,
                 profile: usize| Programme {
            name: name.into(),
            profile,
            clean: true,
            resolution: res.into(),
            vertical,
            go,
            youtube: yt,
            tiktok: tt,
            people,
            calls,
            ..Programme::default()
        };
        Settings {
            studio: "Studio A".into(),
            setup: StudioSetup::example(),
            switcher: SwitcherSettings::default(),
            cameras: CameraSettings::default(),
            console: ConsoleSettings::default(),
            buttons: ButtonsSettings::default(),
            router: RouterSettings::default(),
            audio: AudioSettings::default(),
            recording: RecordSettings::demo(),
            storages: vec![
                Storage {
                    label: "Rédaction vidéo".into(),
                    path: "partages/redaction".into(),
                },
                Storage {
                    label: "Archives des émissions".into(),
                    path: "partages/archives".into(),
                },
            ],
            gos: (1..=4)
                .map(|n| GoSettings {
                    label: format!("GO{n}"),
                    address: String::new(),
                    destination: format!("GO{n}"),
                    sim_busy: n == 2,
                })
                .collect(),
            live: LiveSettings::default(),
            calls: CallSettings::default(),
            agency_input: 10,
            duplex_input: 6,
            programmes: vec![
                p("Le Grand Débrief", 4, 3, true, true, true, "2160p50", 0, 1),
                p("La Matinale", 3, 1, false, false, false, "1080p50", 1, 3),
                p("L'Invité de 13h", 2, 4, true, false, true, "1080p50", 0, 2),
            ],
            profiles: Profile::all(),
            scenarios: crate::scenario::defaults(),
        }
    }
}

impl Settings {
    /// Reads the settings, or the default ones (written there) when the
    /// file does not exist yet.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            let s = Settings::default();
            s.save(path)?;
            return Ok(s);
        }
        let text = std::fs::read_to_string(path)?;
        let s: Settings =
            serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{} : {e}", path.display()))?;
        s.setup
            .validate()
            .map_err(|e| anyhow::anyhow!("{} : {e}", path.display()))?;
        Ok(s)
    }

    /// Writes atomically (a temporary file, then renamed).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Whether every device is simulated (a studio to try Studio).
    pub fn simulated(&self) -> bool {
        self.switcher.kind == "sim" && self.audio.sdp.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_and_fill_missing_fields() {
        let s = Settings::default();
        let text = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(s, back);
        let partial: Settings = serde_json::from_str(
            r#"{"studio":"Studio B","switcher":{"kind":"atem","address":"10.0.0.2"}}"#,
        )
        .unwrap();
        assert_eq!(partial.studio, "Studio B");
        assert_eq!(partial.switcher.vmix_split, 20);
        assert_eq!(partial.gos.len(), 4);
    }

    #[test]
    fn load_writes_the_defaults_once() {
        let dir = std::env::temp_dir().join(format!("strata-config-{}", std::process::id()));
        let path = dir.join("reglages.json");
        let _ = std::fs::remove_dir_all(&dir);
        let s = Settings::load(&path).unwrap();
        assert!(path.exists());
        let mut changed = s.clone();
        changed.studio = "Studio C".into();
        changed.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap().studio, "Studio C");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
