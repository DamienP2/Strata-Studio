//! The ST 2110 plant's nodal, by NMOS IS-05 without a registry: each
//! device's Connection API is declared in the administration, with the
//! senders and receivers Studio may route, by name.
//!
//! To route a source to a destination:
//!
//! 1. `GET <sender's node>/single/senders/<id>/transportfile`: the SDP;
//! 2. `PATCH <receiver's node>/single/receivers/<id>/staged` with the
//!    sender, `master_enable`, the SDP and an immediate activation.
//!
//! The routes are read from each receiver's `…/active` (`sender_id`).
//! A node's address is its Connection API, e.g.
//! `http://192.168.30.21/x-nmos/connection/v1.1`.

use crate::{DeviceError, Result, Router};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A sender or a receiver Studio may use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// As it appears in Studio (« CAM 1 », « GO1 »).
    pub name: String,
    /// The node's Connection API.
    pub node: String,
    /// The sender's or receiver's id (UUID).
    pub id: String,
}

pub struct NmosRouter {
    pub senders: Vec<Endpoint>,
    pub receivers: Vec<Endpoint>,
    agent: ureq::Agent,
}

/// The body of the staged PATCH.
pub fn staged(sender_id: &str, sdp: &str) -> serde_json::Value {
    serde_json::json!({
        "sender_id": sender_id,
        "master_enable": true,
        "activation": { "mode": "activate_immediate" },
        "transport_file": { "data": sdp, "type": "application/sdp" }
    })
}

impl NmosRouter {
    pub fn new(senders: Vec<Endpoint>, receivers: Vec<Endpoint>) -> Self {
        NmosRouter {
            senders,
            receivers,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build(),
        }
    }

    fn find<'a>(list: &'a [Endpoint], name: &str) -> Option<&'a Endpoint> {
        list.iter()
            .find(|e| e.name.eq_ignore_ascii_case(name.trim()))
    }

    fn url(e: &Endpoint, kind: &str, rest: &str) -> String {
        format!(
            "{}/single/{kind}/{}/{rest}",
            e.node.trim_end_matches('/'),
            e.id
        )
    }
}

impl Router for NmosRouter {
    fn name(&self) -> String {
        format!(
            "NMOS IS-05 ({} sources, {} destinations)",
            self.senders.len(),
            self.receivers.len()
        )
    }
    fn route(&mut self, destination: &str, source: &str) -> Result<()> {
        let refused = |e: String| DeviceError::Refused("NMOS IS-05".into(), e);
        let rx = Self::find(&self.receivers, destination)
            .ok_or_else(|| refused(format!("pas de destination « {destination} »")))?;
        let tx = Self::find(&self.senders, source)
            .ok_or_else(|| refused(format!("pas de source « {source} »")))?;
        let sdp = self
            .agent
            .get(&Self::url(tx, "senders", "transportfile"))
            .call()
            .map_err(|e| refused(format!("SDP de {source} : {e}")))?
            .into_string()
            .map_err(|e| refused(e.to_string()))?;
        self.agent
            .request("PATCH", &Self::url(rx, "receivers", "staged"))
            .send_json(staged(&tx.id, &sdp))
            .map_err(|e| refused(format!("{destination} : {e}")))?;
        Ok(())
    }
    fn routes(&self) -> Vec<(String, String)> {
        self.receivers
            .iter()
            .filter_map(|rx| {
                let active: serde_json::Value = self
                    .agent
                    .get(&Self::url(rx, "receivers", "active"))
                    .call()
                    .ok()?
                    .into_json()
                    .ok()?;
                let sid = active.get("sender_id")?.as_str()?;
                let src = self
                    .senders
                    .iter()
                    .find(|s| s.id == sid)
                    .map_or_else(|| sid.to_string(), |s| s.name.clone());
                Some((rx.name.clone(), src))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testhttp::FakeHttp;

    #[test]
    fn routes_a_sender_to_a_receiver() {
        let fake = FakeHttp::start(vec![
            (
                "GET /x-nmos/connection/v1.1/single/senders/s-pgm/transportfile",
                200,
                "v=0\r\no=- 1 1 IN IP4 10.0.0.5\r\n",
            ),
            (
                "PATCH /x-nmos/connection/v1.1/single/receivers/r-go1/staged",
                200,
                "{}",
            ),
            (
                "GET /x-nmos/connection/v1.1/single/receivers/r-go1/active",
                200,
                r#"{"sender_id":"s-pgm","master_enable":true}"#,
            ),
        ]);
        let node = format!("http://{}/x-nmos/connection/v1.1", fake.addr());
        let mut r = NmosRouter::new(
            vec![Endpoint {
                name: "PGM".into(),
                node: node.clone(),
                id: "s-pgm".into(),
            }],
            vec![Endpoint {
                name: "GO1".into(),
                node,
                id: "r-go1".into(),
            }],
        );
        r.route("GO1", "pgm").unwrap();
        assert_eq!(r.routes(), vec![("GO1".to_string(), "PGM".to_string())]);
        let got = fake.requests();
        let body: serde_json::Value = serde_json::from_str(&got[1].1).unwrap();
        assert_eq!(body["sender_id"], "s-pgm");
        assert_eq!(body["activation"]["mode"], "activate_immediate");
        assert!(body["transport_file"]["data"]
            .as_str()
            .unwrap()
            .starts_with("v=0"));
    }

    #[test]
    fn an_unknown_name_is_refused() {
        let mut r = NmosRouter::new(vec![], vec![]);
        assert!(r.route("GO1", "PGM").is_err());
    }
}
