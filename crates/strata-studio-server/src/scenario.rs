//! The automations, drawn as nodes in the administration: a trigger (the
//! shoot starts, is launched, ends, an alert), conditions, then actions on
//! the devices — most of them through Bitfocus Buttons (the lights in DMX,
//! the ON AIR light), some straight to the devices (nodal, cameras,
//! console).
//!
//! [`plan`] walks the graph from the triggers of an event and gives the
//! actions in order; the server carries them out.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    /// « Commencer un tournage »: the studio wakes up.
    Start,
    /// The recording is launched.
    Launch,
    /// « Terminer ».
    End,
    /// Something went wrong (a live stream dropped, a guest lost…).
    Alert,
    /// A person took the hand at the panel.
    Manual,
}

/// What a node is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum What {
    Trigger {
        on: Event,
    },
    /// A GO, YouTube or TikTok was chosen.
    IfLive,
    /// A GO was chosen.
    IfGo,
    /// The 9:16 was chosen.
    IfVertical,
    /// Someone sits at this seat.
    IfSeat {
        seat: u8,
    },
    /// Press a Buttons button.
    Press {
        page: u16,
        row: u8,
        column: u8,
        #[serde(default)]
        label: String,
    },
    /// Set a Buttons variable.
    Variable {
        name: String,
        value: String,
    },
    /// Route on the nodal; `{GO}` is the chosen GO's destination.
    Route {
        destination: String,
        source: String,
    },
    CamerasPower {
        on: bool,
    },
    CameraPreset {
        camera: u8,
        preset: u8,
    },
    /// Open the occupied seats' microphones (`on`), or mute them all.
    Mics {
        on: bool,
    },
    Wait {
        ms: u64,
    },
}

impl What {
    pub fn is_condition(&self) -> bool {
        matches!(
            self,
            What::IfLive | What::IfGo | What::IfVertical | What::IfSeat { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(flatten)]
    pub what: What,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wire {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub nodes: Vec<Node>,
    pub wires: Vec<Wire>,
}

fn yes() -> bool {
    true
}

/// What the conditions are asked against.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub go: Option<String>,
    pub live: bool,
    pub vertical: bool,
    pub seats: Vec<u8>,
}

/// The actions an event sets off, in order (depth first, in the order the
/// wires were drawn). A condition that does not hold stops its branch.
pub fn plan(scenarios: &[Scenario], event: Event, ctx: &Context) -> Vec<What> {
    let mut out = Vec::new();
    for sc in scenarios.iter().filter(|s| s.enabled) {
        let mut seen = std::collections::BTreeSet::new();
        for n in &sc.nodes {
            if n.what == (What::Trigger { on: event }) {
                walk(sc, &n.id, ctx, &mut out, &mut seen);
            }
        }
    }
    out
}

fn walk(
    sc: &Scenario,
    id: &str,
    ctx: &Context,
    out: &mut Vec<What>,
    seen: &mut std::collections::BTreeSet<String>,
) {
    for w in sc.wires.iter().filter(|w| w.from == id) {
        if !seen.insert(w.to.clone()) {
            continue;
        }
        let Some(n) = sc.nodes.iter().find(|n| n.id == w.to) else {
            continue;
        };
        let pass = match &n.what {
            What::IfLive => ctx.live,
            What::IfGo => ctx.go.is_some(),
            What::IfVertical => ctx.vertical,
            What::IfSeat { seat } => ctx.seats.contains(seat),
            What::Trigger { .. } => false,
            action => {
                let mut a = action.clone();
                if let (What::Route { destination, .. }, Some(go)) = (&mut a, &ctx.go) {
                    *destination = destination.replace("{GO}", go);
                }
                if !matches!(&a, What::Route { destination, .. } if destination.contains("{GO}")) {
                    out.push(a);
                }
                true
            }
        };
        if pass {
            walk(sc, &n.id, ctx, out, seen);
        }
    }
}

/// The studio's first automations (to adapt in the administration).
pub fn defaults() -> Vec<Scenario> {
    let node = |id: &str, x: f32, y: f32, what: What| Node {
        id: id.into(),
        x,
        y,
        what,
    };
    let wire = |a: &str, b: &str| Wire {
        from: a.into(),
        to: b.into(),
    };
    let press = |page, row, column, label: &str| What::Press {
        page,
        row,
        column,
        label: label.into(),
    };
    vec![Scenario {
        id: "standard".into(),
        name: "Tournage standard".into(),
        enabled: true,
        nodes: vec![
            node("t1", 20.0, 20.0, What::Trigger { on: Event::Start }),
            node("a1", 260.0, 0.0, press(2, 0, 1, "Lumière par défaut")),
            node("a2", 260.0, 70.0, What::CamerasPower { on: true }),
            node("t2", 20.0, 200.0, What::Trigger { on: Event::Launch }),
            node("a3", 260.0, 150.0, press(2, 1, 1, "Lumière antenne")),
            node("a4", 260.0, 220.0, What::Mics { on: true }),
            node("c1", 260.0, 290.0, What::IfGo),
            node(
                "a5",
                500.0,
                290.0,
                What::Route {
                    destination: "{GO}".into(),
                    source: "PGM".into(),
                },
            ),
            node(
                "a6",
                260.0,
                360.0,
                What::Variable {
                    name: "studio_onair".into(),
                    value: "ON AIR".into(),
                },
            ),
            node("t3", 20.0, 470.0, What::Trigger { on: Event::End }),
            node("a7", 260.0, 450.0, What::Mics { on: false }),
            node("a8", 500.0, 450.0, press(2, 0, 2, "Lumière éteinte")),
            node("a9", 500.0, 520.0, What::CamerasPower { on: false }),
            node(
                "a10",
                260.0,
                520.0,
                What::Variable {
                    name: "studio_onair".into(),
                    value: "".into(),
                },
            ),
            node("t4", 20.0, 620.0, What::Trigger { on: Event::Alert }),
            node("a11", 260.0, 620.0, press(4, 1, 1, "Voyant orange")),
        ],
        wires: vec![
            wire("t1", "a1"),
            wire("t1", "a2"),
            wire("t2", "a3"),
            wire("t2", "a4"),
            wire("t2", "c1"),
            wire("c1", "a5"),
            wire("t2", "a6"),
            wire("t3", "a7"),
            wire("a7", "a8"),
            wire("a8", "a9"),
            wire("t3", "a10"),
            wire("t4", "a11"),
        ],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_routes_the_chosen_go_only_when_there_is_one() {
        let sc = defaults();
        let with_go = plan(
            &sc,
            Event::Launch,
            &Context {
                go: Some("GO3".into()),
                ..Default::default()
            },
        );
        assert!(with_go.contains(&What::Route {
            destination: "GO3".into(),
            source: "PGM".into()
        }));
        assert_eq!(
            with_go[0],
            What::Press {
                page: 2,
                row: 1,
                column: 1,
                label: "Lumière antenne".into()
            }
        );
        let without = plan(&sc, Event::Launch, &Context::default());
        assert!(!without.iter().any(|w| matches!(w, What::Route { .. })));
        assert_eq!(without.len(), 3);
    }

    #[test]
    fn end_follows_the_chain_in_order() {
        let a = plan(&defaults(), Event::End, &Context::default());
        assert_eq!(a[0], What::Mics { on: false });
        assert!(matches!(a[1], What::Press { column: 2, .. }));
        assert_eq!(a[2], What::CamerasPower { on: false });
    }

    #[test]
    fn a_disabled_scenario_does_nothing_and_json_round_trips() {
        let mut sc = defaults();
        let json = serde_json::to_string(&sc).unwrap();
        assert!(json.contains("\"type\":\"press\""));
        let back: Vec<Scenario> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, sc);
        sc[0].enabled = false;
        assert!(plan(&sc, Event::Start, &Context::default()).is_empty());
    }
}
