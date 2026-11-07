//! The studio as it is set up: the seats (one person and one microphone
//! each), the cameras and the shots each camera can give.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A seat on the set: a place, a microphone, a person when one sits there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SeatId(pub u8);

/// A camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CamId(pub u8);

/// A shot: a camera and what it frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ShotId(pub u16);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Seat {
    pub id: SeatId,
    /// The place, as the set plan shows it ("Place 1").
    pub label: String,
    /// The person sitting there, given from the tablet for each show.
    #[serde(default)]
    pub guest: Option<String>,
    pub mic: Mic,
}

impl Seat {
    /// The person's name, else the place.
    pub fn display_name(&self) -> &str {
        self.guest.as_deref().unwrap_or(&self.label)
    }
}

/// Where a seat's microphone is found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mic {
    /// Its channel on the audio console (1-based): mute, name, scene.
    pub console_channel: u8,
    /// Its channel in the AES67 copy Studio listens to (0-based).
    pub dante_channel: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CameraModel {
    /// Sony FR7 (VISCA over IP, CGI, OpenTrackIO).
    SonyFr7,
    /// Sony BRC-AM7 (VISCA over IP, CGI).
    SonyBrcAm7,
    /// A camera that does not move.
    Fixed,
}

impl CameraModel {
    pub fn is_ptz(self) -> bool {
        !matches!(self, CameraModel::Fixed)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Camera {
    pub id: CamId,
    pub label: String,
    pub model: CameraModel,
    /// The switcher input it arrives on (ATEM or vMix input number).
    pub switcher_input: u16,
    /// Its control address (VISCA over IP, CGI), when it moves.
    #[serde(default)]
    pub address: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "seats")]
pub enum ShotKind {
    /// The whole set.
    Wide,
    /// One person.
    Single(SeatId),
    /// Several people (a two-shot, a side of the table).
    Group(Vec<SeatId>),
}

impl ShotKind {
    pub fn shows(&self, seat: SeatId) -> bool {
        match self {
            ShotKind::Wide => true,
            ShotKind::Single(s) => *s == seat,
            ShotKind::Group(g) => g.contains(&seat),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    pub id: ShotId,
    pub camera: CamId,
    pub kind: ShotKind,
    /// The PTZ preset giving this shot; none on a camera that does not move.
    #[serde(default)]
    pub preset: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioSetup {
    pub name: String,
    pub seats: Vec<Seat>,
    pub cameras: Vec<Camera>,
    pub shots: Vec<Shot>,
}

impl StudioSetup {
    pub fn seat(&self, id: SeatId) -> Option<&Seat> {
        self.seats.iter().find(|s| s.id == id)
    }

    pub fn camera(&self, id: CamId) -> Option<&Camera> {
        self.cameras.iter().find(|c| c.id == id)
    }

    pub fn shot(&self, id: ShotId) -> Option<&Shot> {
        self.shots.iter().find(|s| s.id == id)
    }

    pub fn camera_on_input(&self, input: u16) -> Option<&Camera> {
        self.cameras.iter().find(|c| c.switcher_input == input)
    }

    pub fn seat_name(&self, id: SeatId) -> String {
        self.seat(id).map_or_else(
            || format!("Place {}", id.0),
            |s| s.display_name().to_string(),
        )
    }

    /// What is wrong with this setup, if anything.
    pub fn validate(&self) -> Result<(), String> {
        let mut seats = BTreeSet::new();
        for s in &self.seats {
            if !seats.insert(s.id) {
                return Err(format!("place {} déclarée deux fois", s.id.0));
            }
        }
        let mut cams = BTreeSet::new();
        let mut inputs = BTreeSet::new();
        for c in &self.cameras {
            if !cams.insert(c.id) {
                return Err(format!("caméra {} déclarée deux fois", c.id.0));
            }
            if !inputs.insert(c.switcher_input) {
                return Err(format!(
                    "entrée {} du mélangeur prise par deux caméras",
                    c.switcher_input
                ));
            }
        }
        let mut shots = BTreeSet::new();
        for s in &self.shots {
            if !shots.insert(s.id) {
                return Err(format!("plan {} déclaré deux fois", s.id.0));
            }
            let cam = self
                .camera(s.camera)
                .ok_or_else(|| format!("plan {} : caméra {} inconnue", s.id.0, s.camera.0))?;
            if cam.model.is_ptz() != s.preset.is_some() {
                return Err(format!(
                    "plan {} : un preset sur une caméra mobile, aucun sur une caméra fixe",
                    s.id.0
                ));
            }
            let people: Vec<SeatId> = match &s.kind {
                ShotKind::Wide => vec![],
                ShotKind::Single(p) => vec![*p],
                ShotKind::Group(g) => g.clone(),
            };
            if let Some(p) = people.iter().find(|p| !seats.contains(p)) {
                return Err(format!("plan {} : place {} inconnue", s.id.0, p.0));
            }
        }
        Ok(())
    }

    /// The studio of the proposal: five seats around a table, four PTZ
    /// cameras (FR7 or BRC-AM7). Camera 1 gives the wide shot and the
    /// sides of the table; cameras 2 to 4 give every person alone.
    pub fn example() -> Self {
        let seats = (1..=5)
            .map(|n| Seat {
                id: SeatId(n),
                label: format!("Place {n}"),
                guest: None,
                mic: Mic {
                    console_channel: n,
                    dante_channel: u16::from(n - 1),
                },
            })
            .collect();
        let cameras = (1..=4)
            .map(|n| Camera {
                id: CamId(n),
                label: format!("Cam {n}"),
                model: if n == 1 {
                    CameraModel::SonyFr7
                } else {
                    CameraModel::SonyBrcAm7
                },
                switcher_input: u16::from(n),
                address: Some(format!("192.168.10.{}", 10 + n)),
            })
            .collect();
        let mut shots = vec![
            Shot {
                id: ShotId(100),
                camera: CamId(1),
                kind: ShotKind::Wide,
                preset: Some(1),
            },
            Shot {
                id: ShotId(101),
                camera: CamId(1),
                kind: ShotKind::Group(vec![SeatId(1), SeatId(2), SeatId(3)]),
                preset: Some(2),
            },
            Shot {
                id: ShotId(102),
                camera: CamId(1),
                kind: ShotKind::Group(vec![SeatId(4), SeatId(5)]),
                preset: Some(3),
            },
        ];
        for cam in 2..=4u8 {
            for seat in 1..=5u8 {
                shots.push(Shot {
                    id: ShotId(u16::from(cam) * 100 + u16::from(seat)),
                    camera: CamId(cam),
                    kind: ShotKind::Single(SeatId(seat)),
                    preset: Some(seat),
                });
            }
        }
        StudioSetup {
            name: "Studio exemple".into(),
            seats,
            cameras,
            shots,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_is_valid_and_round_trips() {
        let s = StudioSetup::example();
        s.validate().unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<StudioSetup>(&json).unwrap(), s);
    }

    #[test]
    fn a_shot_on_an_unknown_seat_is_refused() {
        let mut s = StudioSetup::example();
        s.shots[0].kind = ShotKind::Single(SeatId(9));
        assert!(s.validate().is_err());
    }
}
