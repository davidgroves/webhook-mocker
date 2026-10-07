use std::{net::SocketAddr, path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand};
use thiserror::Error;

use crate::store::ChannelKind;

/// Chromium/Firefox "unsafe ports" that block browser UI access.
const BROWSER_BLOCKED_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669,
    6697, 10080,
];

#[derive(Debug, Parser)]
#[command(name = "webhook-mocker", about = "Dev-only Slack/Teams webhook mocker")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Listen address (host:port). Env: WM_LISTEN
    #[arg(
        long,
        env = "WM_LISTEN",
        default_value = "127.0.0.1:5080",
        global = true
    )]
    pub listen: SocketAddr,

    /// Public base URL used when printing webhook URLs. Env: WM_PUBLIC_URL
    #[arg(long, env = "WM_PUBLIC_URL")]
    pub public_url: Option<String>,

    /// Create named channels at startup: kind:name (e.g. slack:alerts)
    #[arg(long = "channel", value_name = "KIND:NAME")]
    pub channels: Vec<ChannelPreset>,

    /// Turn validation warnings into hard provider errors
    #[arg(long, env = "WM_STRICT", default_value_t = false)]
    pub strict: bool,

    /// Auto-create channels on first webhook hit
    #[arg(long, env = "WM_AUTO_CREATE", default_value_t = true)]
    pub auto_create: bool,

    /// Max messages retained per channel
    #[arg(long, env = "WM_MAX_MESSAGES", default_value_t = 500)]
    pub max_messages: usize,

    /// Max request body size in bytes
    #[arg(long, env = "WM_MAX_BODY", default_value_t = 1_048_576)]
    pub max_body: usize,

    /// Optional JSON snapshot path for persistence across restarts
    #[arg(long, env = "WM_SNAPSHOT")]
    pub snapshot: Option<PathBuf>,

    /// Log as JSON
    #[arg(long, env = "WM_JSON_LOGS", default_value_t = false)]
    pub json_logs: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Probe /healthz on the configured listen address (for distroless HEALTHCHECK)
    Healthcheck,
}

#[derive(Debug, Clone)]
pub struct ChannelPreset {
    pub kind: ChannelKind,
    pub name: String,
}

#[derive(Debug, Error)]
pub enum PresetError {
    #[error("expected KIND:NAME, got `{0}`")]
    BadFormat(String),
    #[error("unknown channel kind `{0}` (use slack, teams, teams-legacy, or generic)")]
    UnknownKind(String),
}

impl FromStr for ChannelPreset {
    type Err = PresetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (kind_raw, name) = s
            .split_once(':')
            .ok_or_else(|| PresetError::BadFormat(s.to_string()))?;
        if name.is_empty() {
            return Err(PresetError::BadFormat(s.to_string()));
        }
        let kind = match kind_raw {
            "slack" => ChannelKind::Slack,
            "teams" | "teams-workflows" => ChannelKind::TeamsWorkflows,
            "teams-legacy" | "teams-o365" => ChannelKind::TeamsLegacy,
            "generic" => ChannelKind::Generic,
            other => return Err(PresetError::UnknownKind(other.to_string())),
        };
        Ok(Self {
            kind,
            name: name.to_string(),
        })
    }
}

impl Cli {
    pub fn public_base(&self) -> String {
        self.public_url
            .clone()
            .unwrap_or_else(|| format!("http://{}", self.listen))
    }

    pub fn warn_if_browser_blocked(&self) {
        if is_browser_blocked_port(self.listen.port()) {
            tracing::warn!(
                port = self.listen.port(),
                "listen port is blocked by Chrome/Firefox; webhook API will work but the UI will not open in a browser"
            );
        }
    }
}

pub fn is_browser_blocked_port(port: u16) -> bool {
    BROWSER_BLOCKED_PORTS.contains(&port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_presets() {
        let p: ChannelPreset = "slack:alerts".parse().unwrap();
        assert_eq!(p.kind, ChannelKind::Slack);
        assert_eq!(p.name, "alerts");
    }

    #[test]
    fn blocks_sip_port() {
        assert!(is_browser_blocked_port(5060));
        assert!(!is_browser_blocked_port(5080));
    }
}
