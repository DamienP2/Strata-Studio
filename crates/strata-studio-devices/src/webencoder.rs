//! The « GO » platforms: one Blackmagic Streaming Encoder 4K per GO, driven
//! by its REST API (`http://<address>/control/api/v1`, Blackmagic
//! « Streaming REST API », January 2026):
//!
//! - `GET /livestreams/0`: `status` is `Idle`, `Connecting`, `Streaming`,
//!   `Flushing` or `Interrupted`, with the bitrate and the duration;
//! - `PUT /livestreams/0/start` and `PUT /livestreams/0/stop`: 204.
//!
//! A GO that is not `Idle` is taken (another studio streams on it): the
//! tablet greys it out.

use crate::{DeviceError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A GO as the tablet sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum GoState {
    /// Free to use.
    Idle,
    /// Streaming (or connecting, flushing): taken.
    Live {
        status: String,
        bitrate: u64,
        seconds: u64,
    },
    /// Does not answer.
    Offline { error: String },
}

impl GoState {
    pub fn is_free(&self) -> bool {
        matches!(self, GoState::Idle)
    }
}

/// What `GET /livestreams/0` gives.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Livestream {
    pub status: String,
    #[serde(default)]
    pub bitrate: u64,
    #[serde(default)]
    pub duration: Option<u64>,
}

impl From<Livestream> for GoState {
    fn from(l: Livestream) -> Self {
        if l.status == "Idle" {
            GoState::Idle
        } else {
            GoState::Live {
                status: l.status,
                bitrate: l.bitrate,
                seconds: l.duration.unwrap_or(0),
            }
        }
    }
}

/// A streaming encoder behind a GO.
pub trait GoEncoder: Send {
    fn name(&self) -> String;
    fn state(&mut self) -> GoState;
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
}

pub struct BlackmagicEncoder {
    label: String,
    base: String,
    agent: ureq::Agent,
}

impl BlackmagicEncoder {
    /// `address`: the encoder's IP address or host name.
    pub fn new(label: impl Into<String>, address: &str) -> Self {
        let base = if address.starts_with("http") {
            address.trim_end_matches('/').to_string()
        } else {
            format!("http://{address}")
        };
        BlackmagicEncoder {
            label: label.into(),
            base: format!("{base}/control/api/v1"),
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .build(),
        }
    }

    fn put(&self, path: &str) -> Result<()> {
        self.agent
            .put(&format!("{}{path}", self.base))
            .call()
            .map(|_| ())
            .map_err(|e| DeviceError::Refused(self.name(), e.to_string()))
    }
}

impl GoEncoder for BlackmagicEncoder {
    fn name(&self) -> String {
        format!("{} (Streaming Encoder {})", self.label, self.base)
    }
    fn state(&mut self) -> GoState {
        match self
            .agent
            .get(&format!("{}/livestreams/0", self.base))
            .call()
        {
            Ok(r) => match r.into_json::<Livestream>() {
                Ok(l) => l.into(),
                Err(e) => GoState::Offline {
                    error: e.to_string(),
                },
            },
            Err(e) => GoState::Offline {
                error: e.to_string(),
            },
        }
    }
    fn start(&mut self) -> Result<()> {
        self.put("/livestreams/0/start")
    }
    fn stop(&mut self) -> Result<()> {
        self.put("/livestreams/0/stop")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testhttp::FakeHttp;

    #[test]
    fn reads_the_state_and_starts() {
        let fake = FakeHttp::start(vec![
            (
                "GET /control/api/v1/livestreams/0",
                200,
                r#"{"status":"Idle","bitrate":0,"effectiveVideoFormat":"1920x1080p50"}"#,
            ),
            ("PUT /control/api/v1/livestreams/0/start", 204, ""),
            (
                "GET /control/api/v1/livestreams/0",
                200,
                r#"{"status":"Streaming","bitrate":9000000,"effectiveVideoFormat":"1920x1080p50","duration":12}"#,
            ),
        ]);
        let mut go = BlackmagicEncoder::new("GO1", &fake.addr());
        assert_eq!(go.state(), GoState::Idle);
        go.start().unwrap();
        assert_eq!(
            go.state(),
            GoState::Live {
                status: "Streaming".into(),
                bitrate: 9_000_000,
                seconds: 12
            }
        );
        assert_eq!(fake.requests().len(), 3);
    }

    #[test]
    fn an_encoder_that_does_not_answer_is_offline() {
        let mut go = BlackmagicEncoder::new("GO4", "127.0.0.1:1");
        assert!(matches!(go.state(), GoState::Offline { .. }));
    }
}
