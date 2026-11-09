//! The calls (Studiocall, Teams, Zoom) through Quicklink StudioEdge, by the
//! Quicklink Studio REST API v1.5 (« API v1.5 Reference », Quicklink
//! support):
//!
//! | Studio needs | Request |
//! | --- | --- |
//! | a token, valid 24 h | `GET createNewToken/:username/:password`, then `Authorization: Bearer <token>` |
//! | a guest and its invitation link | `POST createguest` `{guests: [{startdate, enddate, name}]}` → `{uid, token, name}`; link `https://quicklink.network/portal/Communicator.php?Token=<token>` |
//! | is the guest there, in call, losing packets | `GET user/:uid`: the publisher object (`status` *Ready* / *In Call*, `warning` *warning* beyond 2 % loss over 20 s), or « … is not logged in » |
//! | put the guest on an output | `POST joindistributor/:uid/:roomid/:position`, `leavedistributor/…` |
//! | cut their sound or picture | `POST mute/:uid/:audio/:video` (true: on) |
//!
//! The API's address is the studio's (set in the administration). The
//! state is read by asking (`GET user/:uid`) rather than by Socket.IO.

use crate::{DeviceError, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub const LINK: &str = "https://quicklink.network/portal/Communicator.php?Token=";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guest {
    pub uid: String,
    pub token: String,
    pub name: String,
    pub link: String,
}

/// Where a guest is.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum GuestState {
    /// Not connected yet.
    Waiting,
    /// Connected. `unstable`: more than 2 % packets lost over 20 s.
    Connected {
        in_call: bool,
        unstable: bool,
        audio: bool,
        video: bool,
    },
    /// The API does not answer.
    Unknown { error: String },
}

/// The calls of the studio.
pub trait Calls: Send {
    fn name(&self) -> String;
    /// One guest per call, valid from `start` to `end` (`YYYY-MM-DD HH:MM:SS`).
    fn create_guest(&mut self, name: &str, start: &str, end: &str) -> Result<Guest>;
    fn guest(&mut self, uid: &str) -> GuestState;
    /// Put the guest on a position of the distributor room (an output of
    /// StudioEdge, so a switcher input).
    fn place(&mut self, uid: &str, position: u16) -> Result<()>;
    fn unplace(&mut self, uid: &str, position: u16) -> Result<()>;
    fn mute(&mut self, uid: &str, audio: bool, video: bool) -> Result<()>;
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Publisher {
    #[serde(default)]
    status: String,
    #[serde(default)]
    warning: String,
    #[serde(default, rename = "configAudio")]
    config_audio: serde_json::Value,
    #[serde(default, rename = "configVideo")]
    config_video: serde_json::Value,
}

fn truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => s == "true",
        serde_json::Value::Null => true,
        _ => true,
    }
}

/// The publisher object, or « not logged in » (any answer that is not one).
fn parse_user(text: &str) -> GuestState {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(v) if v.is_object() => {
            let p: Publisher = serde_json::from_value(v).unwrap_or_default();
            GuestState::Connected {
                in_call: p.status.eq_ignore_ascii_case("in call"),
                unstable: p.warning.eq_ignore_ascii_case("warning"),
                audio: truthy(&p.config_audio),
                video: truthy(&p.config_video),
            }
        }
        _ => GuestState::Waiting,
    }
}

/// The token in whatever form the API gives it: `{"token": …}`, a JSON
/// string, or plain text.
fn parse_token(text: &str) -> Option<String> {
    let t = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(o)) => o.get("token")?.as_str()?.to_string(),
        Ok(serde_json::Value::String(s)) => s,
        _ => text.to_string(),
    };
    let t = t.trim().to_string();
    (!t.is_empty()).then_some(t)
}

fn parse_guests(text: &str) -> Vec<(String, String, String)> {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let list = match &v {
        serde_json::Value::Array(a) => a.clone(),
        serde_json::Value::Object(o) => match o.get("guests") {
            Some(serde_json::Value::Array(a)) => a.clone(),
            _ => vec![v.clone()],
        },
        _ => vec![],
    };
    list.iter()
        .filter_map(|g| {
            let s = |k: &str| match g.get(k)? {
                serde_json::Value::String(s) => Some(s.clone()),
                other => Some(other.to_string()),
            };
            Some((s("uid")?, s("token")?, s("name").unwrap_or_default()))
        })
        .collect()
}

pub struct Quicklink {
    base: String,
    username: String,
    password: String,
    distributor_room: String,
    token: Option<(String, Instant)>,
    agent: ureq::Agent,
}

impl Quicklink {
    /// `api`: the API's address (`https://…`); `distributor_room`: the
    /// StudioEdge distributor room whose positions are the outputs.
    pub fn new(api: &str, username: &str, password: &str, distributor_room: &str) -> Self {
        Quicklink {
            base: api.trim_end_matches('/').to_string(),
            username: username.into(),
            password: password.into(),
            distributor_room: distributor_room.into(),
            token: None,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(5))
                .build(),
        }
    }

    fn refused(&self, e: impl ToString) -> DeviceError {
        DeviceError::Refused(self.name(), e.to_string())
    }

    /// A token, renewed an hour before its 24 hours are over.
    fn token(&mut self) -> Result<String> {
        if let Some((t, at)) = &self.token {
            if at.elapsed() < Duration::from_secs(23 * 3600) {
                return Ok(t.clone());
            }
        }
        let url = format!(
            "{}/createNewToken/{}/{}",
            self.base,
            crate::buttons::url_encode(&self.username),
            crate::buttons::url_encode(&self.password)
        );
        let text = self
            .agent
            .get(&url)
            .call()
            .map_err(|e| self.refused(e))?
            .into_string()
            .map_err(|e| self.refused(e))?;
        let t = parse_token(&text).ok_or_else(|| self.refused("pas de jeton"))?;
        self.token = Some((t.clone(), Instant::now()));
        Ok(t)
    }

    fn request(
        &mut self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<String> {
        let token = self.token()?;
        let req = self
            .agent
            .request(method, &format!("{}/{path}", self.base))
            .set("Authorization", &format!("Bearer {token}"));
        let res = match body {
            Some(b) => req.send_json(b),
            None => req.call(),
        };
        match res {
            Ok(r) => r.into_string().map_err(|e| self.refused(e)),
            Err(ureq::Error::Status(401, _)) => {
                self.token = None;
                Err(self.refused("jeton refusé"))
            }
            Err(ureq::Error::Status(_, r)) => Ok(r.into_string().unwrap_or_default()),
            Err(e) => Err(self.refused(e)),
        }
    }
}

impl Calls for Quicklink {
    fn name(&self) -> String {
        format!("Quicklink StudioEdge ({})", self.base)
    }
    fn create_guest(&mut self, name: &str, start: &str, end: &str) -> Result<Guest> {
        let body =
            serde_json::json!({ "guests": [{ "startdate": start, "enddate": end, "name": name }] });
        let text = self.request("POST", "createguest", Some(body))?;
        let (uid, token, n) = parse_guests(&text)
            .into_iter()
            .next()
            .ok_or_else(|| self.refused(format!("invité non créé : {text}")))?;
        Ok(Guest {
            link: format!("{LINK}{token}"),
            uid,
            token,
            name: if n.is_empty() { name.into() } else { n },
        })
    }
    fn guest(&mut self, uid: &str) -> GuestState {
        match self.request("GET", &format!("user/{uid}"), None) {
            Ok(text) => parse_user(&text),
            Err(e) => GuestState::Unknown {
                error: e.to_string(),
            },
        }
    }
    fn place(&mut self, uid: &str, position: u16) -> Result<()> {
        let room = self.distributor_room.clone();
        self.request(
            "POST",
            &format!("joindistributor/{uid}/{room}/{position}"),
            None,
        )
        .map(|_| ())
    }
    fn unplace(&mut self, uid: &str, position: u16) -> Result<()> {
        let room = self.distributor_room.clone();
        self.request(
            "POST",
            &format!("leavedistributor/{uid}/{room}/{position}"),
            None,
        )
        .map(|_| ())
    }
    fn mute(&mut self, uid: &str, audio: bool, video: bool) -> Result<()> {
        self.request("POST", &format!("mute/{uid}/{audio}/{video}"), None)
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testhttp::FakeHttp;

    #[test]
    fn token_forms() {
        assert_eq!(
            parse_token(r#"{"token":" gRCw "}"#).as_deref(),
            Some("gRCw")
        );
        assert_eq!(parse_token(r#""abc""#).as_deref(), Some("abc"));
        assert_eq!(parse_token("4eC39H\n").as_deref(), Some("4eC39H"));
        assert_eq!(parse_token(""), None);
    }

    #[test]
    fn a_user_not_logged_in_is_waiting() {
        assert_eq!(parse_user("\"g987 is not logged in\""), GuestState::Waiting);
        assert_eq!(parse_user("g987 is not logged in"), GuestState::Waiting);
        assert_eq!(
            parse_user(
                r#"{"uid":"g987","status":"In Call","warning":"warning","configAudio":"true","configVideo":"false"}"#
            ),
            GuestState::Connected {
                in_call: true,
                unstable: true,
                audio: true,
                video: false
            }
        );
    }

    #[test]
    fn creates_a_guest_and_places_it() {
        let fake = FakeHttp::start(vec![
            (
                "GET /createNewToken/studio/s%20cret",
                200,
                r#"{"token":"T0K"}"#,
            ),
            (
                "POST /createguest",
                200,
                r#"[{"uid":"g987","token":"QCgm","name":"Call 1"}]"#,
            ),
            ("POST /joindistributor/g987/12/2", 200, "OK"),
        ]);
        let mut ql = Quicklink::new(&format!("http://{}", fake.addr()), "studio", "s cret", "12");
        let g = ql
            .create_guest("Call 1", "2026-11-08 10:00:00", "2026-11-08 14:00:00")
            .unwrap();
        assert_eq!(g.uid, "g987");
        assert_eq!(g.link, format!("{LINK}QCgm"));
        ql.place("g987", 2).unwrap();
        let got = fake.requests();
        assert_eq!(got.len(), 3);
        assert!(got[1].1.contains("\"startdate\":\"2026-11-08 10:00:00\""));
    }
}
