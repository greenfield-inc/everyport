use anyhow::{bail, Context};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// A `ppm serve` bearer token. `Debug` hides the value, so logs and error
/// reports never carry it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Token(pub String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

/// How to reach `ppm` on a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// The `ppm` binary bundled with the app, on this machine.
    Sidecar { path: PathBuf },
    /// A command prefix that runs a program on the machine, such as
    /// `ssh devbox` or `docker exec -i box`. `ppm_path` is where `ppm` lives
    /// there, as found by [`crate::install::probe`].
    Command {
        argv_prefix: Vec<String>,
        ppm_path: String,
    },
    /// `ppm serve`, reached over HTTP.
    Http { url: String, token: Token },
}

impl Connection {
    /// Reads the `ppm://` code that `ppm serve` prints: base64url of
    /// `{"url": ..., "token": ...}`.
    pub fn from_code(code: &str) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Code {
            url: String,
            token: String,
        }
        let Some(payload) = code.trim().strip_prefix("ppm://") else {
            bail!("A connection code starts with ppm://");
        };
        let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .context("The connection code is damaged. Copy it again from `ppm serve`.")?;
        let Code { url, token } = serde_json::from_slice(&json)
            .context("The connection code is damaged. Copy it again from `ppm serve`.")?;
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
        let code = "ppm://eyJ1cmwiOiJodHRwOi8vMTI3LjAuMC4xOjc3NjciLCJ0b2tlbiI6InMzY3JldCJ9";
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
        assert!(Connection::from_code("ppm://not base64!").is_err());
    }
}
