use std::{net::SocketAddr, path::PathBuf, str::FromStr};

use clap::{Parser, Subcommand};
use config::{Config, ConfigError, Environment, File, FileFormat};
use serde::Deserialize;
use thiserror::Error;

use crate::faults::FaultProfile;
use crate::store::ChannelKind;

/// Chromium/Firefox "unsafe ports" that block browser UI access.
const BROWSER_BLOCKED_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669,
    6697, 10080,
];

const DEFAULT_CONFIG_BASENAME: &str = "webhook-mocker";

#[derive(Debug, Parser)]
#[command(
    name = "webhook-mocker",
    about = "Dev-only Slack/Teams webhook mocker",
    after_help = "Config file: --config PATH, WM_CONFIG, or ./webhook-mocker.yaml\nPrecedence: CLI flags > WM_* env > YAML > defaults"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// YAML config file (env: WM_CONFIG). If omitted, loads ./webhook-mocker.yaml when present.
    #[arg(short, long, env = "WM_CONFIG", global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Listen address (host:port)
    #[arg(long, global = true)]
    pub listen: Option<SocketAddr>,

    /// Public base URL used when printing webhook URLs
    #[arg(long, global = true)]
    pub public_url: Option<String>,

    /// Create named channels at startup: kind:name (e.g. slack:alerts). Replaces file channels when set.
    #[arg(long = "channel", value_name = "KIND:NAME", global = true)]
    pub channels: Vec<ChannelPreset>,

    /// Turn validation warnings into hard provider errors (`--strict` / `--strict=false`)
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
    pub strict: Option<bool>,

    /// Auto-create channels on first webhook hit
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
    pub auto_create: Option<bool>,

    /// Max messages retained per channel
    #[arg(long, global = true)]
    pub max_messages: Option<usize>,

    /// Max request body size in bytes
    #[arg(long, global = true)]
    pub max_body: Option<usize>,

    /// Optional JSON snapshot path for persistence across restarts
    #[arg(long, global = true)]
    pub snapshot: Option<PathBuf>,

    /// Log as JSON
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", action = clap::ArgAction::Set)]
    pub json_logs: Option<bool>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Probe /healthz on the configured listen address (for distroless HEALTHCHECK)
    Healthcheck,
}

/// Resolved application settings after layering defaults, YAML, env, and CLI.
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub listen: SocketAddr,
    pub public_url: Option<String>,
    #[serde(default)]
    pub channels: Vec<ChannelPreset>,
    pub strict: bool,
    pub auto_create: bool,
    pub max_messages: usize,
    pub max_body: usize,
    pub snapshot: Option<PathBuf>,
    pub json_logs: bool,
}

#[derive(Debug, Clone)]
pub struct ChannelPreset {
    pub kind: ChannelKind,
    pub name: String,
    /// Optional stable path token (Slack token / Teams id / generic id).
    pub token: Option<String>,
    pub slack_team: Option<String>,
    pub slack_bot: Option<String>,
    /// Fault profile applied at channel creation (YAML long form only).
    pub faults: FaultProfile,
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
            token: None,
            slack_team: None,
            slack_bot: None,
            faults: FaultProfile::default(),
        })
    }
}

impl<'de> Deserialize<'de> for ChannelPreset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LongForm {
            kind: String,
            name: String,
            #[serde(default)]
            token: Option<String>,
            #[serde(default)]
            slack_team: Option<String>,
            #[serde(default)]
            slack_bot: Option<String>,
            #[serde(default)]
            faults: FaultProfile,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper {
            Short(String),
            Long(LongForm),
        }

        match Helper::deserialize(deserializer)? {
            Helper::Short(s) => s.parse().map_err(serde::de::Error::custom),
            Helper::Long(long) => {
                let mut preset: ChannelPreset = format!("{}:{}", long.kind, long.name)
                    .parse()
                    .map_err(serde::de::Error::custom)?;
                preset.token = long.token;
                preset.slack_team = long.slack_team;
                preset.slack_bot = long.slack_bot;
                preset.faults = long.faults;
                Ok(preset)
            }
        }
    }
}

impl Settings {
    /// Load settings: defaults ← YAML ← `WM_*` env ← CLI overrides.
    pub fn load(cli: &Cli) -> Result<Self, ConfigError> {
        let mut builder = Config::builder()
            .set_default("listen", "127.0.0.1:5080")?
            .set_default("strict", false)?
            .set_default("auto_create", true)?
            .set_default("max_messages", 500)?
            .set_default("max_body", 1_048_576)?
            .set_default("json_logs", false)?;

        if let Some(path) = &cli.config {
            builder = builder.add_source(File::from(path.clone()).format(FileFormat::Yaml));
        } else {
            builder = builder.add_source(
                File::with_name(DEFAULT_CONFIG_BASENAME)
                    .format(FileFormat::Yaml)
                    .required(false),
            );
        }

        // Separator `__` keeps underscores in keys (WM_PUBLIC_URL → public_url).
        builder = builder.add_source(
            Environment::with_prefix("WM")
                .separator("__")
                .try_parsing(true),
        );

        if let Some(listen) = cli.listen {
            builder = builder.set_override("listen", listen.to_string())?;
        }
        if let Some(ref public_url) = cli.public_url {
            builder = builder.set_override("public_url", public_url.clone())?;
        }
        if let Some(strict) = cli.strict {
            builder = builder.set_override("strict", strict)?;
        }
        if let Some(auto_create) = cli.auto_create {
            builder = builder.set_override("auto_create", auto_create)?;
        }
        if let Some(max_messages) = cli.max_messages {
            builder = builder.set_override("max_messages", max_messages as i64)?;
        }
        if let Some(max_body) = cli.max_body {
            builder = builder.set_override("max_body", max_body as i64)?;
        }
        if let Some(ref snapshot) = cli.snapshot {
            builder = builder.set_override("snapshot", snapshot.display().to_string())?;
        }
        if let Some(json_logs) = cli.json_logs {
            builder = builder.set_override("json_logs", json_logs)?;
        }

        let mut settings: Self = builder.build()?.try_deserialize()?;

        if !cli.channels.is_empty() {
            settings.channels = cli.channels.clone();
        }

        Ok(settings)
    }

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
    use std::io::Write;

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

    #[test]
    fn loads_yaml_file() {
        let dir = tempfile_dir();
        let path = dir.join("test.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
listen: 0.0.0.0:5099
public_url: http://example.test:5099
strict: true
auto_create: false
max_messages: 42
max_body: 2048
json_logs: true
channels:
  - slack:alerts
  - kind: teams
    name: ops
  - kind: slack
    name: flaky
    token: flaky-token
    slack_team: T9
    slack_bot: B9
    faults:
      force_429: true
      retry_after_secs: 2
  - kind: generic
    name: slow
    token: slow-hook
    faults:
      delay_ms: 250
      fail_percent: 50
      status: 503
"#
        )
        .unwrap();

        let cli = Cli {
            command: None,
            config: Some(path),
            listen: None,
            public_url: None,
            channels: vec![],
            strict: None,
            auto_create: None,
            max_messages: None,
            max_body: None,
            snapshot: None,
            json_logs: None,
        };
        let s = Settings::load(&cli).unwrap();
        assert_eq!(s.listen, "0.0.0.0:5099".parse().unwrap());
        assert_eq!(s.public_url.as_deref(), Some("http://example.test:5099"));
        assert!(s.strict);
        assert!(!s.auto_create);
        assert_eq!(s.max_messages, 42);
        assert_eq!(s.max_body, 2048);
        assert!(s.json_logs);
        assert_eq!(s.channels.len(), 4);
        assert_eq!(s.channels[0].name, "alerts");
        assert_eq!(s.channels[1].kind, ChannelKind::TeamsWorkflows);

        let flaky = &s.channels[2];
        assert_eq!(flaky.name, "flaky");
        assert_eq!(flaky.token.as_deref(), Some("flaky-token"));
        assert_eq!(flaky.slack_team.as_deref(), Some("T9"));
        assert_eq!(flaky.slack_bot.as_deref(), Some("B9"));
        assert!(flaky.faults.force_429);
        assert_eq!(flaky.faults.retry_after_secs, Some(2));

        let slow = &s.channels[3];
        assert_eq!(slow.kind, ChannelKind::Generic);
        assert_eq!(slow.token.as_deref(), Some("slow-hook"));
        assert_eq!(slow.faults.delay_ms, Some(250));
        assert_eq!(slow.faults.fail_percent, Some(50));
        assert_eq!(slow.faults.status, Some(503));
    }

    #[test]
    fn cli_overrides_file() {
        let dir = tempfile_dir();
        let path = dir.join("test.yaml");
        std::fs::write(&path, "listen: 127.0.0.1:5000\nstrict: false\n").unwrap();

        let cli = Cli {
            command: None,
            config: Some(path),
            listen: Some("127.0.0.1:6000".parse().unwrap()),
            public_url: None,
            channels: vec!["generic:hook".parse().unwrap()],
            strict: Some(true),
            auto_create: None,
            max_messages: None,
            max_body: None,
            snapshot: None,
            json_logs: None,
        };
        let s = Settings::load(&cli).unwrap();
        assert_eq!(s.listen.port(), 6000);
        assert!(s.strict);
        assert_eq!(s.channels.len(), 1);
        assert_eq!(s.channels[0].name, "hook");
    }

    fn tempfile_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "webhook-mocker-config-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
