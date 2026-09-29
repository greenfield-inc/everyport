use anyhow::{bail, Context};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// An `everyport serve` bearer token. `Debug` hides the value, so logs and error
/// reports never carry it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Token(pub String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

/// How to reach `everyport` on a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// The `everyport` binary bundled with the app, on this machine.
    Sidecar { path: PathBuf },
    /// A command prefix that runs a program on the machine, such as
    /// `ssh devbox` or `docker exec -i box`. `everyport_path` is where `everyport` lives
    /// there, as found by [`crate::client::install::probe`].
    Command {
        argv_prefix: Vec<String>,
        everyport_path: String,
    },
    /// `everyport serve`, reached over HTTP.
    Http { url: String, token: Token },
}

impl Connection {
    /// Reads the `everyport://` code that `everyport serve` prints: base64url of
    /// `{"url": ..., "token": ...}`.
    pub fn from_code(code: &str) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Code {
            url: String,
            token: String,
        }
        let Some(payload) = code.trim().strip_prefix("everyport://") else {
            bail!("A connection code starts with everyport://");
        };
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .context("The connection code is damaged. Copy it again from `everyport serve`.")?;
        let Code { url, token } = serde_json::from_slice(&json)
            .context("The connection code is damaged. Copy it again from `everyport serve`.")?;
        Ok(Self::Http {
            url: url.trim_end_matches('/').to_string(),
            token: Token(token),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_serve_code() {
        // printf '{"url":"http://127.0.0.1:7767","token":"s3cret"}' | base64 | tr '+/' '-_' | tr -d '='
        let code = "everyport://eyJ1cmwiOiJodHRwOi8vMTI3LjAuMC4xOjc3NjciLCJ0b2tlbiI6InMzY3JldCJ9";
        let expected = Connection::Http {
            url: "http://127.0.0.1:7767".into(),
            token: Token("s3cret".into()),
        };
        assert!(!format!("{expected:?}").contains("s3cret"));
        assert_eq!(Connection::from_code(code).unwrap(), expected);
        assert_eq!(
            Connection::from_code(&format!(" {code}==\n")).unwrap(),
            expected
        );
    }

    #[test]
    fn rejects_other_codes() {
        assert!(Connection::from_code("pane-remote://eyJ9").is_err());
        assert!(Connection::from_code("everyport://not base64!").is_err());
    }
}
