//! The Stream Deck + (8 keys, 4 dials) along the whole journey, through
//! Bitfocus Buttons:
//!
//! - Studio writes what each key and dial shows into Buttons' custom
//!   variables: `studio_deck_title`, `studio_deck_hint`, `studio_k1` …
//!   `studio_k8` (the label), `studio_k1_on` … (`1` chosen, `0`, `x`
//!   unavailable), `studio_d1` … `studio_d4`;
//! - each key in Buttons shows `$(custom:studio_k1)` and sends
//!   `POST http://<studio>:8095/api/deck/key/1`; each dial sends
//!   `POST …/api/deck/dial/1/1` (right) or `…/dial/1/-1` (left).
//!
//! Keys 7 and 8 are always « Retour » and « Valider ». Launching and
//! ending are pressed twice (a Stream Deck has no long press through
//! Buttons): the first press arms, the second within 3 s acts.

use crate::config::Settings;
use crate::session::{DraftPatch, Session};
use serde::Serialize;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub label: String,
    pub sub: String,
    /// `1` chosen, `0` not chosen, `x` unavailable, empty: an empty key.
    pub on: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dial {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckView {
    pub title: String,
    pub hint: String,
    pub keys: Vec<Key>,
    pub dials: Vec<Dial>,
}

/// What a key or a dial asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Patch(DraftPatch),
    Programme(usize),
    Start,
    Next,
    Prev,
    TestOk,
    TestRetry,
    Launch,
    End,
    Restart,
    Nothing,
}

fn key(label: &str, sub: &str, on: Option<bool>) -> Key {
    Key {
        label: label.into(),
        sub: sub.into(),
        on: match on {
            Some(true) => "1".into(),
            Some(false) => "0".into(),
            None => String::new(),
        },
    }
}

fn unavailable(label: &str, sub: &str) -> Key {
    Key {
        label: label.into(),
        sub: sub.into(),
        on: "x".into(),
    }
}

/// The 8 keys of a step: up to six choices, then Retour and Valider.
fn with_nav(mut choices: Vec<Key>, s: &Session) -> Vec<Key> {
    choices.resize(6, Key::default());
    choices.push(if s.step > 2 {
        key("Retour", "", None)
    } else {
        Key::default()
    });
    choices.push(key(
        "Valider",
        if s.blocked().is_some() {
            "à compléter"
        } else {
            ""
        },
        None,
    ));
    choices
}

pub fn view(s: &Session, settings: &Settings, gos_free: &[bool], armed: bool) -> DeckView {
    let d = &s.draft;
    let res4k = d.resolution == "2160p50";
    let dials = vec![
        Dial {
            label: "Personnes".into(),
            value: d.people.to_string(),
        },
        Dial {
            label: "Calls".into(),
            value: d.calls.to_string(),
        },
        Dial {
            label: "Qualité".into(),
            value: if res4k { "4K".into() } else { "Full HD".into() },
        },
        Dial {
            label: "Format".into(),
            value: if d.vertical {
                "+9:16".into()
            } else {
                "16:9".into()
            },
        },
    ];
    let (title, hint, keys) = match s.step {
        1 => (
            "Studio prêt".to_string(),
            "Appuyez pour allumer le studio.".to_string(),
            vec![key("Commencer", "un tournage", None)],
        ),
        2 => (
            "Vérification".into(),
            if s.blocked().is_some() {
                "Studio vérifie les appareils…".into()
            } else {
                "Tout répond.".into()
            },
            with_nav(vec![], s),
        ),
        3 => (
            "Programme".into(),
            "Choisissez le programme.".into(),
            with_nav(
                settings
                    .programmes
                    .iter()
                    .take(6)
                    .map(|p| key(&p.name, "", Some(d.name == p.name)))
                    .collect(),
                s,
            ),
        ),
        4 => (
            "Enregistrement".into(),
            "Qu'enregistre-t-on ?".into(),
            with_nav(
                vec![
                    key("PGM", "programme", Some(d.pgm)),
                    key("CLEAN", "sans habillage", Some(d.clean)),
                    key("4 caméras", "ISO", Some(d.iso)),
                    key("Micros", "un WAV chacun", Some(d.mics)),
                    key("Tout", "", None),
                ],
                s,
            ),
        ),
        5 => (
            "Stockage".into(),
            "Où ranger les fichiers ?".into(),
            with_nav(
                settings
                    .storages
                    .iter()
                    .take(6)
                    .enumerate()
                    .map(|(i, st)| key(&st.label, "", Some(d.storage == i)))
                    .collect(),
                s,
            ),
        ),
        6 => (
            "Résolution".into(),
            "En quelle qualité ?".into(),
            with_nav(
                vec![
                    key("Full HD", "1080p50", Some(!res4k)),
                    key("4K", "2160p50", Some(res4k)),
                ],
                s,
            ),
        ),
        7 => (
            "Format".into(),
            "Pour quels écrans ?".into(),
            with_nav(
                vec![
                    key("16:9", "seul", Some(!d.vertical)),
                    key("+ 9:16", "fabriqué après", Some(d.vertical)),
                ],
                s,
            ),
        ),
        8 => {
            let mut k: Vec<Key> = (1..=4u8)
                .map(|n| {
                    let label = settings
                        .gos
                        .get(n as usize - 1)
                        .map_or(format!("GO{n}"), |g| g.label.clone());
                    if gos_free.get(n as usize - 1).copied().unwrap_or(false) {
                        key(&label, "libre", Some(d.go == n))
                    } else {
                        unavailable(&label, "en service")
                    }
                })
                .collect();
            k.push(key("YouTube", "16:9", Some(d.youtube)));
            k.push(key("TikTok", "9:16", Some(d.tiktok)));
            (
                "Diffusion".into(),
                "On diffuse en direct ?".into(),
                with_nav(k, s),
            )
        }
        9 => (
            "Personnes".into(),
            "Tournez la 1re molette : combien en plateau ?".into(),
            with_nav(vec![], s),
        ),
        10 => (
            "Flux extérieurs".into(),
            "2e molette : les calls.".into(),
            with_nav(
                vec![
                    key("Agence", "de presse", Some(d.agency)),
                    key("Duplex", "Aviwest", Some(d.duplex)),
                ],
                s,
            ),
        ),
        11 => (
            "Placement".into(),
            "Les places par défaut ; à changer sur la tablette.".into(),
            with_nav(vec![], s),
        ),
        12 => {
            let current = s.tests.get(s.test_index);
            let hint = match current {
                Some(t) if s.hearing.low => format!("{} : plus près du micro.", t.label),
                Some(t) if s.hearing.heard => format!("{} : bien reçu.", t.label),
                Some(t) => format!("{} : parlez.", t.label),
                None => "Tous les tests sont faits.".into(),
            };
            let ok = current.is_some()
                && (s.hearing.heard || current.is_some_and(|t| t.seat.is_none()))
                && !s.hearing.low;
            (
                format!(
                    "Tests {}/{}",
                    (s.test_index + 1).min(s.tests.len()),
                    s.tests.len()
                ),
                hint,
                with_nav(
                    vec![
                        key("Refaire", "", None),
                        if ok {
                            key("OK", "image et son", None)
                        } else {
                            unavailable("OK", "image et son")
                        },
                        key("Zone 9:16", "", Some(s.screen == "zone")),
                        key("9:16 seul", "", Some(s.screen == "only")),
                    ],
                    s,
                ),
            )
        }
        13 if s.shooting.is_some() => {
            let secs = s
                .shooting
                .as_ref()
                .map_or(0, |(_, t)| t.elapsed().as_secs());
            (
                format!("EN TOURNAGE {:02}:{:02}", secs / 60, secs % 60),
                if armed {
                    "Appuyez encore pour terminer.".into()
                } else {
                    "Appuyez deux fois pour terminer.".into()
                },
                vec![
                    Key::default(),
                    Key::default(),
                    Key::default(),
                    Key::default(),
                    Key::default(),
                    Key::default(),
                    Key::default(),
                    key(if armed { "CONFIRMER" } else { "Terminer" }, "", None),
                ],
            )
        }
        13 => (
            "Récapitulatif".into(),
            if armed {
                "Appuyez encore pour lancer.".into()
            } else {
                "Appuyez deux fois pour lancer.".into()
            },
            {
                let mut k = vec![Key::default(); 6];
                k.push(key("Retour", "", None));
                k.push(key(if armed { "CONFIRMER" } else { "Lancer" }, "", None));
                k
            },
        ),
        _ => (
            "C'est dans la boîte".into(),
            "Le studio range et s'éteint.".into(),
            vec![key("Nouveau", "tournage", None)],
        ),
    };
    let mut keys = keys;
    keys.resize(8, Key::default());
    DeckView {
        title,
        hint,
        keys,
        dials,
    }
}

/// What key `n` (1 to 8) does at this step.
pub fn press(s: &Session, settings: &Settings, n: usize, armed: bool) -> Act {
    let d = &s.draft;
    if s.step == 1 {
        return if n == 1 { Act::Start } else { Act::Nothing };
    }
    if s.step >= 14 {
        return if n == 1 { Act::Restart } else { Act::Nothing };
    }
    if s.step == 13 {
        return match (n, s.shooting.is_some(), armed) {
            (8, true, true) => Act::End,
            (8, false, true) => Act::Launch,
            (8, _, false) => Act::Nothing,
            (7, false, _) => Act::Prev,
            _ => Act::Nothing,
        };
    }
    match n {
        7 => return Act::Prev,
        8 => return Act::Next,
        _ => {}
    }
    let p = |f: &dyn Fn(&mut DraftPatch)| {
        let mut patch = DraftPatch::default();
        f(&mut patch);
        Act::Patch(patch)
    };
    match (s.step, n) {
        (3, k) if k <= settings.programmes.len().min(6) => Act::Programme(k - 1),
        (4, 1) => p(&|x| x.pgm = Some(!d.pgm)),
        (4, 2) => p(&|x| x.clean = Some(!d.clean)),
        (4, 3) => p(&|x| x.iso = Some(!d.iso)),
        (4, 4) => p(&|x| x.mics = Some(!d.mics)),
        (4, 5) => p(&|x| {
            x.pgm = Some(true);
            x.clean = Some(true);
            x.iso = Some(true);
            x.mics = Some(true);
        }),
        (5, k) if k <= settings.storages.len().min(6) => p(&|x| x.storage = Some(k - 1)),
        (6, 1) => p(&|x| x.resolution = Some("1080p50".into())),
        (6, 2) => p(&|x| x.resolution = Some("2160p50".into())),
        (7, 1) => p(&|x| x.vertical = Some(false)),
        (7, 2) => p(&|x| x.vertical = Some(true)),
        (8, k @ 1..=4) => p(&|x| x.go = Some(if d.go == k as u8 { 0 } else { k as u8 })),
        (8, 5) => p(&|x| x.youtube = Some(!d.youtube)),
        (8, 6) => p(&|x| x.tiktok = Some(!d.tiktok)),
        (10, 1) => p(&|x| x.agency = Some(!d.agency)),
        (10, 2) => p(&|x| x.duplex = Some(!d.duplex)),
        (12, 1) => Act::TestRetry,
        (12, 2) => Act::TestOk,
        (12, 3) => p(&|x| x.screen = Some("zone".into())),
        (12, 4) => p(&|x| x.screen = Some("only".into())),
        _ => Act::Nothing,
    }
}

/// What dial `n` (1 to 4) turned by `delta` does.
pub fn dial(s: &Session, n: usize, delta: i32) -> Act {
    let d = &s.draft;
    let mut x = DraftPatch::default();
    match n {
        1 => x.people = Some((d.people as i32 + delta).clamp(1, 5) as u8),
        2 => x.calls = Some((d.calls as i32 + delta).clamp(0, 4) as u8),
        3 => {
            x.resolution = Some(if delta > 0 {
                "2160p50".into()
            } else {
                "1080p50".into()
            })
        }
        4 => x.vertical = Some(delta > 0),
        _ => return Act::Nothing,
    }
    Act::Patch(x)
}

/// The Buttons variables for a view.
pub fn variables(v: &DeckView) -> Vec<(String, String)> {
    let mut out = vec![
        ("studio_deck_title".to_string(), v.title.clone()),
        ("studio_deck_hint".to_string(), v.hint.clone()),
    ];
    for (i, k) in v.keys.iter().enumerate() {
        let text = if k.sub.is_empty() {
            k.label.clone()
        } else {
            format!("{}\\n{}", k.label, k.sub)
        };
        out.push((format!("studio_k{}", i + 1), text));
        out.push((format!("studio_k{}_on", i + 1), k.on.clone()));
    }
    for (i, dl) in v.dials.iter().enumerate() {
        out.push((
            format!("studio_d{}", i + 1),
            format!("{}\\n{}", dl.label, dl.value),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_has_eight_keys_and_four_dials() {
        let settings = Settings::default();
        for step in 1..=14 {
            let s = Session {
                step,
                ..Default::default()
            };
            let v = view(&s, &settings, &[true, false, true, true], false);
            assert_eq!(v.keys.len(), 8, "étape {step}");
            assert_eq!(v.dials.len(), 4);
        }
    }

    #[test]
    fn keys_choose_and_the_busy_go_is_unavailable() {
        let settings = Settings::default();
        let s = Session {
            step: 8,
            ..Default::default()
        };
        let v = view(&s, &settings, &[true, false, true, true], false);
        assert_eq!(v.keys[1].on, "x");
        assert_eq!(
            press(&s, &settings, 3, false),
            Act::Patch(DraftPatch {
                go: Some(3),
                ..Default::default()
            })
        );
        assert_eq!(press(&s, &settings, 8, false), Act::Next);
        let s3 = Session {
            step: 3,
            ..Default::default()
        };
        assert_eq!(press(&s3, &settings, 2, false), Act::Programme(1));
    }

    #[test]
    fn launching_needs_two_presses() {
        let settings = Settings::default();
        let s = Session {
            step: 13,
            ..Default::default()
        };
        assert_eq!(press(&s, &settings, 8, false), Act::Nothing);
        assert_eq!(press(&s, &settings, 8, true), Act::Launch);
    }

    #[test]
    fn variables_for_buttons() {
        let v = view(&Session::default(), &Settings::default(), &[], false);
        let vars = variables(&v);
        assert!(vars.contains(&(
            "studio_k1".to_string(),
            "Commencer\\nun tournage".to_string()
        )));
        assert_eq!(vars.len(), 2 + 16 + 4);
    }
}
