use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::Arc,
};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use uuid::Uuid;

use utoipa::ToSchema;

use crate::faults::FaultProfile;
use crate::render::RenderedMessage;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelKind {
    Slack,
    TeamsWorkflows,
    TeamsLegacy,
    Generic,
}

impl ChannelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::TeamsWorkflows => "teams",
            Self::TeamsLegacy => "teams-legacy",
            Self::Generic => "generic",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Slack => "Slack",
            Self::TeamsWorkflows => "Teams Workflows",
            Self::TeamsLegacy => "Teams Legacy",
            Self::Generic => "Generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Channel {
    pub id: Uuid,
    pub kind: ChannelKind,
    pub name: String,
    /// Path token used in webhook URLs (Slack uses team/bot/token; others use a single id).
    pub token: String,
    /// Slack-specific path parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack_team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack_bot: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub archived: bool,
    pub faults: FaultProfile,
    pub unread: usize,
}

impl Channel {
    pub fn webhook_path(&self) -> String {
        match self.kind {
            ChannelKind::Slack => format!(
                "/slack/services/{}/{}/{}",
                self.slack_team.as_deref().unwrap_or("T0"),
                self.slack_bot.as_deref().unwrap_or("B0"),
                self.token
            ),
            ChannelKind::TeamsWorkflows => format!("/teams/workflows/{}", self.token),
            ChannelKind::TeamsLegacy => format!("/teams/webhookb2/{}", self.token),
            ChannelKind::Generic => format!("/hooks/{}", self.token),
        }
    }

    pub fn webhook_url(&self, public_base: &str) -> String {
        format!(
            "{}{}",
            public_base.trim_end_matches('/'),
            self.webhook_path()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CaptureResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Capture {
    pub id: Uuid,
    pub channel_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub received_at: OffsetDateTime,
    pub remote_addr: Option<String>,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body_bytes: Vec<u8>,
    pub body_utf8: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub body_json: Option<serde_json::Value>,
    pub warnings: Vec<String>,
    pub rendered: RenderedMessage,
    pub response: CaptureResponse,
}

impl Capture {
    pub fn body_preview(&self) -> String {
        if let Some(s) = &self.body_utf8 {
            if s.len() > 200 {
                format!("{}…", &s[..200])
            } else {
                s.clone()
            }
        } else {
            format!("<{} binary bytes>", self.body_bytes.len())
        }
    }

    pub fn body_hex(&self) -> String {
        hex::encode(&self.body_bytes)
    }

    pub fn curl(&self, public_base: &str) -> String {
        let url = format!(
            "{}{}{}",
            public_base.trim_end_matches('/'),
            self.path,
            self.query
                .as_ref()
                .map(|q| format!("?{q}"))
                .unwrap_or_default()
        );
        let mut parts = vec![format!("curl -X {} '{}'", self.method, url)];
        for (k, v) in &self.headers {
            let lower = k.to_ascii_lowercase();
            if lower == "host" || lower == "content-length" {
                continue;
            }
            parts.push(format!("  -H '{}: {}'", k, v.replace('\'', "'\\''")));
        }
        if !self.body_bytes.is_empty() {
            if let Some(utf8) = &self.body_utf8 {
                parts.push(format!("  --data-binary '{}'", utf8.replace('\'', "'\\''")));
            } else {
                parts.push(format!(
                    "  --data-binary @<(echo -n '{}' | xxd -r -p)",
                    hex::encode(&self.body_bytes)
                ));
            }
        }
        parts.join(" \\\n")
    }
}

#[derive(Debug, Clone)]
pub enum StoreEvent {
    ChannelCreated(Channel),
    ChannelUpdated(Channel),
    ChannelDeleted {
        id: Uuid,
    },
    MessageCaptured {
        channel_id: Uuid,
        capture: Box<Capture>,
    },
    MessagesCleared {
        channel_id: Uuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Snapshot {
    channels: Vec<Channel>,
    messages: HashMap<Uuid, Vec<Capture>>,
}

#[derive(Debug)]
struct Inner {
    channels: HashMap<Uuid, Channel>,
    /// Lookup by webhook path token key.
    by_path_key: HashMap<String, Uuid>,
    messages: HashMap<Uuid, VecDeque<Capture>>,
    max_messages: usize,
}

#[derive(Clone)]
pub struct Store {
    inner: Arc<RwLock<Inner>>,
    tx: broadcast::Sender<StoreEvent>,
    snapshot_path: Option<std::path::PathBuf>,
}

impl Store {
    pub fn new(max_messages: usize, snapshot_path: Option<std::path::PathBuf>) -> Self {
        let (tx, _) = broadcast::channel(1024);
        let mut store = Self {
            inner: Arc::new(RwLock::new(Inner {
                channels: HashMap::new(),
                by_path_key: HashMap::new(),
                messages: HashMap::new(),
                max_messages: max_messages.max(1),
            })),
            tx,
            snapshot_path,
        };
        if let Some(path) = store.snapshot_path.clone()
            && path.exists()
            && let Err(err) = store.load_snapshot(&path)
        {
            tracing::warn!(error = %err, path = %path.display(), "failed to load snapshot");
        }
        store
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StoreEvent> {
        self.tx.subscribe()
    }

    fn path_key(kind: ChannelKind, token: &str, team: Option<&str>, bot: Option<&str>) -> String {
        match kind {
            ChannelKind::Slack => format!(
                "slack:{}:{}:{}",
                team.unwrap_or("T0"),
                bot.unwrap_or("B0"),
                token
            ),
            ChannelKind::TeamsWorkflows => format!("teams:{token}"),
            ChannelKind::TeamsLegacy => format!("teams-legacy:{token}"),
            ChannelKind::Generic => format!("generic:{token}"),
        }
    }

    pub fn create_channel(
        &self,
        kind: ChannelKind,
        name: String,
        token: Option<String>,
        slack_team: Option<String>,
        slack_bot: Option<String>,
    ) -> Channel {
        let token = token.unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        let team = if kind == ChannelKind::Slack {
            Some(slack_team.unwrap_or_else(|| "T0".into()))
        } else {
            None
        };
        let bot = if kind == ChannelKind::Slack {
            Some(slack_bot.unwrap_or_else(|| "B0".into()))
        } else {
            None
        };
        let channel = Channel {
            id: Uuid::new_v4(),
            kind,
            name,
            token: token.clone(),
            slack_team: team.clone(),
            slack_bot: bot.clone(),
            created_at: OffsetDateTime::now_utc(),
            archived: false,
            faults: FaultProfile::default(),
            unread: 0,
        };
        let key = Self::path_key(kind, &token, team.as_deref(), bot.as_deref());
        {
            let mut inner = self.inner.write();
            inner.by_path_key.insert(key, channel.id);
            inner.messages.insert(channel.id, VecDeque::new());
            inner.channels.insert(channel.id, channel.clone());
        }
        let _ = self.tx.send(StoreEvent::ChannelCreated(channel.clone()));
        self.maybe_persist();
        channel
    }

    pub fn get_or_create_by_path(
        &self,
        kind: ChannelKind,
        token: &str,
        team: Option<&str>,
        bot: Option<&str>,
        auto_create: bool,
    ) -> Option<Channel> {
        let key = Self::path_key(kind, token, team, bot);
        {
            let inner = self.inner.read();
            if let Some(id) = inner.by_path_key.get(&key) {
                return inner.channels.get(id).cloned();
            }
        }
        if !auto_create {
            return None;
        }
        let name = format!("{}-{}", kind.as_str(), &token[..token.len().min(8)]);
        Some(self.create_channel(
            kind,
            name,
            Some(token.to_string()),
            team.map(str::to_string),
            bot.map(str::to_string),
        ))
    }

    pub fn list_channels(&self) -> Vec<Channel> {
        let mut list: Vec<_> = self.inner.read().channels.values().cloned().collect();
        list.sort_by_key(|a| a.created_at);
        list
    }

    pub fn get_channel(&self, id: Uuid) -> Option<Channel> {
        self.inner.read().channels.get(&id).cloned()
    }

    pub fn update_faults(&self, id: Uuid, faults: FaultProfile) -> Option<Channel> {
        let channel = {
            let mut inner = self.inner.write();
            let ch = inner.channels.get_mut(&id)?;
            ch.faults = faults;
            ch.clone()
        };
        let _ = self.tx.send(StoreEvent::ChannelUpdated(channel.clone()));
        self.maybe_persist();
        Some(channel)
    }

    pub fn set_archived(&self, id: Uuid, archived: bool) -> Option<Channel> {
        let channel = {
            let mut inner = self.inner.write();
            let ch = inner.channels.get_mut(&id)?;
            ch.archived = archived;
            ch.clone()
        };
        let _ = self.tx.send(StoreEvent::ChannelUpdated(channel.clone()));
        self.maybe_persist();
        Some(channel)
    }

    pub fn delete_channel(&self, id: Uuid) -> bool {
        let removed = {
            let mut inner = self.inner.write();
            let Some(ch) = inner.channels.remove(&id) else {
                return false;
            };
            let key = Self::path_key(
                ch.kind,
                &ch.token,
                ch.slack_team.as_deref(),
                ch.slack_bot.as_deref(),
            );
            inner.by_path_key.remove(&key);
            inner.messages.remove(&id);
            true
        };
        if removed {
            let _ = self.tx.send(StoreEvent::ChannelDeleted { id });
            self.maybe_persist();
        }
        removed
    }

    pub fn push_capture(&self, capture: Capture) {
        let channel_id = capture.channel_id;
        {
            let mut inner = self.inner.write();
            if let Some(ch) = inner.channels.get_mut(&channel_id) {
                ch.unread = ch.unread.saturating_add(1);
            }
            let max = inner.max_messages;
            let q = inner.messages.entry(channel_id).or_default();
            q.push_back(capture.clone());
            while q.len() > max {
                q.pop_front();
            }
        }
        let _ = self.tx.send(StoreEvent::MessageCaptured {
            channel_id,
            capture: Box::new(capture),
        });
        self.maybe_persist();
    }

    pub fn list_messages(&self, channel_id: Uuid) -> Vec<Capture> {
        self.inner
            .read()
            .messages
            .get(&channel_id)
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_message(&self, channel_id: Uuid, message_id: Uuid) -> Option<Capture> {
        self.inner
            .read()
            .messages
            .get(&channel_id)?
            .iter()
            .find(|m| m.id == message_id)
            .cloned()
    }

    pub fn clear_messages(&self, channel_id: Uuid) -> bool {
        let ok = {
            let mut inner = self.inner.write();
            if !inner.channels.contains_key(&channel_id) {
                return false;
            }
            if let Some(q) = inner.messages.get_mut(&channel_id) {
                q.clear();
            }
            if let Some(ch) = inner.channels.get_mut(&channel_id) {
                ch.unread = 0;
            }
            true
        };
        if ok {
            let _ = self.tx.send(StoreEvent::MessagesCleared { channel_id });
            self.maybe_persist();
        }
        ok
    }

    pub fn mark_read(&self, channel_id: Uuid) {
        let mut inner = self.inner.write();
        if let Some(ch) = inner.channels.get_mut(&channel_id) {
            ch.unread = 0;
        }
    }

    pub fn message_count(&self, channel_id: Uuid) -> usize {
        self.inner
            .read()
            .messages
            .get(&channel_id)
            .map(|q| q.len())
            .unwrap_or(0)
    }

    fn maybe_persist(&self) {
        if let Some(path) = &self.snapshot_path
            && let Err(err) = self.save_snapshot(path)
        {
            tracing::warn!(error = %err, "failed to save snapshot");
        }
    }

    fn save_snapshot(&self, path: &Path) -> Result<(), String> {
        let snap = {
            let inner = self.inner.read();
            Snapshot {
                channels: inner.channels.values().cloned().collect(),
                messages: inner
                    .messages
                    .iter()
                    .map(|(k, v)| (*k, v.iter().cloned().collect()))
                    .collect(),
            }
        };
        let json = serde_json::to_vec_pretty(&snap).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    fn load_snapshot(&mut self, path: &Path) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let snap: Snapshot = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        let mut inner = self.inner.write();
        inner.channels.clear();
        inner.by_path_key.clear();
        inner.messages.clear();
        for ch in snap.channels {
            let key = Self::path_key(
                ch.kind,
                &ch.token,
                ch.slack_team.as_deref(),
                ch.slack_bot.as_deref(),
            );
            let id = ch.id;
            inner.by_path_key.insert(key, id);
            inner.channels.insert(id, ch);
            inner.messages.insert(id, VecDeque::new());
        }
        for (id, msgs) in snap.messages {
            let max = inner.max_messages;
            let q = inner.messages.entry(id).or_default();
            for m in msgs {
                q.push_back(m);
            }
            while q.len() > max {
                q.pop_front();
            }
        }
        Ok(())
    }
}
