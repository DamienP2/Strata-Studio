//! The studio's settings, kept in `reglages.json` in Studio's data folder
//! and edited in the administration (or by the installer): the devices
//! and their addresses, the encoders, the platforms, the storages, the
//! programmes, the directing profiles and the automations.
//!
//! An address left empty means the studio has no such device yet: Studio
//! says it is « non réglé », and does not pretend.

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
            kind: String::new(),
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
            control: String::new(),
            travel_ms: 1200,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConsoleSettings {
    /// `sim`, `m32` or `dm5`.
    pub kind: String,
    pub address: String,
}


#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ButtonsSettings {
    /// `address:port` of Bitfocus Buttons' HTTP API (empty: not set).
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
            kind: "none".into(),
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
    /// The Streaming Encoder's address (empty: not set).
    pub address: String,
    /// The nodal's destination feeding it.
    pub destination: String,
}

impl Default for GoSettings {
    fn default() -> Self {
        GoSettings {
            label: "GO".into(),
            address: String::new(),
            destination: String::new(),
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
    /// The Quicklink API's address (empty: not set).
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
    /// A real studio, nothing set yet: every device is « non réglé » until
    /// the administration (or the installer) gives its address.
    fn default() -> Self {
        Settings {
            studio: "Studio".into(),
            setup: Self::blank_setup(),
            switcher: SwitcherSettings::default(),
            cameras: CameraSettings::default(),
            console: ConsoleSettings::default(),
            buttons: ButtonsSettings::default(),
            router: RouterSettings::default(),
            audio: AudioSettings::default(),
            recording: RecordSettings::unset(),
            storages: vec![],
            gos: vec![],
            live: LiveSettings::default(),
            calls: CallSettings::default(),
            agency_input: 10,
            duplex_input: 6,
            programmes: vec![],
            profiles: Profile::all(),
            scenarios: crate::scenario::defaults(),
        }
    }
}

impl Settings {
    /// Five seats, four cameras and their shots, with no address: the
    /// studio's shape, to fill in.
    pub fn blank_setup() -> StudioSetup {
        let mut s = StudioSetup::example();
        s.name = "Studio".into();
        s.remotes.clear();
        for c in &mut s.cameras {
            c.address = None;
        }
        s
    }
}

/// What a check reports for a device not set.
pub const UNSET: &str = "non réglé";

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
        let mut s: Settings =
            serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{} : {e}", path.display()))?;
        // Only a file written by Studio 0.2 carries its simulation's marks.
        let from_0_2 = text.contains("\"light\"") || text.contains("\"simBusy\"");
        if from_0_2 && s.forget_simulation() {
            s.save(path)?;
        }
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

    /// Studio 0.2 wrote a simulated studio into new settings: simulated
    /// devices, test pictures, example programmes and GO, shares in the
    /// working folder. They are taken out; true if anything was.
    pub fn forget_simulation(&mut self) -> bool {
        let before = serde_json::to_string(self).unwrap_or_default();
        let never_set = self.switcher.kind == "sim";
        // The example's camera addresses, never set by anyone.
        if matches!(self.cameras.control.as_str(), "" | "sim") {
            for c in &mut self.setup.cameras {
                if c.address
                    .as_deref()
                    .is_some_and(|a| a.starts_with("192.168.10.1") && a.len() == 13)
                {
                    c.address = None;
                }
            }
        }
        for k in [
            &mut self.switcher.kind,
            &mut self.console.kind,
            &mut self.cameras.control,
        ] {
            if k == "sim" {
                k.clear();
            }
        }
        if self.router.kind == "sim" {
            self.router.kind = "none".into();
        }
        for c in &mut self.recording.channels {
            if c.input.iter().any(|x| x.contains("lavfi")) {
                c.input.clear();
            }
        }
        self.storages.retain(|s| !s.path.starts_with("partages"));
        self.gos.retain(|g| !g.address.trim().is_empty());
        if never_set {
            let examples = ["Le Grand Débrief", "La Matinale", "L'Invité de 13h"];
            self.programmes
                .retain(|p| !examples.contains(&p.name.as_str()));
            if self.studio == "Studio A" {
                self.studio = "Studio".into();
            }
        }
        serde_json::to_string(self).unwrap_or_default() != before
    }

    /// The devices not set yet (by the label of their check). The nodal is
    /// optional (« none »).
    pub fn unset(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if !matches!(self.switcher.kind.as_str(), "atem" | "vmix")
            || self.switcher.address.trim().is_empty()
        {
            v.push("Mélangeur");
        }
        if self.cameras.control != "visca"
            || self
                .setup
                .cameras
                .iter()
                .any(|c| c.address.as_deref().unwrap_or("").trim().is_empty())
        {
            v.push("Caméras");
        }
        if !matches!(self.console.kind.as_str(), "m32" | "dm5")
            || self.console.address.trim().is_empty()
        {
            v.push("Console");
        }
        if self.audio.sdp.trim().is_empty() {
            v.push("Micros");
        }
        if self.buttons.address.trim().is_empty() {
            v.push("Lumière");
        }
        if matches!(self.router.kind.as_str(), "sim" | "") {
            v.push("Nodal");
        }
        if self.recording.channels.iter().any(|c| c.input.is_empty()) {
            v.push("Images");
        }
        if self.storages.is_empty() {
            v.push("Stockage");
        }
        v
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
        assert!(partial.gos.is_empty() && partial.programmes.is_empty());
        assert!(!partial.unset().contains(&"Mélangeur"));
        assert!(partial.unset().contains(&"Caméras"));
        assert_eq!(Settings::default().unset().len(), 7);
    }

    #[test]
    fn the_simulated_studio_of_0_2_is_forgotten() {
        let old = r#"{"studio":"Studio A","switcher":{"kind":"sim","address":""},"router":{"kind":"sim"},
          "recording":{"codec":"h264","accel":"cpu","container":"mov","light":true,"channels":[{"name":"PGM","input":["-re","-f","lavfi","-i","testsrc2"]}]},
          "storages":[{"label":"Rédaction vidéo","path":"partages/redaction"},{"label":"NAS","path":"/mnt/nas"}],
          "gos":[{"label":"GO1","address":"","destination":"GO1","simBusy":false},{"label":"GO2","address":"10.0.0.2","destination":"GO2","simBusy":true}],
          "programmes":[{"name":"La Matinale"},{"name":"Mon émission"}]}"#;
        let mut s: Settings = serde_json::from_str(old).unwrap();
        assert!(s.forget_simulation());
        assert_eq!(s.switcher.kind, "");
        assert_eq!(s.router.kind, "none");
        assert!(s.recording.channels[0].input.is_empty());
        assert_eq!(s.storages.len(), 1);
        assert_eq!(s.gos.len(), 1);
        assert_eq!(s.programmes.len(), 1);
        assert_eq!(s.studio, "Studio");
        assert!(!s.forget_simulation());
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
