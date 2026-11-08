//! What the host says, turned into hints for the Director.
//!
//! A speech recogniser running on the server (Whisper, locally) transcribes
//! the host's microphone; [`Listener::hear`] reads each piece of text and
//! says what the host is probably about to do: go to a remote source
//! ("on retrouve tout de suite Julie en duplex"), come back to the set
//! ("merci Julie, on revient en plateau"), or give the floor to a guest
//! ("Alice, qu'en pensez-vous ?").
//!
//! A hint is never an order. It carries:
//! - **when**: now, soon, or later ("tout à l'heure on retrouvera…") ;
//! - **how sure** the listener is ([`Hint::confidence`]), from the words
//!   heard: a name, the kind of source, a verb of arrival, a question ;
//! - the **layout** asked for, when the host names one ("en grand",
//!   "côte à côte", "en vignette").
//!
//! The Director prepares on a weak hint and acts on a strong one, and only
//! at a natural moment (the end of the host's sentence, or the first words
//! of the person given the floor). Negations ("pas de duplex aujourd'hui")
//! and what guests say are ignored. Like the Director, the listener is a
//! pure function of what it is given.

use crate::model::{RemoteId, RemoteKind, SeatId, StudioSetup};
use serde::{Deserialize, Serialize};

/// A piece of transcript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    pub t_ms: u64,
    pub seat: SeatId,
    pub text: String,
    /// The recogniser will not change this text any more.
    #[serde(rename = "final")]
    pub is_final: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "id")]
pub enum Target {
    Remote(RemoteId),
    /// A guest on the set, given the floor.
    Seat(SeatId),
    /// Back to the set from a remote source.
    Set,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum When {
    Now,
    Soon,
    Later,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// Side by side, half and half.
    Split,
    /// The source full frame, the host in a small box.
    Pip,
    /// The source alone.
    Full,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub t_ms: u64,
    pub target: Target,
    pub when: When,
    /// From 0 to 1.
    pub confidence: f32,
    /// The layout the host asked for, if any.
    #[serde(default)]
    pub layout: Option<Layout>,
    /// The text it comes from, for the journal.
    pub heard: String,
}

const LATER: &[&str] = &[
    "tout a l'heure",
    "plus tard",
    "apres la pause",
    "dans la deuxieme partie",
    "en fin d'emission",
    "dans quelques minutes",
    "on retrouvera",
    "nous retrouverons",
    "on ira",
    "nous irons",
    "on rejoindra",
    "nous rejoindrons",
];
const SOON: &[&str] = &[
    "dans un instant",
    "juste apres",
    "dans une minute",
    "tout de suite apres",
    "on va retrouver",
    "nous allons retrouver",
    "on va rejoindre",
    "nous allons rejoindre",
    "on va partir",
    "on va ecouter",
    "on va voir",
];
const ARRIVAL: &[&str] = &[
    "on retrouve",
    "nous retrouvons",
    "retrouvons",
    "on rejoint",
    "nous rejoignons",
    "rejoignons",
    "bonjour",
    "on part",
    "partons",
    "direction",
    "on file",
    "place a",
    "on accueille",
    "nous accueillons",
    "nous rejoint",
    "est avec nous",
    "en ligne avec nous",
    "tout de suite",
    "maintenant",
];
const DUPLEX: &[&str] = &[
    "duplex",
    "en direct de",
    "en direct depuis",
    "sur place",
    "notre envoye",
    "notre envoyee",
    "notre correspondant",
    "notre correspondante",
    "envoye special",
    "envoyee speciale",
    "sur le terrain",
];
const CALL: &[&str] = &[
    "en ligne",
    "au telephone",
    "par telephone",
    "en visio",
    "en visioconference",
    "a distance",
    "en liaison",
    "nous rejoint",
];
const AGENCY: &[&str] = &["images", "regardez", "regardons", "on regarde", "agence"];
const LAYOUT_SPLIT: &[&str] = &["cote a cote", "a deux", "face a face"];
const LAYOUT_FULL: &[&str] = &[
    "en grand",
    "plein ecran",
    "on l'ecoute",
    "ecoutons",
    "on regarde",
    "regardons",
    "regardez",
];
const LAYOUT_PIP: &[&str] = &["en vignette", "en incrustation", "en medaillon", "en petit"];
const BACK: &[&str] = &[
    "on revient en plateau",
    "retour en plateau",
    "retour plateau",
    "on se retrouve en plateau",
    "de retour en plateau",
    "on rentre en plateau",
    "merci pour ce duplex",
    "merci pour ces images",
    "on revient ici",
];
const FAREWELL: &[&str] = &[
    "merci",
    "au revoir",
    "a tres vite",
    "a bientot",
    "bonne soiree",
    "bonne journee",
];
const ADDRESS: &[&str] = &[
    "qu'en pensez vous",
    "qu'est ce que vous en pensez",
    "vous en pensez quoi",
    "votre avis",
    "je me tourne vers",
    "la parole a",
    "a vous",
    "je vous donne la parole",
    "votre reaction",
    "vous reagissez",
    "qu'en dites vous",
];
const NEGATION: &[&str] = &["pas", "plus", "sans", "jamais", "aucun", "aucune", "ni"];

/// Lower case, no accents, words separated by single spaces; `'` kept.
fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars().flat_map(|c| c.to_lowercase()) {
        let c = match ch {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '’' | '\'' => '\'',
            'œ' => {
                out.push_str("oe");
                continue;
            }
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(c);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where `phrase` starts in `text`, as a word index; `None` when absent.
fn find(text: &str, phrase: &str) -> Option<usize> {
    let hay = format!(" {text} ");
    let at = hay.find(&format!(" {phrase} "))?;
    Some(hay[..at].split_whitespace().count())
}

/// The phrase is said, and not denied just before ("pas de duplex").
fn said(text: &str, phrase: &str) -> bool {
    let Some(at) = find(text, phrase) else {
        return false;
    };
    let words: Vec<&str> = text.split_whitespace().collect();
    let from = at.saturating_sub(3);
    !words[from..at]
        .iter()
        .any(|w| NEGATION.contains(w) || w.ends_with("n'") || *w == "n")
}

fn any_said(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|p| said(text, p))
}

fn denied(text: &str, phrases: &[&str]) -> bool {
    phrases
        .iter()
        .any(|p| find(text, p).is_some() && !said(text, p))
}

/// The ways a person is called: their first name and their full name.
fn names(full: Option<&str>) -> Vec<String> {
    let Some(full) = full.map(norm).filter(|n| !n.is_empty()) else {
        return vec![];
    };
    let first = full.split(' ').next().unwrap_or_default().to_string();
    if first == full {
        vec![full]
    } else {
        vec![full, first]
    }
}

pub struct Listener {
    host: Option<SeatId>,
    guests: Vec<(SeatId, Vec<String>)>,
    remotes: Vec<(RemoteId, RemoteKind, Vec<String>)>,
}

impl Listener {
    pub fn new(setup: &StudioSetup) -> Self {
        let host = setup.host_seat();
        Listener {
            host,
            guests: setup
                .seats
                .iter()
                .filter(|s| Some(s.id) != host)
                .map(|s| (s.id, names(s.guest.as_deref())))
                .filter(|(_, n)| !n.is_empty())
                .collect(),
            remotes: setup
                .remotes
                .iter()
                .map(|r| (r.id, r.kind, names(r.guest.as_deref())))
                .collect(),
        }
    }

    /// The hints in what was just heard. Only the host's words count.
    pub fn hear(&self, h: &Heard) -> Vec<Hint> {
        if Some(h.seat) != self.host {
            return vec![];
        }
        let text = norm(&h.text);
        if text.is_empty() {
            return vec![];
        }
        let question = h.text.trim_end().ends_with('?');
        let partial = if h.is_final { 1.0 } else { 0.8 };
        let hint = |target, when, confidence: f32, layout| Hint {
            t_ms: h.t_ms,
            target,
            when,
            confidence: (confidence * partial).min(1.0),
            layout,
            heard: h.text.clone(),
        };
        let named = |ns: &Vec<String>| ns.iter().any(|n| said(&text, n));

        // Back to the set: it wins over everything else said with it.
        let named_remote = self.remotes.iter().any(|(_, _, ns)| named(ns));
        if any_said(&text, BACK) || (named_remote && any_said(&text, FAREWELL)) {
            let c = 0.55
                + if named_remote { 0.25 } else { 0.0 }
                + if any_said(&text, BACK) { 0.15 } else { 0.0 };
            return vec![hint(Target::Set, When::Now, c, None)];
        }

        let when = if any_said(&text, LATER) {
            When::Later
        } else if any_said(&text, SOON) {
            When::Soon
        } else if any_said(&text, ARRIVAL) {
            When::Now
        } else {
            When::Soon
        };
        let asked = if any_said(&text, LAYOUT_PIP) {
            Some(Layout::Pip)
        } else if any_said(&text, LAYOUT_SPLIT) {
            Some(Layout::Split)
        } else if any_said(&text, LAYOUT_FULL) {
            Some(Layout::Full)
        } else {
            None
        };

        let mut out = Vec::new();
        let mut kinds_taken = Vec::new();
        for (id, kind, ns) in &self.remotes {
            let words = match kind {
                RemoteKind::Duplex => DUPLEX,
                RemoteKind::Call => CALL,
                RemoteKind::Agency => AGENCY,
            };
            let is_named = named(ns);
            if !is_named && (denied(&text, words) || kinds_taken.contains(kind)) {
                continue;
            }
            let mut c = 0.0;
            if is_named {
                c += 0.5;
            }
            if any_said(&text, words) {
                c += 0.35;
            }
            if c == 0.0 {
                continue;
            }
            if any_said(&text, ARRIVAL) {
                c += 0.2;
            }
            if c >= 0.35 {
                kinds_taken.push(*kind);
                let layout = asked.or(Some(if *kind == RemoteKind::Agency {
                    Layout::Full
                } else {
                    Layout::Split
                }));
                out.push(hint(Target::Remote(*id), when, c, layout));
            }
        }

        for (seat, ns) in &self.guests {
            if !named(ns) {
                continue;
            }
            let address = any_said(&text, ADDRESS);
            if !question && !address {
                continue;
            }
            let c = 0.45 + if question { 0.25 } else { 0.0 } + if address { 0.2 } else { 0.0 };
            out.push(hint(Target::Seat(*seat), When::Now, c, None));
        }
        out.retain(|h| h.confidence >= 0.35);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn studio() -> StudioSetup {
        let mut s = StudioSetup::example();
        let names = ["Paul Martin", "Alice Durand", "Karim Benali"];
        for (seat, n) in s.seats.iter_mut().zip(names) {
            seat.guest = Some(n.into());
        }
        s.remotes[0].guest = Some("Marc Lefèvre".into());
        s.remotes[1].guest = Some("Julie Roux".into());
        s
    }

    fn hear(text: &str) -> Vec<Hint> {
        Listener::new(&studio()).hear(&Heard {
            t_ms: 0,
            seat: SeatId(1),
            text: text.into(),
            is_final: true,
        })
    }

    #[test]
    fn going_to_the_duplex_now() {
        let h = hear("On retrouve tout de suite Julie en duplex à Lyon.");
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!(h[0].target, Target::Remote(RemoteId(2)));
        assert_eq!(h[0].when, When::Now);
        assert_eq!(h[0].layout, Some(Layout::Split));
        assert!(h[0].confidence >= 0.9, "{}", h[0].confidence);
    }

    #[test]
    fn later_is_not_now() {
        let h = hear("Tout à l'heure, on retrouvera Julie en duplex.");
        assert_eq!(h[0].when, When::Later);
    }

    #[test]
    fn a_denied_duplex_is_no_hint() {
        assert!(hear("Pas de duplex aujourd'hui, désolé.").is_empty());
        assert!(hear("Il n'y a plus de duplex ce soir.").is_empty());
    }

    #[test]
    fn a_call_without_a_name() {
        let h = hear("Notre invité nous rejoint au téléphone, bonjour !");
        assert_eq!(h[0].target, Target::Remote(RemoteId(1)));
        assert_eq!(h[0].when, When::Now);
    }

    #[test]
    fn the_host_asks_for_a_layout() {
        let h = hear("On l'écoute en grand : Marc, à vous.");
        assert_eq!(h[0].target, Target::Remote(RemoteId(1)));
        assert_eq!(h[0].layout, Some(Layout::Full));
    }

    #[test]
    fn back_to_the_set() {
        let h = hear("Merci Julie, on revient en plateau.");
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].target, Target::Set);
        assert!(h[0].confidence >= 0.9);
    }

    #[test]
    fn a_question_to_a_guest() {
        let h = hear("Alice, qu'en pensez-vous ?");
        assert_eq!(h[0].target, Target::Seat(SeatId(2)));
        assert!(h[0].confidence >= 0.85);
        assert!(
            hear("Comme le disait Alice tout à l'heure.").is_empty(),
            "a mention is not a question"
        );
    }

    #[test]
    fn only_the_host_steers() {
        let l = Listener::new(&studio());
        let guest = Heard {
            t_ms: 0,
            seat: SeatId(2),
            text: "On retrouve Julie en duplex.".into(),
            is_final: true,
        };
        assert!(l.hear(&guest).is_empty());
    }

    #[test]
    fn a_partial_transcript_counts_less() {
        let l = Listener::new(&studio());
        let p = l.hear(&Heard {
            t_ms: 0,
            seat: SeatId(1),
            text: "On retrouve Julie en duplex".into(),
            is_final: false,
        });
        let f = l.hear(&Heard {
            t_ms: 0,
            seat: SeatId(1),
            text: "On retrouve Julie en duplex".into(),
            is_final: true,
        });
        assert!(p[0].confidence < f[0].confidence);
    }
}
